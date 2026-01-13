/**
 * ApexKit Client SDK v1.4.0
 * A vanilla JavaScript client for the ApexKit API.
 * Compatible with modern Browsers and Node.js (v18+).
 */
export class ApexKit {
    /**
     * Initialize the ApexKit client.
     * @param {string} baseUrl - The URL of your ApexKit API (e.g., 'http://127.0.0.1:5000').
     */
    constructor(baseUrl) {
        // Ensure no trailing slash for consistent path building
        this.baseUrl = baseUrl.replace(/\/$/, "");
        this.token = null;
        this.currentUser = null;
    }

    /**
     * Creates a new client instance pointed at a specific Sandbox session.
     * All subsequent API calls on the returned instance will be routed to that sandbox.
     * @param {string} uuid - The Sandbox Session ID.
     * @returns {ApexKit} A new SDK instance.
     */
    sandbox(uuid) {
        // Construct the sandbox URL: http://host/sandbox/{uuid}
        const sandboxUrl = `${this.baseUrl}/sandbox/${uuid}`;
        const instance = new ApexKit(sandboxUrl);
        
        // Copy auth state to the sandbox instance
        instance.token = this.token;
        instance.currentUser = this.currentUser;
        instance.sandboxId = uuid;
        
        return instance;
    }

    /**
     * Creates a new client instance pointed at a specific Tenant.
     * All subsequent API calls on the returned instance will be routed to that tenant.
     * @param {string} tenantId - The Tenant ID.
     * @returns {ApexKit} A new SDK instance.
     */
    tenant(tenantId) {
        // Construct the tenant URL: http://host/tenant/{tenantId}
        const tenantUrl = `${this.baseUrl}/tenant/${tenantId}`;
        const instance = new ApexKit(tenantUrl);
        
        // Copy auth state to the tenant instance
        instance.token = this.token;
        instance.currentUser = this.currentUser;
        
        return instance;
    }

    /**
     * Manually set the JWT token (e.g., after loading from localStorage).
     * @param {string} token - The JWT string.
     */
    setToken(token) {
        this.token = token;
    }

    /**
     * Get the current auth token.
     * @returns {string|null}
     */
    getToken() {
        return this.token;
    }

    /**
     * Internal request handler using the Fetch API.
     * @private
     * @param {string} endpoint - The API path.
     * @param {object} [options={}] - Fetch options.
     * @param {string} [options.method] - HTTP Method.
     * @param {object} [options.headers] - HTTP Headers.
     * @param {object|FormData} [options.body] - Request body.
     * @param {object} [options.params] - Query parameters.
     * @param {boolean} [options.isRoot] - If true, does not prepend '/api/v1'.
     * @returns {Promise<any>} The JSON response data.
     * @throws {Error} If the API returns a non-2xx status code.
     */
    async _request(endpoint, options = {}) {
        let path = endpoint;

        // Prefix with /api/v1 unless 'isRoot' is true (e.g. for /graphql or /render)
        if (!options.isRoot && !endpoint.startsWith('/api/v1')) {
            path = endpoint.startsWith('/') ? `/api/v1${endpoint}` : `/api/v1/${endpoint}`;
        }

        const url = new URL(`${this.baseUrl}${path}`);

        // Handle Query Parameters
        if (options.params) {
            Object.keys(options.params).forEach(key => {
                let value = options.params[key];
                if (value !== undefined && value !== null) {
                    if (typeof value === 'object' && key === 'filter') {
                        value = JSON.stringify(value);
                    }
                    url.searchParams.append(key, value);
                }
            });
        }

        const headers = {
            ...options.headers,
        };

        // Attach Auth Token if available
        if (this.token) {
            headers['Authorization'] = `Bearer ${this.token}`;
        }

        const config = {
            method: options.method || 'GET',
            headers,
        };

        // Handle Body
        if (options.body) {
            // If FormData (File Upload), let browser set Content-Type boundary
            if (typeof FormData !== 'undefined' && options.body instanceof FormData) {
                config.body = options.body;
            } else {
                // JSON Body
                headers['Content-Type'] = 'application/json';
                config.body = JSON.stringify(options.body);
            }
        }

        try {
            const response = await fetch(url.toString(), config);

            // Handle 204 No Content
            if (response.status === 204) {
                return null;
            }

            const contentType = response.headers.get("content-type");

            // Handle non-JSON responses
            if (contentType && (contentType.includes("text/plain") || contentType.includes("text/html"))) {
                const text = await response.text();
                if (!response.ok) throw new Error(text || 'API Error');
                return text;
            }

            const data = await response.json();

            // Handle GraphQL Errors
            if (options.isRoot && data.errors) {
                 const error = new Error(data.errors[0].message || 'GraphQL Error');
                 error.details = data.errors;
                 throw error;
            }

            if (!response.ok) {
                const error = new Error(data.message || 'API Error');
                error.status = response.status;
                error.code = data.error; 
                error.details = data.details;
                throw error;
            }

            return data;
        } catch (err) {
            throw err;
        }
    }

