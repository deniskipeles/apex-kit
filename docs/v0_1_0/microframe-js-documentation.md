# ApexKit + Microframe: Progressive Enhancement Guide

This guide explains how to use **Microframe** within **ApexKit Templates** to add rich interactivity to your server-rendered pages.

## The Philosophy: Progressive Enhancement

ApexKit (server-side) handles the heavy lifting: fetching data, security, and rendering the initial HTML via Tera. Microframe (client-side) takes over specific "islands" of the page to handle complex state, animations, or immediate user feedback without a server round-trip.

---

## 1. Setup

First, ensure `microframe.js` is accessible to your frontend. Since ApexKit serves static files from the `static/` directory, place the built `microframe.js` there.

**In your Base Template (`templates/layout.html`):**

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <!-- ... other meta tags ... -->
    
    <!-- 1. Load Microframe as a Module -->
    <script type="module" src="/styles.css"></script> <!-- Assuming CSS is here -->
    <script type="module">
        // Make Microframe available globally or import in specific script tags
        import Microframe from '/static/microframe.js';
        window.Microframe = Microframe; 
    </script>
</head>
<body>
    {% block content %}{% endblock %}
</body>
</html>
```

---

## 2. The "Island" Strategy

Instead of turning your whole page into a Single Page App (SPA), you keep the page server-rendered but define custom elements for interactive parts.

### Example: An Interactive "Like" Button

Imagine rendering a blog post. The content is static (server-side), but the "Like" button needs to update instantly and send an API request in the background.

**Template Code (`templates/posts/view.html`):**

```html
<!-- 1. Server-Rendered Content (Tera) -->
<article>
    <h1>{{ post.title }}</h1>
    <p>{{ post.content }}</p>

    <!-- 2. Microframe Component "Island" -->
    <!-- We pass server data (post.id and current likes) via attributes -->
    <like-button 
        post-id="{{ post.id }}" 
        initial-likes="{{ post.likes }}">
    </like-button>
</article>

<!-- 3. Client-Side Logic inside <script> -->
<script type="module">
    import { Component, define, html, css } from '/static/microframe.js';

    class LikeButton extends Component {
        // Observe attributes to sync Server Data -> Client State
        static get observedAttributes() { return ['initial-likes', 'post-id']; }

        init() {
            // Hydrate state from server-rendered attributes
            this.state = {
                likes: parseInt(this.getAttribute('initial-likes') || 0),
                liked: false,
                loading: false
            };
        }

        styles() {
            return css`
                button { 
                    padding: 8px 16px; 
                    border-radius: 20px; 
                    border: 1px solid #ddd;
                    background: white;
                    cursor: pointer;
                    transition: all 0.2s;
                }
                button.liked { background: #ffebee; border-color: #ef5350; color: #ef5350; }
                button:disabled { opacity: 0.7; cursor: wait; }
            `;
        }

        async toggleLike() {
            if (this.state.loading) return;

            // 1. Optimistic UI Update (Instant feedback)
            const isLiking = !this.state.liked;
            this.setState({ 
                liked: isLiking,
                likes: isLiking ? this.state.likes + 1 : this.state.likes - 1
            });

            // 2. Background API Call
            try {
                // Use ApexKit Script Runner Endpoint
                await fetch(`/api/v1/run/toggle_like`, {
                    method: 'POST',
                    body: JSON.stringify({ id: this.state['post-id'] })
                });
            } catch (err) {
                // Revert if failed
                this.setState({ 
                    liked: !isLiking,
                    likes: isLiking ? this.state.likes - 1 : this.state.likes + 1
                });
                alert("Failed to connect to server");
            }
        }

        render() {
            return html`
                <button 
                    class="${this.state.liked ? 'liked' : ''}" 
                    @click="toggleLike">
                    ♥ ${this.state.likes} Likes
                </button>
            `;
        }
    }

    // Register the tag
    define('like-button', LikeButton);
