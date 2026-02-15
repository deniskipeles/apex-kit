# 🪝 Script Hooks Guide

**Version:** 0.1.0
**Context:** Server-Side JavaScript (Boa Engine)

Hooks allow you to intercept system events, validate data, modify API responses, and enforce business logic without modifying the core Rust backend. They run synchronously within the database transaction (for write hooks) or the request lifecycle.

---

## 1. How Hooks Work

When an API request or system event occurs (e.g., `POST /records` or a Tenant Request), ApexKit checks the database for active scripts registered to that specific **Trigger**.

*   **Runtime**: Scripts execute in the isolated **Boa JS Engine**.
*   **Blocking**: `before_` hooks can abort an operation by throwing an `Error`.
*   **Transformation**: Hooks like `before_create` or `after_list_records` expect you to return the modified data object.
*   **Targeting**: Scripts can be **Global** (run on all collections) or **Targeted** (run only on a specific collection like `orders`).

---

## 2. Global API Reference

These objects are available in every script context.

| Object | Description | Example |
| :--- | :--- | :--- |
| **`$db`** | Scoped Database Access. | `await $db.records.find(null, "users", { active: true })` |
| **`$run`** | Execute other scripts. | `await $run.script("shared-logic", { id: 1 })` |
| **`$cache`** | Scoped Key-Value store. | `await $cache.incr("hits", 1)` |
| **`$http`** | Make external requests. | `await $http.get("https://api.com")` |
| **`$zip`** | In-memory ZIP & FS tools. | `await $zip.readFile("logo.png")` |
| **`$cmd`** | **Root Only.** Run shell. | `await $cmd.run("ls", ["-la"])` |
| **`log(msg)`** | Write to System Logs. | `log("Processing ID: " + e.record.id)` |

---

## 3. Hook Categories

### A. Data Write Hooks
**Triggers:** `before_create_record`, `after_create_record`, `before_update_record`, `after_update_record`, `before_delete_record`, `after_delete_record`

Used for validation, data normalization, or triggering side effects (like emails).

**Example: Validation & Normalization**
*Trigger: `before_create_record` | Target: `products`*
```javascript
export default async function(e) {
    // 1. Validate
    if (e.record.data.price < 0) {
        throw new Error("Price cannot be negative");
    }
    
    // 2. Normalize
    e.record.data.slug = $util.slugify(e.record.data.name);
    
    // You MUST return the data object to save it
    return e.record.data; 
}
```

---

### B. Read & Filter Hooks
**Triggers:** `before_list_records`, `after_list_records`, `before_get_record`, `after_get_record`

Used to enforce dynamic Row-Level Security (RLS) or mask sensitive data.

**Example: Dynamic RLS (Force Filter)**
*Trigger: `before_list_records` | Target: `posts`*
```javascript
export default async function(e) {
    // If not admin, force filter to only show own posts
    if (e.auth.role !== 'admin') {
        const filter = e.data.filter ? JSON.parse(e.data.filter) : {};
        filter.owner_id = e.auth.id;
        e.data.filter = JSON.stringify(filter);
    }
    return e.data; // Return modified query options
}
```

---

### C. Traffic & Quota Hooks
**Triggers:** `before_tenant_request`, `after_tenant_request`, `before_sandbox_request`

These hooks run on **every incoming HTTP request** to a specific scope. Use them for rate limiting or custom usage tracking.

**Example: Simple Rate Limiter**
*Trigger: `before_tenant_request`*
```javascript
export default async function(e) {
    const key = `quota:${e.data.ip}`;
    const count = await $cache.incr(key, 1);
    
    if (count > 100) {
        throw new Error("Rate limit exceeded (100 req/min)");
    }
}
```

---

### D. System & Auth Hooks
**Triggers:** `before_user_create`, `before_file_upload`, `before_collection_create`

**Example: Domain Restriction**
*Trigger: `before_user_create`*
```javascript
export default async function(e) {
    if (!e.data.email.endsWith("@company.com")) {
        throw new Error("Registration restricted to internal employees.");
    }
}
```

---

## 4. Hook Context (`e`) Reference

The `e` object varies based on the trigger:

| Property | Description | Availability |
| :--- | :--- | :--- |
| **`e.trigger`** | The trigger name. | All |
| **`e.auth`** | Current user (`id`, `email`, `role`). | If logged in |
| **`e.record`** | The record being processed (`id`, `data`). | Write Hooks |
| **`e.collection`** | Metadata of target table (`id`, `name`). | Data Hooks |
| **`e.data`** | The payload (QueryOptions, UserData, etc). | Filter/System Hooks |

---

## 5. Troubleshooting & Best Practices

1.  **Infinite Loops**: Be careful calling `$db.insert` for `Collection A` inside an `after_create` hook for `Collection A`. This will trigger the hook again recursively.
2.  **Error Messages**: Messages thrown in `before_` hooks are sent directly to the client as a `422 Unprocessable Entity` response.
3.  **Return Values**: Always remember to return the data object (`e.record.data` or `e.data`) in transformation hooks. If you return `null` or nothing, the system may assume you are blocking the change.
4.  **Logging**: Use `log()` frequently during development. Logs are viewable in the **Admin UI > Logs** section.
5.  **Visibility**: If you want a script to be callable by Tenants via `$run.script()`, set its Visibility to **Public** (Root only).