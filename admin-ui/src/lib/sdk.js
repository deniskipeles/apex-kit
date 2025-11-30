/**
 * PowerBase Client SDK v1.4.0
 * A vanilla JavaScript client for the Tinybase API.
 * Compatible with modern Browsers and Node.js (v18+).
 */
export class PowerBase {
    /**
     * Initialize the PowerBase client.
     * @param {string} baseUrl - The URL of your Tinybase API (e.g., 'http://127.0.0.1:5000').
     */
    constructor(baseUrl) {
        // Ensure no trailing slash for consistent path building
        this.baseUrl = baseUrl.replace(/\/$/, "");
        this.token = null;
        this.currentUser = null;
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
        };
    }

    // ==========================================
    // 3. AI Actions (LLM Integration)
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
             * @param {object} variables - Variables to replace in the template (e.g., { text: "..." }).
             * @returns {Promise<{result: string}>} The AI response.
             */
            run: (slug, variables) => this._request(`/ai/run/${slug}`, { method: 'POST', body: { variables } })
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
             * @returns {Promise<Array<object>>}
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
             * @returns {Promise<object>}
             */
            get: (recordId) => this._request(`/collections/${collectionId}/records/${recordId}`),

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
            }
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
             * @param {string} filename 
             */
            getFileUrl: (filename) => `${this.baseUrl}/api/v1/storage/file/${filename}`
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
}