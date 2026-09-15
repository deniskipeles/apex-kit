# 🖥️ Server-Side Rendering (SSR) & Templating Engine

**Version:** 0.1.0  
**Context:** Full-Stack HTML Generation, HTMX Integration, and Dynamic Routing

ApexKit features a powerful built-in Server-Side Rendering (SSR) engine. It bridges the flexibility of **Server-Side TypeScript/JavaScript Controllers** with the blazing-fast **Tera HTML templating engine** (similar to Jinja2, Django, and Twig).

This architecture allows you to fetch data securely on the server and render dynamic HTML before it ever reaches the user's browser, ensuring top-tier SEO, fast initial paints, and seamless HTMX compatibility.

---

## 1. Anatomy of a Template

A template in ApexKit consists of two intertwined parts:
1. **The Controller (`<script server>`):** An isolated TypeScript/JavaScript block that executes securely on the backend before the page renders.
2. **The View (HTML):** A Tera HTML template that consumes the JSON returned by the Controller.

To embed server logic, wrap your controller inside a `<script server>` or `<script type="server/ts">` tag at the top of your HTML file:

```html
<script server>
export default async function(req) {
    // 1. Parse the incoming request context
    const payload = await req.json();
    
    // 2. Fetch data using the global $db API (Scoped to current Tenant/Sandbox)
    const posts = await $db.records.list('posts', { limit: 5 });
    
    // 3. Return JSON state to the HTML template
    return { 
        posts: posts.items,
        title: "Latest News",
        viewer: payload.headers['user-agent']
    };
}
</script>

<!-- The HTML below receives the returned JSON as variables -->
<div class="container mx-auto p-8">
    <h1 class="text-3xl font-bold">{{ title }}</h1>
    <ul class="mt-4 space-y-2">
        {% for post in posts %}
            <li class="p-4 bg-slate-800 rounded">{{ post.data.title }}</li>
        {% else %}
            <li>No posts found.</li>
        {% endfor %}
    </ul>
    <small class="text-slate-500">Rendered for: {{ viewer }}</small>
</div>
```

*(Note: Legacy delimiters like `// ---@@ssr` and Astro-style `---` frontmatter are also still supported for backwards compatibility).*

---

## 2. The Request Payload & Authorization

Templates are automatically served via the `/render/{slug}` dynamic routes (e.g. `/render/dashboard` or `/tenant/client-a/render/dashboard`).

When a user visits a template route, the Controller's `req.json()` method yields a comprehensive payload containing URL parameters, headers, the parsed request body (JSON or Form Data), and the authenticated user's claims.

### The Payload Object Context
```json
{
  "params": { 
    "slug": "dashboard" // Extracted from URL path and query strings (?id=5)
  },
  "headers": { 
    "user-agent": "Mozilla/5.0...",
    "host": "localhost:5000",
    "cookie": "apex_session=..."
  },
  "is_htmx": true, // True if the request was made via an hx-get/hx-post
  "auth": { 
    "id": 1, 
    "email": "user@example.com", 
    "role": "admin" 
  }, // Null if the user is not logged in
  "body": {
    "search": "database limits" // Parsed from POST JSON or application/x-www-form-urlencoded
  }
}
```

### Protecting a Private Route
You can build secure, private pages by checking the `auth` object. If the user is unauthenticated, return a standard `401 Unauthorized` Response. The `apex.js` frontend client automatically intercepts this and redirects the user to `/render/login`.

```html
<script server>
export default async function(req) {
    const payload = await req.json();
    
    // Block unauthenticated users gracefully
    if (!payload.auth) {
        return new Response({ error: "Unauthorized" }, { status: 401 });
    }
    
    // Fetch data specifically for this user
    const myTasks = await $db.records.list('tasks', { 
        filter: { owner_id: payload.auth.id } 
    });

    return { user: payload.auth, tasks: myTasks.items };
}
</script>
```

---

## 3. The Universal Client (`apex.js`) & HTMX

