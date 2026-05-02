# Advanced Templating & SSR

This document explores advanced patterns for building dynamic, interactive user interfaces with the ApexKit Templating Engine.

## Dynamic Layouts & Inheritance

While `{% include %}` is useful for small components, `{% extends %}` allows you to define a master layout and swap out the content.

### Base Layout (`layouts/main`)
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{% block title %}My App{% endblock %}</title>
    <script src="/static/js/htmx.js"></script>
    <script src="/static/js/apex.js"></script>
    <link rel="stylesheet" href="/static/tailwind-dark.full.css">
</head>
<body class="bg-gray-900 text-white">
    <nav class="p-4 border-b border-gray-800">
        <a href="/">Home</a>
    </nav>
    <main>
        {% block content %}{% endblock %}
    </main>
</body>
</html>
```

### Page Template (`render/posts`)
```html
{% extends "layouts/main" %}

{% block title %}Blog Posts{% endblock %}

{% block content %}
<script>
// ---@@ssr
export default async function(req) {
    const posts = await $db.records.list('posts');
    return { posts: posts.items };
}
// ---@@ssr
</script>

<div class="p-8">
    <h1 class="text-2xl font-bold">Latest Posts</h1>
    <div class="grid gap-4 mt-4">
        {% for post in posts %}
            <div class="p-4 bg-gray-800 rounded shadow">
                <h2 class="text-xl">{{ post.title }}</h2>
                <p class="text-gray-400">{{ post.excerpt }}</p>
            </div>
        {% endfor %}
    </div>
</div>
{% endblock %}
```

## Integration with HTMX

ApexKit + HTMX is a powerful combination for building SPA-like experiences without complex frontend frameworks.

### Partial Updates
You can render only a fragment of a template when an HTMX request is detected.

```html
<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    const tasks = await $db.records.list('tasks');

    return {
        tasks: tasks.items,
        is_htmx: payload.is_htmx
    };
}
// ---@@ssr
</script>

{% if is_htmx %}
    <!-- Only this block is returned for HTMX requests -->
    {% for task in tasks %}
        <div class="task-item">{{ task.title }}</div>
    {% endfor %}
{% else %}
    <!-- Full page load -->
    {% extends "layouts/main" %}
    {% block content %}
        <div id="task-list" hx-get="/render/tasks" hx-trigger="load">
            Loading tasks...
        </div>
    {% endblock %}
{% endif %}
```

## Custom Filters & Logic in Templates

Tera provides several filters, but sometimes you need complex logic. It is always recommended to perform this logic in the **JavaScript Controller** and pass the result to the view.

### Good Practice: Logic in Controller
```javascript
// ---@@ssr
export default async function(req) {
    const user = await $db.records.get('users', 'u123');

    // Perform complex logic here
    const statusColor = user.active ? 'green' : 'red';
    const formattedDate = new Date(user.last_login).toLocaleDateString();

    return {
        user,
        statusColor,
        formattedDate
    };
}
// ---@@ssr
```

### Rendering JSON for Frontend JS
Sometimes you need to pass server-side data to a client-side script.

```html
<script>
    // Safely embed server data into client-side JS
    const config = {{ config_json | safe }};
    console.log("App Config:", config);
</script>
```

## Error Handling in Controllers

If your controller fails, it can return a standard error page.

```javascript
// ---@@ssr
export default async function(req) {
    try {
        const data = await someRiskyOperation();
        return { data };
    } catch (err) {
        // Return a custom error response
        return new Response({
            error: "Something went wrong",
            message: err.message
        }, { status: 500 });
    }
}
// ---@@ssr
```
