// --- 1. DOM MORPHING ALGORITHM ---
// Intelligently updates the DOM without wiping it, preserving focus, selection, and scroll state.
function morph(oldNode, newNode) {
    if (oldNode.nodeType !== newNode.nodeType || oldNode.tagName !== newNode.tagName) {
        oldNode.replaceWith(newNode.cloneNode(true));
        return;
    }

    if (oldNode.nodeType === Node.TEXT_NODE) {
        if (oldNode.textContent !== newNode.textContent) {
            oldNode.textContent = newNode.textContent;
        }
        return;
    }

    // ✅ ADD THIS FIX: Ignore comments and other non-element nodes
    if (oldNode.nodeType !== Node.ELEMENT_NODE) {
        return; 
    }

    const oldAttrs = oldNode.attributes;
    const newAttrs = newNode.attributes;

    // Remove missing attributes
    for (let i = oldAttrs.length - 1; i >= 0; i--) {
        const name = oldAttrs[i].name;
        if (!newNode.hasAttribute(name)) {
            oldNode.removeAttribute(name);
            if (name.startsWith('.')) oldNode[name.slice(1)] = undefined;
        }
    }

    // Add or update attributes
    for (let i = 0; i < newAttrs.length; i++) {
        const name = newAttrs[i].name;
        const value = newAttrs[i].value;
        
        // Handle Boolean Attributes (?disabled="true")
        if (name.startsWith('?')) {
            const propName = name.slice(1);
            const isTrue = value !== 'false' && value !== 'null' && value !== 'undefined' && value !== '';
            if (oldNode[propName] !== isTrue) oldNode[propName] = isTrue;
            if (isTrue) oldNode.setAttribute(propName, '');
            else oldNode.removeAttribute(propName);
            continue;
        }

        // Handle Property Bindings (.value="hello")
        if (name.startsWith('.')) {
            const propName = name.slice(1);
            if (oldNode[propName] !== value) oldNode[propName] = value;
            if (oldNode.getAttribute(name) !== value) oldNode.setAttribute(name, value);
            continue;
        }

        // Standard Attributes
        if (oldNode.getAttribute(name) !== value) {
            oldNode.setAttribute(name, value);
        }
    }

    // Preserve Input values (Critical for typing without losing cursor focus)
    if (oldNode.tagName === 'INPUT' || oldNode.tagName === 'TEXTAREA') {
        const newVal = newNode.getAttribute('.value') || newNode.getAttribute('value') || newNode.value || '';
        // Only update if value actually changed in state
        if (oldNode.value !== newVal) {
            oldNode.value = newVal;
        }
    }

    // Recursively morph children
    const oldChildren = Array.from(oldNode.childNodes);
    const newChildren = Array.from(newNode.childNodes);
    const max = Math.max(oldChildren.length, newChildren.length);

    for (let i = 0; i < max; i++) {
        if (!oldChildren[i]) {
            oldNode.appendChild(newChildren[i].cloneNode(true));
        } else if (!newChildren[i]) {
            oldNode.removeChild(oldChildren[i]);
        } else {
            morph(oldChildren[i], newChildren[i]);
        }
    }
}


// --- 2. BASE COMPONENT CLASS ---
export class Component extends HTMLElement {
    constructor() {
        super();
        this.router = window.microRouter || null;
        this._root = this.attachShadow({ mode: 'open' });
        this._initialized = false;
        this._internalState = {};
        
        // Proxy intercepts direct state mutations (e.g. this.state.query = 'abc') and triggers re-render
        this._stateProxy = this._createStateProxy(this._internalState);
    }

    get state() {
        return this._stateProxy;
    }

    set state(newState) {
        this._internalState = newState;
        this._stateProxy = this._createStateProxy(this._internalState);
        this.requestUpdate();
    }

    _createStateProxy(obj) {
        return new Proxy(obj, {
            set: (target, prop, value) => {
                target[prop] = value;
                this.requestUpdate();
                return true;
            }
        });
    }

    setState(newState) {
        Object.assign(this._internalState, newState);
        this.requestUpdate();
    }

    requestUpdate() {
        // Debounce rendering to next animation frame for performance
        if (this._updateFrame) cancelAnimationFrame(this._updateFrame);
        this._updateFrame = requestAnimationFrame(() => this.update());
    }

    connectedCallback() {
        this.init();
        this.update();
        this.mounted();
    }

    disconnectedCallback() {
        this.unmounted();
    }

    attributeChangedCallback(name, oldValue, newValue) {
        if (oldValue !== newValue) {
            this.state[name] = newValue;
        }
    }

    // Lifecycle Hooks for subclassing
    init() {}
    mounted() {}
    unmounted() {}
    styles() { return ''; }
    render() { return '<div></div>'; }

    update() {
        const template = document.createElement('template');
        template.innerHTML = `<style>${this.styles()}</style>${this.render()}`;

        if (!this._initialized) {
            // First render: attach everything directly
            this._root.replaceChildren(template.content.cloneNode(true));
            this._initialized = true;
            this.bindEvents(); // Attach delegators once
        } else {
            // Subsequent renders: morph the DOM
            const oldChildren = Array.from(this._root.childNodes);
            const newChildren = Array.from(template.content.childNodes);
            const max = Math.max(oldChildren.length, newChildren.length);

            for (let i = 0; i < max; i++) {
                if (!oldChildren[i]) {
                    this._root.appendChild(newChildren[i].cloneNode(true));
                } else if (!newChildren[i]) {
                    this._root.removeChild(oldChildren[i]);
                } else {
                    morph(oldChildren[i], newChildren[i]);
                }
            }
        }
    }