    // ==========================================
    // 1. Authentication
    // ==========================================

    get auth() {
        return {
            /**
             * Log in an existing user.
             * @param {string} email 
             * @param {string} password 
             * @returns {Promise<{token: string, user: object}>}
             */
            login: async (email, password) => {
                const res = await this._request('/auth/login', {
                    method: 'POST',
                    body: { email, password }
                });
                this.token = res.token;
                this.currentUser = res.user;
                return res;
            },

            /**
             * Register a new user account.
             * @param {string} email 
             * @param {string} password 
             * @returns {Promise<{token: string, user: object}>}
             */
            register: async (email, password) => {
                const res = await this._request('/auth/register', {
                    method: 'POST',
                    body: { email, password }
                });
                this.token = res.token;
                this.currentUser = res.user;
                return res;
            },

            /**
             * Retrieve the currently authenticated user's profile.
             * Requires a valid JWT token to be set.
             * 
             * @returns {Promise<{id: number, email: string, role: string, metadata: object}>} User object
             * @throws {Error} 401 Unauthorized if not logged in.
             */
            getMe: () => this._request('/auth/me'),

            /**
             * Logout (clears local state only).
             */
            logout: () => {
                this.token = null;
                this.currentUser = null;
            },

            /**
             * Initiate GitHub OAuth Login.
             * 
             * @param {string} [redirectTo] - The URL to redirect back to after GitHub auth.
             * @returns {Promise<void>} - Redirects the browser window immediately.
             */
            loginWithGithub: async (redirectTo) => {
                // We use _request to respect the current instance's baseUrl (Tenant/Sandbox)
                // Note: The backend endpoint returns a 302 Redirect.
                // Fetch cannot handle 302 redirects opaquely to get the target URL string easily.
                // However, since we know the structure, we can construct the URL client-side
                // using the instance's baseUrl property.
                
                let path = '/auth/github';
                
                // Ensure proper path construction if using Tenant/Sandbox prefix
                // The _request logic does this internally, but we need the raw URL string for window.location.
                // We replicate the path logic from _request here:
                if (!path.startsWith('/api/v1')) {
                     // Check if baseUrl already includes /api/v1 or if we need to append it?
                     // Based on your SDK constructor, baseUrl is just the host.
                     // Based on _request logic: path = `/api/v1${path}`
                     path = `/api/v1${path}`;
                }

                const url = new URL(`${this.baseUrl}${path}`);
                
                if (redirectTo) {
                    url.searchParams.append('redirect_to', redirectTo);
                }

                // Redirect the user
                window.location.href = url.toString();
            },
        };
    }

    // ==========================================
    // 2. Admin System Management
    // ==========================================

