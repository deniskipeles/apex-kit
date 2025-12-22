# ApexKit Renderer & Templating Documentation

The ApexKit Renderer is a **Hybrid Server-Side Rendering (SSR) Engine** built on top of [Tera](https://keats.github.io/tera/). It allows you to build dynamic web applications using HTML mixed with logic, seamlessly integrated with the ApexKit database and Scripting Engine.

It is designed to work perfectly with **HTMX** for dynamic, SPA-like experiences without the complexity of client-side routing.

---

## 1. Basic Concepts

### URL Structure
To render a template, access it via the `/render/` endpoint using its **slug**:

*   **Template Slug:** `pages/home`
*   **Public URL:** `https://your-app.com/render/pages/home`

### The Context
Every template has access to a global `context` object containing data. You can access variables using double curly braces: `{{ variable_name }}`.

**Default Variables:**
| Variable | Description | Example |
| :--- | :--- | :--- |
| `params` | URL Query parameters | `{{ params.search }}` |
| `headers` | HTTP Request headers | `{{ headers['user-agent'] }}` |
| `is_htmx` | Boolean, true if request came from HTMX | `{% if is_htmx %}...{% endif %}` |
| `body` | JSON or Form body (if POST request) | `{{ body.title }}` |

---

## 2. Data Access Methods

There are two ways to get data into your templates.

### Method A: Database Helpers (Direct Access)
You can query the database directly inside your HTML using helper functions.

**⚠️ IMPORTANT SYNTAX RULE:** You **MUST** use keyword arguments (e.g., `col='...'`). Positional arguments (e.g., `('users', 1)`) will cause an error.

#### `db_find(col, filter)`
Fetches a list of records.
*   `col`: (string) Collection name.
*   `filter`: (optional, json/object) MongoDB-style filter (currently simple equality).

```html
<!-- Fetch all posts -->
{% set posts = db_find(col='posts') %}

<!-- Fetch active users -->
{% set active_users = db_find(col='users', filter={"status": "active"}) %}

<ul>
{% for user in active_users %}
    <li>{{ user.email }}</li>
{% endfor %}
</ul>
```

#### `db_find_one(col, id)`
Fetches a single record by ID.

```html
{% set author = db_find_one(col='users', id=123) %}
<h1>Author: {{ author.email }}</h1>
```

---

### Method B: Loader Scripts (Complex Logic)
For complex logic (e.g., joining data, external API calls, permissions), verify data in a **Script** before rendering.

1.  **Create a Script** (e.g., `load_dashboard_data`):
    ```javascript
    // Script: load_dashboard_data
    export default async function(req) {
        // 1. Get query param
        const category = req.body.params.category || "general";
        
        // 2. Fetch from DB
        const posts = await $db.find('posts', { category: category });
        
        // 3. Return data object
        return new Response({ 
            page_title: "Dashboard - " + category,
            recent_posts: posts,
            stats: { count: posts.length }
        });
    }
    ```

2.  **Attach Script to Template:**
    In the Admin UI -> Templates -> Edit Template -> Select `load_dashboard_data` in the **Linked Script** dropdown.

3.  **Use Data in Template:**
    The script's return object is merged into the template context.
    ```html
    <!-- Template: dashboard -->
    <h1>{{ page_title }}</h1>
    <p>Total posts: {{ stats.count }}</p>
    
    {% for post in recent_posts %}
       <div>{{ post.title }}</div>
    {% endfor %}
    ```

---

## 3. Composing UI (Includes)

You can build reusable components (navbars, cards, footers) and include them in other templates.

**Syntax:** `{% include "slug/of/template" %}`

**Example:**
*   Template 1 (`components/header`):
    ```html
    <nav>Logo | Home | About</nav>
    ```
*   Template 2 (`pages/home`):
    ```html
    {% include "components/header" %}
    <main>Welcome!</main>
    ```

---

## 4. HTMX Integration (The "Hybrid" Architecture)

ApexKit is optimized for HTMX. You can request *specific components* to update parts of a page without reloading.

### Example: Dynamic Search
**1. The Component (`components/search_results`):**
```html
<!-- This template lists results based on 'q' param -->
{% set results = db_find(col='products', filter={"name": params.q}) %}
<ul id="results-list">
    {% for item in results %}
        <li>{{ item.name }}</li>
    {% endfor %}
</ul>
```

**2. The Main Page (`pages/search`):**
```html
<input type="text" 
       name="q"
       hx-get="/render/components/search_results" 
       hx-target="#results-list" 
       hx-trigger="keyup changed delay:500ms"
       placeholder="Search products...">

<div id="results-list">
    <!-- Initial empty state or default results -->
    {% include "components/search_results" %}
</div>
```

---

## 5. Troubleshooting / Common Errors

### `Template Engine Compilation Failed`
*   **Cause:** You likely used positional arguments in a helper function.
*   **Wrong:** `db_find('users', null)`
*   **Right:** `db_find(col='users')` or `db_find(col='users', filter=null)`

### `Template not found`
*   **Cause:** The slug in your URL (`/render/my-slug`) or your `{% include 'my-slug' %}` tag does not match any template in the database exactly.
*   **Fix:** Check the Admin UI > Templates list for the exact slug.

### Variables not showing up
*   **Cause:** If using a Loader Script, ensure the script returns a `Response` object with a JSON body.
*   **Fix:** Ensure script ends with `return new Response({ my_var: "value" });`.