    bindEvents() {
        // Event Delegation: We attach one listener to the shadow root per event type.
        // This means we never have to re-bind events when the DOM morphs or changes!
        const events = ['click', 'input', 'submit', 'change', 'keydown', 'keyup', 'mouseover', 'mouseout'];
        
        events.forEach(eventType => {
            // Some events don't bubble well (like submit), but capturing them at the root works
            this._root.addEventListener(eventType, (e) => {
                const path = e.composedPath();
                
                for (const target of path) {
                    if (target === this._root) break;
                    if (target.nodeType === Node.ELEMENT_NODE) {
                        const handlerAttr = target.getAttribute(`@${eventType}`);
                        
                        if (handlerAttr) {
                            if (typeof this[handlerAttr] === 'function') {
                                // Method reference: @click="handleClick"
                                this[handlerAttr](e);
                            } else {
                                // Inline Arrow Function execution: @input="(e) => this.setState(...)"
                                try {
                                    const func = new Function('e', `
                                        try {
                                            const handler = ${handlerAttr};
                                            if (typeof handler === 'function') {
                                                handler.call(this, e);
                                            } else {
                                                eval(${JSON.stringify(handlerAttr)});
                                            }
                                        } catch(err) {
                                            console.error("Microframe Event Error in inline handler:", err);
                                        }
                                    `);
                                    func.call(this, e); // Execute with `this` bound to the component instance
                                } catch(err) {
                                    console.error("Microframe Event Parsing Error:", err);
                                }
                            }
                        }
                    }
                }
            });
        });
    }
}


// --- 3. HELPER UTILITIES ---
export const define = (name, componentClass) => {
    if (!customElements.get(name)) customElements.define(name, componentClass);
};

export const html = (strings, ...values) => String.raw({ raw: strings }, ...values);
export const css = (strings, ...values) => String.raw({ raw: strings }, ...values);


// --- 4. ROUTER CLASS ---
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

        this.params = {};
        this.query = {};
        this.activeElement = null;

        this.push = this.push.bind(this);
        this.replace = this.replace.bind(this);
        this.back = this.back.bind(this);

        this.init();
    }

    init() {
        window.microRouter = this;
        this.handleRoute();
        window.addEventListener('popstate', () => this.handleRoute());
        document.addEventListener('click', (e) => this.handleLinkClick(e));
        if (this.options.hashMode) {
            window.addEventListener('hashchange', () => this.handleRoute());
        }
    }

    getCurrentPath() {
        if (this.options.hashMode) return window.location.hash.slice(1) || '/';
        const path = window.location.pathname;
        const base = this.options.basePath;
        return path.startsWith(base) ? path.slice(base.length) || '/' : path;
    }

    matchRoute(path) {
        return this.routes.find(route => {
            if (route.path === '*') return true;
            return this.pathToRegex(route.path).test(path);
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
        const paramNames = (route.path.match(/:([^/]+)/g) || []).map(p => p.slice(1));
        const matches = path.match(this.pathToRegex(route.path));
        if (matches) {
            paramNames.forEach((name, i) => { params[name] = matches[i + 1]; });
        }
        return params;
    }

    handleRoute() {
        const path = this.getCurrentPath();
        const route = this.matchRoute(path);

        if (!route) {
            const notFound = this.routes.find(r => r.path === '*');
            if (notFound) this.render(notFound);
            else this.options.rootElement.innerHTML = '<h1>404 Not Found</h1>';
            return;
        }

        this.params = this.extractParams(route, path);
        if (route.before && !route.before(this.params, this.query)) return;

        this.render(route);
        route.after && route.after(this.params, this.query);
        window.dispatchEvent(new CustomEvent('route-change', { detail: { route, params: this.params } }));
    }

    render(route) {
        const root = this.options.rootElement;
        if (this.activeElement) {
            if (this.activeElement.unmount) this.activeElement.unmount();
            this.activeElement.remove();
        }

        const { component } = route;
        let element = null;

        if (typeof component === 'string') {
            if (customElements.get(component)) element = document.createElement(component);
            else {
                root.insertAdjacentHTML('beforeend', component);
                element = root.lastElementChild;
            }
        } else if (typeof component === 'function') {
            try { element = new component(); } 
            catch (e) {
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
            if (Object.keys(this.params).length) {
                Object.entries(this.params).forEach(([k, v]) => element.setAttribute(k, v));
            }
            if ('router' in element || element.tagName.includes('-')) element.router = this;
            if (!element.isConnected) root.appendChild(element);
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
        try { return new URL(href, window.location.origin).origin === window.location.origin; } 
        catch { return false; }
    }

    push(path, state = {}) {
        if (this.options.hashMode) window.location.hash = path;
        else window.history.pushState(state, '', this.options.basePath + path);
        this.handleRoute();
    }

    replace(path, state = {}) {
        if (this.options.hashMode) window.location.replace(`#${path}`);
        else window.history.replaceState(state, '', this.options.basePath + path);
        this.handleRoute();
    }

    back() { window.history.back(); }
}

export const createRouter = (routes, options) => new Router(routes, options);

export default { Component, Router, define, createRouter, html, css };