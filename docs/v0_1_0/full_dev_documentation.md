# 📖 ApexKit Comprehensive Developer Documentation

**Version:** 0.1.0  
**System Architecture:** Rust (Axum) + SQLite (Rusqlite + JSONB) + QuickJS (with Oxc TypeScript/TSX Compiler) + Tantivy Full-Text Search + Candle/HNSW & ONNX Vector Engine + Tera SSR

---

## Table of Contents

1. [System Architecture](#1-system-architecture)
2. [Authentication, Scoping & Security Policies](#2-authentication-scoping--security-policies)
3. [Data Modeling & Collections](#3-data-modeling--collections)
4. [The Query Engine & Relational Expansion](#4-the-query-engine--relational-expansion)
5. [Server-Side Scripting & Edge Runtime](#5-server-side-scripting--edge-runtime)
6. [AI Native Features (Actions, Vectors, Search)](#6-ai-native-features)
7. [Real-Time Subscriptions (WebSocket & SSE)](#7-real-time-subscriptions)
8. [Storage & Resumable Uploads](#8-storage--resumable-uploads)
9. [Automated Scheduler & Cron Jobs](#9-automated-scheduler--cron-jobs)
10. [Replication (Master-Replica gRPC)](#10-replication)

---

## 1. System Architecture

ApexKit is a single-binary Backend-as-a-Service (BaaS) engineered for low latency, memory safety, and high-throughput data operations.

```
+-------------------------------------------------------------------------+
|                              Axum Web Layer                             |
|        REST APIs (/api/v1)  |  GraphQL (/graphql)  |  WebSockets (/ws)   |
+-------------------------------------------------------------------------+
                                     │
                                     ▼
+-------------------------------------------------------------------------+
|                  Auth & Multi-Tenant Scope Resolver                     |
|           Root Scope   |   Tenant Scopes   |   Sandbox Scopes           |
+-------------------------------------------------------------------------+
                                     │
        ┌────────────────────────────┼────────────────────────────┐
        ▼                            ▼                            ▼
+-------------------+      +--------------------+      +--------------------+
|  SQLite Storage   |      |  Tantivy Search    |      |  AI Vector Engine  |
|  - core.db        |      |  - Lucene-style    |      |  - HNSW index      |
|  - data.db (JSONB)|      |    stemming        |      |  - Candle & ONNX   |
|  - logs.db        |      |  - Multi-field     |      |    (Text/Vision)   |
|  - system.db      |      |    typo tolerance  |      |                    |
|  - vectors.db     |      +--------------------+      +--------------------+
|  - WriteManager   |                                             ▲
|    WAL Batcher    |                                             │
+-------------------+                                             │
        │                                                         │
        └────────────────────────────┬────────────────────────────┘
                                     ▼
+-------------------------------------------------------------------------+
|                      QuickJS Edge Scripting Engine                      |
|         TypeScript / TSX (Oxc Transpiler) | Virtual File System         |
|         QuantumScheduler (10ms Preemptive CPU Round-Robin)              |
|         $db, $files, $fs, $http, $wasm, $queue, $realtime, $ai          |
+-------------------------------------------------------------------------+
```

### Physical Multi-Tenancy
Rather than relying solely on logical row-level `tenant_id` WHERE clauses, ApexKit provides **physical, file-level database isolation**:
* **Root Scope:** `storage/system/` (`core.db`, `data.db`, `logs.db`, `system.db`, `vectors.db`)
* **Tenant Scope:** `storage/tenants/{tenant_id}/` (Dedicated database files, Tantivy indexes, and media uploads)
* **Sandbox Scope:** `storage/sandboxes/session_{id}/` (Ephemeral playground for testing, migrations, and AI Architect sessions)

---

## 2. Authentication, Scoping & Security Policies

### Authentication & Token Claims
Authentication uses JWT tokens containing signed scope claims:

```json
{
  "sub": "user@example.com",
  "uid": 101,
  "role": "admin",
  "scope": "tenant:client_alpha",
  "exp": 1799834800
}
```

### Scoped Contexts
* `root`: Full administrative privileges across global configuration, system metrics, and all tenant workspaces.
* `tenant:{id}`: Bounded to the designated customer workspace. Attempts to read other tenants fail with `403 Forbidden`.
* `sandbox:{id}`: Bounded to an ephemeral sandbox session.

### Security Policies & Row-Level Security (RLS)
Every collection defines four independent policy rules: `read`, `create`, `update`, `delete`.

#### String Shorthand Syntax
* `public`: Open access to all callers.
* `auth`: Caller must present a valid JWT or API Key.
* `admin`: Caller must have `role: "admin"`.
* `owner:{field_name}`: Compares the user's ID (`auth.id`) to the specified field on the record.

#### JSON-Based ABAC / RBAC Syntax
For deep relational checks and incoming payload validation:

```json
{
  "$or": [
    { "@request.auth.role": "admin" },
    {
      "$and": [
        { "author_id": "@request.auth.id" },
        { "@request.record.status": { "$neq": "published" } }
      ]
    }
  ]
}
```

* **SQL Pushdown:** Read policies compile directly into SQLite `WHERE` clauses for fast, index-accelerated pagination without in-memory filtering bottlenecks.
* **Pre-Run Payload Checks:** On `create` and `update`, rules validate client-submitted data before it commits.

---

## 3. Data Modeling & Collections

Collections structure your application records, maintain relationships, and configure indexing strategies.

### Supported Field Types
* **Primitives:** `string`, `text`, `number`, `boolean`, `email`, `url`, `date`, `select`, `json`
* **Media & Binary:** `file` (Managed upload reference), `blob` (Base64 data)
* **Geospatial:** `geopoint` (`{ "lat": number, "lng": number }`)
* **Relational:** `owner` (Links to system User ID, supports `auto: true`), `relation` (Links to target collection ID)
* **AI & Embeddings:** `vector` (Numeric array dimension), `vectorize: true` on text/file fields

### Indexing Strategies
* **`sql_indexed: true`**: Builds an automated SQLite B-Tree index on `json_extract(data, '$.field')`.
* **`ose_indexed: true`**: Adds field text to the embedded Tantivy full-text index with English stemming (`en_stem`).
* **`vectorize: true`**: Automatically generates and stores vector coordinates on record creation and mutation.

---

## 4. The Query Engine & Relational Expansion

### RESTful Records Endpoint
`GET /api/v1/collections/{id_or_name}/records`

#### Query Parameters
* `page`: Page number (default: `1`).
* `per_page`: Page size (default: `30`, max: `100`).
* `sort`: Comma-separated sort keys. Prefix with `-` for descending (e.g. `?sort=-created,title`).
* `filter`: URL-encoded JSON filter object.
* `expand`: Comma-separated relationship paths.

### Complex Filtering Syntax
Supports MongoDB-style operators: `$eq`, `$neq`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`, `$contains`, `$like`, `$and`, `$or`.

```json
{
  "$and": [
    { "status": "active" },
    { "price": { "$gte": 50, "$lte": 500 } },
    { "tags": { "$in": ["tech", "featured"] } }
  ]
}
```

### Relational Joins & Expansion (`expand`)
Traverse forward, reverse, and owner relationships in a single query:

```http
GET /api/v1/collections/posts/records?expand=author_id,comments(5,0).user_id
```

```json
{
  "id": 105,
  "data": {
    "title": "Getting Started with ApexKit",
    "author_id": 42
  },
  "expand": {
    "author_id": {
      "id": 42,
      "email": "author@example.com",
      "role": "user"
    },
    "comments": [
      {
        "id": 201,
        "data": { "body": "Great article!" },
        "expand": {
          "user_id": { "id": 88, "email": "reader@example.com" }
        }
      }
    ]
  }
}
```

### Advanced SQL Query Engine
`POST /api/v1/collections/{id}/query`

```json
{
  "select": [
    "category",
    { "fn": "sum", "field": "price", "as": "total_revenue" },
    { "fn": "avg", "field": "price", "as": "avg_ticket" },
    { "fn": "count", "field": "id", "as": "order_count" }
  ],
  "where": { "status": "completed" },
  "group_by": ["category"],
  "sort": "-total_revenue"
}
```

---

## 5. Server-Side Scripting & Edge Runtime

ApexKit integrates **QuickJS** (`rquickjs`) with the **Oxc TypeScript/TSX transpiler** to provide a fast, sandboxed JavaScript runtime with native Web APIs (`fetch`, `Request`, `Response`, `Headers`, `URL`, `crypto`).

### Execution & Preemptive Scheduling
* **Quantum Scheduler:** Long-running tasks are preempted every **10ms** to prevent CPU starvation and ensure HTTP handlers remain responsive.
* **Scoped Semaphores:** Webhook endpoints enforce concurrent execution limits to protect system resources.

### Global Built-in APIs

| Global | Purpose | Example Method |
| :--- | :--- | :--- |
| **`$db`** | Scoped Database Access | `await $db.records.list("posts", { filter: { active: true } })` |
| **`$files`** | Storage & Signed URLs | `await $files.save("report.pdf", data, "application/pdf")` |
| **`$fs`** | Scoped Virtual Filesystem | `await $fs.write("config.json", JSON.stringify(data))` |
| **`$http` / `fetch`** | External HTTP Requests | `await fetch("https://api.stripe.com/v1/charges")` |
| **`$queue`** | Background Task Spawner | `await $queue.spawn(async (pid, req) => { ... }, { timeoutMs: 300000 })` |
| **`$wasm`** | WASM & WASI Execution | `await $wasm.runWasi("pdftotext.wasm", ["input.pdf", "output.txt"])` |
| **`$ai`** | Embeddings & Math | `await $ai.embed("Query text")` |
| **`$realtime`** | Custom Signal Dispatch | `await $realtime.send("chat_room", "NewMsg", { text: "Hello" })` |
| **`$cache`** | In-Memory Key-Value Store | `await $cache.incr("rate_limit_key", 1)` |
| **`$util`** | Crypto & String Utilities | `$util.uuid()`, `$util.slugify(text)`, `$util.hash(data, "sha256")` |
| **`$mail`** | SMTP Outbound Email | `await $mail.send("user@test.com", "Subject", "Body text")` |
| **`$cmd`** | Shell Execution (*Root Only*)| `await $cmd.run("ffmpeg", ["-i", "input.mp4", "output.mp4"])` |

### Script Webhook Example

```typescript
// Webhook: ./webhooks/order-processor.ts
import { Hono } from "https://esm.sh/hono";

const app = new Hono();

app.post("/checkout", async (c) => {
    const { items, customer_id } = await c.req.json();
    
    // Calculate total and write to database
    let total = items.reduce((acc: number, item: any) => acc + item.price * item.quantity, 0);

    const { id: orderId } = await $db.records.create("orders", {
        customer_id,
        items,
        total,
        status: "pending"
    });

    // Spawn background task for payment processing
    await $queue.spawn(async (pid, req) => {
        const { order_id, amount } = await req.json();
        // Payment processing logic...
    }, { args: { order_id: orderId, amount: total } });

    return c.json({ success: true, order_id: orderId });
});

export default async function (req: Request) {
    return app.fetch(req);
}
```

---

## 6. AI Native Features

### A. Vector Embeddings & Similarity Search
* **Backbones Supported:**
  * Text: BGE Small/Base, GTE Small, EmbeddingGemma, Qwen3-Embedding (Candle & ONNX Runtime).
  * Vision: SigLIP, SigLIP2, CLIP, OpenCLIP, DINOv2.
* **Endpoints:**
  * `POST /api/v1/collections/{id}/search-vector-with-text`: Text-to-text semantic matching.
  * `POST /api/v1/collections/{id}/search-image-vector-with-image`: Image-to-image similarity.
  * `POST /api/v1/collections/{id}/search-image-vector-with-text`: Text-to-image cross-modal search.

### B. AI Actions (Prompt Templates)
* **Endpoint:** `POST /api/v1/ai/run/{slug}`
* **Capabilities:** Supports Google Gemini, Groq, and OpenAI with encrypted API keys, variable substitution, vision attachments, search grounding, and SSE token streaming.

---

## 7. Real-Time Subscriptions

### WebSockets (`ws://host/ws` or `/tenant/{id}/ws`)
Supports database lifecycle mutations, custom ephemeral signaling, and low-latency instant search over an open socket.

```typescript
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

const realtime = new ApexKitRealtimeWSClient("https://api.your-app.com", token);
realtime.connect();

// Subscribe to database changes
realtime.subscribe({
  collectionId: 5,
  eventType: "Insert",
  dataFilter: { priority: "urgent" }
});

// Custom signaling channel
realtime.subscribe({ channel: "room_101" });
realtime.sendSignal("room_101", "UserTyping", { username: "Alice" });

realtime.onEvent((msg) => {
  console.log("Realtime event:", msg);
});
```

### Server-Sent Events (`GET /sse?channel=...`)
Read-only event stream for push notifications, metrics, and live dashboards.

---

## 8. Storage & Resumable Uploads

### Storage Drivers
* **Local Filesystem:** Stored under `storage/{scope}/uploads/` with on-the-fly thumbnail generation and caching.
* **S3-Compatible Cloud Storage:** Direct integration with AWS S3, Cloudflare R2, and MinIO with signed URL generation.

### Image Optimization & OpenGraph
* **Dynamic Transformations:** `GET /storage/file/{filename}?thumb=400x300&format=webp&quality=85&blur=2`
* **Dynamic OpenGraph Render:** `GET /storage/files/opengraph?template=default&data=[...]`

### Resumable Uploads (Tus 1.0.0)
Upload multi-gigabyte files with chunking, pause, resume, and integrity validation via `POST /storage/upload/tus`.

---

## 9. Automated Scheduler & Cron Jobs

* **Multi-Tenant Scheduling:** Runs scheduled scripts or loopback webhooks with 6-part cron expressions (`0 */15 * * * *`).
* **Master-Only Execution:** If running as a replica (`APEXKIT_MASTER_URL`), the scheduler is locked to prevent duplicate job executions across nodes.
* **Fast-Fail Minute Locks:** Prevents duplicate runs using distributed minute cache stamps.

---

## 10. Replication

For high-traffic deployments, ApexKit supports horizontal read scaling via gRPC:

```
[Write Request] ──> [Replica Node] ──(gRPC ExecuteWrite)──> [Master Node]
                                                                    │
[Read Request]  <── [Replica Node] <──(gRPC StreamEvents)───────────┘
```

* **Snapshots:** Replicas bootstrap by downloading full SQLite database snapshots from the Master.
* **Changeset Streaming:** SQLite WAL session changesets stream binary diffs to replicas in real-time.
* **Write Forwarding:** Replicas forward write operations to the Master over gRPC, maintaining strong write consistency and low-latency local reads.
