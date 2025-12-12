export class Component extends HTMLElement {
    constructor() {
        super();
        this.state = {};
        this.router = null; // Will be injected by Router
        this._root = this.attachShadow({ mode: 'open' });
        this._styles = '';
    }

    // 1. Lifecycle: Setup
    connectedCallback() {
        this.styles();
        this.init();
        this.update();
        this.mounted();
    }

    disconnectedCallback() {
        this.unmounted();
    }

    attributeChangedCallback(name, oldValue, newValue) {
        if (oldValue !== newValue) {
            // Auto-update state with attribute values (useful for router params)
            this.setState({ [name]: newValue });
        }
    }

    // 2. User Overrides
    init() { /* Initialize state here */ }
    mounted() { /* DOM is ready */ }
    unmounted() { /* Cleanup */ }

    // Define styles (returns CSS string)
    styles() { return ''; }

    // Define template (returns HTML string)
    render() { return '<div></div>'; }

    // 3. Core Logic
    setState(newState) {
        this.state = { ...this.state, ...newState };
        this.update();
    }

    update() {
        // Basic diffing: re-render HTML. 
        // In a production version, you'd use a lit-html style tag function here.
        const template = document.createElement('template');
        template.innerHTML = `
        <style>${this.styles()}</style>
        ${this.render()}
      `;

        // Clear shadow root and append new content
        // Note: This is a naive re-render. For complex apps, use a diffing library.
        this._root.replaceChildren(template.content.cloneNode(true));

        this.bindEvents();
    }

    bindEvents() {
        // Auto-bind @click, @input, etc. (Simple event delegation)
        const elements = this._root.querySelectorAll('*');
        elements.forEach(el => {
            Array.from(el.attributes).forEach(attr => {
                if (attr.name.startsWith('@')) {
                    const eventName = attr.name.slice(1);
                    const methodName = attr.value;
                    el.addEventListener(eventName, (e) => {
                        if (this[methodName]) {
                            this[methodName](e);
                        } else {
                            console.warn(`Method ${methodName} not found in component`);
                        }
                    });
                    el.removeAttribute(attr.name);
                }
            });
        });
    }
}

// Helper to define components cleanly
export const define = (name, componentClass) => {
    if (!customElements.get(name)) {
        customElements.define(name, componentClass);
    }
};

// Tagged templates helpers (for syntax highlighting)
export const html = (strings, ...values) => String.raw({ raw: strings }, ...values);
export const css = (strings, ...values) => String.raw({ raw: strings }, ...values);

export class Router {
    static instance = null;

    constructor(routes = [], options = {}) {
        if (Router.instance) return Router.instance;
        Router.instance = this;

        this.routes = routes;
        this.options = {
            basePath: '',
            rootElement: document.body,
            linkSelector: 'a[href]:not([target]):not([download])',
            hashMode: false,
            ...options
        };

        this.currentRoute = null;
        this.params = {};
        this.query = {};
        this.activeElement = null;

        // Bind methods
        this.push = this.push.bind(this);
        this.replace = this.replace.bind(this);
        this.back = this.back.bind(this);

        this.init();
    }

    init() {
        // Expose router globally for router-link components
        window.microRouter = this;

        this.handleRoute();
        window.addEventListener('popstate', () => this.handleRoute());
        document.addEventListener('click', (e) => this.handleLinkClick(e));
        if (this.options.hashMode) {
            window.addEventListener('hashchange', () => this.handleRoute());
        }
    }

    getCurrentPath() {
        if (this.options.hashMode) {
            return window.location.hash.slice(1) || '/';
        }
        const path = window.location.pathname;
        const base = this.options.basePath;
        return path.startsWith(base) ? path.slice(base.length) || '/' : path;
    }

    matchRoute(path) {
        return this.routes.find(route => {
            if (route.path === '*') return true;
            const regex = this.pathToRegex(route.path);
            return regex.test(path);
        });
    }

    pathToRegex(path) {
        const pattern = '^' + path
            .replace(/[.+?^${}()|[\]\\]/g, '\\$&')
            .replace(/:[^/]+/g, '([^/]+)')
            .replace(/\*/g, '(.*)')
            .replace(/\\$/, '') + '$';
        return new RegExp(pattern);
    }

    extractParams(route, path) {
        const params = {};
        if (route.path === '*') return params;

        const paramNames = (route.path.match(/:([^/]+)/g) || [])
            .map(p => p.slice(1));
        const regex = this.pathToRegex(route.path);
        const matches = path.match(regex);

        if (matches) {
            paramNames.forEach((name, i) => {
                params[name] = matches[i + 1];
            });
        }

        return params;
    }