</script>
```

---

## 3. Hydrating Complex Data (JSON)

Sometimes you need to pass an array or object from the Server (Tera) to the Client (Microframe).

**Technique:** Encode the JSON string into an attribute.

**ApexKit Template:**
```html
{% set comments = db_find(col='comments', filter={"post_id": post.id}) %}

<!-- Pass the JSON object stringified -->
<comment-section 
    data-comments='{{ comments | json_encode() }}'>
</comment-section>

<script type="module">
    import { Component, define, html } from '/static/microframe.js';

    class CommentSection extends Component {
        static get observedAttributes() { return ['data-comments']; }

        init() {
            this.state = { list: [] };
        }

        // Native Web Component Hook
        attributeChangedCallback(name, oldVal, newVal) {
            if (name === 'data-comments' && newVal) {
                try {
                    // Parse the Server JSON
                    const comments = JSON.parse(newVal);
                    this.setState({ list: comments });
                } catch(e) {
                    console.error("Failed to parse server data");
                }
            }
        }

        render() {
            return html`
                <h3>Comments (${this.state.list.length})</h3>
                <ul>
                    ${this.state.list.map(c => `<li>${c.text}</li>`).join('')}
                </ul>
            `;
        }
    }
    define('comment-section', CommentSection);
</script>
```

---

## 4. Combining with HTMX

ApexKit works beautifully with HTMX. Microframe fills the gaps where HTMX feels "clunky" (like complex local state or high-frequency updates).

**Scenario:** A list of items is loaded via HTMX, but each item has a complex "Edit" modal controlled by Microframe.

**Template:**
```html
<div hx-get="/render/components/items_list" hx-trigger="load">
    <!-- HTMX injects content here... -->
</div>
```

**Template (`components/items_list`):**
```html
{% for item in items %}
    <!-- HTMX loaded this HTML, but the browser automatically upgrades
         this tag into a Microframe component! -->
    <rich-item-card 
        id="{{ item.id }}" 
        title="{{ item.title }}">
    </rich-item-card>
{% endfor %}
```

**Client Script (Global or in Layout):**
```javascript
// This runs once. As HTMX injects new <rich-item-card> tags,
// the browser detects them and boots up the component logic instantly.
define('rich-item-card', class extends Component {
    // ... logic for drag-and-drop, complex validation, or specialized UI ...
});
```

---

## 5. Patterns & Best Practices

### 1. Don't Over-Componentize
If a section of your page doesn't have *internal state* (it just displays data), **use standard HTML/Tera**. Only use Microframe when:
*   You need `this.state`.
*   You need immediate DOM manipulation (drag & drop, canvas, audio/video).
*   You need to maintain data between HTMX swaps (using the Microframe Router/Store).

### 2. Styles: Global vs Shadow DOM
*   **Global (Tailwind):** ApexKit uses Tailwind. If you want your Microframe component to use Tailwind classes, you must inject the Tailwind stylesheet into the Shadow DOM, OR avoid Shadow DOM (Microframe supports Shadow DOM by default, but you can modify it to use Light DOM if you prefer global styles).

    *Workaround to use Tailwind in Shadow DOM:*
    ```javascript
    styles() {
        return css`
            @import url('/styles.css'); /* Import global styles */
            :host { display: block; }
        `;
    }
    ```

### 3. Server Script vs Client Script
*   **ApexKit Scripts (in DB, executed via `/run/`):** Run on the **Server**. Secure. Can access `$db`.
*   **Microframe Scripts (inside `<script>` tags):** Run on the **Browser**. Insecure. Must use `fetch()` to talk to the DB.

### 4. Code Splitting
For large applications, do not write all your components inside `<script>` tags in the HTML.
1.  Create a file `static/js/components/my-widget.js`.
2.  Import it in your template:
    ```html
    <script type="module" src="/static/js/components/my-widget.js"></script>
    <my-widget></my-widget>
    ```