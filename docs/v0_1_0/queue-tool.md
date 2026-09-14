# ApexKit `$queue` Developer Guide

The `$queue` API is ApexKit's built-in background job and task execution engine. It allows developers to offload long-running, CPU-intensive, or asynchronous operations from synchronous webhooks without blocking client requests or hitting webhook execution limits.

---

## 1. Why `$queue`?

| Feature | Standard Webhooks (`/api/v1/webhook/*`) | Background Queue (`$queue`) |
| :--- | :--- | :--- |
| **Max CPU Time** | **1 second** (`SCRIPT_MAX_CPU_MS`) | **Up to 20 minutes** (configurable) |
| **Concurrency Limit** | Enforces scoped semaphores (DDoS protection) | **Bypasses** the webhook semaphore |
| **Execution Model** | Blocks the HTTP connection until resolved | Spawns in the background; returns immediately |
| **CPU Scheduling** | Runs synchronously | Sliced into **10ms quantum chunks** |
| **Best For** | Fast CRUD operations, validation, routing | Heavy data processing, WASM, bulk migrations |

### The Quantum Scheduler
When you spawn a job using `$queue`, it does **not** lock up a system core or freeze the Tokio runtime. ApexKit's `QuantumScheduler` preempts the background task every **10ms**, yielding CPU time back to the operating system and allowing concurrent HTTP requests to be handled without latency spikes.

---

## 2. Global `$queue` API Reference

The `$queue` object is available globally in all server-side JavaScript/TypeScript scripts and webhooks.

### 1. `$queue.spawn(fnOrCode, options)`
Spawns an asynchronous job and returns an execution receipt.

```typescript
const job = await $queue.spawn(
    async (pid: string, req: Request) => {
        // Background logic
    },
    options?: {
        timeoutMs?: number,
        args?: any
    }
): Promise<{ pid: string, status: string }>;
```

#### Parameters
* **`fnOrCode`** (`Function` | `string`):
  * A function with the signature `async (pid, req) => { ... }`.
  * **Important:** Because the function is stringified and executed in an isolated runtime, **it cannot access variables outside its closure**. All inputs must be passed via `options.args`.
* **`options.timeoutMs`** (`number`, optional):
  * Maximum pure CPU execution time allocated to this job in milliseconds.
  * Defaults to `300,000` (5 minutes). Hard-capped by `QUEUE_JOB_MAX_TIMEOUT_MS` (default: 20 minutes).
* **`options.args`** (`any`, optional):
  * Data to pass to the task. Accessible inside the job by reading `await req.json()` or `req.args`.

#### Returns
```json
{
  "pid": "job_4df7d98b1a3b",
  "status": "queued"
}
```

---

### 2. `$queue.status(pid)`
Queries the current lifecycle state of a job.

```typescript
const status = await $queue.status(pid: string): Promise<{
    pid: string,
    status: "queued" | "running" | "completed" | "failed" | "timed_out" | "not_found",
    runtime_ms: number,
    error?: string | null
}>;
```

* **Tenant Isolation:** A tenant or sandbox can only inspect jobs spawned within its own context. Querying a `pid` from another tenant returns `"status": "not_found"`.

---

### 3. `$queue.result(pid)`
Fetches the terminal status and the return value produced by the job.

```typescript
const result = await $queue.result(pid: string): Promise<{
    pid: string,
    status: string,
    runtime_ms: number,
    result: any | null,
    error?: string | null
}>;
```

---

## 3. Practical Patterns & Examples

### Pattern 1: Fire-and-Forget Heavy Computation
Use this when an endpoint needs to accept an event, trigger an intense calculation, and respond instantly with `202 Accepted`.

```javascript
// Webhook: /api/v1/webhook/process-stats
export default async function (req) {
    const input = await req.json();

    // 1. Offload heavy computation to the background
    const job = await $queue.spawn(async (pid, jobReq) => {
        const payload = await jobReq.json();
        
        // Long-running CPU calculation
        let total = 0;
        for (let i = 0; i < payload.count; i++) {
            total += Math.sqrt(i);
        }

        // Save result directly to the database inside the job
        await $db.records.create("computations", {
            job_id: pid,
            total: total,
            finished_at: new Date().toISOString()
        });

        return { success: true, total };
    }, {
        timeoutMs: 600_000, // 10 minutes allocated
        args: { count: input.count || 10_000_000 }
    });

    // 2. Respond immediately to caller in ~10-15ms
    return new Response({
        message: "Job accepted and processing in background.",
        job_id: job.pid
    }, { status: 202 });
}
```

---

### Pattern 2: Client Polling Architecture
When frontend clients need to wait for a result, split the workflow into a **Spawn Webhook** and a **Poll Webhook**.