    get admins() {
        return {
            // --- Collections ---
            /**
             * List all database collections.
             * @returns {Promise<Array<object>>}
             */
            listCollections: () => this._request('/collections'),

            /**
             * Create a new collection.
             * @param {string} name - The name of the collection (table).
             * @param {object} schema - The schema definition object.
             * @returns {Promise<object>} The created collection.
             */
            createCollection: (name, schema) => this._request('/collections', { method: 'POST', body: { name, schema } }),

            /**
             * Get a single collection by ID.
             * @param {number|string} id 
             * @returns {Promise<object>}
             */
            getCollection: (id) => this._request(`/collections/${id}`),

            /**
             * Update a collection's name or schema.
             * @param {number|string} id 
             * @param {object} payload 
             * @returns {Promise<object>}
             */
            updateCollection: (id, payload) => this._request(`/collections/${id}`, { method: 'PUT', body: payload }),
            
            /**
             * Patch a collection's name or schema.
             * @param {number|string} id 
             * @param {object} payload 
             * @returns {Promise<object>}
             */
            patchCollection: (id, payload) => this._request(`/collections/${id}`, { method: 'PATCH', body: payload }),

            /**
             * Delete a collection.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            deleteCollection: (id) => this._request(`/collections/${id}`, { method: 'DELETE' }),

            /**
             * List system configurations.
             * @returns {Promise<Array<object>>}
             */
            listConfigs: () => this._request('/admin/config'),

            /**
             * Set or update a system configuration.
             * @param {string} key 
             * @param {string} value 
             * @param {boolean} encrypt 
             * @returns {Promise<void>}
             */
            setConfig: (key, value, encrypt) => this._request('/admin/config', { 
                method: 'POST', 
                body: { key, value, encrypt } 
            }),

            /**
             * Delete a system configuration.
             * @param {string} key 
             * @returns {Promise<void>}
             */
            deleteConfig: (key) => this._request(`/admin/config/${encodeURIComponent(key)}`, { method: 'DELETE' }),

            // --- Users ---
            /**
             * List all registered users (Admin only). with pagination, sorting, and filtering.
             * @param {object} [options={}] 
             * @param {number} [options.page]
             * @param {number} [options.per_page]
             * @param {string} [options.sort] - e.g. "-created"
             * @param {object|string} [options.filter] - e.g. { "status": "active" }
             * @param {string} [options.expand] - e.g. "author,comments"
             * @returns {Promise<{items: Array<object>, total: number}>} Object containing items array and total count.
             */
            listUsers: (options = {}) => this._request(`/admin/users`, { method: 'GET', params: options }),

            /**
             * Delete a user by ID (Admin only).
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            deleteUser: (id) => this._request(`/admin/users/${id}`, { method: 'DELETE' }),

            // --- Configuration ---
            /**
             * Get current system settings (SMTP, Storage, AI, etc.).
             * Secrets are masked.
             * @returns {Promise<object>}
             */
            getSettings: () => this._request('/admin/settings'),

            /**
             * Update system settings.
             * @param {object} settings - The settings object to merge.
             * @returns {Promise<object>}
             */
            updateSettings: (settings) => this._request('/admin/settings', { method: 'PUT', body: settings }),
            
            /**
             * Patch system settings.
             * @param {object} settings - The settings object to merge.
             * @returns {Promise<object>}
             */
            patchSettings: (settings) => this._request('/admin/settings', { method: 'PATCH', body: settings }),

            /**
             * Test S3 Storage Configuration.
             * @param {object} config - { bucket, region, endpoint, access_key, secret_key }
             * @returns {Promise<object>} Success message or throws error.
             */
            testS3StorageConnection: (config) => this._request('/admin/storage/test', { 
                method: 'POST', 
                body: config 
            }),

            /**
             * Migrate files between storage backends.
             * @param {string} source - "local" | "s3"
             * @param {string} destination - "local" | "s3"
             */
            migrateStorage: (source, destination) => this._request('/admin/storage/migrate', {
                method: 'POST',
                body: { source, destination }
            }),

            /**
             * Force a system reload.
             * Syncs GraphQL schema, Cron jobs, and caches.
             * @returns {Promise<object>} Status message.
             */
            reloadSystem: () => this._request('/admin/system/reload', { method: 'POST', body: JSON.stringify({}) }),

            /**
             * Re-build the Tantivy search index for a specific collection.
             * Useful if the index becomes out of sync with the database.
             * @param {number|string} collectionId - The ID of the collection.
             * @returns {Promise<object>} Success message.
             */
            reIndex: (collectionId) => this._request(`/admin/collections/${collectionId}/reindex`, { method: 'POST', body: JSON.stringify({}) }),

            /**
             * Trigger a background job to re-generate AI embeddings (vectors) for a collection.
             * Scans all records and queues embedding generation for fields marked as 'vectorize'.
             * @param {number|string} collectionId - The ID of the collection.
             * @returns {Promise<object>} Status message and number of jobs queued.
             */
            revectorizeCollection: (collectionId) => this._request(`/admin/collections/${collectionId}/revectorize`, { method: 'POST', body: {force: false} }),

            /**
             * Import data from a File (CSV or JSON).
             * Automatically infers schema if the collection does not exist.
             * @param {string} collectionName - The name of the target collection.
             * @param {File} file - The file object to upload.
             * @returns {Promise<object>} Import statistics (records imported, collection created status).
             */
            
            importData: (collectionName, file) => {
                const formData = new FormData();
                formData.append('collection_name', collectionName);
                formData.append('file', file);
                // _request automatically detects FormData and sets headers appropriately
                return this._request('/admin/import-data', { method: 'POST', body: formData });
            },

            /**
             * Export collection data to a downloadable Blob.
             * @param {number|string} collectionId - The ID of the collection.
             * @param {'json'|'csv'} [format='json'] - The desired export format.
             * @returns {Promise<Blob>} The binary blob of the file.
             */
            exportData: async (collectionId, format = 'json') => {
                // Direct fetch is used here to handle Blob response type specifically
                const url = `${this.baseUrl}/api/v1/admin/export-data/${collectionId}?format=${format}`;
                const headers = {};
                if (this.token) headers['Authorization'] = `Bearer ${this.token}`;
                
                const response = await fetch(url, { method: 'GET', headers });
                
                if (!response.ok) {
                    throw new Error(`Export failed: ${response.statusText}`);
                }
                return response.blob();
            },

            /**
             * Get dashboard data (stats, charts, logs).
             * @returns {Promise<object>} Dashboard analytics data.
             */
            getDashboardStats: async () => {
                return this._request('/admin/dashboard'); 
            },

            /**
             * Create a new Tenant (Database instance).
             * @param {string} tenantId - Unique alphanumeric ID (e.g. "client-a").
             */
            createTenant: (tenantId) => this._request('/admin/tenants', { 
                method: 'POST', 
                body: { tenant_id: tenantId } 
            }),
            
            /**
             * List all Tenants.
             * @returns {Promise<string[]>} List of tenant IDs.
             */
            listTenants: () => this._request('/admin/tenants', { method: 'GET' }),
        };
    }

    // ==========================================
    // 3. AI Actions & Architect
    // ==========================================

