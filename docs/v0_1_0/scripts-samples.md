# 📚 Script Samples Library

**Version:** 0.1.0  
**Context:** Server-Side TypeScript / JavaScript (QuickJS Engine with Oxc Transpilation)

This library provides production-ready, copy-pasteable script samples demonstrating how to leverage ApexKit's built-in global built-ins (`$db`, `$files`, `$fs`, `$queue`, `$wasm`, `$ai`, `$cache`, `$realtime`, `$mail`, `$util`, `$env`, and `$cmd`).

---

### 1. Manual Webhooks & API Endpoints
*Trigger: `manual` | Access: `POST /api/v1/webhook/{name}` or `/api/v1/run/{name}`*

#### A. Analytical Sales Report (Query Engine)
Executes complex SQL grouping and aggregation pipeline over collection data.

```typescript
// Script Name: sales-performance-report
// Trigger: manual | Path: ./webhooks/sales-performance-report.ts

export default async function (req: Request) {
    const body = await req.json().catch(() => ({}));
    const category = body.category;

    const report = await $db.query({
        from: "sales",
        select: [
            "region",
            { fn: "sum", field: "amount", as: "total_revenue" },
            { fn: "count", field: "id", as: "order_count" },
            { fn: "avg", field: "amount", as: "avg_ticket" }
        ],
        where: category ? { category } : {},
        group_by: ["region"],
        sort: "-total_revenue"
    });

    return new Response(JSON.stringify({
        generated_at: new Date().toISOString(),
        regions: report
    }), {
        headers: { "Content-Type": "application/json" }
    });
}
```

#### B. Multi-File Asset Bundler & Zipper
Reads attachments from scoped storage, bundles them into an in-memory ZIP archive, saves the resulting archive back to disk, and registers its metadata.

```typescript
// Script Name: bundle-attachments-zip
// Trigger: manual | Path: ./webhooks/bundle-attachments-zip.ts

export default async function (req: Request) {
    const { folder_name } = await req.json();
    
    // 1. Fetch file metadata records matching folder
    const files = await $db.records.list("attachments", {
        filter: { folder: folder_name }
    });
    
    const zipMap: Record<string, string> = {};
    for (const file of files.items) {
        // $files.read returns Base64 data from storage (Local or S3)
        const b64 = await $files.read(file.data.filename);
        zipMap[file.data.original_name] = b64;
    }

    // 2. Create archive and save to storage
    const zipBase64 = await $zip.create(zipMap);
    const saved = await $files.save(
        `${folder_name}_export.zip`, 
        $util.base64DecodeBuffer(zipBase64), 
        "application/zip"
    );

    return new Response(JSON.stringify({
        message: "Archive bundle created successfully",
        download_url: saved.url,
        filename: saved.filename
    }), {
        headers: { "Content-Type": "application/json" }
    });
}
```

---

### 2. Shared System Logic (Root Public Functions)
*Trigger: `manual` | Visibility: `public` | Context: Created in Root App*

#### FFmpeg Video Transcoder ($cmd)
A Root-level public utility script that allows tenants to invoke native server-side shell commands (FFmpeg) safely.

```typescript
// Script Name: system-ffmpeg-transcoder
// Trigger: manual | Visibility: public | Path: ./webhooks/system-ffmpeg-transcoder.ts

export default async function (req: Request) {
    const body = await req.json();
    const { input_file, output_name } = body;
    const callerScope = body.__caller_scope;

    // Ensure only tenants invoke this shared service
    if (!callerScope || !callerScope.Tenant) {
        return new Response(JSON.stringify({ error: "Access Denied: Tenants only" }), { status: 403 });
    }

    const tenantId = callerScope.Tenant;
    const inputPath = `storage/tenants/${tenantId}/uploads/${input_file}`;
    const outputPath = `storage/tenants/${tenantId}/uploads/${output_name}.mp4`;

    // Execute shell binary via Root-scoped $cmd
    const result = await $cmd.run("ffmpeg", [
        "-y",
        "-i", inputPath,
        "-vf", "scale=1280:-1",
        "-c:v", "libx264",
        "-crf", "23",
        outputPath
    ], { timeout: 60000 });

    if (result.status !== 0) {
        return new Response(JSON.stringify({
            success: false,
            error: "Transcoding failed",
            stderr: result.stderr
        }), { status: 500, headers: { "Content-Type": "application/json" } });
    }

    return new Response(JSON.stringify({
        success: true,
        output_file: `${output_name}.mp4`
    }), {
        headers: { "Content-Type": "application/json" }
    });
}
```

