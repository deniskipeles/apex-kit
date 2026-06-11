/**
 * ApexKit Universal Frontend Client
 * Automatically handles Scope Routing (Tenant/Sandbox) and JWT Auth Injection.
 */
(function() {
    if (window.$apex && window.$apex.collection) return;

    // 1. DYNAMIC SCOPE DETECTOR
    function getScopePrefix() {
        const match = window.location.pathname.match(/^\/(tenant|sandbox)\/[^\/]+/);
        return match ? match[0] : ''; 
    }
    
    const SCOPE_PREFIX = getScopePrefix();

    // 2. GLOBAL FETCH WRAPPER
    const originalFetch = window.fetch;
    window.fetch = async function() {
        let args = Array.prototype.slice.call(arguments);
        let resource = args[0];
        let config = args[1] || {};

        if (typeof resource === 'string' && resource.startsWith('/') && !resource.startsWith(SCOPE_PREFIX)) {
            args[0] = SCOPE_PREFIX + resource;
        }

        const token = localStorage.getItem('apex_token');
        if (token) {
            config.headers = config.headers || {};
            if (config.headers instanceof Headers) {
                if (!config.headers.has('Authorization')) {
                    config.headers.set('Authorization', 'Bearer ' + token);
                }
            } else {
                if (!config.headers['Authorization'] && !config.headers['authorization']) {
                    config.headers['Authorization'] = 'Bearer ' + token;
                }
            }
            args[1] = config;
        }

        return originalFetch.apply(this, args);
    };

    // 3. HTMX INTERCEPTOR
    document.addEventListener('htmx:configRequest', function(evt) {
        const token = localStorage.getItem('apex_token');
        if (token) {
            evt.detail.headers['Authorization'] = 'Bearer ' + token;
        }

        let path = evt.detail.path;
        if (path.startsWith('/') && !path.startsWith(SCOPE_PREFIX)) {
            evt.detail.path = SCOPE_PREFIX + path;
        }
    });

    // 4. GLOBAL HELPER UTILITY ($apex)
    window.$apex = {
        scope: SCOPE_PREFIX,
        
        login: async function(email, password) {
            const res = await fetch('/api/v1/auth/login', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email, password })
            });
            const data = await res.json();
            if (res.ok) {
                this.setToken(data.token);
            }
            return { ok: res.ok, status: res.status, data };
        },

        logout: function(redirectPath = '/render/login') {
            localStorage.removeItem('apex_token');
            if (redirectPath) {
                this.redirect(redirectPath);
            }
        },

        getToken: () => localStorage.getItem('apex_token'),
        setToken: (t) => localStorage.setItem('apex_token', t),

        // --- SMART REDIRECT ---
        redirect: function(path) {
            if (typeof path !== 'string') return;
            if (path.startsWith('/') && !path.startsWith(this.scope)) {
                window.location.href = this.scope + path;
            } else {
                window.location.href = path;
            }
        },

        // --- DATA COLLECTIONS SDK ---
        collection: function(collectionId) {
            return {
                list: async (options = {}) => {
                    const params = new URLSearchParams(options);
                    const res = await fetch(`/api/v1/collections/${collectionId}/records?${params.toString()}`);
                    return res.json();
                },
                get: async (recordId, options = {}) => {
                    const params = new URLSearchParams(options);
                    const res = await fetch(`/api/v1/collections/${collectionId}/records/${recordId}?${params.toString()}`);
                    return res.json();
                },
                create: async (data) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/records`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ data })
                    });
                    return res.json();
                },
                update: async (recordId, data) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/records/${recordId}`, {
                        method: 'PUT',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ data })
                    });
                    return res.json();
                },
                delete: async (recordId) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/records/${recordId}`, {
                        method: 'DELETE'
                    });
                    return res.status === 204;
                },
                searchVector: async (field, vector, limit = 10) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/search-vector`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ field, vector, limit })
                    });
                    return res.json();
                },
                getVector: async (recordId) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/get-vector/${recordId}`);
                    return res.json();
                }
            };
        },

        // --- FILES / STORAGE SDK ---
        get files() {
            const self = this;
            return {
                list: async (page = 1, perPage = 20) => {
                    const res = await fetch(`/api/v1/storage/files?page=${page}&per_page=${perPage}`);
                    return res.json();
                },
                upload: async (file) => {
                    const formData = new FormData();
                    formData.append('file', file);
                    const res = await fetch(`/api/v1/storage/upload`, {
                        method: 'POST',
                        body: formData
                    });
                    return res.json();
                },
                delete: async (id) => {
                    const res = await fetch(`/api/v1/storage/files/${id}`, {
                        method: 'DELETE'
                    });
                    return res.status === 204;
                },
                getFileUrl: (filename) => {
                    if (filename.startsWith('http://') || filename.startsWith('https://')) {
                        return filename;
                    }
                    const cleanName = filename.replace(/^\//, '');
                    return `${window.location.origin}${self.scope}/api/v1/storage/file/${cleanName}`;
                }
            };
        },

        // --- AI ACTIONS RUNNER ---
        get ai() {
            return {
                run: async (slug, variables) => {
                    const res = await fetch(`/api/v1/ai/run/${slug}`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ variables })
                    });
                    return res.json();
                }
            };
        }
    };

    // 5. AUTO-REDIRECT ON UNAUTHORIZED
    document.addEventListener('htmx:responseError', function(evt) {
        if (evt.detail.xhr.status === 401) {
            console.warn("[ApexKit] Session expired or unauthorized. Cleared token.");
            window.$apex.logout('/render/login');
        }
    });
})();