    get ai() {
        return {
            /**
             * List configured AI actions/prompts.
             * @returns {Promise<Array<object>>}
             */
            getActions: () => this._request('/admin/ai/actions'),

            /**
             * Create a new AI action template.
             * @param {object} data - { name, slug, model, template, system_prompt }
             * @returns {Promise<object>}
             */
            createAction: (data) => this._request('/admin/ai/actions', { method: 'POST', body: data }),

            /**
             * Delete an AI action.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            deleteAction: (id) => this._request(`/admin/ai/actions/${id}`, { method: 'DELETE' }),
            
            /**
             * Execute a defined AI action.
             * @param {string} slug - The slug of the action (e.g., 'summarize').
             * @param {object} variables - Variables to replace in the template.
             * @returns {Promise<object>} The AI response.
             */
            run: (slug, variables) => this._request(`/ai/run/${slug}`, { method: 'POST', body: { variables } }),

            // --- AI ARCHITECT SESSIONS ---

            /**
             * List active AI Architect sessions.
             * @returns {Promise<Array<object>>}
             */
            listSessions: () => this._request('/admin/ai/sessions'),

            /**
             * Start a new AI Architect session.
             * @param {string} name - Project name.
             * @param {string} [initialPrompt] - First instruction.
             * @param {string} [model] - LLM Model ID.
             * @param {string} [cloneStrategy] - clone strategy from request
             * @param {number} [cloneRecordLimit] - Number of records add from root app
             * @returns {Promise<object>} New session object.
             */
            createSession: (name, initialPrompt, model, cloneStrategy, cloneRecordLimit) => this._request('/admin/ai/sessions', { 
                method: 'POST', 
                body: { name, initial_prompt: initialPrompt, model, clone_strategy: cloneStrategy, clone_record_limit: cloneRecordLimit } 
            }),

            /**
             * Send a message to the Architect in a specific session.
             * Generates a pending manifest but does not apply it.
             * @param {string} sessionId
             * @param {string} prompt
             * @param {string} [model]
             * @returns {Promise<object>} Updated session with diff_summary.
             */
            chat: (sessionId, prompt, model) => this._request(`/admin/ai/sessions/${sessionId}/chat`, { 
                method: 'POST', 
                body: { prompt, model } 
            }),

            /**
             * Apply pending changes from an AI Session to the Sandbox DB.
             * @param {string} sessionId
             * @returns {Promise<object>} Updated session.
             */
            applySessionChanges: (sessionId) => this._request(`/admin/ai/sessions/${sessionId}/apply`, { method: 'POST' }),

            /**
             * Publish a session as a Plugin (Commit to Production).
             * @param {string} sessionId
             * @returns {Promise<object>} Plugin definition.
             */
            publishSession: (sessionId) => this._request(`/admin/ai/sessions/${sessionId}/publish`, { method: 'POST' }),

            /**
             * 
             */
            listPlugins: () => this._request('/ai/plugins'), // Check router path, usually /api/v1/ai/plugins

            /**
             * 
             */
            editCode: (prompt, currentCode, contextType, model) => this._request('/admin/ai/edit-code', {
                method: 'POST',
                body: { 
                    prompt, 
                    current_code: currentCode, 
                    context_type: contextType, 
                    model 
                }
            })
        };
    }

    // ==========================================
    // 4. Server-Side Scripting (JS)
    // ==========================================

    get scripts() {
        return {
            /**
             * List all server-side scripts.
             * @returns {Promise<Array<object>>}
             */
            list: () => this._request('/admin/scripts'),

            /**
             * Create a new script.
             * @param {object} data - { name, trigger_type, code, active }
             * @returns {Promise<object>}
             */
            create: (data) => this._request('/admin/scripts', { method: 'POST', body: data }),

            /**
             * Delete a script.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            delete: (id) => this._request(`/admin/scripts/${id}`, { method: 'DELETE' }),

            /**
             * Manually execute a script by name.
             * @param {string} name - The script name (slug).
             * @param {object} variables - Input data accessible as `$input` in the script.
             * @returns {Promise<any>} The result returned by the script.
             */
            run: (name, variables) => this._request(`/run/${name}`, { method: 'POST', body: variables })
        };
    }

    // ==========================================
    // 5. Templates (HTML/HTMX Rendering)
    // ==========================================

