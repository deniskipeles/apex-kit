# 🗄️ Database Tool (`$db` & `$root.db`)

**Version:** 0.1.0
**Context:** Server-Side Scripting

Access to the database is split into two objects to ensure security and clarity.

## 1. `$db` (Scoped Access)
Available in **ALL** scripts (Root, Tenant, Sandbox).
*   **Behavior:** Automatically targets the database of the current execution scope.
*   **Signature:** Methods do **NOT** accept a context ID argument.
*   **Usage:**
    ```javascript
    // List records in the CURRENT tenant
    const posts = await $db.records.list("posts", { page: 1 });
    ```

## 2. `$root.db` (Privileged Access)
Available **ONLY** in scripts running in the **Root** scope.
*   **Behavior:** Allows accessing ANY tenant or sandbox database.
*   **Signature:** All methods require `contextId` as the **first argument**.
*   **Usage:**
    ```javascript
    // List records in Tenant "client-a"
    const posts = await $root.db.records.list("tenant:client-a", "posts", { page: 1 });
    ```

---

## 3. The `ApexKit` Helper Class (Recommended)
To simplify switching between contexts, the global `ApexKit` class handles the underlying `$db` vs `$root.db` logic automatically.

### Initialization
```javascript
// 1. Target Current Scope (Default)
const app = new ApexKit(); 
// Uses global $db

// 2. Target Specific Tenant (Root Script Only)
const clientA = new ApexKit().tenant("client-a");
// Uses global $root.db with "tenant:client-a" prefix
```

### API Reference (via Helper Class)

#### Records
```javascript
// List
await app.collection("posts").list({ filter: { active: true } });

// Create
await app.collection("posts").create({ title: "New" });

// Get
await app.collection("posts").get(123, { expand: "author" });

// Vector Search
await app.collection("docs").searchVector("embedding", [0.1, 0.2...], 5);
```

#### Analytical Query
```javascript
await app.query({
    from: "orders",
    select: ["status", { fn: "count", field: "id", as: "total" }],
    group_by: ["status"]
});
```

#### Files
```javascript
await app.files.list(20, 0); // Limit, Offset
```