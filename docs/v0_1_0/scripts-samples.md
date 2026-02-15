# 📚 Script Samples Library

**Version:** 0.1.0
**Context:** JavaScript Server Runtime (Boa)

This library provides copy-pasteable examples for common tasks in ApexKit, demonstrating the power of scoped database access, in-memory archiving, and shared Root-to-Tenant logic.

---

### 1. Manual API Endpoints
*Trigger: `manual` | Access: `POST /api/v1/run/{name}`*

#### Sales Performance Report (Analytical Engine)
Uses the `$db.query` engine to aggregate data directly in SQL.

```javascript
export default async function(req) {
    const { category } = await req.json();

    const report = await $db.query(null, {
        "from": "sales",
        "select": [
            "region",
            { "fn": "sum", "field": "amount", "as": "total_revenue" },
            { "fn": "count", "field": "id", "as": "order_count" },
            { "fn": "avg", "field": "amount", "as": "avg_ticket" }
        ],
        "where": category ? { "category": category } : {},
        "group_by": ["region"],
        "sort": "-total_revenue"
    });

    return new Response({
        generated_at: new Date().toISOString(),
        regions: report
    });
}
```

#### Multi-File Asset Bundler
Reads binary files from the current scope's storage and creates a ZIP archive.

```javascript
export default async function(req) {
    const { folder_name } = await req.json();
    
    // 1. Fetch metadata for files in this "folder"
    const files = await $db.find("attachments", { folder: folder_name });
    
    const zipMap = {};
    for (const file of files) {
        // readFile returns Base64 from the scoped storage (Local or S3)
        const b64 = await $zip.readFile(file.filename);
        zipMap[file.original_name] = b64;
    }

    // 2. Create ZIP and save back to storage
    const zipB64 = await $zip.create(zipMap);
    const saved = await $zip.saveFile(`${folder_name}_export.zip`, zipB64);

    return new Response({
        message: "Bundle created",
        download_url: saved.url,
        size_bytes: saved.size
    });
}
```

---

### 2. Shared System Logic (Root Functions)
*Trigger: `manual` | Visibility: `public` | Context: Created in Root App*

#### FFmpeg Video Processor
Demonstrates how a Root script uses `$cmd` to provide heavy processing to Tenants.

```javascript
// Root Script Name: "system-ffmpeg"
export default async function(req) {
    const { input_url, output_name } = await req.json();
    const caller = req.body.__caller_scope;

    if (!caller.Tenant) throw new Error("Tenants only");

    // Root can execute shell commands
    const result = await $cmd.run("ffmpeg", [
        "-i", input_url,
        "-vf", "scale=1280:-1",
        "-c:v", "libx264",
        "-crf", "23",
        output_name
    ], { timeout: 60000 });

    return new Response({
        status: result.status === 0 ? "success" : "failed",
        logs: result.stderr
    });
}
```

---

### 3. Database Event Hooks
*Trigger: `before_create`, `after_list_records`, etc.*

#### Dynamic Row-Level Security (Filter Hook)
Automatically restricts a `list` request to only show records owned by the user.

```javascript
// Trigger: before_list_records | Target: "projects"
export default async function(e) {
    // Skip for admins
    if (e.auth.role === 'admin') return e.data;

    // Parse existing filter or start new
    const filter = e.data.filter ? JSON.parse(e.data.filter) : {};
    
    // Inject ownership constraint
    filter.owner_id = e.auth.id;
    
    // Update the query options
    e.data.filter = JSON.stringify(filter);
    
    return e.data;
}
```

#### Slack Notification (Side Effect)
Triggers an external webhook after a record is successfully saved.

```javascript
// Trigger: after_create | Target: "leads"
export default async function(e) {
    const webhook = await $env.get("SLACK_WEBHOOK_URL");
    
    const message = {
        text: `🚀 *New Lead:* ${e.record.data.email}\nSource: ${e.record.data.source}`
    };

    await fetch(webhook, {
        method: "POST",
        body: JSON.stringify(message)
    });
}
```

---

### 4. Traffic & Quota Management
*Trigger: `before_tenant_request` | Context: Root level*

#### Atomic Rate Limiter
Prevents API abuse by tracking requests per IP in the system cache.

```javascript
export default async function(e) {
    const ip = e.data.ip;
    const window = new Date().toISOString().slice(0, 16); // Minute resolution
    const cacheKey = `rate:${ip}:${window}`;

    // Increment atomically
    const count = await $cache.incr(cacheKey, 1);

    if (count > 60) {
        throw new Error("Rate limit exceeded. Try again in a minute.");
    }
}
```

---

### 5. AI & Vector Search
*Trigger: `manual`*

#### Semantic Knowledge Base Search
Converts a query to a vector and searches the HNSW index.

```javascript
export default async function(req) {
    const { q } = await req.json();

    // 1. Generate Embedding using the scoped AI provider
    const vector = await $ai.embed(q);

    // 2. Search specific collection vector field
    const matches = await $db.records.searchVector(
        "knowledge_base", 
        "content_vec", 
        vector, 
        5
    );

    return new Response({
        query: q,
        results: matches.map(m => ({
            id: m.id,
            title: m.data.title,
            relevance: m._score
        }))
    });
}
```