    get templates() {
        return {
            /**
             * List all HTML templates.
             * @returns {Promise<Array<object>>}
             */
            list: () => this._request('/admin/templates'),

            /**
             * Create a new template.
             * @param {object} data - { slug, content, script_id }
             * @returns {Promise<object>}
             */
            create: (data) => this._request('/admin/templates', { method: 'POST', body: data }),

            /**
             * Update a template.
             * @param {number|string} id 
             * @param {object} data 
             * @returns {Promise<void>}
             */
            update: (id, data) => this._request(`/admin/templates/${id}`, { method: 'PUT', body: data }),
            
            /**
             * Patch a template.
             * @param {number|string} id 
             * @param {object} data 
             * @returns {Promise<void>}
             */
            patch: (id, data) => this._request(`/admin/templates/${id}`, { method: 'PATCH', body: data }),

            /**
             * Delete a template.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            delete: (id) => this._request(`/admin/templates/${id}`, { method: 'DELETE' })
        };
    }

    // ==========================================
    // 6. Data Collection Operations
    // ==========================================

    /**
     * Access operations for a specific data collection.
     * @param {number|string} collectionId - ID or Name of the collection.
     */
    collection(collectionId) {
        return {
            /**
             * List records with pagination, sorting, and filtering.
             * @param {object} [options={}] 
             * @param {number} [options.page]
             * @param {number} [options.per_page]
             * @param {string} [options.sort] - e.g. "-created"
             * @param {object|string} [options.filter] - e.g. { "status": "active" }
             * @param {string} [options.expand] - e.g. "author,comments"
             * @returns {Promise<{items: Array<object>, total: number}>} Object containing items array and total count.
             */
            list: (options = {}) => this._request(`/collections/${collectionId}/records`, { method: 'GET', params: options }),

            /**
             * Perform a full-text search (SQL-based).
             * @param {object} query 
             * @returns {Promise<Array<object>>}
             */
            searchRecordsWithSQL: (query) => this._request(`/collections/${collectionId}/query`, { method: 'POST', body: { query } }),

            /**
             * Perform an ultra-fast Optimized Search Engine via Tantivy Index (No SQL).
             * @param {string} query 
             * @returns {Promise<Array<object>>}
             */
            searchRecordsWithOSE: (query) => this._request(`/collections/${collectionId}/search`, { method: 'GET', params: { q: query } }),
            
            /**
             * Perform an ultra-fast Instant Search via Tantivy Index (No SQL).
             * @param {string} query 
             * @returns {Promise<Array<{id: number, score: number, snippet: object}>>}
             */
            searchRecordsInstantlyWithOSE: (query) => this._request(`/collections/${collectionId}/instant-search`, { method: 'GET', params: { q: query } }),

            /**
             * Create a new record.
             * @param {object} data 
             * @returns {Promise<object>}
             */
            create: (data) => this._request(`/collections/${collectionId}/records`, { method: 'POST', body: { data } }),

            /**
             * Get a single record by ID.
             * @param {number|string} recordId 
             * @param {string} [options.expand] - e.g. "author,comments"
             * @returns {Promise<object>}
             */
            get: (recordId, options = {}) => this._request(`/collections/${collectionId}/records/${recordId}`, { method: 'GET', params: options }),

            /**
             * Update a record.
             * @param {number|string} recordId 
             * @param {object} data 
             * @returns {Promise<object>}
             */
            update: (recordId, data) => this._request(`/collections/${collectionId}/records/${recordId}`, { method: 'PUT', body: { data } }),
            
            /**
             * Patch a record.
             * @param {number|string} recordId 
             * @param {object} data 
             * @returns {Promise<object>}
             */
            patch: (recordId, data) => this._request(`/collections/${collectionId}/records/${recordId}`, { method: 'PATCH', body: { data } }),

            /**
             * Delete a record.
             * @param {number|string} recordId 
             * @returns {Promise<void>}
             */
            delete: (recordId) => this._request(`/collections/${collectionId}/records/${recordId}`, { method: 'DELETE' }),

            // --- Relations ---

            /**
             * Add a relationship edge between records.
             * @param {number|string} originRecordId 
             * @param {number|string} targetCollectionId 
             * @param {number|string} targetRecordId 
             * @param {string} relationName 
             */
            addRelation: (originRecordId, targetCollectionId, targetRecordId, relationName) => {
                return this._request(`/collections/${collectionId}/records/${originRecordId}/relations`, {
                    method: 'POST',
                    body: {
                        target_collection_id: parseInt(targetCollectionId),
                        target_record_id: parseInt(targetRecordId),
                        relation_name: relationName
                    }
                });
            },

            /**
             * Remove a relationship edge.
             */
            removeRelation: (originRecordId, targetCollectionId, targetRecordId, relationName) => {
                return this._request(`/collections/${collectionId}/records/${originRecordId}/relations`, {
                    method: 'DELETE',
                    body: {
                        target_collection_id: parseInt(targetCollectionId),
                        target_record_id: parseInt(targetRecordId),
                        relation_name: relationName
                    }
                });
            },
            /**
             * Perform a semantic vector search using a raw float array.
             * @param {string} field - The field name storing vectors (e.g. "description_vec").
             * @param {Array<number>} vector - The embedding vector.
             * @param {number} [limit=10] - Max results.
             * @returns {Promise<Array<object>>} List of matching records.
             */
            searchVector: (field, vector, limit = 10) => this._request(`/collections/${collectionId}/search-vector`, {
                method: 'POST',
                body: { field, vector, limit }
            }),

            /**
             * Perform a semantic search by converting text to vector on the server.
             * Automatically aggregates scores if multiple vector fields exist.
             * @param {string} queryText - The natural language query.
             * @param {number} [limit=10] - Max results.
             * @returns {Promise<Array<object>>} List of matching records.
             */
            searchTextVector: (queryText, limit = 10) => this._request(`/collections/${collectionId}/search-text-vector`, {
                method: 'POST',
                body: { query_text: queryText, limit }
            }),
        };
    }

    // ==========================================
    // 7. File Storage
    // ==========================================

    get files() {
        return {
            /**
             * List uploaded files.
             * @param {number} [page=1] 
             * @param {number} [perPage=20] 
             * @returns {Promise<{items: Array<object>, total: number}>}
             */
            list: (page = 1, perPage = 20) => this._request('/storage/files', { method: 'GET', params: { page, per_page: perPage } }),

            /**
             * Upload a file.
             * @param {File} file - The file object from input.
             * @returns {Promise<object>} Metadata of uploaded file.
             */
            upload: (file) => {
                const formData = new FormData();
                formData.append('file', file);
                return this._request('/storage/upload', { method: 'POST', body: formData });
            },

            /**
             * Delete a file by ID.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            delete: (id) => this._request(`/storage/files/${id}`, { method: 'DELETE' }),

            /**
             * Helper to get public URL.
             * Smartly detects if the context is Root, Tenant, or Sandbox based on the current SDK instance.
             * Also handles S3/External URLs gracefully.
             * @param {string} filename 
             */
            getFileUrl: (filename) => {
                // 1. If it's already a full URL (e.g. S3), return as is
                if (filename.startsWith('http://') || filename.startsWith('https://')) {
                    return filename;
                }

                // 2. Clean inputs
                const base = this.baseUrl.replace(/\/$/, "");
                const name = filename.replace(/^\//, "");

                // 3. Construct URL relative to current context (Root/Tenant/Sandbox)
                // The `baseUrl` is automatically adjusted when you use pb.tenant('id') or pb.sandbox('id')
                return `${base}/api/v1/storage/file/${name}`;
            }
        };
    }

    // ==========================================
    // 8. Logs
    // ==========================================

    get logs() {
        return {
            /**
             * Fetch system audit logs.
             * @returns {Promise<Array<object>>}
             */
            list: () => this._request('/admin/logs')
        };
    }

    // ==========================================
    // 9. GraphQL
    // ==========================================

    /**
     * Execute a GraphQL query.
     * @param {string} query 
     * @param {object} [variables={}] 
     * @returns {Promise<{data: object, errors?: Array}>}
     */
    async graphql(query, variables = {}) {
        return this._request('/graphql', {
            method: 'POST',
            isRoot: true,
            body: { query, variables }
        });
    }
    
    // ==========================================
    // 10. Custom Helpers
    // ==========================================
    get utils() {
        return {
            /**
             * Strips all HTML tags from a string, returning only the text content.
             * Handles malformed HTML and entities properly.
             * @param html - The HTML string to strip
             * @returns Plain text without HTML tags
             */
            stripHtmlTags: function (html) {
                if (!html) return '';
                const doc = new DOMParser().parseFromString(html, 'text/html');
                return doc.body.textContent || '';
            }
        }
    }
}

/**
 * ============================================
 * ApexKit Realtime — Usage Guide
 * ============================================
 *
 * This client supports Database Change Events, Custom Ephemeral Events,
 * and high-performance Instant Search over WebSockets.
 *
 * --------------------------------------------
 * 1. Start the connection
 * --------------------------------------------
 *
 * @example
 * const realtime = new ApexKitRealtimeWSClient(pb.baseUrl, pb.getToken());
 * realtime.connect();
 *
 * --------------------------------------------
 * 2. Subscribe to Data Changes (DB)
 * --------------------------------------------
 *
 * @example
 * realtime.subscribe({
 *   collectionId: 5,
 *   eventType: "Update",
 *   dataFilter: { priority: "high" }
 * });
 *
 * --------------------------------------------
 * 3. Subscribe to Custom Channels (Chat/Signals)
 * --------------------------------------------
 *
 * @example
 * realtime.subscribe({
 *   channel: "room_123",       // Namespace: tenant_id::room_123
 *   customEvent: "NewMessage"  // Optional: Filter specific event name
 * });
 *
 * --------------------------------------------
 * 4. Send a Signal (Client-to-Client Broadcast)
 * --------------------------------------------
 *
 * @example
 * realtime.sendSignal("room_123", "UserTyping", { user: "Alice" });
 *
 * --------------------------------------------
 * 5. Instant Search (Request-Response)
 * --------------------------------------------
 *
 * @example
 * const results = await realtime.search(1, "search query", 5);
 * console.log(results); // [{ id: 1, score: 2.5, snippet: {...} }]
 *
 * --------------------------------------------
 * 6. Handle Events
 * --------------------------------------------
 *
 * @example
 * realtime.onEvent((msg) => {
 *   // Handle DB Event
 *   if (msg.event === "Insert") {
 *      console.log("Record Created:", msg.payload.data);
 *   }
 *   
 *   // Handle Custom Event
 *   if (msg.event === "Custom") {
 *      const { event, data } = msg.payload;
 *      if (event === "UserTyping") console.log(`${data.user} is typing...`);
 *   }
 * });
 * ============================================
 */

export class ApexKitRealtimeWSClient {
    constructor(url, token) {
        this.url = url.replace("http", "ws") + "/ws"; // Auto-switch protocol
        this.token = token;
        this.socket = null;
        this.reconnectInterval = 3000;
        this.listeners = [];
        this.isConnected = false;
        
        // Default filter (Listen to nothing until subscribed)
        this.currentFilter = {}; 
        
        // Store pending search requests (ID -> Promise Resolve/Reject)
        this.pendingRequests = new Map();
    }

    connect() {
        this.socket = new WebSocket(this.url);

        this.socket.onopen = () => {
            console.log("[ApexKit] Realtime Connected");
            this.isConnected = true;
            
            // Resend subscription if reconnecting
            if (this.currentFilter && Object.keys(this.currentFilter).length > 0) {
                this.subscribe(this.currentFilter);
            }
        };

        this.socket.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);

                // 1. Handle Search Responses (Request-Response Pattern)
                // These have a 'request_id' and are not broadcast to general listeners
                if (msg.request_id) {
                    this._handleRequestResponse(msg);
                    return;
                }

                // 2. Handle Standard Broadcasts (DB Events & Signals)
                this.notify(msg);

            } catch (e) {
                if (event.data === "Pong") return; // Heartbeat
                console.error("WS Parse Error", e);
            }
        };

        this.socket.onclose = () => {
            this.isConnected = false;
            
            // Clear pending search requests so they don't hang forever
            this.pendingRequests.forEach((req) => req.reject(new Error("Socket closed")));
            this.pendingRequests.clear();
            
            console.log("[ApexKit] Disconnected. Retrying...");
            setTimeout(() => this.connect(), this.reconnectInterval);
        };
    }

