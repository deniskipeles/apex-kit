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
             * Logout (clears local state only).
             */
            logout: () => {
                this.token = null;
                this.currentUser = null;
            }
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
            updateCollection: (id, payload) => this._request(`/collections/${id}`, { method: 'PATCH', body: payload }),

            /**
             * Delete a collection.
             * @param {number|string} id 
             * @returns {Promise<void>}
             */
            deleteCollection: (id) => this._request(`/collections/${id}`, { method: 'DELETE' }),

            // --- Users ---
            /**
             * List all registered users (Admin only).
             * @returns {Promise<Array<object>>}
             */
            listUsers: () => this._request('/admin/users'),

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
            updateSettings: (settings) => this._request('/admin/settings', { method: 'PATCH', body: settings }),

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
            revectorizeCollection: (collectionId) => this._request(`/admin/collections/${collectionId}/revectorize`, { method: 'POST', body: JSON.stringify({}) }),

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
             * @returns {Promise<object>} New session object.
             */
            createSession: (name, initialPrompt, model) => this._request('/admin/ai/sessions', { 
                method: 'POST', 
                body: { name, initial_prompt: initialPrompt, model } 
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
            publishSession: (sessionId) => this._request(`/admin/ai/sessions/${sessionId}/publish`, { method: 'POST' })
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
            update: (id, data) => this._request(`/admin/templates/${id}`, { method: 'PATCH', body: data }),

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
             * @param {string} query 
             * @returns {Promise<Array<object>>}
             */
            search: (query) => this._request(`/collections/${collectionId}/search`, { method: 'GET', params: { q: query } }),

            /**
             * Perform an ultra-fast Instant Search via Tantivy Index (No SQL).
             * @param {string} query 
             * @returns {Promise<Array<{id: number, score: number, snippet: object}>>}
             */
            instantSearch: (query) => this._request(`/collections/${collectionId}/instant-search`, { method: 'GET', params: { q: query } }),

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
            update: (recordId, data) => this._request(`/collections/${collectionId}/records/${recordId}`, { method: 'PATCH', body: { data } }),

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
 * This example demonstrates how to:
 *  1. Establish a realtime connection
 *  2. Subscribe to filtered events
 *  3. Listen and react to updates
 *
 * --------------------------------------------
 * 1. Start the realtime connection
 * --------------------------------------------
 *
 * @example
 * const realtime = new ApexKitRealtime(pb.baseUrl, pb.getToken());
 * realtime.connect();
 *
 * --------------------------------------------
 * 2. Subscribe to specific data changes
 * --------------------------------------------
 *
 * Subscribe to updates on the `tickets` collection,
 * but only receive events where the ticket priority
 * is set to `"high"`.
 *
 * @example
 * realtime.subscribe({
 *   collectionId: 5,           // Collection ID for 'tickets'
 *   eventType: "Update",       // Event type to listen for
 *   dataFilter: {              // Mongo-style filter
 *     priority: "high"
 *   }
 * });
 *
 * --------------------------------------------
 * 3. Listen for realtime events
 * --------------------------------------------
 *
 * Handle incoming events and update the UI
 * or trigger notifications when changes occur.
 *
 * @example
 * realtime.onEvent((event) => {
 *   if (event.event === "Update") {
 *     console.log("Ticket Updated:", event.payload.data);
 *     // Refresh UI or show a toast notification
 *   }
 * });
 *
 * ============================================
 */

export class ApexKitRealtime {
    constructor(url, token) {
        this.url = url.replace("http", "ws") + "/ws"; // Auto-switch protocol
        this.token = token;
        this.socket = null;
        this.reconnectInterval = 3000;
        this.listeners = [];
        this.isConnected = false;
        
        // Default filter (Listen to nothing until subscribed)
        this.currentFilter = {}; 
    }

    connect() {
        this.socket = new WebSocket(this.url);

        this.socket.onopen = () => {
            console.log("[ApexKit] Realtime Connected");
            this.isConnected = true;
            // 1. Authenticate (If you implement Auth Handshake later)
            // 2. Resend subscription if reconnecting
            if (this.currentFilter) {
                this.subscribe(this.currentFilter);
            }
        };

        this.socket.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                // msg format: { event: "Insert", payload: { ... } }
                this.notify(msg);
            } catch (e) {
                if (event.data === "Pong") return; // Heartbeat
                console.error("WS Parse Error", e);
            }
        };

        this.socket.onclose = () => {
            this.isConnected = false;
            console.log("[ApexKit] Disconnected. Retrying...");
            setTimeout(() => this.connect(), this.reconnectInterval);
        };
    }

    /**
     * Send a filter to the server to narrow down events.
     * Matches the Rust `ClientMessage::Subscribe` struct.
     */
    subscribe(filter) {
        this.currentFilter = filter;
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({
                type: "Subscribe",
                payload: {
                    collection_id: filter.collectionId, // Optional
                    record_id: filter.recordId,         // Optional
                    event_type: filter.eventType,       // "Insert", "Update", "Delete"
                    filter: filter.dataFilter           // The Mongo-style JSON filter
                }
            }));
        }
    }

    unsubscribe() {
        this.currentFilter = {};
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({ type: "Unsubscribe" }));
        }
    }

    // Internal observer pattern
    onEvent(callback) {
        this.listeners.push(callback);
        return () => this.listeners = this.listeners.filter(l => l !== callback);
    }

    notify(msg) {
        this.listeners.forEach(cb => cb(msg));
    }
}