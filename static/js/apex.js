/**
 * ApexKit Universal Frontend Client
 * Handles Dynamic Scope Routing (Tenant/Sandbox), JWT Auth Injection, State Management, and Fluent Webhook SDK.
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

    // 4. REACTIVE STATE STORE ($apex.state)
    const _stateListeners = new Set();
    const _stateStore = Object.assign({}, window.__SSR_STATE__ || {});

    const stateHandler = {
        get: function(pathKey) {
            if (!pathKey) return _stateStore;
            return pathKey.split('.').reduce((acc, part) => (acc && acc[part] !== undefined) ? acc[part] : undefined, _stateStore);
        },

        set: function(keyOrObject, value) {
            if (typeof keyOrObject === 'string') {
                const keys = keyOrObject.split('.');
                let current = _stateStore;
                for (let i = 0; i < keys.length - 1; i++) {
                    if (!current[keys[i]] || typeof current[keys[i]] !== 'object') {
                        current[keys[i]] = {};
                    }
                    current = current[keys[i]];
                }
                current[keys[keys.length - 1]] = value;
            } else if (keyOrObject && typeof keyOrObject === 'object') {
                Object.assign(_stateStore, keyOrObject);
            }
            _stateListeners.forEach(fn => fn(_stateStore));
        },

        on: function(listener) {
            if (typeof listener === 'function') {
                _stateListeners.add(listener);
                return () => _stateListeners.delete(listener);
            }
            return () => {};
        }
    };

    // 5. GLOBAL HELPER UTILITY ($apex)
    window.$apex = {
        scope: SCOPE_PREFIX,
        state: stateHandler,

        // --- AUTHENTICATION ---
        auth: {
            login: async function(email, password) {
                const res = await fetch('/api/v1/auth/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ email, password })
                });
                const data = await res.json();
                if (res.ok && data.token) {
                    window.$apex.setToken(data.token);
                    window.$apex.state.set('auth', data.user);
                }
                return { ok: res.ok, status: res.status, data };
            },

            register: async function(email, password, role = 'user', metadata = {}) {
                const res = await fetch('/api/v1/auth/register', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ email, password, role, metadata })
                });
                const data = await res.json();
                if (res.ok && data.token) {
                    window.$apex.setToken(data.token);
                    window.$apex.state.set('auth', data.user);
                }
                return { ok: res.ok, status: res.status, data };
            },

            getMe: async function() {
                const res = await fetch('/api/v1/auth/me');
                if (!res.ok) return null;
                const user = await res.json();
                window.$apex.state.set('auth', user);
                return user;
            },

            updateMeMetadata: async function(metadata = {}) {
                const res = await fetch('/api/v1/auth/me', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ metadata })
                });
                const user = await res.json();
                if (res.ok) {
                    window.$apex.state.set('auth', user);
                }
                return { ok: res.ok, status: res.status, data: user };
            },

            loginWithGithub: function(redirectTo) {
                const url = new URL(`${window.location.origin}${window.$apex.scope}/api/v1/auth/github`);
                if (redirectTo) url.searchParams.append('redirect_to', redirectTo);
                window.location.href = url.toString();
            },

            loginWithGoogle: function(redirectTo) {
                const url = new URL(`${window.location.origin}${window.$apex.scope}/api/v1/auth/google`);
                if (redirectTo) url.searchParams.append('redirect_to', redirectTo);
                window.location.href = url.toString();
            },

            logout: function(redirectPath = '/render/login') {
                localStorage.removeItem('apex_token');
                window.$apex.state.set('auth', null);
                if (redirectPath) {
                    window.$apex.redirect(redirectPath);
                }
            }
        },

        // Backward compatibility
        login: function(email, password) { return this.auth.login(email, password); },
        logout: function(redirectPath) { return this.auth.logout(redirectPath); },
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
                    const params = new URLSearchParams();
                    Object.entries(options).forEach(([k, v]) => {
                        if (v !== undefined && v !== null) {
                            params.append(k, typeof v === 'object' ? JSON.stringify(v) : v);
                        }
                    });
                    const res = await fetch(`/api/v1/collections/${collectionId}/records?${params.toString()}`);
                    return res.json();
                },

                get: async (recordId, options = {}) => {
                    const params = new URLSearchParams();
                    if (options.expand) params.append('expand', options.expand);
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
                        method: 'PATCH',
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

                // Tantivy Full-Text Search
                searchOSE: async (query, options = {}) => {
                    const params = new URLSearchParams(Object.assign({ q: query }, options));
                    const res = await fetch(`/api/v1/collections/${collectionId}/search?${params.toString()}`);
                    return res.json();
                },

                instantSearchOSE: async (query, limit = 10) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/instant-search?q=${encodeURIComponent(query)}&limit=${limit}`);
                    return res.json();
                },

                // Query Engine
                query: async (queryPayload) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/query`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(queryPayload)
                    });
                    return res.json();
                },

                // Vector Search Endpoints
                searchVectorWithVector: async (field, vector, options = {}) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/search-vector-with-vector`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(Object.assign({ field, vector }, options))
                    });
                    return res.json();
                },

                searchVectorWithText: async (queryText, options = {}) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/search-vector-with-text`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(Object.assign({ query_text: queryText }, options))
                    });
                    return res.json();
                },

                searchImageVectorWithImage: async (imageData, limit = 10) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/search-image-vector-with-image`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ image_data: imageData, limit })
                    });
                    return res.json();
                },

                searchImageVectorWithText: async (queryText, limit = 10) => {
                    const res = await fetch(`/api/v1/collections/${collectionId}/search-image-vector-with-text`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ query_text: queryText, limit })
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

                getFileUrl: (filename, options) => {
                    if (filename.startsWith('http://') || filename.startsWith('https://')) {
                        return filename;
                    }
                    const cleanName = filename.replace(/^\//, '');
                    const url = new URL(`${window.location.origin}${self.scope}/api/v1/storage/file/${cleanName}`);
                    
                    if (options) {
                        if (typeof options === 'string') {
                            url.searchParams.append('thumb', options);
                        } else if (typeof options === 'object') {
                            if (options.thumb) url.searchParams.append('thumb', options.thumb);
                            if (options.format) url.searchParams.append('format', options.format);
                            if (options.quality) url.searchParams.append('quality', String(options.quality));
                            if (options.blur) url.searchParams.append('blur', String(options.blur));
                        }
                    }
                    return url.toString();
                },

                getOpenGraphUrl: (templateSlugOrBase64, data, options = {}) => {
                    const url = new URL(`${window.location.origin}${self.scope}/api/v1/storage/files/opengraph`);
                    url.searchParams.append('template', templateSlugOrBase64);
                    url.searchParams.append('data', JSON.stringify(data));
                    if (options.format) url.searchParams.append('format', options.format);
                    if (options.quality) url.searchParams.append('quality', String(options.quality));
                    return url.toString();
                }
            };
        },

        // --- FLUENT WEBHOOK CLIENT (SDK PARITY) ---
        webhook: function(name) {
            const execute = async function(method, subpathOrData, data) {
                let subpath = '';
                let payload = data || {};

                if (typeof subpathOrData === 'string') {
                    let clean = subpathOrData.trim();
                    if (!clean || clean === '/' || clean === './') {
                        subpath = '';
                    } else {
                        if (!clean.startsWith('/')) {
                            clean = `/${clean}`;
                        }
                        const [pathPart = '', queryPart] = clean.split('?');
                        const normalizedPath = pathPart.replace(/\/+$/, '');
                        subpath = queryPart !== undefined ? `${normalizedPath}?${queryPart}` : normalizedPath;
                    }
                } else if (subpathOrData !== undefined && subpathOrData !== null) {
                    payload = subpathOrData;
                }

                let endpoint = `/api/v1/webhook/${name}${subpath}`;
                const isGetOrHead = method === 'GET' || method === 'HEAD';

                const config = {
                    method: method.toUpperCase(),
                    headers: {}
                };

                if (isGetOrHead) {
                    if (payload && typeof payload === 'object' && Object.keys(payload).length > 0) {
                        const urlParams = new URLSearchParams();
                        Object.entries(payload).forEach(([k, v]) => {
                            if (v !== undefined && v !== null) {
                                urlParams.append(k, typeof v === 'object' ? JSON.stringify(v) : v);
                            }
                        });
                        const separator = endpoint.includes('?') ? '&' : '?';
                        endpoint += `${separator}${urlParams.toString()}`;
                    }
                } else {
                    if (typeof FormData !== 'undefined' && payload instanceof FormData) {
                        config.body = payload;
                    } else {
                        config.headers['Content-Type'] = 'application/json';
                        config.body = JSON.stringify(payload);
                    }
                }

                const res = await fetch(endpoint, config);

                if (res.status === 204) {
                    return null;
                }

                const contentType = res.headers.get('content-type') || '';
                if (contentType.includes('application/json')) {
                    return res.json();
                }
                return res.text();
            };

            return {
                get: (subpathOrParams, params) => execute('GET', subpathOrParams, params),
                post: (subpathOrBody, body) => execute('POST', subpathOrBody, body),
                put: (subpathOrBody, body) => execute('PUT', subpathOrBody, body),
                patch: (subpathOrBody, body) => execute('PATCH', subpathOrBody, body),
                delete: (subpathOrParams, params) => execute('DELETE', subpathOrParams, params),
                options: (subpathOrParams, params) => execute('OPTIONS', subpathOrParams, params),
                head: (subpathOrParams, params) => execute('HEAD', subpathOrParams, params),
                execute: (method, subpathOrPayload, payload) => execute(method, subpathOrPayload, payload),
                // Legacy run() fallback
                run: (payload, subpath) => execute('POST', subpath, payload)
            };
        },

        // --- AI ACTIONS RUNNER ---
        get ai() {
            return {
                run: async (slug, variables, onChunk) => {
                    const res = await fetch(`/api/v1/ai/run/${slug}`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ variables })
                    });

                    const contentType = res.headers.get('content-type') || '';
                    if (contentType.includes('text/event-stream') && onChunk) {
                        const reader = res.body.getReader();
                        const decoder = new TextDecoder('utf-8');
                        let fullText = '';
                        let buffer = '';

                        while (true) {
                            const { done, value } = await reader.read();
                            if (done) break;

                            buffer += decoder.decode(value, { stream: true });
                            const lines = buffer.split('\n');
                            buffer = lines.pop() || '';

                            for (const line of lines) {
                                if (line.startsWith('data:')) {
                                    const chunk = line.slice(5).trim();
                                    if (chunk && chunk !== '[DONE]') {
                                        onChunk(chunk);
                                        fullText += chunk;
                                    }
                                }
                            }
                        }
                        return { result: fullText, metadata: null };
                    }

                    return res.json();
                }
            };
        }
    };

    // 6. AUTO-REDIRECT ON 401 UNAUTHORIZED
    document.addEventListener('htmx:responseError', function(evt) {
        if (evt.detail.xhr.status === 401) {
            console.warn("[ApexKit] Session expired or unauthorized. Cleared token.");
            window.$apex.auth.logout('/render/login');
        }
    });
})();