    /**
     * Close the connection.
     */
    disconnect() {
        if (this.socket) {
            this.socket.close();
            this.socket = null;
            this.isConnected = false;
            console.log("[ApexKit] WebSocket Disconnected manually");
        }
    }

    /**
     * Send a filter to the server to narrow down events.
     * Matches the Rust `ClientMessage::Subscribe` struct.
     * 
     * @param {Object} filter
     * @param {number} [filter.collectionId] - Filter by Collection ID (DB Events)
     * @param {number} [filter.recordId] - Filter by specific Record ID (DB Events)
     * @param {string} [filter.eventType] - "Insert", "Update", "Delete" (DB Events)
     * @param {Object} [filter.dataFilter] - MongoDB-style JSON filter (DB & Custom Events)
     * @param {string} [filter.channel] - Channel name for Custom Events (e.g. "chat_room")
     * @param {string} [filter.customEvent] - Specific Custom Event name (e.g. "Typing")
     */
    subscribe(filter) {
        this.currentFilter = filter;
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({
                type: "Subscribe",
                payload: {
                    // Standard DB Filters
                    collection_id: filter.collectionId, 
                    record_id: filter.recordId,         
                    event_type: filter.eventType,       
                    filter: filter.dataFilter,
                    
                    // Custom Event Filters
                    channel: filter.channel,
                    custom_event: filter.customEvent
                }
            }));
        }
    }

    /**
     * Broadcast an ephemeral message to a specific channel.
     * Useful for "typing" indicators, cursors, or notifications.
     * 
     * @param {string} channel - The channel name (e.g. "room_1")
     * @param {string} eventName - The event label (e.g. "UserTyping")
     * @param {Object} data - Arbitrary JSON payload
     */
    sendSignal(channel, eventName, data) {
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({
                type: "Signal",
                payload: {
                    channel: channel,
                    event: eventName,
                    data: data
                }
            }));
        } else {
            console.warn("[ApexKit] Socket not open, cannot send signal.");
        }
    }

    /**
     * Perform an Instant Search over WebSocket.
     * Returns a Promise that resolves with the results.
     * 
     * @param {number|string} collectionId - The collection to search
     * @param {string} query - The search text
     * @param {number} [limit=10] - Max results
     * @returns {Promise<Array>} List of results
     */
    search(collectionId, query, limit = 10) {
        return new Promise((resolve, reject) => {
            if (!this.isConnected || !this.socket) {
                return reject(new Error("Socket not connected"));
            }

            // Generate a unique ID for this request
            const requestId = crypto.randomUUID();

            // Set a timeout to prevent hanging promises (e.g., 5 seconds)
            const timeout = setTimeout(() => {
                if (this.pendingRequests.has(requestId)) {
                    this.pendingRequests.delete(requestId);
                    reject(new Error("Search request timed out"));
                }
            }, 5000);

            // Store the promise handlers
            this.pendingRequests.set(requestId, { resolve, reject, timeout });

            // Send command
            this.socket.send(JSON.stringify({
                type: "Search",
                payload: {
                    collection_id: Number(collectionId),
                    query: query,
                    limit: limit,
                    request_id: requestId
                }
            }));
        });
    }

    unsubscribe() {
        this.currentFilter = {};
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({ type: "Unsubscribe" }));
        }
    }

    /**
     * Register a callback for broadcast events.
     * @param {Function} callback 
     * @returns {Function} Unsubscribe closure
     */
    onEvent(callback) {
        this.listeners.push(callback);
        return () => this.listeners = this.listeners.filter(l => l !== callback);
    }

    /**
     * Internal: Dispatch events to listeners
     */
    notify(msg) {
        this.listeners.forEach(cb => cb(msg));
    }

    /**
     * Internal: Handle Search Request/Response resolution
     */
    _handleRequestResponse(msg) {
        const req = this.pendingRequests.get(msg.request_id);
        if (req) {
            clearTimeout(req.timeout); // Clear the failsafe timeout
            this.pendingRequests.delete(msg.request_id);

            if (msg.type === "Error") {
                req.reject(new Error(msg.message));
            } else {
                req.resolve(msg.results); // Resolve with the data array
            }
        }
    }
}

