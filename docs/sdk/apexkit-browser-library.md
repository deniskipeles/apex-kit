# ApexKit Frontend Client (`apex.js`) — Complete Documentation

The ApexKit Frontend Client (`apex.js`) is an auto-injected or drop-in JavaScript browser library that provides:
1. **Zero-Configuration Dynamic Scope Routing**: Automatically detects and prefixes API requests with tenant and sandbox environments (`/tenant/:id`, `/sandbox/:id`).
2. **Automatic JWT Authentication**: Injects `Authorization: Bearer <token>` into standard browser `fetch` requests and HTMX triggers.
3. **Reactive State Management (`$apex.state`)**: Hydrates server-side rendered (SSR) state from `window.__SSR_STATE__` into a reactive client store with event listeners.
4. **Data Collections & Full-Text/Vector Search**: CRUD operations, Tantivy full-text search, and multimodal vector similarity matching.
5. **Storage, Image Transformations, & OpenGraph Generation**: Asset uploads, resizing/blurring, and dynamic SVG-to-raster OpenGraph URLs.
6. **AI Prompt Action Streaming**: Real-time Server-Sent Events (SSE) streaming for LLMs.

---

## Table of Contents

1. [Installation & Setup](#1-installation--setup)
2. [Reactive State Store (`$apex.state`)](#2-reactive-state-store-apexstate)
3. [Authentication (`$apex.auth`)](#3-authentication-apexauth)
4. [Collections & Record Operations](#4-collections--record-operations)
5. [Search & Vector Queries](#5-search--vector-queries)
6. [Storage & OpenGraph (`$apex.files`)](#6-storage--opengraph-apexfiles)
7. [Webhooks & Scripts (`$apex.webhook`)](#7-webhooks--scripts-apexwebhook)
8. [AI Prompt Actions (`$apex.ai`)](#8-ai-prompt-actions-apexai)
9. [HTMX & Tailwind Integration](#9-htmx--tailwind-integration)

---

## 1. Installation & Setup

### Automatic Injection (SSR Templates)
When rendering templates via `/render/:slug`, `apex.js` is injected into the HTML markup along with the Tailwind CDN (`https://cdn.tailwindcss.com`). No installation is required.

### Manual Inclusion (Static SPAs or External Pages)
```html
<script src="/static/js/apex.js"></script>
```

Once loaded, `window.$apex` is available globally in the browser.

---

## 2. Reactive State Store (`$apex.state`)

SSR templates initialize `window.__SSR_STATE__` with request query parameters, URL path variables, and authenticated user credentials. `$apex.state` synchronizes with this state and allows client-side components (Alpine.js, HTMX, Vanilla JS) to react to updates.

### Methods

| Method | Parameters | Description |
| :--- | :--- | :--- |
| `$apex.state.get(pathKey?)` | `pathKey?: string` | Returns the entire state or reads nested values using dot-notation (e.g. `'auth.user.email'`). |
| `$apex.state.set(keyOrObject, value?)` | `string \| object, any` | Updates state properties and notifies active subscribers. |
| `$apex.state.on(callback)` | `(state) => void` | Subscribes to state changes. Returns an `unsubscribe` function. |

### Example Usage

```javascript
// 1. Read values from SSR state
const currentUser = $apex.state.get('auth');
const queryParamId = $apex.state.get('params.id');

// 2. Subscribe to reactive changes
const unsubscribe = $apex.state.on((newState) => {
    console.log("State updated:", newState);
    document.getElementById("user-display").innerText = newState.auth?.email || "Guest";
});

// 3. Update state dynamically (e.g., after custom actions)
$apex.state.set('auth.role', 'admin');
$apex.state.set({ cartCount: 5, theme: 'dark' });

// 4. Unsubscribe when done
// unsubscribe();
```

---

## 3. Authentication (`$apex.auth`)

All successful authentication methods persist the JWT to `localStorage.getItem('apex_token')` and automatically sync the authenticated user to `$apex.state.get('auth')`.

### Email / Password Login
```javascript
const { ok, status, data } = await $apex.auth.login("user@example.com", "securePassword123");

if (ok) {
    console.log("Logged in successfully:", data.user);
    $apex.redirect("/render/dashboard");
} else {
    alert(data.message || "Invalid credentials");
}
```

### Registration
```javascript
const { ok, status, data } = await $apex.auth.register(
    "newuser@example.com",
    "securePassword123",
    "user", // role (default: "user")
    { displayName: "Alex", newsletter: true } // metadata object
);

if (ok) {
    console.log("User registered:", data.user);
}
```

### Current User Profile Management
```javascript
// Fetch latest authenticated user data from DB
const user = await $apex.auth.getMe();

// Update user metadata
const { ok, data } = await $apex.auth.updateMeMetadata({
    bio: "Full-stack developer on ApexKit",
    preferences: { theme: "dark" }
});
```

### OAuth2 Social Login
```javascript
// Redirects to GitHub OAuth and redirects back to target path on success
$apex.auth.loginWithGithub("/render/profile");

// Redirects to Google OAuth
$apex.auth.loginWithGoogle("/render/profile");
```

### Logout & Token Access
```javascript
// Logs out, clears the token from localStorage, sets $apex.state.auth = null, and redirects
$apex.auth.logout("/render/login");

// Direct token access
const token = $apex.getToken();
$apex.setToken("custom_jwt_token_string");
```

---

## 4. Collections & Record Operations

Collection operations are accessed via `$apex.collection(collectionNameOrId)`.

### List Records
```javascript
const posts = await $apex.collection("posts").list({
    page: 1,
    per_page: 20,
    sort: "-created", // Prefix with '-' for DESC
    filter: { status: "published", views: { $gt: 10 } },
    expand: "author_id,comments(5)" // Expand relations and sub-relations
});

console.log(`Total: ${posts.total}`, posts.items);
```

### Retrieve a Single Record
```javascript
const post = await $apex.collection("posts").get(12, {
    expand: "author_id"
});

console.log(post.data.title, post.expand.author_id.data.email);
```

### Create a Record
```javascript
const newRecord = await $apex.collection("posts").create({
    title: "Getting Started with ApexKit",
    content: "ApexKit is a high-performance single-node BaaS.",
    status: "published",
    author_id: $apex.state.get("auth.id")
});

console.log("Created Record ID:", newRecord.id);
```

### Update / Patch a Record
```javascript
const updated = await $apex.collection("posts").update(12, {
    title: "Updated Title",
    views: 45
});
```

### Delete a Record
```javascript
const success = await $apex.collection("posts").delete(12);
if (success) {
    console.log("Record deleted");
}
```

---

## 5. Search & Vector Queries

ApexKit provides three search layers: Tantivy Full-Text Search (OSE), the SQL Pipeline Query Engine, and Multimodal Vector Similarity.

### A. Tantivy Full-Text Search (OSE)
```javascript
// Standard Full-Text Search with pagination
const searchResults = await $apex.collection("articles").searchOSE("machine learning", {
    page: 1,
    per_page: 10
});

// Low-latency instant search preview (ideal for search-as-you-type inputs)
const instant = await $apex.collection("articles").instantSearchOSE("rust", 5);
instant.forEach(item => {
    console.log(`ID: ${item.id}, Score: ${item.score}, Snippet:`, item.snippet);
});
```

### B. Advanced SQL & Pipeline Query Engine
```javascript
const queryResult = await $apex.collection("orders").query({
    from: "orders",
    select: [
        "customer_id",
        { field: "total_price", fn: "sum", as: "revenue" },
        { field: "created", fn: "month", as: "order_month" }
    ],
    where: { status: "completed" },
    group_by: ["customer_id", "order_month"],
    sort: "-revenue",
    limit: 10
});
```

### C. Multimodal Vector Search
```javascript
// 1. Text-to-Text Vector Search (Embeds query on the server and runs HNSW distance search)
const semanticallySimilar = await $apex.collection("articles").searchVectorWithText("artificial intelligence", {
    per_page: 5
});

// 2. Text-to-Image Search (Finds image records matching textual descriptions using CLIP/SigLIP)
const imagesMatchingQuery = await $apex.collection("gallery").searchImageVectorWithText("red sports car at sunset", 10);

// 3. Image-to-Image Search (Finds similar images using a base64 encoded input)
const similarImages = await $apex.collection("gallery").searchImageVectorWithImage("data:image/jpeg;base64,...", 5);

// 4. Raw Vector Vector Search (Pre-computed embedding array)
const rawVecResults = await $apex.collection("articles").searchVectorWithVector("embedding_field", [0.012, -0.043, 0.981], {
    per_page: 10
});

// 5. Fetch Vector Coordinates for an Existing Record
const recordVectors = await $apex.collection("articles").getVector(12);
```

---

## 6. Storage & OpenGraph (`$apex.files`)

### Upload Files
```javascript
const fileInput = document.querySelector('input[type="file"]');
const file = fileInput.files[0];

const uploaded = await $apex.files.upload(file);
console.log("File uploaded:", uploaded.filename, uploaded.url);
```

### Dynamic Image URL Builder (Resizing, WebP, Blur)
Transforms original images on the fly with edge caching and automatic `304 Not Modified` conditional headers:

```javascript
// 1. Resize by dimensions (width x height)
const thumbUrl = $apex.files.getFileUrl("hero-banner.jpg", "400x300");

// 2. Format conversion, quality tuning, and Gaussian blurring
const optimizedUrl = $apex.files.getFileUrl("avatar.png", {
    thumb: "150x150",
    format: "webp", // 'webp' | 'png' | 'jpeg' | 'avif'
    quality: 85,    // 1 - 100
    blur: 2.5       // Gaussian sigma radius
});

document.getElementById("my-img").src = optimizedUrl;
```

### Dynamic OpenGraph (OG) Image URL Generation
Generates a pre-signed URL to render dynamic SVG-to-PNG cards for `<meta property="og:image">`:

```javascript
const ogImageUrl = $apex.files.getOpenGraphUrl("default", [
    { type: "text", target: "TITLE", value: "How to Build Serverless Apps with ApexKit" },
    { type: "text", target: "SUBTITLE", value: "Step by step architecture breakdown" },
    { type: "image", target: "IMAGE_URL", value: "logo.png" }
], {
    format: "png",
    quality: 90
});

// Put directly in header meta tags
document.querySelector('meta[property="og:image"]').setAttribute("content", ogImageUrl);
```

---

## 7. Webhooks & Scripts (`$apex.webhook`)

Execute named backend JavaScript / TypeScript scripts and webhooks over HTTP with automatic payload parsing:

#### What is supported on `$apex.webhook("name")`:
- **All standard HTTP methods**: `.get()`, `.post()`, `.put()`, `.patch()`, `.delete()`, `.options()`, `.head()`, and `.execute(method, ...)`.
- **Smart parameter overloading**:
  - `await $apex.webhook("orders").get({ status: "active" })` *(direct query params)*
  - `await $apex.webhook("orders").get("/summary", { year: 2026 })` *(subpath + query params)*
  - `await $apex.webhook("pay").post({ amount: 99.99 })` *(direct body)*
  - `await $apex.webhook("stripe").post("/v1/charge", { amount: 99.99 })` *(subpath + body)*
- **Automatic path sanitization**: Handles missing/redundant leading or trailing slashes and preserves search parameters without 404 route mismatches.

```javascript
// 1. GET with Query Parameters: /api/v1/webhook/analytics?range=30d
const stats = await $apex.webhook("analytics").get({ range: "30d" });

// 2. GET with a Subpath: /api/v1/webhook/users/profile?id=42
const profile = await $apex.webhook("users").get("/profile", { id: 42 });

// 3. POST with Payload: /api/v1/webhook/process-payment
const receipt = await $apex.webhook("process-payment").post({
    amount: 199.99,
    currency: "USD"
});

// 4. POST with Subpath & Payload: /api/v1/webhook/stripe/v1/charges
const charge = await $apex.webhook("stripe").post("/v1/charges", {
    token: "tok_visa"
});

// 5. DELETE Endpoint
await $apex.webhook("cache-purger").delete({ tag: "home_feed" });

// 6. Custom HTTP Verb
await $apex.webhook("custom-handler").execute("CUSTOM_METHOD", "/action", { key: "val" });
```

---

## 8. AI Prompt Actions (`$apex.ai`)

Trigger pre-configured LLM Prompt Actions with Server-Sent Events (SSE) streaming support:

```javascript
// 1. Standard Execution
const result = await $apex.ai.run("summarize-article", {
    text: "Long article body text..."
});
console.log("Summary:", result.result);

// 2. Real-Time Streaming Output (SSE)
const outputElement = document.getElementById("ai-output");

await $apex.ai.run(
    "code-explainer", 
    { code: "function bubbleSort(arr) { ... }" },
    (chunk) => {
        // Appends tokens as they stream from the LLM
        outputElement.innerText += chunk;
    }
);
```

---

## 9. HTMX & Tailwind Integration

`apex.js` integrates directly with HTMX and Tailwind CSS in SSR templates.

### Automatic HTMX Authorization & Scope Routing
Any HTMX attribute automatically resolves tenant and sandbox prefixes and injects active tokens without manual header configurations:

```html
<!-- Automatically transforms into: /tenant/abc/api/v1/collections/posts/records -->
<button 
    hx-get="/api/v1/collections/posts/records" 
    hx-target="#post-list" 
    hx-swap="innerHTML"
    class="px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg shadow font-medium">
    Load Posts
</button>

<div id="post-list"></div>
```

### Alpine.js Reactive Binding with `$apex.state`
```html
<div x-data="{ user: $apex.state.get('auth') }" x-init="$apex.state.on(s => user = s.auth)">
    <template x-if="user">
        <div class="p-4 bg-slate-800 text-white rounded-md">
            Logged in as: <span class="font-bold text-indigo-400" x-text="user.email"></span>
            <button @click="$apex.auth.logout('/render/login')" class="ml-4 text-rose-400 underline">Logout</button>
        </div>
    </template>
    <template x-if="!user">
        <a href="/render/login" class="text-indigo-400 underline">Sign in to your account</a>
    </template>
</div>
```