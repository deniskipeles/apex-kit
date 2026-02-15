# 🚀 ApexKit Developer API Documentation

**Version:** 0.1.0  
**Base URL:** `http://localhost:5000/api/v1` (Default)  
**SDK Requirement:** `apexkit-sdk` v0.1.0+

ApexKit is a monolithic Backend-as-a-Service providing a type-safe REST API, high-performance search, real-time subscriptions, and a sandboxed JavaScript runtime.

---

## 1. SDK Initialization

Install the SDK via npm or use the ESM module directly.

```javascript
import { ApexKit } from 'apexkit-sdk';

// Initialize the client
const apex = new ApexKit('https://api.your-app.com');

// Set an existing token if available
apex.setToken('YOUR_JWT_TOKEN');
```

---

## 2. Multi-Tenancy & Scoping

ApexKit is scope-aware. You can switch between the Root App, Tenants, or Sandboxes fluently. The SDK automatically handles the URL routing.

```javascript
// Target a specific customer's database
const tenant = apex.tenant('client-alpha');

// Target an ephemeral AI playground
const sandbox = apex.sandbox('session-uuid-123');

// All subsequent calls on 'tenant' or 'sandbox' are isolated
const records = await tenant.collection('posts').list();
```

---

## 3. Authentication

Tokens are valid only for the scope they were issued in.

### Login / Register
```javascript
// Standard User Login
const auth = await apex.auth.login('user@email.com', 'password');
console.log(auth.user.id, auth.user.role);

// GitHub OAuth (Browser)
apex.auth.loginWithGithub('https://your-frontend.com/callback');
```

### Identity
```javascript
const me = await apex.auth.getMe();
console.log(`Current Scope: ${me.scope}`); // e.g., "tenant:client-alpha"
```

---

## 4. Collections & Records

### Listing Records (with Filters & Joins)
```javascript
const result = await apex.collection('posts').list({
    page: 1,
    per_page: 20,
    sort: '-created', // Newest first
    filter: {
        "status": "published",
        "category": { "$in": ["tech", "news"] }
    },
    expand: 'author_id,comments(5).user_id' // Join relations + nested expansion
});

console.log(result.items, result.total);
```

### CRUD Operations
```javascript
// Create
const record = await apex.collection('posts').create({
    title: "Hello World",
    content: "Content here..."
});

// Update (Partial)
await apex.collection('posts').patch(record.id, {
    status: "published"
});

// Delete
await apex.collection('posts').delete(record.id);
```

---

## 5. Analytical Query Engine

Execute complex aggregations and post-processing pipelines directly from the client.

```javascript
const stats = await apex.collection('sales').query({
    "select": [
        "category",
        { "fn": "sum", "field": "total", "as": "revenue" },
        { "fn": "count", "field": "id", "as": "orders" }
    ],
    "where": { "status": "completed" },
    "group_by": ["category"],
    "pipeline": [
        { "op": "pivot", "args": { "key": "category", "value": "revenue", "agg": "sum" } }
    ]
});
```

---

## 6. High-Performance Search

### Instant Search (Tantivy)
Fast fuzzy search for autocomplete and global search. Requires `ose_indexed: true`.
```javascript
const hits = await apex.collection('products').searchRecordsInstantlyWithOSE("iphne");
// Returns: [{ id: 1, score: 2.1, snippet: { name: "<b>iPhone</b> 15" } }]
```

### Vector Search (AI)
Semantic search based on meaning. Requires `vectorize: true`.
```javascript
const matches = await apex.collection('docs').searchTextVector("How to reset password?");
```

---

## 7. AI Actions (LLMs)

Run predefined Generative AI prompt templates securely.

```javascript
const response = await apex.ai.run('content-summarizer', {
    text: "Long article body...",
    length: "short"
});

console.log(response.result); // AI generated text
console.log(response.metadata); // Citations and search sources
```

---

## 8. Files & Storage

ApexKit handles Local or S3 storage transparently.

```javascript
// 1. Upload
const fileInput = document.getElementById('upload');
const storedFile = await apex.files.upload(fileInput.files[0]);

// 2. Get Public URL (Scoped)
const url = apex.files.getFileUrl(storedFile.filename);

// 3. Dynamic Resizing
const thumb = `${url}?thumb=100x100`;
```

---

## 9. Real-Time (WebSockets)

Subscribe to changes with server-side filtering.

```javascript
import { ApexKitRealtimeWSClient } from 'apexkit-sdk';

const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// Subscribe to urgent tickets
realtime.subscribe({
    collectionId: 5,
    dataFilter: { "priority": "urgent" }
});

realtime.onEvent((msg) => {
    if (msg.type === "Insert") console.log("New Urgent Ticket!", msg.payload.data);
});
```

---

## 10. GraphQL

A dynamic schema is automatically available for all environments.

```javascript
const query = `
  query GetProfile($id: ID!) {
    users(where: { id: $id }) {
      items {
        email
        posts {
          title
        }
      }
    }
  }
`;

const data = await apex.graphql(query, { id: "10" });
```

---

## 11. Error Handling

ApexKit returns standardized JSON errors.

```javascript
try {
    await apex.collection('orders').create({ amt: -1 });
} catch (err) {
    console.error(err.status); // 422
    console.error(err.code);   // "validation_error"
    console.error(err.details); // { amt: "Must be positive" }
}
```