#### Step A: Spawn Endpoint (`POST /api/v1/webhook/export-start`)
```javascript
export default async function (req) {
    const body = await req.json();

    const job = await $queue.spawn(async (pid, jobReq) => {
        const { collectionName } = await jobReq.json();
        
        // Fetch up to 10,000 records
        const records = await $db.records.list(collectionName, { limit: 10000 });

        // Heavy CSV formatting
        let csv = "id,created,data\n";
        for (const r of records.items) {
            csv += `"${r.id}","${r.created}","${JSON.stringify(r.data).replace(/"/g, '""')}"\n`;
        }

        // Save as a storage file
        const file = await $files.save(`export_${Date.now()}.csv`, csv, "text/csv");

        return { download_url: file.url, total_records: records.items.length };
    }, {
        timeoutMs: 120_000,
        args: { collectionName: body.collection || "orders" }
    });

    return new Response({ job_id: job.pid }, { status: 202 });
}
```

#### Step B: Poll Endpoint (`GET /api/v1/webhook/export-status?pid=...`)
```javascript
export default async function (req) {
    const url = new URL(req.url);
    const pid = url.searchParams.get("pid");

    if (!pid) {
        return new Response({ error: "Missing 'pid' query parameter." }, { status: 400 });
    }

    const jobStatus = await $queue.status(pid);

    if (jobStatus.status === "completed") {
        const finalResult = await $queue.result(pid);
        return new Response({
            status: "completed",
            runtime_ms: jobStatus.runtime_ms,
            data: finalResult.result
        });
    }

    if (jobStatus.status === "failed" || jobStatus.status === "timed_out") {
        return new Response({
            status: jobStatus.status,
            error: jobStatus.error
        }, { status: 500 });
    }

    // Still queued or running
    return new Response({
        status: jobStatus.status,
        runtime_ms: jobStatus.runtime_ms
    }, { status: 200 });
}
```

---

### Pattern 3: Heavy WASM Processing
Because `$queue` provides minutes of CPU budget and bypasses connection semaphores, it is suitable for running WebAssembly binaries (like `ffmpeg`, image encoders, or transformers).

```javascript
// Webhook: /api/v1/webhook/transform-image
import init, { resize, PhotonImage } from "https://esm.sh/@silvia-odwyer/photon@0.3.3";

export default async function (req) {
    const { filename } = await req.json();

    const job = await $queue.spawn(async (pid, jobReq) => {
        const { targetFile } = await jobReq.json();

        // 1. Initialize WASM inside the background task
        await init("https://esm.sh/@silvia-odwyer/photon@0.3.3/es2022/photon_rs_bg.wasm");

        // 2. Read base64 image from persistent storage
        const b64 = await $files.read(targetFile);
        const inputBytes = new Uint8Array($util.base64DecodeBuffer(b64));

        // 3. Perform CPU-heavy WASM transformation
        const img = PhotonImage.new_from_byteslice(inputBytes);
        const resized = resize(img, 1920, 1080, 1);
        const outputBytes = resized.get_bytes_jpeg(85);

        // 4. Save transformed image back to disk
        const saved = await $files.save(
            `hd_${targetFile}`,
            $util.base64Encode(outputBytes),
            "image/jpeg"
        );

        return { transformed_url: saved.url };
    }, {
        timeoutMs: 180_000,
        args: { targetFile: filename }
    });

    return new Response({
        status: "processing",
        job_id: job.pid
    });
}
```

---

## 4. Quotas and Execution Limits

To prevent accidental resource exhaustion, `$queue` incorporates two independent protective layers:

```
+-------------------------------------------------------------------+
|                        1-Hour Sliding Window                       |
|   (Tracks cumulative CPU seconds across all jobs in scope)       |
|                                                                   |
|   Job 1: 5 min CPU  +  Job 2: 7 min CPU  <=  20 min Allowed/hr    |
+-------------------------------------------------------------------+
                                  │
                                  ▼
+-------------------------------------------------------------------+
|                      Single Job Ceiling                           |
|   (Caps maximum execution time of any individual task)           |
|                                                                   |
|   Job CPU Limit: Max 20 min  |  Wall-Clock Timeout: Max 40 min   |
+-------------------------------------------------------------------+
```

1. **Per-Job CPU Cap:** Prevents an infinite loop in a single job from running forever.
2. **Cumulative Scope Window:** Tracks total CPU consumption across a rolling window (e.g. 1 hour). If a tenant exceeds their aggregate quota, new `$queue.spawn()` calls are rejected with `QuotaExceeded` until the window slides forward.

### Environment Variable Reference

Configure these settings in your `.env` file to customize capacity:

| Variable | Default | Description |
| :--- | :--- | :--- |
| `QUEUE_JOB_MAX_TIMEOUT_MS` | `1200000` (20 mins) | Hard maximum CPU time an individual job can request. |
| `QUEUE_JOB_DEFAULT_TIMEOUT_MS` | `300000` (5 mins) | Default CPU time allocated if `options.timeoutMs` is omitted. |
| `QUEUE_WINDOW_SECS` | `3600` (1 hour) | Duration of the sliding tracking window. |
| `QUEUE_MAX_CPU_SECS_PER_WINDOW` | `1200` (20 mins) | Total cumulative CPU time allowed per scope inside the window. |
| `QUEUE_WALL_TIMEOUT_MULTIPLIER` | `2` | Multiplier applied to CPU time to calculate total wall-clock allowance (accounts for I/O and sleep). |

---

## 5. Developer Best Practices

1. **Do Not Close Over Outer Variables:**
   ```javascript
   // ❌ BAD: 'data' is not defined inside the worker
   const data = await req.json();
   await $queue.spawn(async (pid, jobReq) => {
       console.log(data); // ReferenceError
   });

   // ✅ GOOD: Pass inputs via the 'args' option
   const data = await req.json();
   await $queue.spawn(async (pid, jobReq) => {
       const args = await jobReq.json();
       console.log(args); // Works correctly
   }, { args: data });
   ```

2. **Handle Database & File Workflows Internally:**
   Background tasks have complete access to `$db`, `$files`, `$fs`, `$util`, `$mail`, and `$http`. Instead of passing huge data arrays over JSON parameters, save or reference records using database identifiers and allow the worker to load and mutate records directly.

3. **Check for Job Completion Gracefully:**
   When polling, use an exponential backoff strategy (e.g., 500ms, 1s, 2s) rather than tight polling loops to minimize database and CPU overhead.