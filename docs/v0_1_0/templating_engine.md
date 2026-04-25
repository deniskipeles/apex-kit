# Server-Side Rendering & Templates

ApexKit features a powerful built-in Server-Side Rendering (SSR) engine. It combines the flexibility of **JavaScript Controllers** with the blazing-fast **Tera HTML templating engine**. 

This architecture allows you to fetch data securely on the server and render dynamic HTML before it ever reaches the user's browser.

---

## 1. Anatomy of a Template

A template in ApexKit consists of two parts:
1. **The Controller (JS):** A Server-Side JavaScript block that executes securely on the backend.
2. **The View (HTML):** A Tera HTML template that consumes the JSON returned by the Controller.

To enable syntax highlighting in standard code editors (like VS Code), we wrap the Controller logic inside a standard `<script>` tag using special `// ---@@ssr` delimiters.

```html
<script>
// ---@@ssr
export default async function(req) {
    // 1. Parse the incoming request
    const payload = await req.json();
    
    // 2. Fetch data using the global $db API
    const posts = await $db.records.list('posts', { limit: 5 });
    
    // 3. Return JSON to the HTML template
    return { 
        posts: posts.items,
        title: "Latest News",
        viewer: payload.headers['user-agent']
    };
}
// ---@@ssr
</script>

<!-- The HTML below receives the returned JSON as variables -->
<div class="container mx-auto p-8">
    <h1>{{ title }}</h1>
    <ul>
        {% for post in posts %}
            <li>{{ post.title }}</li>
        {% else %}
            <li>No posts found.</li>
        {% endfor %}
    </ul>
    <small>Rendered for: {{ viewer }}</small>
</div>
```

---

## 2. The Request Payload & Authorization

Templates are automatically accessible via the `/render/{slug}` URL. 

When a user visits a template route, the Controller's `req.json()` method yields a payload containing URL parameters, headers, and the authenticated user's claims.

### The Payload Object
```json
{
  "params": { 
    "id": "5" // Extracted from URL query string (e.g., ?id=5)
  },
  "headers": { 
    "user-agent": "Mozilla/5.0...",
    "host": "localhost:5000"
  },
  "is_htmx": true, // True if the request was made via HTMX
  "auth": { 
    "id": 1, 
    "email": "user@example.com", 
    "role": "admin" 
  } // Null if the user is not logged in
}
```

### Protecting a Route
You can easily build secure, private pages by checking the `auth` object. If the user is not authenticated, simply return a standard HTTP Response with a `401 Unauthorized` status. The frontend client (`apex.js`) will catch this and redirect the user to the login page.

```javascript
<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    
    // Block unauthenticated users
    if (!payload.auth) {
        return new Response({ error: "Unauthorized" }, { status: 401 });
    }
    
    // Fetch data specifically for this user
    const myTasks = await $db.records.list('tasks', { 
        filter: { owner_id: payload.auth.id } 
    });

    return { user: payload.auth, tasks: myTasks.items };
}
// ---@@ssr
</script>
```

---

## 3. The Universal Client (`apex.js`) & HTMX

ApexKit is uniquely designed to run multi-tenant architecture and isolated sandboxes natively. To ensure your HTML templates work flawlessly across the Root app, Tenants, and Sandboxes *without hardcoding URLs*, ApexKit includes a built-in script called `apex.js`.

### What `apex.js` does:
1. **Dynamic Routing:** Automatically detects if the app is running in `/tenant/xyz` or `/sandbox/abc` and prefixes all API requests.
2. **Token Injection:** Automatically retrieves the JWT from `localStorage` and injects it into all `fetch()` and `htmx` headers.
3. **Auth Helpers:** Exposes a global `$apex` object with `.login()` and `.logout()` methods.

### Setting up the Base Layout
Always include HTMX and `apex.js` in your base template or `index.html`:

```html
<!DOCTYPE html>
<html>
<head>
    <script src="/static/js/htmx.js"></script>
    <!-- Automatically handles Auth Headers & Scope Routing! -->
    <script src="/static/js/apex.js"></script>
</head>
<body>
    <!-- 
        HTMX requests are auto-prefixed and auto-authenticated.
        You write "/api/v1/run/buy_now", but apex.js converts it to 
        "/tenant/123/api/v1/run/buy_now" behind the scenes!
    -->
    <button hx-post="/api/v1/run/buy_now">Purchase</button>
</body>
</html>
```

### Creating a Login Form
Use the `$apex` helper to easily log users in and route them to the dashboard:

```html
<form onsubmit="event.preventDefault(); handleLogin(this)">
    <input id="email" type="email" placeholder="Email">
    <input id="password" type="password" placeholder="Password">
    <button type="submit">Login</button>
</form>

<script>
async function handleLogin(form) {
    const res = await $apex.login(form.email.value, form.password.value);
    
    if (res.ok) {
        // $apex.scope contains the current environment prefix (e.g. "/tenant/123")
        window.location.href = $apex.scope + '/render/dashboard';
    } else {
        alert(res.data.message);
    }
}
</script>
```

---

## 4. Components & Includes

As your UI grows, you should split it into reusable components. ApexKit supports this natively via the `{% include %}` tag.

### ⚠️ The Golden Rule of Components
**The SSR JavaScript block (`// ---@@ssr`) only executes on the Route Controller.**
Any JavaScript written inside an included component template will be ignored. The Route template (the one mapped to the URL) must fetch **all** the necessary data for itself and its children, and pass it down.

**Example Component (`components/navbar`):**
```html
<nav class="bg-dark text-white p-4 flex justify-between">
    <div class="logo">My App</div>
    <div>
        {% if user %}
            <span>Hello, {{ user.email }}</span>
            <button onclick="$apex.logout()">Logout</button>
        {% else %}
            <a href="/render/login">Login</a>
        {% endif %}
    </div>
</nav>
```

**Example Route Controller (`dashboard`):**
```html
<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    return { user: payload.auth };
}
// ---@@ssr
</script>

<div>
    <!-- The navbar component automatically inherits the 'user' variable -->
    {% include "components/navbar" %}

    <main class="p-8">
        <h1>Dashboard Content</h1>
    </main>
</div>
```

---

## 5. Tera Syntax Cheat Sheet

ApexKit uses the Tera templating engine (similar to Jinja2, Django, and Twig).

### Variables & Output
```html
<!-- Print a variable -->
{{ user.email }}

<!-- Apply filters -->
{{ post.title | upper }}
{{ post.content | safe }} <!-- Renders HTML without escaping -->
{{ posts | length }}

<!-- Dump JSON (Great for debugging!) -->
{{ data | debug }}
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
<ul>
{% for item in items %}
    <!-- loop.index starts at 1, loop.index0 starts at 0 -->
    <li>{{ loop.index }}. {{ item.name }}</li>
{% else %}
    <li>No items found.</li>
{% endfor %}
</ul>
```