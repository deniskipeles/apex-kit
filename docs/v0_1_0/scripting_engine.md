# ⚙️ Scripting Engine Documentation

**Version:** 0.1.0  
**Runtime:** QuickJS (`rquickjs`) with Oxc TypeScript/TSX Transpiler  
**Execution Model:** Sandboxed, Async-first with 10ms Quantum Preemptive CPU Scheduling

The ApexKit Scripting Engine allows developers to extend backend logic using JavaScript and TypeScript. It executes in-process with zero cold-start overhead, providing direct access to isolated databases, file storage, WebAssembly modules, background queues, and AI models.

---

## 1. Script & Webhook Structure

Every script must default-export an async function taking a standard Web API `Request` and returning a `Response`. You can write plain JavaScript, TypeScript, or use micro-frameworks like Hono:

```typescript
// Webhook: ./webhooks/hello-world.ts
export default async function (req: Request): Promise<Response> {
    const body = await req.json().catch(() => ({}));
    const name = body.name || "World";

    console.log(`Processing greeting for ${name}`);

    return new Response({
        message: `Hello, ${name}!`,
        timestamp: new Date().toISOString()
    }, {
        status: 200,
        headers: { "X-Powered-By": "ApexKit" }
    });
}
```

### Running with Hono Framework
```typescript
// Webhook: ./webhooks/api-router.ts
import { Hono } from "https://esm.sh/hono";

const app = new Hono();

app.get("/users", async (c) => {
    const users = await $db.users.create("user@test.com", "pass123", "user");
    return c.json(users);
});

app.post("/items", async (c) => {
    const body = await c.req.json();
    const item = await $db.records.create("items", body);
    return c.json(item, 201);
});

export default async function (req: Request) {
    return app.fetch(req);
}
```

---

## 2. Standard Web APIs in the Runtime

ApexKit's JavaScript runtime provides standard Web APIs out of the box:

* **`Request` & `Response`**: Supports `.json()`, `.text()`, `.arrayBuffer()`, and `.clone()`. Binary responses can return `ArrayBuffer` or `Uint8Array`.
* **`Headers`**: Case-insensitive HTTP header map.
* **`URL` & `URLSearchParams`**: Full URL parsing, query manipulation, and formatting.
* **`fetch(url, options)`**: Standard HTTP client with support for relative paths (auto-prefixed with active tenant URL) and timeouts.
* **`TextEncoder` & `TextDecoder`**: UTF-8 string encoding and decoding.
* **`crypto`**: `crypto.randomUUID()`, `crypto.getRandomValues()`, and `crypto.subtle` (`digest`, `importKey`, `sign`, `verify` for HMAC and SHA).
* **`btoa` & `atob`**: Base64 encoding and decoding helpers.
* **`setTimeout` & `clearTimeout`**: Non-blocking timer scheduling.

---

## 3. Global Built-in APIs Reference

### A. `$db` (Scoped Database Access)
Provides context-aware access to the active Tenant or Sandbox database.

```typescript
// Records API
const list = await $db.records.list("posts", { filter: { status: "published" }, limit: 10 });
const post = await $db.records.get("posts", 105, "author_id");
const { id } = await $db.records.create("posts", { title: "New Post", status: "published" });
const updated = await $db.records.update("posts", 105, { views: 45 });
await $db.records.delete("posts", 105);

// Search & Vectors
const vecMatches = await $db.records.searchVector("posts", "content", queryVec, 5);
const instantHits = await $db.records.instantSearch("posts", "sqlite", 5);
const coords = await $db.records.getVector("posts", 105);

// Analytical Query Engine
const stats = await $db.query({
    from: "sales",
    select: ["category", { fn: "sum", field: "amount", as: "total" }],
    group_by: ["category"]
});

// Users, Collections, Files
const user = await $db.users.get("admin@test.com");
const collections = await $db.collections.list();
const files = await $db.files.list(20, 0);
```

---

### B. `$files` (Storage & Asset Management)
Direct access to persistent storage (Local Filesystem or S3):

```typescript
// 1. Read binary file as Base64 string
const base64Str = await $files.read("avatar.jpg");

// 2. Save binary data (string, ArrayBuffer, or Uint8Array)
const saved = await $files.save("report.pdf", pdfBuffer, "application/pdf");
console.log(saved.id, saved.url, saved.filename);

// 3. Generate pre-signed URL (TTL in seconds)
const signedUrl = await $files.getSignedUrl(saved.filename, 3600);

// 4. Delete file
await $files.delete(saved.id);
```

---

### C. `$fs` (Virtual Filesystem Scratchpad)
Scoped filesystem for temporary files, data transformations, and WASI tool inputs/outputs:

```typescript
// Text files
await $fs.write("tmp.txt", "Hello world");
const content = await $fs.read("tmp.txt");

// Binary files (Base64 or ArrayBuffer/Uint8Array)
await $fs.writeBytes("output.bin", binaryData);
const base64Bytes = await $fs.readBytes("output.bin");

// Directory management
await $fs.mkdir("workspace");
const files = await $fs.list("workspace");
const stat = await $fs.stat("tmp.txt");
const exists = await $fs.exists("tmp.txt");
await $fs.delete("workspace");
```

---

### D. `$queue` (Background Task Orchestration)
Offload long-running operations from synchronous webhooks:

