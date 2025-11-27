/**
 * PowerBase Client SDK
 * A vanilla JavaScript client for the Tinybase API.
 * Compatible with modern Browsers and Node.js (v18+).
 *
 * @version 1.3.0
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
     * Get the currently logged-in user details.
     * @returns {object|null}
     */
    getUser() {
        return this.currentUser;
    }

    /**
     * Internal request handler using the Fetch API.
     * @private
     * @param {string} endpoint - The API path.
     * @param {object} [options={}] - Fetch options (method, headers, body, params, isRoot).
     * @returns {Promise<any>} The JSON response data.
     * @throws {Error} If the API returns a non-2xx status code.
     */
    async _request(endpoint, options = {}) {
        let path = endpoint;

        // Prefix with /api/v1 unless 'isRoot' is true (e.g. for /graphql)
        if (!options.isRoot && !endpoint.startsWith('/api/v1')) {
            path = endpoint.startsWith('/') ? `/api/v1${endpoint}` : `/api/v1/${endpoint}`;
        }

        const url = new URL(`${this.baseUrl}${path}`);

        // Handle Query Parameters
        if (options.params) {
            Object.keys(options.params).forEach(key => {
                let value = options.params[key];
                if (value !== undefined && value !== null) {
                    // The API expects 'filter' to be a JSON string
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

            // Handle non-JSON responses (like verifying email returning plain text)
            if (contentType && contentType.includes("text/plain")) {
                const text = await response.text();
                if (!response.ok) throw new Error(text || 'API Error');
                return text;
            }

            const data = await response.json();

            // Handle GraphQL Errors (which return 200 OK but contain an 'errors' array)
            if (options.isRoot && data.errors) {
                 const error = new Error(data.errors[0].message || 'GraphQL Error');
                 error.details = data.errors;
                 throw error;
            }

            if (!response.ok) {
                const error = new Error(data.message || 'API Error');
                error.status = response.status;
                error.code = data.error; // e.g., 'validation_error'
                error.details = data.details; // Validation details
                throw error;
            }

            return data;
        } catch (err) {
            throw err;
        }
    }

    // ==========================================
    // Authentication & Users
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
             * Verify an email address using a token.
             * @param {string} token - Token received in email link.
             * @returns {Promise<string>} Success message.
             */
            verifyEmail: async (token) => {
                return this._request('/auth/verify', {
                    method: 'GET',
                    params: { token }
                });
            },

            /**
             * Resend verification email.
             * @param {string} email 
             * @returns {Promise<void>}
             */
            resendVerification: async (email) => {
                await this._request('/auth/verify/resend', {
                    method: 'POST',
                    body: { email }
                });
            },

            /**
             * Get the GitHub OAuth Login URL.
             * Redirect `window.location.href` to this result to start OAuth flow.
             * @returns {string}
             */
            getGithubLoginUrl: () => {
                return `${this.baseUrl}/api/v1/auth/github`;
            },

            /**
             * Log out (clears local state).
             */
            logout: () => {
                this.token = null;
                this.currentUser = null;
            }
        };
    }

    // ==========================================
    // Admin / System Management
    // ==========================================

    get admins() {
        return {
            /**
             * List all collections.
             * @returns {Promise<Array<{id: number, name: string, schema: object}>>}
             */
            listCollections: async () => {
                return this._request('/collections');
            },

            /**
             * Create a new collection.
             * @param {string} name 
             * @param {object} schema - { fields: { title: { type: "string", required: true } } }
             * @returns {Promise<object>}
             */
            createCollection: async (name, schema) => {
                return this._request('/collections', {
                    method: 'POST',
                    body: { name, schema }
                });
            },

            /**
             * Get details of a specific collection.
             * @param {number} id 
             * @returns {Promise<object>}
             */
            getCollection: async (id) => {
                return this._request(`/collections/${id}`);
            },

            /**
             * Update a collection.
             * @param {number} id 
             * @param {object} payload 
             * @returns {Promise<object>}
             */
            updateCollection: async (id, payload) => {
                return this._request(`/collections/${id}`, {
                    method: 'PATCH',
                    body: payload
                });
            },

            /**
             * Delete a collection.
             * @param {number} id 
             * @returns {Promise<boolean>}
             */
            deleteCollection: async (id) => {
                await this._request(`/collections/${id}`, { method: 'DELETE' });
                return true;
            },

            /**
             * Update encrypted system configuration (Admin Only).
             * @param {string} key - e.g., 'smtp_pass', 'github_client_secret'
             * @param {string} value - The plain text value to encrypt and store.
             * @returns {Promise<void>}
             */
            updateSystemConfig: async (key, value) => {
                await this._request('/admin/config', {
                    method: 'POST',
                    body: { key, value }
                });
            },

            /**
             * 
             */
            // users
            listUsers: () => this._request('/admin/users'),
            deleteUser: (id) => this._request(`/admin/users/${id}`, { method: 'DELETE' }),
            // settings
            getSettings: () => this._request('/admin/settings'),
            updateSettings: (settings) => this._request('/admin/settings', { method: 'PATCH', body: settings }),
            // Reload System
            reloadSystem: () => this._request('/admin/system/reload', { method: 'POST', body:JSON.stringify({}) }),
            listLogs: () => this._request('/admin/logs'),
        };
    }

    // ==========================================
    // Records (Data)
    // ==========================================

    /**
     * Access operations for a specific collection.
     * @param {number} collectionId 
     */
    collection(collectionId) {
        return {
            /**
             * List records.
             * @param {object} [options={}] 
             * @param {number} [options.page=1]
             * @param {number} [options.per_page=30]
             * @param {string} [options.sort] - "-created" (desc) or "created" (asc)
             * @param {object} [options.filter] - { "field": "value" }
             * @param {string} [options.expand] - "author,comments.user" (Recursive relation fetch)
             * @returns {Promise<Array<{id: number, data: object}>>}
             */
            list: async (options = {}) => {
                return this._request(`/collections/${collectionId}/records`, {
                    method: 'GET',
                    params: options
                });
            },

            /**
             * Full-text search records (Tantivy).
             * @param {string} query - The search query text.
             * @returns {Promise<Array<{id: number, data: object}>>}
             */
            search: async (query) => {
                return this._request(`/collections/${collectionId}/search`, {
                    method: 'GET',
                    params: { q: query }
                });
            },

            /**
             * Create a record.
             * @param {object} data 
             * @returns {Promise<{id: number, data: object}>}
             */
            create: async (data) => {
                return this._request(`/collections/${collectionId}/records`, {
                    method: 'POST',
                    body: { data }
                });
            },

            /**
             * Get a record by ID.
             * @param {number} recordId 
             * @returns {Promise<{id: number, data: object}>}
             */
            get: async (recordId) => {
                return this._request(`/collections/${collectionId}/records/${recordId}`);
            },

            /**
             * Update a record.
             * @param {number} recordId 
             * @param {object} data 
             * @returns {Promise<{id: number, data: object}>}
             */
            update: async (recordId, data) => {
                return this._request(`/collections/${collectionId}/records/${recordId}`, {
                    method: 'PATCH',
                    body: { data }
                });
            },

            /**
             * Delete a record.
             * @param {number} recordId 
             * @returns {Promise<boolean>}
             */
            delete: async (recordId) => {
                await this._request(`/collections/${collectionId}/records/${recordId}`, {
                    method: 'DELETE'
                });
                return true;
            },

            // --- Relations ---

            /**
             * Create a relation (edge) between this record and another.
             * @param {number} originRecordId - The ID of the record in the current collection.
             * @param {number} targetCollectionId - The ID of the collection to link to.
             * @param {number} targetRecordId - The ID of the record to link to.
             * @param {string} relationName - The name of the relation (e.g., 'author', 'comments').
             * @returns {Promise<void>}
             */
            addRelation: async (originRecordId, targetCollectionId, targetRecordId, relationName) => {
                await this._request(`/collections/${collectionId}/records/${originRecordId}/relations`, {
                    method: 'POST',
                    body: {
                        target_collection_id: targetCollectionId,
                        target_record_id: targetRecordId,
                        relation_name: relationName
                    }
                });
            },

            /**
             * Delete a relation (edge).
             * @param {number} originRecordId 
             * @param {number} targetCollectionId 
             * @param {number} targetRecordId 
             * @param {string} relationName 
             * @returns {Promise<void>}
             */
            removeRelation: async (originRecordId, targetCollectionId, targetRecordId, relationName) => {
                await this._request(`/collections/${collectionId}/records/${originRecordId}/relations`, {
                    method: 'DELETE',
                    body: {
                        target_collection_id: targetCollectionId,
                        target_record_id: targetRecordId,
                        relation_name: relationName
                    }
                });
            },

            // INSTANT SEARCH
            instantSearch: (query) => this._request(`/collections/${collectionId}/instant-search`, { 
                method: 'GET', 
                params: { q: query } 
            })
        };
    }
    
    get files() {
        return {
            // Update list to pass query params
            list: (page = 1, perPage = 20) => 
                this._request('/storage/files', { method: 'GET', params: { page, per_page: perPage } }),
            
            upload: (file) => {
                const formData = new FormData();
                formData.append('file', file);
                return this._request('/storage/upload', { method: 'POST', body: formData });
            },
            
            // Add delete
            delete: (id) => this._request(`/storage/files/${id}`, { method: 'DELETE' })
        }
    }

    get ai() {
        return {
            getActions: () => this._request('/admin/ai/actions'),
            createAction: (data) => this._request('/admin/ai/actions', { method: 'POST', body: data }),
            deleteAction: (id) => this._request(`/admin/ai/actions/${id}`, { method: 'DELETE' }),
            
            // The method apps will use:
            run: (slug, variables) => this._request(`/ai/run/${slug}`, { method: 'POST', body: { variables } })
        }
    }
    
    get logs() {
        return {
            list: () => this._request('/admin/logs', { method: 'GET' })
        }
    }

    // ==========================================
    // GraphQL
    // ==========================================

    /**
     * Execute a GraphQL query or mutation.
     * @param {string} query - The GraphQL query string.
     * @param {object} [variables={}] - Query variables.
     * @returns {Promise<{data: object, errors?: Array}>}
     */
    async graphql(query, variables = {}) {
        return this._request('/graphql', {
            method: 'POST',
            isRoot: true, // Tells _request NOT to add /api/v1 prefix
            body: { query, variables }
        });
    }

    // ==========================================
    // File Storage
    // ==========================================

    get storage() {
        return {
            /**
             * Upload a file.
             * @param {File | Blob} file - Browser File object or Blob.
             * @returns {Promise<{id: number, url: string, filename: string}>}
             */
            upload: async (file) => {
                const formData = new FormData();
                formData.append('file', file);

                // Content-Type header is automatically set by browser with boundary
                return this._request('/storage/upload', {
                    method: 'POST',
                    body: formData
                });
            },

            /**
             * Get the public URL for a file.
             * @param {string} filename 
             * @returns {string}
             */
            getFileUrl: (filename) => {
                return `${this.baseUrl}/api/v1/storage/file/${filename}`;
            }
        };
    }

    // ==========================================
    // Real-time (WebSockets)
    // ==========================================

    /**
     * Subscribe to real-time database events.
     * @param {function(object): void} onEvent - Callback for events (Insert, Update, Delete).
     * @returns {WebSocket} The socket instance. Call .close() to unsubscribe.
     */
    subscribe(onEvent) {
        const wsProtocol = this.baseUrl.startsWith('https') ? 'wss' : 'ws';
        // Strip protocol from baseUrl to append ws://
        const host = this.baseUrl.replace(/^https?:\/\//, '');
        const wsUrl = `${wsProtocol}://${host}/ws`;

        // Check environment support
        if (typeof WebSocket === 'undefined') {
            console.warn("PowerBase: WebSocket not available in this environment.");
            return null;
        }

        const socket = new WebSocket(wsUrl);

        socket.onopen = () => console.debug("PowerBase: Realtime Connected");

        socket.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                onEvent(data);
            } catch (e) {
                console.error("PowerBase: Realtime parse error", e);
            }
        };

        socket.onerror = (err) => console.error("PowerBase: Realtime Error", err);

        return socket;
    }
}

// Export for Module Systems (Node.js / Bundlers)
if (typeof module !== 'undefined' && typeof module.exports !== 'undefined') {
    module.exports = PowerBase;
} else {
    // Browser Global
    window.PowerBase = PowerBase;
}