    extractQuery() {
        const search = window.location.search.slice(1);
        if (!search) return {};

        return Object.fromEntries(
            search.split('&').map(param => {
                const [key, value] = param.split('=');
                return [decodeURIComponent(key), decodeURIComponent(value || '')];
            })
        );
    }

    handleRoute() {
        const path = this.getCurrentPath();
        const route = this.matchRoute(path);

        if (!route) {
            this.handleNotFound();
            return;
        }

        this.params = this.extractParams(route, path);
        this.query = this.extractQuery();

        if (route.before && !route.before(this.params, this.query)) {
            return;
        }

        this.currentRoute = route;
        this.render(route);

        route.after && route.after(this.params, this.query);

        window.dispatchEvent(new CustomEvent('route-change', {
            detail: { route, params: this.params, query: this.query }
        }));
    }

    render(route) {
        const root = this.options.rootElement;

        // Cleanup previous element
        if (this.activeElement) {
            if (this.activeElement.unmount) this.activeElement.unmount();
            this.activeElement.remove();
            this.activeElement = null;
        }

        const { component } = route;
        let element = null;

        if (typeof component === 'string') {
            // Assume component is a Tag Name (e.g. 'home-page')
            if (customElements.get(component)) {
                element = document.createElement(component);
            } else {
                // Raw HTML string
                root.insertAdjacentHTML('beforeend', component);
                element = root.lastElementChild;
            }
        } else if (typeof component === 'function') {
            // Class constructor or function returning HTML
            try {
                element = new component();
            } catch (e) {
                const result = component(this.params, this.query);
                if (typeof result === 'string') {
                    root.insertAdjacentHTML('beforeend', result);
                    element = root.lastElementChild;
                } else if (result instanceof HTMLElement) {
                    element = result;
                }
            }
        }

        if (element) {
            // Inject Router Params as Attributes (for Microframe Component observation)
            if (Object.keys(this.params).length) {
                Object.entries(this.params).forEach(([key, value]) => {
                    element.setAttribute(key, value);
                });
            }

            // Inject router instance directly if it's a Microframe Component
            // Checks if 'router' property exists or if it has the Microframe init method
            if ('router' in element || typeof element.onInit === 'function' || element.tagName.includes('-')) {
                element.router = this;
            }

            // Add to DOM if not added via insertAdjacentHTML
            if (!element.isConnected) {
                root.appendChild(element);
            }
        }

        this.activeElement = element;
    }

    handleLinkClick(e) {
        const link = e.target.closest(this.options.linkSelector);
        if (!link || !this.isSameOrigin(link.href)) return;
        if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        if (link.hasAttribute('router-ignore')) return;

        e.preventDefault();
        const path = link.getAttribute('href');
        if (path) this.push(path);
    }

    isSameOrigin(href) {
        try {
            const url = new URL(href, window.location.origin);
            return url.origin === window.location.origin;
        } catch {
            return false;
        }
    }

    push(path, state = {}) {
        if (this.options.hashMode) {
            window.location.hash = path;
        } else {
            window.history.pushState(state, '', this.options.basePath + path);
        }
        this.handleRoute();
    }

    replace(path, state = {}) {
        if (this.options.hashMode) {
            window.location.replace(`#${path}`);
        } else {
            window.history.replaceState(state, '', this.options.basePath + path);
        }
        this.handleRoute();
    }

    back() { window.history.back(); }
    forward() { window.history.forward(); }

    handleNotFound() {
        const notFound = this.routes.find(r => r.path === '*');
        if (notFound) {
            this.render(notFound);
        } else {
            this.options.rootElement.innerHTML = '<h1>404 Not Found</h1>';
        }
    }
}

// --- THIS WAS MISSING ---
export const createRouter = (routes, options) => new Router(routes, options);

// Router Link Component
export class RouterLink extends HTMLElement {
    connectedCallback() {
        this.render();
        window.addEventListener('route-change', () => this.updateState());
    }

    render() {
        const href = this.getAttribute('href');
        const text = this.textContent;
        // Create actual anchor for accessibility/SEO
        this.innerHTML = `<a href="${href}" class="${this.getAttribute('class') || ''}">${text}</a>`;
        this.updateState();
    }

    updateState() {
        const href = this.getAttribute('href');
        const router = window.microRouter;
        if (!router || !href) return;

        const currentPath = router.getCurrentPath();
        const isActive = href === '/' ? currentPath === '/' : currentPath.startsWith(href);

        const anchor = this.querySelector('a');
        if (anchor) {
            if (isActive) anchor.classList.add('active');
            else anchor.classList.remove('active');
        }
    }
}
customElements.define('router-link', RouterLink);

const Microframe = {
    Component,
    Router,
    define,
    createRouter,
    html,
    css
};

export default Microframe;