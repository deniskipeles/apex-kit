import { Collection, AppRecord, SystemLog, AdminUser, StoredFile, InstantResult, Script } from '../types';
import { PowerBase } from './sdk';

// Initialize SDK
const apiUrl = import.meta.env.VITE_API_URL || 'http://127.0.0.1:5000';
export const pb = new PowerBase(apiUrl);

// Load persisted token
const storedToken = localStorage.getItem('tinybase_token');
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
                ...def,
                type: uiType
            };
        });
    }

    // 2. Map Relations (Backend stores them separately, UI treats them as fields)
    if (col.schema && col.schema.relations) {
        Object.entries(col.schema.relations).forEach(([name, def]: [string, any]) => {
            schemaArray.push({
                name,
                type: 'relation',
                relationTo: def.target_collection,
                required: false // Relations are complex, usually optional in schema view
            });
        });
    }

    const rules = col.schema?.policies || { read: 'public', create: 'admin', update: 'admin', delete: 'admin' };

    return {
        id: col.id.toString(),
        name: col.name,
        type: 'base',
        schema: schemaArray,
        rules: rules,
        created: new Date().toISOString(),
        updated: new Date().toISOString()
    };
};

// --- HELPER: Transform Frontend Schema -> Backend Format ---
const transformToBackendSchema = (data: Partial<Collection>) => {
    const schema: any = { 
        fields: {}, 
        relations: {}, // Initialize relations object
        policies: data.rules || {} 
    };
    
    if (data.schema) {
        data.schema.forEach(field => {
            const { name, ...rest } = field;
            const backendField = { ...rest };

            // CASE 1: RELATION
            // Move to schema.relations, do NOT add to schema.fields
            if (backendField.type === 'relation') {
                schema.relations[name] = {
                    target_collection: backendField.relationTo,
                    relation_type: 'one' // Default to 'one' for now
                };
                return; 
            }

            // CASE 2: BOOLEAN
            if (backendField.type === 'bool') {
                (backendField as any).type = 'boolean';
            }
            
            // CASE 3: UI-ONLY TYPES
            // Map types not yet supported by backend to 'text' to avoid crash
            if (['date', 'url', 'email', 'select', 'file'].includes(backendField.type)) {
                 (backendField as any).type = 'text'; 
            }

            // CASE 4: STANDARD FIELDS
            schema.fields[name] = backendField;
        });
    }
    return schema;
};

export const apiClient = {
  auth: {
    login: async (email: string, password: string) => {
      const response = await pb.auth.login(email, password);
      localStorage.setItem('tinybase_token', response.token);
      const user = {
          id: response.user.id.toString(),
          email: response.user.email,
          lastActive: new Date().toISOString()
      };
      return { token: response.token, user };
    },
    logout: async () => {
      pb.auth.logout();
      localStorage.removeItem('tinybase_token');
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
    create: async(data: Partial<AdminUser>): Promise<AdminUser> => {
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
    update: async(id: string, data: Partial<AdminUser>): Promise<AdminUser> => {
        // Backend doesn't have user update yet (e.g. changing email/password as admin)
        // We return the data to optimistic update the UI so it doesn't crash
        return { 
            id, 
            email: data.email || '', 
            lastActive: new Date().toISOString(),
            ...data 
        } as AdminUser;
    },
    delete: async(id: string): Promise<void> => {
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
      if(data.rules) backendSchema.policies = data.rules;
      
      const res = await pb.admins.createCollection(data.name, backendSchema);
      return transformCollection(res);
    },
    update: async (id: string, data: Partial<Collection>): Promise<Collection> => {
      const payload: any = { name: data.name };
      if (data.schema || data.rules) {
          payload.schema = transformToBackendSchema(data);
      }
      
      const res = await pb.admins.updateCollection(id, payload);
      return transformCollection(res);
    },
    delete: async (id: string): Promise<void> => {
      return pb.admins.deleteCollection(id);
    }
  },

  records: {
    list: async (collectionId: string, page = 1, perPage = 20, expand = ''): Promise<{ items: AppRecord[], totalItems: number }> => {
      
      // Pass expand to the SDK list call
      const items = await pb.collection(collectionId).list({ 
          page, 
          per_page: perPage,
          expand: expand 
      });
      
      const formattedItems = items.map((item: any) => ({
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
        totalItems: items.length // Note: Backend needs to update to return count in meta
      };
    },
    instantSearch: async (collectionId: string|number, query: string): Promise<InstantResult[]> => {
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

  logs: {
    list: async (): Promise<SystemLog[]> => {
        try {
            // Fetch from real endpoint
            const res = await pb.admins.listLogs(); // You need to add listLogs to SDK admin object
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