```typescript
// Spawn asynchronous task (up to 20 minutes CPU time)
const job = await $queue.spawn(async (pid, jobReq) => {
    const { items } = await jobReq.json();
    
    // Background execution logic...
    return { processed: items.length };
}, {
    timeoutMs: 300000, // 5 minutes
    args: { items: [1, 2, 3] }
});

// Inspect status or retrieve result
const status = await $queue.status(job.pid);
const result = await $queue.result(job.pid);
```

---

### E. `$wasm` (WebAssembly & WASI Engine)
Execute compiled WebAssembly binaries and single-threaded WASI CLI tools:

```typescript
// 1. Call pure WASM exported function
const result = await $wasm.call("add.wasm", "add", [10.5, 20.5]);

// 2. Execute standalone WASI CLI tool with preopened VFS
const run = await $wasm.runWasi("pdftotext.wasm", ["-layout", "input.pdf", "output.txt"], {
    memoryMb: 256,
    timeoutMs: 30000
});

console.log(run.success, run.stdout, run.stderr);
```

---

### F. `$ai` (Local & Cloud Embeddings)
Generate vector embeddings and calculate geometric similarity in memory:

```typescript
// Generate embedding vector
const vector = await $ai.embed("Vector database indexing in Rust");

// Geometric vector math
const similarity = $ai.cosineSimilarity(vecA, vecB);
const normalizedMean = $ai.meanVector([vecA, vecB, vecC]);
```

---

### G. `$cache` (In-Memory Key-Value Store)
Tenant-isolated cache for rate-limiting, counters, and ephemeral states:

```typescript
await $cache.set("session_123", JSON.stringify(sessionData), 300); // 5 min TTL
const data = await $cache.get("session_123");
const hits = await $cache.incr("page_views", 1);
await $cache.delete("session_123");
const keys = await $cache.listKeys();
```

---

### H. `$realtime` (Live Signaling)
Emit real-time WebSocket and SSE signals across custom channels:

```typescript
await $realtime.send("chat_room_1", "UserTyping", { user: "Alice" });
```

---

### I. `$mail` (Outbound SMTP Email)
Dispatch emails via configured SMTP transport:

```typescript
await $mail.send("customer@example.com", "Invoice Ready", "Your invoice is ready to download.");
```

---

### J. `$util` (Utilities & Crypto)
```typescript
const id = $util.uuid();
const slug = $util.slugify("Hello World!");
const hash = $util.hash("text", "sha256");
const hmac = $util.hmac("data", "secret_key");
const b64 = $util.base64Encode("binary");
const decoded = $util.base64Decode(b64);
const ab = $util.textEncode("UTF-8 string");
const str = $util.textDecode(ab);
const hex = $util.hexEncode(ab);
await $util.sleep(100);
```

---

### K. `$env` (Environment Secrets & URLs)
```typescript
const secret = await $env.get("STRIPE_SECRET_KEY");
const exists = await $env.has("API_KEY");
const allVars = await $env.list();

console.log($env.BASE_URL, $env.APP_URL, $env.LOCAL_BASE_URL, $env.LOCAL_APP_URL);
```

---

### L. `$run` (Inter-Script Execution)
Invoke other scripts and public root utilities:

```typescript
const response = await $run.script("payment-gateway", { amount: 100 });
```

---

### M. `$root` (Privileged Root Administration — *Root Scope Only*)
Available exclusively when executing in the Root App context to manage across tenant partitions:

```typescript
// Multi-Tenant Management
await $root.createTenant("client-beta", { tier: "pro" });
await $root.updateTenant("client-beta", { status: "suspended" });
await $root.deleteTenant("client-beta");
const usage = await $root.getTenantDiskUsage("client-beta");
const tenants = await $root.listTenants();

// Cross-Tenant Database Access
const clientPosts = await $root.db.records.list("tenant:client-beta", "posts", { limit: 10 });
```

---

### N. `$cmd` (Shell Process Execution — *Root Scope Only*)
Execute local binaries and shell tools with concurrency controls:

```typescript
// Synchronous run (waits for exit)
const output = await $cmd.run("ls", ["-lh", "storage/system"], { timeout: 5000 });

// Background spawn
const proc = await $cmd.spawn("git", ["pull"], { timeout: 30000 });
const status = await $cmd.status(proc.pid);
await $cmd.kill(proc.pid);

// Set concurrent process limits
await $cmd.setLimit("ffmpeg", 2);
```

---

### O. `console` (Audit Logging)
Commits structured log records directly to the `_system_logs` table:

```typescript
console.log("Informational log message");
console.info("Info log");
console.warn("Warning condition detected");
console.error("Critical error in webhook");
```

---

## 4. Execution Limits & Safety

| Parameter | Default Limit | Description |
| :--- | :--- | :--- |
| **Pure CPU Budget (Webhooks)** | `1,000` ms (`SCRIPT_MAX_CPU_MS`) | Maximum CPU execution time before preemptive abort. |
| **Pure CPU Budget (Background Jobs)** | `300,000` ms (up to 20 mins) | Allocated via `$queue.spawn(..., { timeoutMs })`. |
| **Wall-Clock Timeout** | `60` seconds (`SCRIPT_EXECUTION_TIMEOUT`) | Total allowed wall-clock duration for webhooks. |
| **Concurrent Webhooks per Scope** | `50` (`SCOPE_EXECUTION_LIMIT`) | Scoped semaphore protecting against thread exhaustion. |
| **Concurrent Fetch per Scope** | `5` (`SCOPE_HTTP_LIMIT`) | Prevents socket and file descriptor leaks. |
