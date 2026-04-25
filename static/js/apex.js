/**
 * ApexKit Universal Frontend Client
 * Automatically handles Scope Routing (Tenant/Sandbox) and JWT Auth Injection.
 */
(function() {
    // [NEW] Prevent double initialization if injected multiple times
    if (window.$apex) return;

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

        // Auto-prefix URL
        if (typeof resource === 'string' && resource.startsWith('/') && !resource.startsWith(SCOPE_PREFIX)) {
            args[0] = SCOPE_PREFIX + resource;
        }

        // Auto-inject Token
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
        // A. Auto-inject the Token
        const token = localStorage.getItem('apex_token');
        if (token) {
            evt.detail.headers['Authorization'] = 'Bearer ' + token;
        }

        // B. Auto-prefix the URL
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
                window.location.href = SCOPE_PREFIX + redirectPath;
            }
        },

        getToken: () => localStorage.getItem('apex_token'),
        setToken: (t) => localStorage.setItem('apex_token', t),
    };

    // 5. AUTO-REDIRECT ON UNAUTHORIZED (Optional but recommended)
    // Listens for HTMX auth failures.
    document.addEventListener('htmx:responseError', function(evt) {
        if (evt.detail.xhr.status === 401) {
            console.warn("[ApexKit] Unauthorized. Clearing token.");
            localStorage.removeItem('apex_token');
            // Trigger an event so the app can show a modal, or redirect automatically
            window.dispatchEvent(new CustomEvent('apex:unauthorized'));
        }
    });
})();