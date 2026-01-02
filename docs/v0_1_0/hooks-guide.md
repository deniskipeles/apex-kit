# 🪝 ApexKit Script Hooks Guide

**Version:** 0.1.0
**Context:** Server-Side JavaScript (Boa Engine)

Hooks allow you to intercept system events, validate data, modify API responses, and trigger side effects (like sending emails or calling external webhooks) without modifying the core Rust backend.

## 1. How Hooks Work

When an API request hits ApexKit (e.g., `POST /api/v1/collections/posts/records`), the system checks for active scripts registered to that event (e.g., `before_create`).

*   **Runtime:** Scripts run synchronously within the Rust process.
*   **Context:** Data is passed to the script via the `e` (event) object.
*   **Blocking:** If a `before_` hook throws an `Error`, the API request is **aborted** and the error message is sent to the client.
*   **Transformation:** Some hooks (like `before_list_*` or `after_get_*`) expect you to return modified data, allowing you to filter results or inject calculated fields.

---

## 2. Global Objects

These objects are available in **every** script type.

| Object | Description | Example |
| :--- | :--- | :--- |
| **`log(msg)`** | Writes to the System Audit Log. | `log("User " + e.auth.email + " logged in");` |
| **`$db`** | Internal Database Access (Bypasses permissions). | `await $db.find_one("users", 1)` |
| **`$http`** | Make external HTTP requests. | `await $http.post("https://slack.com/webhook", { ... })` |
| **`$util`** | Utilities (UUID generation). | `$util.uuid()` |
| **`$ai`** | Embeddings & Vector Operations. | `await $ai.embed("text")` |
| **`$env`** | Access encrypted system secrets. | `await $env.get("STRIPE_KEY")` |

---

## 3. Hook Categories

### A. Record Write Hooks (CRUD)
**Triggers:** `before_create`, `after_create`, `before_update`, `after_update`, `before_delete`, `after_delete`

Use these to validate input, enforce business logic, or sync data.

**Context (`e`) Structure:**
```json
{
  "record": { "id": 1, "data": { "title": "New Post" } },
  "collection": { "id": 5, "name": "posts" },
  "auth": { "id": 10, "email": "user@test.com", "role": "admin" },
  "trigger": "before_create"
}
```

**Example: Validation (Block creation if price is negative)**
*Trigger: `before_create` | Target Collection: `products`*
```javascript
export default async function(e) {
    if (e.record.data.price < 0) {
        throw new Error("Price cannot be negative."); // Returns 422 to client
    }
    
    // Auto-calculate a field
    e.record.data.price_with_tax = e.record.data.price * 1.2;

    return e.record.data; // You MUST return the data object
}
```

