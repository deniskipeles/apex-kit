import { APEX_TOKEN } from '../constants';
import { Collection, AppRecord, SystemLog, AdminUser, StoredFile, InstantResult, Script, Template, AiAction } from '../types';
import { PowerBase } from './sdk';

// const env = (import.meta as any).env;
// Initialize SDK
const apiUrl = (import.meta as any).env.DEV
  ? (import.meta as any).env.VITE_API_URL?.trim() || 'http://127.0.0.1:5000'
  : (typeof window !== 'undefined' ? window.origin : 'http://127.0.0.1:5000').trim();
export const pb = new PowerBase(apiUrl);

// Load persisted token
const storedToken = localStorage.getItem(APEX_TOKEN);
if (storedToken) {
  pb.setToken(storedToken);
}


// --- HELPER: Transform Backend Collection to Frontend Interface ---
const transformCollection = (col: any): Collection => {
  if (!col) return col;

  let schemaArray: any[] = [];

  // 1. Map Standard Fields
  if (col.schema && col.schema.fields) {
      schemaArray = Object.entries(col.schema.fields).map(([name, def]: [string, any]) => {
          let uiType = def.type;
          if (uiType === 'boolean') uiType = 'bool'; 
          
          return {
              name,
              type: uiType,
              required: def.required,
              unique: def.unique,
              indexed: def.indexed,
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

  // 2. Map Relations (Separate map in backend -> Field in frontend)
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

  // Sort schema array by position to ensure UI renders correctly
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

// --- HELPER: Transform Frontend Schema -> Backend Format ---
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

          // CASE 1: RELATION (Explicit Type)
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

          // Prepare Backend Field Definition
          const backendField: any = {
              type: field.type,
              required: field.required,
              unique: field.unique,
              indexed: field.indexed,
              default: field.default,
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
  apiUrl: apiUrl,
  stripHtmlTags: pb.utils.stripHtmlTags,
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
  importData: async (collectionName: string, file: File) => {
    const res = await pb.admins.importData(collectionName, file);
    return res;
  },
  exportData: async (collectionId: number | string, format?: "json" | "csv") => {
    const res = await pb.admins.exportData(collectionId, format);
    return res;
  },


  auth: {
    login: async (email: string, password: string) => {
      const response = await pb.auth.login(email, password);
      localStorage.setItem(APEX_TOKEN, response.token);
      const user = {
        id: response.user.id.toString(),
        email: response.user.email,
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
    list: async (): Promise<AdminUser[]> => {
      try {
        // Call the new Admin API
        const res = await pb.admins.listUsers();

        // Map to AdminUser type
        return res.map((u: any) => ({
          id: u.id.toString(),
          email: u.email,
          lastActive: new Date().toISOString(), // API doesn't track this yet, simulate
          avatar: ''
        }));
      } catch (e) {
        console.error("Error fetching users", e);
        return [];
      }
    },
    create: async (data: Partial<AdminUser>): Promise<AdminUser> => {
      // Use Auth Register endpoint to create user
      // Note: We assume a default password if not provided, or require it in UI
      const password = (data as any).password || 'password123';

      const res = await pb.auth.register(data.email!, password);

      return {
        id: res.user.id.toString(),
        email: res.user.email,
        lastActive: new Date().toISOString()
      };
    },
    update: async (id: string, data: Partial<AdminUser>): Promise<AdminUser> => {
      // Backend doesn't have user update yet (e.g. changing email/password as admin)
      // We return the data to optimistic update the UI so it doesn't crash
      return {
        id,
        email: data.email || '',
        lastActive: new Date().toISOString(),
        ...data
      } as AdminUser;
    },
    delete: async (id: string): Promise<void> => {
      await pb.admins.deleteUser(id);
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
    delete: async (id: string): Promise<void> => {
      return pb.admins.deleteCollection(id);
    }
  },

  records: {
    list: async (collectionId: string, page = 1, perPage = 20, expand = '', filter = {}, sort = '-id'): Promise<{ items: AppRecord[], totalItems: number }> => {

      // Pass expand to the SDK list call
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
        created: new Date().toISOString(),
        updated: new Date().toISOString(),
        ...item.data,
        expand: item.expand || {} // Ensure expand object exists
      }));

      return {
        items: formattedItems,
        totalItems: result.total // Note: Backend needs to update to return count in meta
      };
    },
    instantSearch: async (collectionId: string | number, query: string): Promise<InstantResult[]> => {
      if (!query) return [];
      try {
        // Call the SDK
        return await pb.collection(collectionId).instantSearch(query);
      } catch (e) {
        console.error("Instant search failed", e);
        return [];
      }
    },
    create: async (collectionId: string, data: any): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).create(data);
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date().toISOString(),
        updated: new Date().toISOString(),
        ...res.data
      };
    },
    update: async (collectionId: string, id: string, data: any): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).update(id, data);
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date().toISOString(),
        updated: new Date().toISOString(),
        ...res.data
      };
    },
    getOne: async (collectionId: string, recordId: string, expand = ''): Promise<AppRecord> => {
      const res = await pb.collection(collectionId).get(recordId, { expand: expand });
      return {
        id: res.id.toString(),
        collectionId,
        collectionName: '',
        created: new Date().toISOString(),
        updated: new Date().toISOString(),
        ...res.data,
        expand: res.expand || {} // Ensure expand object exists
      };
    },
    delete: async (id: string): Promise<void> => {
      console.warn("Use recordsService.delete(collectionId, recordId) instead");
    }
  },

  files: {
    list: async (page = 1, perPage = 20, search = ''): Promise<{ items: StoredFile[], totalItems: number }> => {
      try {
        const res = await pb.files.list(page, perPage);

        // Map backend response to UI StoredFile type
        const items = res.items.map((f: any) => ({
          id: f.id.toString(),
          name: f.original_name, // Map original_name -> name
          size: f.size,
          mimeType: f.mime_type, // Map snake_case -> camelCase
          url: `${pb.baseUrl}/api/v1/storage/file/${f.filename}`, // Construct public URL
          created: f.created_at,
          updated: f.created_at
        }));

        return {
          items,
          totalItems: res.total || items.length
        };
      } catch (e) {
        console.error("File list error", e);
        return { items: [], totalItems: 0 };
      }
    },
    upload: async (file: File): Promise<StoredFile> => {
      const res = await pb.files.upload(file);
      return {
        id: res.id.toString(),
        name: res.filename, // Note: Upload response might differ slightly, verify backend
        size: file.size, // Optimistic size
        mimeType: file.type,
        url: res.url,
        created: new Date().toISOString(),
        updated: new Date().toISOString()
      };
    },
    delete: async (id: string): Promise<void> => {
      await pb.files.delete(id);
    }
  },

  scripts: {
    list: async (): Promise<Script[]> => {
      const res = await pb.scripts.list();
      // Map API response to UI types if necessary (snake_case -> camelCase)
      // Assuming Rust returns matching fields based on struct
      return res;
    },
    create: async (data: Partial<Script>): Promise<Script> => {
      const res = await pb.scripts.create(data);
      return { ...data, id: res.id } as Script;
    },
    delete: async (id: string): Promise<void> => {
      await pb.scripts.delete(id);
    },
    run: async (name: string, variables: any): Promise<any> => {
      return await pb.scripts.run(name, variables);
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
      await pb.templates.create(data);
    },
    update: async (id: string, data: Partial<Template>) => {
      await pb.templates.update(id, data);
    },
    delete: async (id: string) => {
      await pb.templates.delete(id);
    }
  },

  ai: {
    getActions: async (): Promise<AiAction[]> => {
      const res = await pb.ai.getActions();
      return res
    },
    createAction: async (data: Partial<AiAction>) => {
      await pb.ai.createAction(data);
    },
    deleteAction: async (id: string) => {
      await pb.ai.deleteAction(id);
    },
    run: async (slug: string, variables: Record<string, string>) => {
      return await pb.ai.run(slug, variables);
    }
  },

  logs: {
    list: async (): Promise<SystemLog[]> => {
      try {
        // Fetch from real endpoint
        const res = await pb.logs.list(); // You need to add listLogs to SDK admin object
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
  }
};