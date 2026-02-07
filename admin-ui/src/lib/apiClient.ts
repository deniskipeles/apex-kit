import { APEX_TOKEN } from '../constants';
import { Collection, AppRecord, SystemLog, AuthUser, StoredFile, InstantResult, Script, Template, AiAction, AppVersions, ApiKey, SiteFile, Tenant } from '../types';
import { ApexKit as PowerBase, ApexKitRealtimeWSClient as ApexKitRealtime } from './sdk';

// Initialize SDK
const apiUrl = (import.meta as any).env.DEV
  ? (import.meta as any).env.VITE_API_URL?.trim() || 'http://127.0.0.1:5000'
  : (typeof window !== 'undefined' ? window.origin : 'http://127.0.0.1:5000').trim();

const basePb = new PowerBase(apiUrl);

// Load persisted token
const storedToken = localStorage.getItem(APEX_TOKEN);
if (storedToken) {
  basePb.setToken(storedToken);
}

// --- DYNAMIC CLIENT PROXY ---
export const pb = new Proxy(basePb, {
  get(target, prop, receiver) {
    if (typeof window !== 'undefined') {
      const path = window.location.pathname;

      const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)/);
      if (tenantMatch && tenantMatch[1]) {
        const tenantInstance = target.tenant(tenantMatch[1]);
        return Reflect.get(tenantInstance, prop, receiver);
      }

      const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)/);
      if (sandboxMatch && sandboxMatch[1]) {
        const sandboxInstance = target.sandbox(sandboxMatch[1]);
        return Reflect.get(sandboxInstance, prop, receiver);
      }
    }
    return Reflect.get(target, prop, receiver);
  }
});

export const realtime = new ApexKitRealtime(pb.baseUrl, pb.getToken());

const downloadBlob = (blob: Blob, filename: string) => {
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.URL.revokeObjectURL(url);
};

const rawFetch = async (path: string) => {
  let baseUrl = apiUrl;
  if (typeof window !== 'undefined') {
    const pathName = window.location.pathname;
    const tenantMatch = pathName.match(/^\/_dashboard\/tenant\/([^/]+)/);
    const sandboxMatch = pathName.match(/^\/_dashboard\/sandbox\/([^/]+)/);

    if (tenantMatch) baseUrl += `/tenant/${tenantMatch[1]}`;
    else if (sandboxMatch) baseUrl += `/sandbox/${sandboxMatch[1]}`;
  }

  const token = localStorage.getItem(APEX_TOKEN);
  const res = await fetch(`${baseUrl}/api/v1${path}`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });

  if (!res.ok) {
    const txt = await res.text();
    throw new Error(txt || res.statusText);
  }
  return res;
};