/**
 * ============================================
 * ApexKit SSE Client — Usage Guide
 * ============================================
 *
 * Server-Sent Events are ideal for read-only streams where you don't
 * need to send data back to the server (e.g., news feeds, logs).
 *
 * --------------------------------------------
 * 1. Initialize & Connect
 * --------------------------------------------
 *
 * @example
 * const sse = new ApexKitRealtimeSSEClient('http://localhost:5000');
 *
 * // Connect to specific channel
 * sse.connect({
 *   channel: "room_123",       // Filters for tenant_id::room_123
 *   eventName: "NewMessage"    // Optional: Filter specific event type
 * });
 *
 * --------------------------------------------
 * 2. Handle Events
 * --------------------------------------------
 *
 * @example
 * sse.onEvent((msg) => {
 *   if (msg.type === "Custom") {
 *      console.log("Custom Event:", msg.payload.data);
 *   }
 *   if (msg.type === "Insert") {
 *      console.log("DB Insert:", msg.payload.data);
 *   }
 * });
 *
 * --------------------------------------------
 * 3. Cleanup
 * --------------------------------------------
 *
 * @example
 * sse.disconnect();
 * ============================================
 */

export class ApexKitRealtimeSSEClient {
    /**
     * @param {string} baseUrl - The API base URL (e.g., "http://localhost:5000/api/v1")
     */
    constructor(baseUrl) {
        // Strip trailing slash and ensure we point to root if /api/v1 is passed
        // The SSE endpoint is usually at /sse or /api/v1/sse depending on router setup.
        // Based on your router, it is mapped at the root router level `/sse` but also nested.
        // Assuming the `baseUrl` passed is the root (e.g. http://localhost:5000)
        this.baseUrl = baseUrl.replace(/\/$/, "");
        this.source = null;
        this.listeners = [];
    }

