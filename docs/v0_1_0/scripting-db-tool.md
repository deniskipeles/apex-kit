# 🗄️ Database Scripting Tools (`$db`, `$root.db` & `$apex`)

**Version:** 0.1.0  
**Context:** Server-Side TypeScript / JavaScript (QuickJS Engine)

ApexKit provides two database interfaces for server-side scripts:
1. **`$db` (Scoped Access):** Automatically targets the isolated SQLite database of the current executing scope (Tenant, Sandbox, or Root).
2. **`$root.db` (Privileged Cross-Tenant Access):** Available **exclusively** within scripts executing in the **Root** scope to perform cross-tenant queries and migrations.
3. **`ApexKit` / `$apex` (Fluent Wrapper):** Unified client class providing fluent syntax (`$apex.collection("posts")`) and context switching (`$apex.tenant("client-a")`).

---

## 1. `$db` (Scoped Database Access)

Available in all scripts. Methods automatically bind to the executing tenant's or sandbox's physical SQLite database partition without requiring explicit context identifiers.

### A. Records API (`$db.records`)

```typescript
// 1. List records with filtering, sorting, and joins
const { items, total } = await $db.records.list("posts", {
    page: 1,
    per_page: 20,
    sort: "-created",
    filter: { status: "published" },
    expand: "author_id"
});

// 2. Fetch single record with expansion
const post = await $db.records.get("posts", 105, "author_id,comments(5,0).user_id");

// 3. Create a record (returns { id })
const { id } = await $db.records.create("posts", {
    title: "New Article",
    content: "Body content...",
    status: "published"
});

// 4. Update an existing record
const updated = await $db.records.update("posts", 105, {
    views: 50
});

// 5. Delete a record (returns boolean)
const success = await $db.records.delete("posts", 105);

// 6. Vector similarity search
const vecHits = await $db.records.searchVector("posts", "content", queryVector, 5);

// 7. Get stored vector coordinates for a record
const vectors = await $db.records.getVector("posts", 105);

// 8. Instant full-text search (Tantivy / OSE)
const hits = await $db.records.instantSearch("posts", "sqlite", 5);
```

---

### B. Analytical Query Engine (`$db.query`)

Execute multi-column SQL aggregations and grouping operations:

```typescript
const report = await $db.query({
    from: "orders",
    select: [
        "category",
        { fn: "sum", field: "amount", as: "total_revenue" },
        { fn: "count", field: "id", as: "order_count" },
        { fn: "avg", field: "amount", as: "average_order" }
    ],
    where: { status: "completed" },
    group_by: ["category"],
    sort: "-total_revenue",
    limit: 10
});
```

---

### C. Users, Collections & Files APIs

```typescript
// Users
const user = await $db.users.get("admin@example.com");
const newUser = await $db.users.create("user@example.com", "password123", "user");

// Collections Schema Listing
const collections = await $db.collections.list();

// Physical Storage Metadata
const files = await $db.files.list(20, 0); // limit, offset
```

---

## 2. `$root.db` (Privileged Cross-Tenant Access)

Available **only** when a script executes within the Root App context (`scope: "root"`). All methods require a `contextId` string (`"tenant:{id}"`, `"sandbox:{id}"`, or `"root"`) as their first parameter:

```typescript
// Webhook: ./webhooks/tenant-reporter.ts (Root Only)
export default async function (req: Request) {
    const { targetTenantId } = await req.json();

    const contextId = `tenant:${targetTenantId}`;

    // 1. Query records inside target tenant's database
    const posts = await $root.db.records.list(contextId, "posts", { limit: 5 });

    // 2. Insert record directly into target tenant's database
    const { id } = await $root.db.records.create(contextId, "audit_log", {
        action: "Admin Inspection",
        performed_at: new Date().toISOString()
    });

    // 3. Run analytical queries against target tenant
    const stats = await $root.db.query(contextId, {
        from: "orders",
        select: [{ fn: "count", field: "id", as: "total_orders" }]
    });

    return new Response({
        tenant: targetTenantId,
        posts: posts.items,
        stats: stats[0]
    });
}
```

---

## 3. The Fluent `ApexKit` / `$apex` Helper Class

ApexKit exposes `globalThis.ApexKit` and `globalThis.$apex` inside all script contexts. This provides a unified, object-oriented API that abstracts `$db` and `$root.db` routing:

### A. Scoped Usage (Default Context)

```typescript
// Webhook: ./webhooks/blog-service.ts
export default async function (req: Request) {
    // $apex uses the current scope by default
    const postsCol = $apex.collection("posts");

    const posts = await postsCol.list({ limit: 10 });
    const newPost = await postsCol.create({ title: "Fluent API Post" });

    return new Response({ posts: posts.items, created: newPost });
}
```

---

### B. Tenant Context Switching (Root Scripts Only)

When executing from the Root context, you can fluently switch between tenant or sandbox databases:

```typescript
// Root Script: Cross-Tenant Sync
export default async function (req: Request) {
    // Target tenant "client-a"
    const clientA = $apex.tenant("client-a");
    const clientB = $apex.tenant("client-b");

    // Fetch from client-a and write to client-b
    const sourcePosts = await clientA.collection("posts").list({ limit: 100 });

    for (const post of sourcePosts.items) {
        await clientB.collection("posts").create(post.data);
    }

    return new Response({ synced: sourcePosts.items.length });
}
```

---

## 4. Method Signatures & Reference

### Record Operations via `$apex.collection(name)`

| Method | Parameters | Returns | Description |
| :--- | :--- | :--- | :--- |
| **`list`** | `options?: QueryOptions` | `Promise<ListResult<Record>>` | Paginated listing with filtering and joins. |
| **`get`** | `id: number \| string, options?: { expand?: string }` | `Promise<Record \| null>` | Resolves a single record by primary key. |
| **`create`** | `data: Record<string, any>` | `Promise<{ id: number \| string }>` | Validates and inserts a new record. |
| **`update`** | `id: number \| string, data: Record<string, any>` | `Promise<Record>` | Partially updates and validates a record. |
| **`delete`** | `id: number \| string` | `Promise<boolean>` | Deletes record and cascades relationships. |
| **`searchVector`** | `field: string, vector: number[], limit?: number` | `Promise<(Record & { _score: number })[]>` | HNSW nearest-neighbor vector search. |
| **`getVector`** | `id: number \| string` | `Promise<VectorRecord[]>` | Retrieves raw stored embedding coordinates. |

---

## 5. Security & Isolation Summary

1. **Automatic Tenant Isolation:** Scripts executing in a tenant scope cannot access `$root.db` or invoke `$apex.tenant()`; doing so throws an immediate `Access Denied` error.
2. **Transaction Integrity:** Mutations executed via `$db.records.create` or `$db.records.update` pass through the atomic `WriteManager` WAL batcher, ensuring unique constraints, relation edges, and full-text indexes stay synchronized.
