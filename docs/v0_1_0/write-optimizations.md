# ⚡ Database Performance, WAL & Write Optimizations

**Version:** 0.1.0  
**Context:** Infrastructure Tuning, Concurrency Management, and System Configuration

ApexKit is architected for high-performance data ingestion. It leverages a concurrent SQLite storage engine running in **Write-Ahead Logging (WAL)** mode, paired with an asynchronous **Write Manager** that buffers and groups individual operations into atomic batches to eliminate disk I/O bottlenecks.

---

## 1. The Write Manager (Asynchronous Batching)

Traditional single-binary databases lock the entire file during write operations, creating severe concurrency bottlenecks under heavy load. ApexKit bypasses this limitation using an asynchronous channel batcher (`WriteManager`).

### How It Works:
1. **Queueing:** Incoming write requests (`INSERT`, `UPDATE`, `DELETE`) are pushed into a high-capacity asynchronous channel buffer (`100,000` capacity).
2. **Batching & Draining:** A background worker collects requests until either:
   * The batch size limit is reached (`DB_BATCH_SIZE`).
   * The flush time interval elapses (`DB_FLUSH_MS`).
3. **Atomic Transaction:** The collected operations are executed inside a single SQLite transaction block (`BEGIN IMMEDIATE ... COMMIT`).
4. **Changeset Extraction:** Upon a successful commit, SQLite's binary session extension extracts the WAL changeset delta, broadcasting it instantly to connected replicas over gRPC.

### Tunable Environment Variables
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`DB_BATCH_SIZE`** | `2000` | Maximum number of SQL statements grouped into a single transaction block. |
| **`DB_FLUSH_MS`** | `50` | Maximum milliseconds to wait for a batch to fill before forcing a commit. |

---

## 2. Performance Tuning Scenarios

### Scenario A: High-Throughput Ingestion (NVMe / Dedicated SSD)
If you are running bulk data imports or handling heavy traffic on fast storage:
```env
DB_BATCH_SIZE=5000
DB_FLUSH_MS=10
```
*Result: Grouping more writes reduces expensive disk `fsync` calls, maximizing IOPS throughput.*

### Scenario B: Low-Latency Real-Time Applications
If your application requires data mutations to appear instantly in search indexes and lists:
```env
DB_BATCH_SIZE=100
DB_FLUSH_MS=5
```
*Result: Commits happen more frequently, minimizing the time data spends in memory before hitting disk.*

---

## 3. SQLite Pragmas & Memory-Mapped I/O

On startup, ApexKit automatically applies optimized PRAGMA configurations for every database connection (Core, Data, Logs, System, and Vectors):

* **`PRAGMA journal_mode = WAL;`**: Allows multiple simultaneous readers to query the database without blocking an active write transaction.
* **`PRAGMA synchronous = NORMAL;`**: Reduces unnecessary disk synchronizations while maintaining crash safety in WAL mode.
* **`PRAGMA temp_store = MEMORY;`**: Forces temporary tables and indices to be built and stored entirely in RAM.
* **`PRAGMA cache_size = -64000;`**: Allocates ~64MB of RAM per database connection for page caching.
* **`PRAGMA mmap_size = 30000000000;`**: Enables memory-mapped I/O up to 30GB, allowing OS page caches to handle reads directly with zero-copy speed.

---

## 4. System Environment Variables Reference

### A. Security & Core Infrastructure
| Variable | Required | Description |
| :--- | :--- | :--- |
| **`APEXKIT_MASTER_KEY`** | **Yes** | 32-byte Base64 secret key used to encrypt sensitive configuration values (S3 credentials, AI keys) in the database via AES-256-GCM. |
| **`APEXKIT_ROOT_DOMAIN`** | No | Base domain for multi-tenant subdomain routing (e.g., `mycompany.com`). |
| **`APEXKIT_MASTER_URL`** | No | If defined, runs the node in **Replica Mode**, forwarding all writes to the Master node over gRPC. |

### B. Runtime & Caching
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`PORT`** | `5000` | Port the Axum server listens on. |
| **`CACHE_TTL`** | `300` | Default Time-To-Live (seconds) for `$cache` entries. |
| **`SCRIPT_EXECUTION_TIMEOUT`** | `60` | Maximum wall-clock lifetime (seconds) for serverless scripts. |
| **`SCRIPT_MAX_CPU_MS`** | `1000` | Pure CPU budget (milliseconds) enforced by the Quantum Scheduler per script run. |
| **`APP_ENV`** | `development` | Setting to `production` disables GraphQL introspection and verbose error traces. |

### C. Storage & Archive Limits
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`FILE_UPLOAD_LIMIT`** | `10` | Maximum multipart file upload size in megabytes. |
| **`ARCHIVE_LIMIT`** | `10` | Maximum size in megabytes for `$zip` operations and site deployments. |
| **`APEXKIT_TMP_DIR`** | System Temp | Ephemeral storage scratchpad path for WASI binaries and temporary uploads. |

---

## 5. Production Environment File Example (`.env`)

```bash
# Security & Cluster Identity
APEXKIT_MASTER_KEY="your_base64_encoded_32_byte_secret_key"
APEXKIT_ROOT_DOMAIN="api.mycompany.com"

# Write Batching Tuning
DB_BATCH_SIZE=2000
DB_FLUSH_MS=25

# Runtime Limits
PORT=5000
APP_ENV="production"
SCRIPT_EXECUTION_TIMEOUT=30
SCRIPT_MAX_CPU_MS=1000

# Cache & Storage
CACHE_TTL=600
FILE_UPLOAD_LIMIT=50
ARCHIVE_LIMIT=100
```