ApexKit injects a highly optimized client script (`/static/js/apex.js`) into all rendered templates. This script seamlessly manages physical multi-tenancy and ephemeral sandboxes without requiring you to hardcode URLs.

### What `apex.js` handles automatically:
1. **Dynamic Scope Rewriting:** Automatically detects if the app is running in `/tenant/xyz` or `/sandbox/abc` and intercepts relative links to maintain the scope boundaries.
2. **State Hydration:** Hydrates the JSON returned by your server controller into a globally reactive `window.__SSR_STATE__` object.
3. **Token Injection:** Automatically retrieves the JWT from `localStorage` and injects it into all native `fetch()` calls and `HTMX` triggers.
4. **Auth Helpers:** Exposes a global `$apex` SDK object with `.auth.login()` and `.auth.logout()` methods.

### Setting up the Base Layout
When creating a base layout template, include HTMX and `apex.js` (Tailwind CSS is injected automatically by the server if missing):

```html
<!DOCTYPE html>
<html>
<head>
    <script src="/static/js/htmx.js"></script>
    <script src="/static/js/apex.js"></script>
</head>
<body class="bg-slate-900 text-white">
    <!-- 
        HTMX requests are auto-prefixed and auto-authenticated!
        You write "/api/v1/run/buy_now", but apex.js safely converts it to 
        "/tenant/123/api/v1/run/buy_now" behind the scenes.
    -->
    <button hx-post="/api/v1/run/buy_now" class="bg-indigo-600 p-2 rounded">
        Purchase
    </button>
</body>
</html>
```

---

## 4. Components & Includes

As your UI grows, split it into reusable partials using the `{% include %}` tag.

### ⚠️ The Golden Rule of Components
**The SSR `<script server>` block only executes on the Route Controller (the parent entry point).**
Any server-side TypeScript written inside an *included* child component template is ignored. The Route template mapped to the URL must fetch **all** the necessary data for itself and its children, passing it down the rendering tree.

**Example Child Component (`components/navbar`):**
```html
<nav class="bg-slate-800 text-white p-4 flex justify-between">
    <div class="font-bold">My SaaS</div>
    <div>
        {% if user %}
            <span class="mr-4">Hello, {{ user.email }}</span>
            <button onclick="$apex.auth.logout('/render/login')">Logout</button>
        {% else %}
            <a href="/render/login">Login</a>
        {% endif %}
    </div>
</nav>
```

**Example Parent Route Controller (`dashboard`):**
```html
<script server>
export default async function(req) {
    const payload = await req.json();
    return { user: payload.auth };
}
</script>

<div>
    <!-- The navbar component automatically inherits the 'user' variable -->
    {% include "components/navbar" %}

    <main class="p-8">
        <h1 class="text-2xl">Dashboard Content</h1>
    </main>
</div>
```

---

## 5. Tera Syntax Cheat Sheet

ApexKit uses the Tera templating engine. Here are the most common operations:

### Variables, Filters, & JSON Dumps
```html
<!-- Print a variable -->
{{ user.email }}

<!-- Apply string filters -->
{{ post.data.title | upper }}
{{ post.content | safe }} <!-- Renders raw HTML without escaping -->
{{ posts | length }}

<!-- Dump JSON structures (Great for debugging or passing to Alpine.js) -->
{{ user_profile | json | safe }}
<pre>{{ my_data | debug }}</pre>
```

### Conditionals
```html
{% if user.role == "admin" %}
    <a href="/admin">Admin Panel</a>
{% elif user.role == "editor" %}
    <a href="/editor">Editor Panel</a>
{% else %}
    <p>Standard User</p>
{% endif %}
```

### Loops
```html
<ul class="space-y-2">
{% for item in items %}
    <!-- loop.index starts at 1, loop.index0 starts at 0 -->
    <li>{{ loop.index }}. {{ item.data.name }}</li>
{% else %}
    <li>No items found in the database.</li>
{% endfor %}
</ul>
```