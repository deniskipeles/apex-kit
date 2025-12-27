
# Optimizing ApexKit Database Performance

ApexKit uses a specialized **Write Manager** (`batching.rs`) to handle SQLite's single-writer concurrency model. Instead of locking the database for every single insert (which kills performance), ApexKit buffers writes into memory and commits them in bulk transactions.

You can tune this behavior via environment variables to match your hardware infrastructure.

## Configuration Variables

### 1. `DB_BATCH_SIZE`
**Description:** Determines the maximum number of SQL write operations (inserts/updates/deletes) to buffer in memory before forcing a database commit.
*   **Default:** `1000`
*   **Type:** Integer

### 2. `DB_FLUSH_MS`
**Description:** The maximum time (in milliseconds) the system waits for a batch to fill up. If this time passes and the buffer isn't full, it commits whatever data is currently pending.
*   **Default:** `10`
*   **Type:** Integer (Milliseconds)

---

## Performance Tuning Guide

The optimal settings depend entirely on your hardware constraints (CPU Cores and Disk IOPS).

### Scenario A: The "Beast" (Dedicated Bare Metal / NVMe)
*   **Target:** Hetzner Dedicated (Ryzen/Threadripper), AWS i3en.metal.
*   **Goal:** Maximum throughput. You have abundant CPU cycles and fast NVMe storage. You want to minimize disk `fsync` calls by grouping as many writes as possible, and you want near-zero latency.

```env
# Allow large bursts of traffic (e.g., bulk imports) to sit in RAM
DB_BATCH_SIZE=5000

# Flush rapidly. Your CPU can handle waking up 500 times a second.
DB_FLUSH_MS=2
```
**Result:** Capable of 10,000+ writes/second.

### Scenario B: The "Budget" (Shared vCPU / DigitalOcean Basic)
*   **Target:** AWS t3.micro, DigitalOcean $5 Droplet, Raspberry Pi.
*   **Goal:** Stability and CPU efficiency.
*   **Risk:** If `DB_FLUSH_MS` is too low on a shared CPU, the background thread wakes up constantly, consuming your "CPU Credits" or causing the OS to throttle your application, even if no data is being written.

```env
# Keep memory usage low
DB_BATCH_SIZE=500

# Relax the timer. Check for writes only 20 times a second.
# This saves significant CPU cycles for your API logic.
DB_FLUSH_MS=50
```
**Result:** Lower CPU usage, slightly higher latency (50ms) for data to appear in search results, preventing system freezes.

### Scenario C: General Production (Default)
*   **Target:** Standard VPS (2 vCPU, 4GB RAM).
*   **Goal:** Balance between latency and throughput.

```env
DB_BATCH_SIZE=1000
DB_FLUSH_MS=10
```

---

## The Technical "Why"

### Why Batch Size Matters (The `fsync` Bottleneck)
SQLite in WAL (Write-Ahead Log) mode allows many readers but only **one writer**.
Every time a transaction commits, the operating system must perform an `fsync` to physically write data to the disk platter/NAND to ensure durability.
*   **Without Batching:** 1,000 requests = 1,000 `fsync` operations. This is slow (approx. 50-100 req/sec on HDD).
*   **With Batching:** 1,000 requests = 1 transaction = 1 `fsync` operation. This is incredibly fast.

Increasing `DB_BATCH_SIZE` reduces the number of times we hit the disk, trading RAM usage for write speed.

### Why Flush Time Matters (Context Switching)
The Write Manager runs an infinite loop in a background thread.
*   **High Frequency (2ms):** The thread wakes up, checks the channel, and sleeps. This ensures writes happen almost instantly but forces the CPU to "context switch" constantly. On a shared vCPU, this looks like high usage to the hypervisor.
*   **Low Frequency (50ms):** The thread sleeps longer. Writes might sit in RAM for 50ms before being saved. This is imperceptible to humans but frees up the CPU to handle incoming HTTP requests (Axum/Tokio tasks).