    /**
     * Start the EventSource connection.
     * @param {Object} options
     * @param {string} [options.channel] - The specific channel to subscribe to.
     * @param {string} [options.eventName] - Filter by event name (e.g. "Typing").
     */
    connect({ channel, eventName } = {}) {
        // Close existing if any
        if (this.source) {
            this.source.close();
        }

        // Build URL with Query Params
        const params = new URLSearchParams();
        if (channel) params.append("channel", channel);
        if (eventName) params.append("event", eventName);

        const url = `${this.baseUrl}/sse?${params.toString()}`;

        console.log(`[ApexKit] SSE Connecting to ${url}...`);
        
        this.source = new EventSource(url, { withCredentials: true });

        this.source.onopen = () => {
            console.log("[ApexKit] SSE Connected");
        };

        this.source.onerror = (err) => {
            // EventSource auto-reconnects, but we log errors
            console.error("[ApexKit] SSE Connection Error/Reconnecting...", err);
        };

        this.source.onmessage = (event) => {
            try {
                // Parse the inner JSON data
                const msg = JSON.parse(event.data);
                this.notify(msg);
            } catch (e) {
                console.error("[ApexKit] SSE Parse Error", e);
            }
        };
    }

    /**
     * Close the connection.
     */
    disconnect() {
        if (this.source) {
            this.source.close();
            this.source = null;
            console.log("[ApexKit] SSE Disconnected");
        }
    }

    /**
     * Register a callback for incoming events.
     * @param {Function} callback - Receives the parsed JSON message
     * @returns {Function} Unsubscribe function
     */
    onEvent(callback) {
        this.listeners.push(callback);
        return () => {
            this.listeners = this.listeners.filter(l => l !== callback);
        };
    }

    /**
     * Internal: Dispatch events to listeners
     */
    notify(msg) {
        this.listeners.forEach(cb => cb(msg));
    }
}