**Example: Side Effect (Notify Slack on Delete)**
*Trigger: `after_delete` | Target Collection: `orders`*
```javascript
export default async function(e) {
    const msg = `⚠️ Order #${e.record.id} deleted by ${e.auth.email}`;
    await $http.post("https://hooks.slack.com/services/...", { text: msg });
}
```

---

### B. Read & Filter Hooks
**Triggers:** `before_list_*`, `after_list_*`, `before_get_*`, `after_get_*`

Use these to enforce Row-Level Security (RLS) dynamically, hide sensitive fields, or inject data.

**Example: Force Filter (Users can only list their own posts)**
*Trigger: `before_list_records` | Target Collection: `posts`*
```javascript
export default async function(e) {
    // e.data contains the QueryOptions ({ filter, sort, page })
    
    if (e.auth.role !== 'admin') {
        // Parse existing filter string or create new object
        let currentFilter = e.data.filter ? JSON.parse(e.data.filter) : {};
        
        // Enforce owner check
        currentFilter.owner_id = e.auth.id;
        
        // Update query
        e.data.filter = JSON.stringify(currentFilter);
    }

    return e.data; // Return modified query options
}
```

**Example: Output Masking (Hide email in public profile)**
*Trigger: `after_get_record` | Target Collection: `profiles`*
```javascript
export default async function(e) {
    // e.data is the RecordResponse ({ id, data, expand })
    
    if (!e.auth) {
        // If guest, remove sensitive fields
        delete e.data.data.email;
        delete e.data.data.phone_number;
    }

    return e.data; // Return sanitized record
}
```

---

### C. Authentication Hooks
**Triggers:** `before_user_create`, `after_user_create`, `before_user_delete`

**Example: Allow Registration only for specific domains**
*Trigger: `before_user_create`*
```javascript
export default async function(e) {
    // e.data = { email, role }
    
    if (!e.data.email.endsWith("@company.com")) {
        throw new Error("Registration restricted to company employees.");
    }
    
    // Force role to 'user' even if 'admin' was requested
    e.data.role = "user";
    
    return e.data;
}
```

---

### D. System & Storage Hooks
**Triggers:** `before_file_upload`, `after_file_upload`, `before_collection_create`, etc.

**Example: Audit File Uploads**
*Trigger: `after_file_upload`*
```javascript
export default async function(e) {
    // e.data = { id, filename, size, mime }
    
    if (e.data.size > 10000000) {
        log("Large file uploaded: " + e.data.filename);
    }
    
    // Store metadata in a custom collection
    await $db.insert("upload_logs", {
        file_id: e.data.id,
        uploaded_by: e.auth ? e.auth.id : null,
        timestamp: new Date().toISOString()
    });
}
```

---

### E. AI Hooks
**Triggers:** `before_ai_run`, `after_ai_run`, `on_vectorization_start`

**Example: Budget Control on AI**
*Trigger: `before_ai_run`*
```javascript
export default async function(e) {
    // e.data = { slug, variables }
    
    if (e.auth.role !== 'admin') {
        const usage = await $db.find("ai_usage", { user_id: e.auth.id });
        if (usage.length > 50) {
            throw new Error("Daily AI limit reached.");
        }
    }
}
```

---

## 4. Full Trigger Reference

| Trigger Name | Type | Context Data (`e.data`) | Return Expected? |
| :--- | :--- | :--- | :--- |
| `before_create` | Record | Record Data (JSON) | **Yes** (Modified Data) |
| `after_create` | Record | Record Data (JSON) | No |
| `before_update` | Record | Update Payload (JSON) | **Yes** (Modified Payload) |
| `after_update` | Record | Updated Record (JSON) | No |
| `before_delete` | Record | Existing Record (JSON) | No (Throw to block) |
| `before_list_records` | Filter | QueryOptions `{ filter, sort, page }` | **Yes** (Modified Query) |
| `after_list_records` | Filter | ListResponse `{ items, total }` | **Yes** (Filtered List) |
| `before_get_record` | Void | `{ id, collection }` | No |
| `after_get_record` | Filter | RecordResponse `{ id, data }` | **Yes** (Sanitized Record) |
| `before_user_create` | Auth | `{ email, role }` | **Yes** (Modified User) |
| `after_user_create` | Auth | `{ id, email }` | No |
| `before_file_upload` | System | `{}` | No (Throw to block) |
| `after_file_upload` | System | `{ id, filename, size }` | No |
| `before_ai_run` | AI | `{ slug, variables }` | No (Throw to block) |
| `after_ai_run` | AI | `{ result, metadata }` | No |
| `on_vectorization_start`| AI | `{ collection_id, force }` | No |

## 5. Troubleshooting

1.  **Script Errors:** If your script has a syntax error, the API request will likely fail with a `500 Internal Server Error` or a `Validation Error` containing the script message.
2.  **Logs:** Use `log("message")` inside your script. Go to **Admin UI > Logs** to view the output.
3.  **Infinite Loops:** Be careful when using `$db.insert` inside an `after_create` hook for the *same* collection. This will cause an infinite loop. Always check the collection name or write to a different collection.