const rawFetchWithBody = async (path: string, body: FormData) => {
  let baseUrl = apiUrl;
  if (typeof window !== 'undefined') {
    const pathName = window.location.pathname;
    const tenantMatch = pathName.match(/^\/_dashboard\/tenant\/([^/]+)/);
    const sandboxMatch = pathName.match(/^\/_dashboard\/sandbox\/([^/]+)/);
    if (tenantMatch) baseUrl += `/tenant/${tenantMatch[1]}`;
    else if (sandboxMatch) baseUrl += `/sandbox/${sandboxMatch[1]}`;
  }

  const token = localStorage.getItem(APEX_TOKEN);
  const res = await fetch(`${baseUrl}/api/v1${path}`, {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${token}` },
    body: body
  });
  if (!res.ok) throw new Error(await res.text());
  return await res.json();
};

const transformCollection = (col: any): Collection => {
  if (!col) return col;

  let schemaArray: any[] = [];

  if (col.schema && col.schema.fields) {
    schemaArray = Object.entries(col.schema.fields).map(([name, def]: [string, any]) => {
      let uiType = def.type;
      if (uiType === 'boolean') uiType = 'bool';

      return {
        name,
        type: uiType,
        required: def.required,
        unique: def.unique,
        ose_indexed: def.ose_indexed,
        sql_indexed: def.sql_indexed,
        auto: def.auto,
        default: def.default,
        uid: def.uid,
        position: def.position,
        vectorize: def.vectorize,
        min: def.min,
        max: def.max,
        minLength: def.min_length,
        maxLength: def.max_length,
        pattern: def.pattern,
        options: def.options,
        mimeTypes: def.mime_types,
        maxSize: def.max_size,
        dimension: def.dimension,
        relationTo: def.relation_to,
        originalName: name
      };
    });
  }

  if (col.schema && col.schema.relations) {
    Object.entries(col.schema.relations).forEach(([name, def]: [string, any]) => {
      schemaArray.push({
        name,
        type: 'relation',
        relationTo: def.target_collection,
        required: def.required || false,
        unique: def.relation_type === 'one' ? true : false,
        originalName: name,
        position: def.position || 999,
        uid: def.uid || "gen_rel"
      });
    });
  }

  schemaArray.sort((a, b) => (a.position || 0) - (b.position || 0));

  const rules = col.schema?.policies || { read: 'public', create: 'admin', update: 'admin', delete: 'admin' };
  const fieldHistory = col.schema?.field_history || {};
  const compositeUnique = col.schema?.composite_unique || [];

  return {
    id: col.id.toString(),
    name: col.name,
    type: col.type || 'base',
    schema: schemaArray,
    rules: rules,
    fieldHistory: fieldHistory,
    compositeUnique: compositeUnique,
    created: new Date().toISOString(),
    updated: new Date().toISOString()
  };
};

const transformToBackendSchema = (data: Partial<Collection>) => {
  const schema: any = {
    fields: {},
    relations: {},
    policies: data.rules || {},
    field_history: data.fieldHistory || {},
    composite_unique: data.compositeUnique || []
  };

  if (data.schema) {
    data.schema.forEach(field => {
      const { name } = field;
      if (field.type === 'relation') {
        schema.relations[name] = {
          target_collection: field.relationTo,
          relation_type: field.unique ? 'one' : 'many',
          position: field.position,
          required: field.required,
          uid: field.uid
        };
        return;
      }
      const backendField: any = {
        type: field.type,
        required: field.required,
        unique: field.unique,
        ose_indexed: field.ose_indexed,
        sql_indexed: field.sql_indexed,
        default: field.default,
        auto: field.auto,
        uid: field.uid,
        position: field.position,
        vectorize: field.vectorize,
        min: field.min,
        max: field.max,
        min_length: field.minLength,
        max_length: field.maxLength,
        pattern: field.pattern,
        options: field.options,
        mime_types: field.mimeTypes,
        max_size: field.maxSize,
        dimension: field.dimension,
        relation_to: field.relationTo
      };
      if (backendField.type === 'bool') {
        backendField.type = 'boolean';
      }
      Object.keys(backendField).forEach(key => backendField[key] === undefined && delete backendField[key]);
      schema.fields[name] = backendField;
    });
  }
  return schema;
};

export const apiClient = {
  getScope: () => pb.scope,
  setToken: (token: string) => basePb.setToken(token),
  apiUrl: apiUrl,
  logoUrl: apiUrl + "/logo?thumb=100x100",
  stripHtmlTags: (html: string) => {
    // Basic shim for stripping tags if util isn't exposed yet
    if (!html) return '';
    return html.replace(/<[^>]*>?/gm, '');
  },
  getAdminDashboardStats: pb.admins.getDashboardStats,
  searchVector: (collectionId: string | number, field: string, vector: Array<number>, limit?: number) => pb.collection(collectionId).searchVector(field, vector, limit),
  searchTextVector: (collectionId: string | number, queryText: string, limit?: number) => pb.collection(collectionId).searchTextVector(queryText, limit),
  reIndex: async (collectionId?: string) => {
    const res = await pb.admins.reIndex(collectionId);
    return res;
  },
  revectorizeCollection: async (collectionId?: string) => {
    const res = await pb.admins.revectorizeCollection(collectionId);
    return res;
  },

  system: {
    reload: async () => await pb.admins.reloadSystem(),
    testEmail: async (email: string) => await pb.admins.testEmail(email),
    createBackup: async () => await pb.admins.createBackup(),
    listBackups: async () => await pb.admins.listBackups(),
    downloadBackup: async (filename: string) => {
      const blob = await pb.admins.downloadBackup(filename);
      downloadBlob(blob, filename);
    },
    restoreFromFile: async (filename: string) => await pb.admins.restoreFromFile(filename),
    restoreBackup: async (file: File) => await pb.admins.restoreBackup(file),
  },

  keys: {
    list: async (): Promise<ApiKey[]> => {
      const res = await pb.admins.listApiKeys();
      return res.map((k: any) => ({
        id: k.id.toString(),
        name: k.name,
        prefix: k.prefix,
        role: k.role,
        scope: k.scope,
        bypass_cors: k.bypass_cors,
        created: k.created_at
      }));
    },
    create: async (name: string, role = 'admin', scope = 'root', bypass_cors = true): Promise<{ key: string, info: ApiKey }> => {
      const res = await pb.admins.createApiKey(name, role, scope, bypass_cors);
      return {
        key: res.key,
        info: {
          id: res.info.id.toString(),
          name: res.info.name,
          prefix: res.info.prefix,
          role: res.info.role,
          scope: res.info.scope,
          bypass_cors: res.info.bypass_cors,
          created: res.info.created_at
        }
      };
    },
    update: async (id: string, updates: Partial<ApiKey>) => await pb.admins.updateApiKey(id, updates),
    delete: async (id: string) => await pb.admins.deleteApiKey(id),
  },

  root: {
    createTenant: (id: string) => basePb.admins.createTenant(id),
    deleteTenant: (id: string) => pb.admins.deleteTenant(id),
    updateTenant: (id: string, data: any) => pb.admins.updateTenant(id, data),
    listTenants: async (): Promise<Tenant[]> => {
      // FIX 1: Map SDK result (any[]) to local Tenant type, providing defaults for missing fields
      const res = await basePb.admins.listTenants();
      return res.map((t: any) => ({
        id: t.id,
        name: t.name,
        status: t.status,
        tier: t.tier || 'free', // Default tier
        stats: t.stats || {
          storage_mb: 0,
          max_storage_mb: 0,
          vector_count: 0,
          max_vectors: 0,
          ai_requests: 0,
          max_ai_requests: 0
        },
        created_at: t.created_at
      }));
    },
    updateStatus: async (id: string, status: 'active' | 'suspended' | 'archived') => await pb.admins.updateTenantStatus(id, status),
  },

  auth: {
    listRoles: async () => await pb.auth.listRoles(),
    getMe: async () => {
      const u = await pb.auth.getMe();
      return {
        id: u.id.toString(),
        email: u.email,
        role: u.role,
        lastActive: new Date().toISOString(),
        scope: u.scope
      };
    },
    login: async (email: string, password: string) => {
      const response = await pb.auth.login(email, password);
      localStorage.setItem(APEX_TOKEN, response.token);
      const user = {
        id: response.user.id.toString(),
        email: response.user.email,
        role: response.user.role,
        scope: response.user.scope,
        metadata: response.user.metadata,
        lastActive: new Date().toISOString()
      };
      return { token: response.token, user };
    },
    logout: async () => {
      pb.auth.logout();
      localStorage.removeItem(APEX_TOKEN);
      return true;
    }
  },

  users: {
    list: async (page = 1, perPage = 20, search = ''): Promise<{ items: AuthUser[], total: number }> => {
      try {
        const res = await pb.admins.listUsers({ page, per_page: perPage, sort: "", filter: search });
        const items = res.items.map((u: any) => ({
          id: u.id.toString(),
          email: u.email,
          role: u.role,
          metadata: u.metadata,
          lastActive: new Date().toISOString(),
        }));
        return { items, total: res.total };
      } catch (e) {
        console.error("Error fetching users", e);
        return { items: [], total: 0 };
      }
    },
    create: async (data: Partial<AuthUser>): Promise<AuthUser> => {
      const res = await pb.admins.registerUser(data.email!, data.password, data.role, data.metadata);
      return {
        id: res.user.id.toString(),
        email: res.user.email,
        role: res.user.role,
        metadata: res.user.metadata,
      };
    },
    update: async (id: string, data: Partial<AuthUser>): Promise<AuthUser> => {
      const { email, role, password, metadata } = data;
      const res = await pb.admins.updateUser(id, email, password, role, metadata);
      
      // FIX 2: Explicitly cast 'id' because backend 'User' type has 'id' as string in SDK but might be number from raw response
      const updatedUser: AuthUser = {
        id: res.id.toString(), // Ensure string
        email: res.email || '',
        metadata: res.metadata,
        role: res.role,
      };
      return updatedUser;
    },
    delete: async (id: string): Promise<void> => {
      await pb.admins.deleteUser(id);
    }
  },

  configs: {
    list: async (): Promise<any[]> => await pb.admins.listConfigs(),
    set: async (key: string, value: string, encrypt: boolean = false) => await pb.admins.setConfig(key, value, encrypt),
    delete: async (key: string) => await pb.admins.deleteConfig(key)
  },

  sites: {
    deploy: async (file: File) => await pb.sites.deploy(file),
    list: async (): Promise<SiteFile[]> => {
      try {
        return await pb.sites.listFiles();
      } catch (e) {
        console.error("Failed to list site files", e);
        return [];
      }
    },
    delete: async (path: string): Promise<void> => {
      return await pb.sites.delete(path);
    }
  },

  collections: {
    list: async (): Promise<Collection[]> => {
      const cols = await pb.admins.listCollections();
      return cols.map(transformCollection);
    },
    get: async (id: string): Promise<Collection | undefined> => {
      const col = await pb.admins.getCollection(id);
      return col ? transformCollection(col) : undefined;
    },
    create: async (data: Partial<Collection>): Promise<Collection> => {
      const backendSchema = transformToBackendSchema(data);
      if (data.rules) backendSchema.policies = data.rules;
      if (data.compositeUnique) backendSchema.composite_unique = data.compositeUnique;
      if (data.fieldHistory) backendSchema.field_history = data.fieldHistory;

      const res = await pb.admins.createCollection(data.name, backendSchema);
      return transformCollection(res);
    },
    update: async (id: string, data: Partial<Collection>): Promise<Collection> => {
      const payload: any = { name: data.name };
      if (data.schema || data.rules) {
        payload.schema = transformToBackendSchema(data);
      }
      if (data.rules) payload.policies = data.rules;
      if (data.compositeUnique) payload.composite_unique = data.compositeUnique;
      if (data.fieldHistory) payload.field_history = data.fieldHistory;

      const res = await pb.admins.updateCollection(id, payload);
      return transformCollection(res);
    },
    delete: async (id: string): Promise<void> => pb.admins.deleteCollection(id),
    revectorize: (id: string) => pb.admins.revectorizeCollection(id),
    reIndex: (id: string) => pb.admins.reIndex(id),

    exportSchema: async () => {
      const res = await rawFetch('/admin/export-schema');
      const blob = await res.blob();
      downloadBlob(blob, 'apex_schema.json');
    },
    importSchema: async (file: File, strategy: 'skip' | 'overwrite' | 'error' = 'skip') => {
      return await pb.admins.importSchema(file, strategy);
    },
  },

  records: {
    list: async (collectionId: string, page = 1, perPage = 20, expand = '', filter = {}, sort = '-id'): Promise<{ items: AppRecord[], totalItems: number }> => {
      const result = await pb.collection(collectionId).list({
        page,
        per_page: perPage,
        expand: expand,
        filter: filter,
        sort: sort,
      });

      const formattedItems = result.items.map((item: any) => ({
        id: item.id.toString(),
        collectionId,
        collectionName: 'unknown',
        created: new Date(item.created).toISOString(),
        updated: new Date(item.updated).toISOString(),
        ...item.data,
        expand: item.expand || {}
      }));

      return {
        items: formattedItems,
        totalItems: result.total
      };
    },
    instantSearch: async (collectionId: string | number, query: string): Promise<InstantResult[]> => {
      if (!query) return [];
      try {
        return await pb.collection(collectionId).searchRecordsInstantlyWithOSE(query);
      } catch (e) {
        console.error("Instant search failed", e);
        return [];
      }
    },
    recordsSearchOSE: async (collectionId: string | number, query: string): Promise<InstantResult[]> => {
      if (!query) return [];
      try {
        const result = await pb.collection(collectionId).searchRecordsWithOSE(query);
        return result.map((item: any) => ({
          id: item.id.toString(),
          collectionId,
          collectionName: 'unknown',
          created: new Date(item.created).toISOString(),
          updated: new Date(item.updated).toISOString(),
          ...item.data,
          expand: item.expand || {}
        }));
      } catch (e) {
        console.error("OSE Records search failed", e);
        return [];
      }
    },
    recordsSearchSQL: async (collectionId: string | number, query: any): Promise<InstantResult[]> => {
      if (!query) return [];
      try {
        const result = await pb.collection(collectionId).searchRecordsWithSQL(query);
        return result.map((item: any) => ({
          id: item.id.toString(),
          collectionId,
          collectionName: 'unknown',
          created: new Date(item.created).toISOString(),
          updated: new Date(item.updated).toISOString(),
          ...item.data,
          expand: item.expand || {}
        }));
      } catch (e) {
        console.error("SQL Records search failed", e);
        return [];
      }
    },
    create: async (collectionId: string, data: any): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).create(data);
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date(res.created).toISOString(),
        updated: new Date(res.updated).toISOString(),
        ...res.data
      };
    },
    update: async (collectionId: string, id: string, data: any): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).update(id, data);
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date(res.created).toISOString(),
        updated: new Date(res.updated).toISOString(),
        ...res.data
      };
    },
    getOne: async (collectionId: string, recordId: string, expand = ''): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).get(recordId, { expand: expand });
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date(res.created).toISOString(),
        updated: new Date(res.updated).toISOString(),
        ...res.data,
        expand: res.expand || {}
      };
    },
    delete: async (collectionId: string, recordId: string): Promise<void> => {
      return await pb.collection(collectionId).delete(recordId);
    },
    searchTextVector: async (collectionId: string | number, query: string, limit = 10) => {
      return await pb.collection(collectionId).searchTextVector(query, limit);
    },
    searchVector: async (collectionId: string, field: string, vector: number[], limit = 10) => {
      return await pb.collection(collectionId).searchVector(field, vector, limit);
    },
    getVector: async (collectionId: string, recordId: number | string) => {
      return await pb.collection(collectionId).getVector(recordId);
    },
    importData: async (collectionName: string, file: File) => {
      const res = await pb.admins.importData(collectionName, file);
      return res;
    },
    exportData: async (collectionId: string, format: 'json' | 'csv' = 'json') => {
      const res = await rawFetch(`/admin/export-data/${collectionId}?format=${format}`);
      const blob = await res.blob();
      const disposition = res.headers.get('Content-Disposition');
      let filename = `collection_${collectionId}.${format}`;
      if (disposition && disposition.indexOf('filename=') !== -1) {
        const matches = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/.exec(disposition);
        if (matches != null && matches[1]) {
          filename = matches[1].replace(/['"]/g, '');
        }
      }
      downloadBlob(blob, filename);
    },
  },

  testS3Connection: async (config: any) => {
    const payload = {
      bucket: config.bucket,
      region: config.region,
      endpoint: config.endpoint,
      access_key: config.accessKey,
      secret_key: config.secretKey
    };
    return await pb.admins.testS3StorageConnection(payload);
  },

  migrateStorage: async (source: string, destination: string) => {
    // FIX 3: Cast arguments to specific union type as required by SDK
    return await pb.admins.migrateStorage(source as "local" | "s3", destination as "local" | "s3");
  },

  files: {
    list: async (page = 1, perPage = 20, search = ''): Promise<{ items: StoredFile[], totalItems: number }> => {
      try {
        const res = await pb.files.list(page, perPage);
        const items = res.items.map((f: any) => ({
          id: f.id.toString(),
          name: f.original_name,
          size: f.size,
          mimeType: f.mime_type,
          url: pb.files.getFileUrl(f.filename),
          created: f.created_at,
          updated: f.created_at
        }));
        return { items, totalItems: res.total || items.length };
      } catch (e) {
        console.error("File list error", e);
        return { items: [], totalItems: 0 };
      }
    },
    upload: async (file: File): Promise<StoredFile> => {
      const res = await pb.files.upload(file);
      return {
        id: res.id.toString(),
        name: res.filename,
        size: file.size,
        mimeType: file.type,
        url: res.url,
        created: new Date().toISOString(),
        updated: new Date().toISOString()
      };
    },
    delete: async (id: string): Promise<void> => {
      await pb.files.delete(id);
    },
    getFileUrl: (filename: string) => pb.files.getFileUrl(filename)
  },

  scripts: {
    list: async (): Promise<Script[]> => {
      const res = await pb.scripts.list();
      // FIX 4: Map backend response to local Script type (ensure target_collection)
      return res.map((s: any) => ({
          ...s,
          id: s.id.toString(),
          target_collection: s.target_collection || '' // Default string if missing
      }));
    },
    create: async (data: Partial<Script>): Promise<Script> => {
      // FIX 5: Cast Partial<Script> to any to satisfy Omit type in SDK
      const res = await pb.scripts.create(data as any);
      return { ...data, id: res.id } as Script;
    },
    delete: async (id: string): Promise<void> => {
      await pb.scripts.delete(id);
    },
    run: async (name: string, variables: any): Promise<any> => {
      return await pb.scripts.run(name, variables);
    },
    export: async () => {
      const res = await rawFetch('/admin/export-scripts');
      downloadBlob(await res.blob(), 'scripts.json');
    },
    import: async (file: File) => {
      const formData = new FormData();
      formData.append('file', file);
      return rawFetchWithBody('/admin/import-scripts', formData);
    }
  },

  templates: {
    list: async (): Promise<Template[]> => {
      const res = await pb.templates.list();
      return res.map((t: any) => ({
        ...t,
        id: t.id.toString(),
        script_id: t.script_id ? t.script_id.toString() : null
      }));
    },
    create: async (data: Partial<Template>) => {
      // FIX 6: Cast Partial<Template> to any
      await pb.templates.create(data as any);
    },
    update: async (id: string, data: Partial<Template>) => {
      await pb.templates.update(id, data);
    },
    delete: async (id: string) => {
      await pb.templates.delete(id);
    },
    export: async () => {
      const res = await rawFetch('/admin/export-templates');
      downloadBlob(await res.blob(), 'templates.json');
    },
    import: async (file: File) => {
      const formData = new FormData();
      formData.append('file', file);
      return rawFetchWithBody('/admin/import-templates', formData);
    }
  },

  ai: {
    getActions: async (): Promise<AiAction[]> => {
      const res = await pb.ai.getActions();
      // FIX 7: Map ID to string
      return res.map((a: any) => ({
          ...a,
          id: a.id.toString(),
          system_prompt: a.system_prompt || '',
          config: a.config || {}
      }));
    },
    createAction: async (data: Partial<AiAction>) => {
      await pb.ai.createAction(data);
    },
    deleteAction: async (id: string) => {
      await pb.ai.deleteAction(id);
    },
    run: async (slug: string, variables: Record<string, string>) => {
      return await pb.ai.run(slug, variables);
    },
    listSessions: async () => await pb.ai.listSessions(),
    createSession: async (name: string, initialPrompt?: string, model?: string, cloneStrategy?: string, cloneRecordLimit?: number) => {
      return await pb.ai.createSession(name, initialPrompt, model, cloneStrategy, cloneRecordLimit);
    },
    deleteSession: (id: string) => pb.ai.deleteSession(id),
    chat: async (id: string, prompt: string, model: string) => {
      return await pb.ai.chat(id, prompt, model);
    },
    applySessionChanges: async (id: string) => {
      return await pb.ai.applySessionChanges(id);
    },
    publishSession: async (id: string) => {
      return await pb.ai.publishSession(id);
    },
    listPlugins: async () => {
      return await pb.ai.listPlugins();
    },
    codeEdit: async (prompt: string, currentCode: string, contextType: string, model: string) => {
      return await pb.ai.editCode(prompt, currentCode, contextType, model);
    },
    exportActions: async () => {
      const res = await rawFetch('/admin/export-ai-actions');
      downloadBlob(await res.blob(), 'ai_actions.json');
    },
    importActions: async (file: File) => {
      const formData = new FormData();
      formData.append('file', file);
      return rawFetchWithBody('/admin/import-ai-actions', formData);
    }
  },

  logs: {
    list: async (): Promise<SystemLog[]> => {
      try {
        const res = await pb.logs.list();
        return res.map((l: any) => ({
          id: l.id.toString(),
          level: l.level,
          message: l.message,
          source: l.source,
          timestamp: l.timestamp
        }));
      } catch {
        return [];
      }
    }
  },

  getVersions: async (): Promise<AppVersions> => {
    try {
      const res = await fetch('/version');
      if (!res.ok) throw new Error('Failed to fetch versions');
      return await res.json();
    } catch (e) {
      console.error(e);
      return { root: '0.0.0', api: '0.0.0', core: '0.0.0', vector: '0.0.0' };
    }
  },
};