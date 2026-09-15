# 🪝 Script Hooks Guide

**Version:** 0.1.0  
**Context:** Server-Side JavaScript / TypeScript (QuickJS Engine with Oxc Transpilation)

Hooks allow you to intercept database mutations, enforce complex business constraints, modify API payloads, enrich real-time events, and track multi-tenant lifecycle metrics without modifying the Rust core.

---

## 1. How Hooks Work

When an API request or internal database lifecycle event occurs, ApexKit checks for active scripts registered to that specific **Trigger Type**:

* **Engine:** Executed in-process via **QuickJS** with full TypeScript/TSX support.
* **Preemptive Scheduling:** Protected by the **Quantum Scheduler**, ensuring long scripts yield CPU slices every 10ms and abort if they exceed configured execution limits.
* **Void Hooks vs. Filter Hooks:**
  * **Void Hooks (`before_*`, `after_*`):** Used for validation, logging, or triggering side effects (e.g. sending emails or notifications). A `before_` hook aborts the operation by throwing an `Error` or returning `false`.
  * **Filter Hooks (`before_*`, `after_*`):** Expect a modified data object to be returned, allowing you to transform the data before it is saved or returned to the client.
* **Scope Isolation:** Scripts execute in the database and filesystem context of the triggering Tenant or Sandbox.

---

## 2. Complete Trigger Reference

| Category | Trigger Type | Execution Moment | Use Case |
| :--- | :--- | :--- | :--- |
| **Record Writes** | `before_create_record`<br>`before_update_record` | Before database insert or update. | Schema validation, default calculations, slugification. |
| | `after_create_record`<br>`after_update_record` | After database commit. | Dispatching WebSocket events, triggering emails. |
| | `before_delete_record`<br>`after_delete_record` | Before / after record deletion. | Checking referential integrity, cleanup. |
| **Record Reads** | `before_list_records`<br>`after_list_records` | Before query / after result fetch. | Dynamic query modification, masking sensitive data. |
| | `before_get_record`<br>`after_get_record` | Before fetching single record / after fetch. | Access auditing, computed field enrichment. |
| **Auth & Users** | `before_user_login`<br>`after_user_login` | Before authentication / after login. | IP verification, brute-force rate limits, login logs. |
| | `before_user_create`<br>`after_user_create` | Before user registration / after create. | Domain whitelisting, welcome emails. |
| | `before_user_delete`<br>`after_user_delete` | Before user deletion / after delete. | Preventing root admin deletion, cascading profile purge. |
| | `before_list_users`<br>`after_list_users` | Before / after user listing. | Custom user filtering, metadata masking. |
| **Collections** | `before_collection_create`<br>`after_collection_create` | Before / after collection creation. | Schema governance, automated migrations. |
| | `before_collection_update`<br>`after_collection_update` | Before / after schema update. | Index synchronization, schema auditing. |
| | `before_collection_delete` | Before collection deletion. | Preventing system collection deletion. |
| **Files & Storage**| `before_file_upload`<br>`after_file_upload` | Before file write / after metadata save. | File format validation, virus scanning, OCR. |
| | `before_file_delete`<br>`after_file_delete` | Before / after file deletion. | Quota calculation, storage cleanup. |
| **Relations** | `before_relation_create`<br>`after_relation_create` | Before / after relationship link. | Circular dependency checks, bi-directional graphs. |
| | `before_relation_delete`<br>`after_relation_delete` | Before / after relationship unlink. | Referential cleanup. |
| **Multi-Tenancy** | `before_tenant_create`<br>`after_tenant_create` | Before / after tenant provisioning. | Initializing starter collections, seeding demo data. |
| | `before_tenant_request`<br>`after_tenant_request` | On incoming tenant HTTP call / after response. | Per-tenant rate limiting, egress/ingress tracking. |
| | `before_sandbox_request`<br>`after_sandbox_request` | On incoming sandbox HTTP call / after response. | Sandbox activity tracking. |
| **AI & Vector** | `before_ai_run` | Before prompt evaluation. | RAG vector search injection, variable sanitization. |
| | `after_ai_run` | After prompt completion. | Response logging, token usage tracking. |
| | `on_vectorization_start` | When bulk revectorization begins. | Queue telemetry, status notifications. |

---

## 3. Global Script Built-in Primitives

All hooks have access to the standard runtime globals:

* **`$db`**: Scoped database client (`$db.records.list`, `$db.records.get`, `$db.records.create`, `$db.records.update`, `$db.records.delete`, `$db.query`).
* **`$files`**: Storage engine (`$files.read`, `$files.save`, `$files.getSignedUrl`, `$files.delete`).
* **`$fs`**: Virtual file system (`$fs.read`, `$fs.write`, `$fs.delete`, `$fs.list`, `$fs.stat`).
* **`$http` / `fetch`**: Standard Web Fetch API with automatic base URL resolution.
* **`$realtime`**: Ephemeral signal broadcaster (`$realtime.send(channel, event, data)`).
* **`$queue`**: Background task spawner (`$queue.spawn(fn, options)`).
* **`$cache`**: In-memory key-value store (`$cache.get`, `$cache.set`, `$cache.incr`, `$cache.delete`).
* **`$util`**: Utilities (`$util.uuid()`, `$util.slugify(text)`, `$util.hash(data, alg)`, `$util.hmac(data, key)`).
* **`$mail`**: SMTP mail client (`$mail.send(to, subject, body)`).
* **`console`**: Integrated logger (`console.log`, `console.info`, `console.warn`, `console.error`) committing to `_system_logs`.