---

### 3. Database Event Hooks
*Trigger: `before_create_record`, `after_create_record`*

#### A. Dynamic Row-Level Security Enforcer
Intercepts `list` requests and injects tenant-specific ownership filters dynamically:

```typescript
// Script Name: enforce-project-ownership
// Trigger: before_list_records | Target Collection: projects
// Path: ./webhooks/enforce-project-ownership.ts

export default async function (event: any) {
    // Admins bypass scoping rules
    if (event.auth && event.auth.role === "admin") {
        return event.data;
    }

    const filter = event.data.filter ? JSON.parse(event.data.filter) : {};
    
    // Force constraint: Users can only query their own organization projects
    filter.owner_id = event.auth ? event.auth.id : -1;
    
    event.data.filter = JSON.stringify(filter);
    return event.data;
}
```

#### B. Asynchronous Webhook Notification
Dispatches an external Slack webhook after a record is successfully created:

```typescript
// Script Name: slack-notify-new-lead
// Trigger: after_create_record | Target Collection: leads
// Path: ./webhooks/slack-notify-new-lead.ts

export default async function (event: any) {
    const { id, data } = event.record;
    const webhookUrl = await $env.get("SLACK_WEBHOOK_URL");

    if (!webhookUrl) return;

    await fetch(webhookUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            text: `🚀 *New Lead Captured!*\n• Email: ${data.email}\n• Source: ${data.source}\n• Record ID: ${id}`
        })
    });
}
```

---

### 4. Traffic & Quota Management
*Trigger: `before_tenant_request` | Context: Root level*

#### Atomic Rate Limiter
Prevents API abuse by counting incoming requests per IP address in memory:

```typescript
// Script Name: global-ip-ratelimiter
// Trigger: before_tenant_request | Path: ./webhooks/global-ip-ratelimiter.ts

export default async function (event: any) {
    const ip = event.data.ip || "unknown";
    const minuteBucket = new Date().toISOString().slice(0, 16);
    const cacheKey = `ratelimit:${ip}:${minuteBucket}`;

    const hits = await $cache.incr(cacheKey, 1);

    if (hits > 100) {
        throw new Error("Rate limit exceeded. Maximum 100 requests per minute permitted.");
    }
}
```

---

### 5. AI & Vector Search Integration
*Trigger: `manual`*

#### Semantic Knowledge Base Search
Generates a vector embedding for an incoming query and performs an HNSW vector search against stored document embeddings:

```typescript
// Script Name: semantic-search
// Trigger: manual | Path: ./webhooks/semantic-search.ts

export default async function (req: Request) {
    const { query } = await req.json();

    if (!query) {
        return new Response(JSON.stringify({ error: "Missing query text" }), { status: 400 });
    }

    // 1. Generate text embedding vector
    const queryVector = await $ai.embed(query);

    // 2. Search collection vector field
    const matches = await $db.records.searchVector(
        "knowledge_base",
        "content_embedding",
        queryVector,
        5
    );

    return new Response(JSON.stringify({
        query,
        matches: matches.map(m => ({
            id: m.id,
            title: m.data.title,
            relevance_score: m._score
        }))
    }), {
        headers: { "Content-Type": "application/json" }
    });
}
```
