export const SsrDocs = {
  astroStyle: `---
export default async function(req) {
    const payload = await req.json();
    
    // You have access to the exact same $db API here!
    const post = await $db.records.get('posts', payload.params.id);
    
    return { 
        post: post,
        user_agent: payload.headers['user-agent'] 
    };
}
---

<div class="max-w-2xl mx-auto p-4">
    <h1>{{ post.data.title }}</h1>
    <p>{{ post.data.content }}</p>
    
    <small>Rendered for: {{ user_agent }}</small>
</div>`,

  anatomy: `<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    
    // Fetch data using the backend $db API
    const posts = await $db.records.list('posts', { limit: 5 });
    
    // Return variables to the HTML
    return { 
        posts: posts.items,
        title: "Latest News" 
    };
}
// ---@@ssr
</script>

<!-- The HTML receives the returned JSON -->
<div class="container">
    <h1>{{ title }}</h1>
    <ul>
        {% for post in posts %}
            <li>{{ post.data.title }}</li>
        {% endfor %}
    </ul>
</div>`,

  payload: `{
  "params": { 
    // From URL /render/posts?id=5
    "id": "5" 
  },
  "headers": { 
    "user-agent": "Mozilla/5.0..." 
  },
  "is_htmx": true,
  "auth": { 
    // Null if not logged in
    "id": 1, 
    "email": "user@test.com", 
    "role": "admin" 
  }
}`,

  routeProtection: `// Inside your ---@@ssr block
export default async function(req) {
    const payload = await req.json();
    
    // Block unauthenticated users
    if (!payload.auth) {
        return new Response(
            { error: "Unauthorized" }, 
            { status: 401 }
        );
    }
    
    return { user: payload.auth };
}`,

  clientScript: `<!DOCTYPE html>
<html>
<head>
    <script src="/static/js/htmx.js"></script>
    <script src="/static/js/alpine.js" defer></script>
    <!-- Automatically handles Auth Headers & Scope Routing! -->
    <script src="/static/js/apex.js"></script>
</head>
<body>
    <!-- Login Example -->
    <form onsubmit="event.preventDefault(); $apex.login(email.value, password.value).then(res => { if(res.ok) window.location.href = $apex.scope + '/render/dashboard'; })">
        <input id="email" type="email">
        <input id="password" type="password">
        <button type="submit">Login</button>
    </form>

    <!-- HTMX automatically gets the Token and Scope Prefix! -->
    <button hx-post="/api/v1/run/buy_now">Purchase</button>
    
    <button onclick="$apex.logout()">Logout</button>
</body>
</html>`,

  navbarComponent: `<!-- No JS allowed here. Just HTML/Tera -->
<nav>
    <div class="logo">My App</div>
    {% if user %}
        <span>Hello, {{ user.email }}</span>
        <button onclick="$apex.logout()">Logout</button>
    {% else %}
        <a href="/render/login">Login</a>
    {% endif %}
</nav>`,

  dashboardComponent: `<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    return { user: payload.auth };
}
// ---@@ssr
</script>

<div>
    <!-- Include the component -->
    {% include "components/navbar" %}

    <main>Dashboard Content</main>
</div>`,

  introText: `Templates are automatically accessible at{' '}
<code>/render/&#123;slug&#125;</code>. The <code>req.json()</code> object
contains URL parameters, headers, and the authenticated user's details.`,
};