---

## 4. Practical Hook Examples

### A. Data Normalization & Validation
Automatically slugify a post title and validate minimum pricing before writing to the database:

```typescript
// Script Name: validate-product
// Trigger: before_create_record | Target Collection: products
// Path: ./webhooks/validate-product.ts

export default async function (event) {
    const data = event.record.data;

    // 1. Enforce business validation
    if (typeof data.price !== "number" || data.price <= 0) {
        throw new Error("Price must be a positive number.");
    }

    // 2. Normalize and compute fields
    data.slug = $util.slugify(data.title || "product");
    data.created_by_ip = event.auth ? event.auth.email : "anonymous";

    // Return the modified data object
    return data;
}
```

---

### B. Real-Time Broadcast on Creation
Send an instant WebSocket event to connected clients whenever a new record is created:

```typescript
// Script Name: broadcast-order
// Trigger: after_create_record | Target Collection: orders
// Path: ./webhooks/broadcast-order.ts

export default async function (event) {
    const { id, data } = event.record;

    await $realtime.send("orders_feed", "NewOrderCreated", {
        orderId: id,
        total: data.total,
        customer: data.customer_name,
        timestamp: new Date().toISOString()
    });
}
```

---

### C. Dynamic Row-Level Query Modification
Restrict record listing queries dynamically based on the requester's role:

```typescript
// Script Name: restrict-tasks-view
// Trigger: before_list_records | Target Collection: tasks
// Path: ./webhooks/restrict-tasks-view.ts

export default async function (event) {
    // Admins bypass filter restrictions
    if (event.auth && event.auth.role === "admin") {
        return event.data;
    }

    // If not admin, restrict query to records owned by the current user
    const currentFilter = event.data.filter ? JSON.parse(event.data.filter) : {};
    currentFilter.assigned_to = event.auth ? event.auth.id : -1;

    event.data.filter = JSON.stringify(currentFilter);
    return event.data;
}
```

---

### D. RAG Context Injection for AI Prompts
Intercept an AI Action before execution to inject semantic search results into the prompt variables:

```typescript
// Script Name: inject-rag-context
// Trigger: before_ai_run | Path: ./webhooks/inject-rag-context.ts

export default async function (event) {
    const { slug, vars } = event.data;

    if (slug === "support-assistant" && vars.query) {
        // 1. Generate query embedding
        const queryVec = await $ai.embed(vars.query);

        // 2. Perform vector search on knowledge base
        const matches = await $db.records.searchVector("documentation", "content", queryVec, 3);
        
        // 3. Format matched snippets into context
        vars.context = matches.map(m => `### ${m.data.title}\n${m.data.content}`).join("\n\n");
    }

    return { slug, vars };
}
```

---

### E. Rate Limiting on Tenant Requests
Prevent API abuse on incoming tenant requests:

```typescript
// Script Name: tenant-rate-limiter
// Trigger: before_tenant_request | Path: ./webhooks/tenant-rate-limiter.ts

export default async function (event) {
    const { ip, tenant_id } = event.data;

    const minuteStamp = new Date().toISOString().slice(0, 16);
    const key = `rate:${tenant_id}:${ip}:${minuteStamp}`;

    const requestCount = await $cache.incr(key, 1);

    if (requestCount > 120) {
        throw new Error("Rate limit exceeded. Maximum 120 requests per minute allowed.");
    }
}
```

---

## 5. Event Context Payload Reference (`event`)

The properties received by the hook function depend on the trigger category:

### Record Write Hooks (`before_create_record`, `after_update_record`, etc.)
```typescript
interface RecordHookEvent {
  trigger: string;
  record: {
    id: number | string | null;
    data: Record<string, any>;
  };
  collection: {
    id: number;
    name: string;
    schema?: any;
  };
  auth: {
    id: number;
    email: string;
    role: string;
  } | null;
}
```

### System & Filter Hooks (`before_user_create`, `before_list_records`, `before_tenant_request`, etc.)
```typescript
interface SystemHookEvent {
  trigger: string;
  data: any; // QueryOptions, UserData, RequestContext, etc.
  auth: {
    id: number;
    email: string;
    role: string;
  } | null;
  timestamp?: string;
}
```

---

## 6. Best Practices & Error Handling

1. **Aborting Operations:** To cancel an action in any `before_` hook, throw an `Error`:
   ```typescript
   throw new Error("Validation failed: invalid license format.");
   ```
   The API will immediately abort the transaction and return a `422 Unprocessable Entity` response with your error message.
2. **Always Return Modified Payloads in Filter Hooks:** In transformation hooks (like `before_create_record`, `before_list_records`, `before_ai_run`), ensure you return the data object. Returning `undefined` or `null` will leave the payload unchanged or abort execution.
3. **Avoid Infinite Loops:** If an `after_create_record` hook on collection `orders` calls `$db.records.create("orders", ...)`, it will trigger the same hook again recursively.
4. **Use Background Tasks for Heavy Processing:** For long-running operations (such as video encoding, heavy OCR, or external network requests), dispatch work to `$queue.spawn()` inside the hook rather than blocking the database transaction.
