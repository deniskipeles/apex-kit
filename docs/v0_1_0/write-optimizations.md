# ⚡ Database Performance & Environment Variables

**Version:** 0.1.0
**Context:** Infrastructure Tuning and System Configuration

ApexKit is designed for high-performance data ingestion. It uses a specialized **Write Manager** to overcome the single-writer limitation of SQLite/LibSQL, grouping individual operations into atomic batches to minimize disk I/O overhead.

---

## 1. The Write Manager (Batching)

Unlike standard frameworks that lock the database for every single `INSERT` or `UPDATE`, ApexKit buffers writes in memory.

### How it Works:
1.  **Draining**: Incoming write requests are pushed into a high-capacity async channel.
2.  **Batching**: A background worker collects requests until a specific **Size** is reached OR a **Time** interval passes.
3.  **Transaction**: All collected operations are executed within a single `BEGIN IMMEDIATE ... COMMIT` block.

### Key Tuning Variables:
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`DB_BATCH_SIZE`** | `2000` | Maximum number of SQL operations to group in one transaction. |
| **`DB_FLUSH_MS`** | `50` | Maximum time to wait for a batch to fill before committing anyway. |

---

## 2. Performance Tuning Scenarios

### Scenario A: High Throughput (Dedicated Server / NVMe)
If you are running bulk imports or a high-traffic app on fast storage.
```env
DB_BATCH_SIZE=5000
DB_FLUSH_MS=10
```
*Result: Grouping more writes reduces `fsync` calls, maximizing disk throughput.*

### Scenario B: Low Latency / Real-Time
If you need data to appear in search results or lists almost instantly.
```env
DB_BATCH_SIZE=100
DB_FLUSH_MS=5
```
*Result: Commits happen more frequently, reducing the time data spends in memory.*

---

## 3. System Environment Variables

ApexKit uses environment variables for core security and infrastructure settings.

### 🔐 Security & Identity
| Variable | Required | Description |
| :--- | :--- | :--- |
| **`APEXKIT_MASTER_KEY`** | **Yes** | 32-byte Base64 string used to encrypt secrets (S3 keys, AI keys) in the DB. |
| **`APEX_ROOT_DOMAIN`** | No | The base domain for multi-tenant routing (e.g., `myapp.com`). |

### 🤖 AI & Search
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`APEX_VECTOR_MODEL`** | `all-minilm-l6-v2` | The local model used for embeddings. Options: `bge-small`, `bge-base`, `gte-small`. |
| **`ARCHIVE_LIMIT`** | `10` | Maximum size in MB for `$zip` operations and site deployments. |

### ⚙️ Runtime & Cache
| Variable | Default | Description |
| :--- | :--- | :--- |
| **`PORT`** | `5000` | The port the server listens on. |
| **`CACHE_TTL`** | `300` | Default Time-To-Live (seconds) for `$cache` entries. |
| **`APP_ENV`** | `development` | Setting to `production` disables GraphQL introspection and verbose error logs. |

---

## 4. Database Optimization (Pragmas)

ApexKit automatically applies these SQLite pragmas on startup for every tenant database:

*   **`journal_mode = WAL`**: Enables concurrent readers while one writer is active.
*   **`synchronous = NORMAL`**: Significant performance boost; safe when using WAL mode.
*   **`temp_store = MEMORY`**: Forces temporary tables and indices into RAM.
*   **`mmap_size = 30000000000`**: Maps up to 30GB of the database file into memory for near-instant read access (on supported OSs).

---

## 5. Deployment Example (`.env`)

```bash
# Security
APEXKIT_MASTER_KEY="your-generated-32-byte-base64-key"
APEX_ROOT_DOMAIN="api.myapp.com"

# Tuning
DB_BATCH_SIZE=1000
DB_FLUSH_MS=20
CACHE_TTL=600

# Features
ARCHIVE_LIMIT=50
APEX_VECTOR_MODEL="bge-small"

# Environment
PORT=8080
APP_ENV="production"
```

> **Note**: If `APEXKIT_MASTER_KEY` is not provided at startup, ApexKit will generate a temporary one and print a warning. **Always** persist this key; losing it means you cannot recover encrypted configurations like S3 credentials.