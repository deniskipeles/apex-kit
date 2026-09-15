# 🔄 Horizontal Replication (Master-Replica gRPC & WebSocket)

**Version:** 0.1.0  
**Protocols:** gRPC (HTTP/2 + Protocol Buffers) & WebSocket / HTTP Streaming Fallback

ApexKit provides a real-time **Master-Replica replication engine** designed to scale read throughput horizontally across geographically distributed nodes while maintaining a single, consistent source of truth for write operations.

Rather than running distributed consensus algorithms (like Raft or Paxos), ApexKit operates at the physical SQLite layer using **Snapshot Sync**, **Binary Session Changeset Streams**, and **Transparent Write Forwarding**.

---

## 1. System Architecture

```
[Write Request] ──> [Replica Node] ──(gRPC / HTTP Write Forward)──> [Master Node]
                                                                            │
                                                                  (Commit to SQLite)
                                                                            │
                                                                 (Generate Changeset)
                                                                            │
[Read Request]  <── [Replica Node] <──(gRPC / WS Changeset Stream)──────────┘
```

### Key Roles:
* **Master Node:** The authoritative source of truth. Manages all write operations, commits WAL transactions locally, attaches SQLite session changesets, and broadcasts binary delta frames to connected replicas.
* **Replica Nodes:** Read-only cached instances serving `GET` requests, full-text searches, and GraphQL queries locally with sub-millisecond response times. Any write request received by a replica is automatically forwarded to the Master.

---

## 2. Synchronization Mechanisms

### A. Initial Bootstrap & Snapshot Sync
When a replica starts up, it checks its local filesystem (`core.db`, `data.db`, `system.db`, `vectors.db`). If database files are missing or a full sync is triggered:
1. The replica connects to the Master over gRPC (`FetchDbSnapshot`) or HTTP fallback (`GET /replication/snapshot`).
2. The Master runs an in-memory `PRAGMA wal_checkpoint(PASSIVE)` and streams the raw database files.
3. The replica writes the streamed chunks to a temporary file (`.tmp`), validates the payload, replaces the local database files atomically, and purges stale `-wal` and `-shm` files.

---

### B. Real-Time Changeset Streaming
Once bootstrapped, the replica opens a bidirectional stream with the Master:
1. **Subscription Registration:** The replica sends its `replica_id` and the list of tenant/sandbox scopes it wishes to track (`EventSubscription`).
2. **Binary Changeset Dispatch:** Whenever a mutation (`INSERT`, `UPDATE`, `DELETE`) is committed on the Master, SQLite's `session` extension extracts the binary changeset byte buffer.
3. **Stream Delivery:** The Master dispatches a `DbChangeEvent` message containing `{ scope, db_name, changeset }`.
4. **Local Application:** The replica applies the binary changeset directly to its local database using SQLite's `apply_strm` with `SQLITE_CHANGESET_REPLACE` conflict resolution, then invalidates local memory caches.

---

### C. Transparent Write Forwarding
When a client sends a write request (`POST /api/v1/collections/posts/records`) to a replica:
1. The replica detects that `APEXKIT_MASTER_URL` is set and routes the transaction to `GrpcWriteForwarder`.
2. The forwarder maps query parameters to JSON/Base64 representations and calls the Master's `ExecuteWrite` RPC endpoint (or fallback `/replication/write`).
3. The Master executes the statement inside a `BEGIN IMMEDIATE` transaction, captures the generated row ID, commits the changeset to the replication stream, and responds to the replica.
4. The replica returns the generated primary key and response back to the client.

---

### D. File Synchronization
When binary assets are uploaded to a replica:
1. The replica writes the file locally into its scoped uploads directory.
2. The replica calls `forward_file_to_master` over gRPC (`SyncFile`) or HTTP (`POST /replication/sync-file`).
3. The Master writes the file into its own persistent storage backend (Local FS or S3).
4. If a replica requests a file missing from its local storage, it dynamically proxies and fetches the file from the Master on demand.

---

## 3. Configuration & Deployment

### Step 1: Deploying the Master Node
Run ApexKit with an encryption master key:

```bash
export APEXKIT_MASTER_KEY="your_base64_encoded_32_byte_secret_key"
export PORT=5000

./apexkit
```

*The Master automatically boots both the HTTP Axum API and the gRPC `ReplicationServer` on port `5000` using protocol multiplexing.*

---

### Step 2: Deploying Replica Nodes
Deploy replica instances by providing the Master's URL and the identical `APEXKIT_MASTER_KEY`:

```bash
export APEXKIT_MASTER_KEY="your_base64_encoded_32_byte_secret_key"
export APEXKIT_MASTER_URL="http://master-server-ip:5000"
export PORT=5000

./apexkit
```

#### Optional TLS Configuration for HTTPS / gRPCs
```bash
export APEXKIT_TLS_CA_PATH="/path/to/custom_ca.pem"
```

#### Forcing HTTP / WebSocket Fallback
If running in restricted network environments where HTTP/2 gRPC traffic is blocked or stripped by load balancers:
```bash
export APEXKIT_FORCE_HTTP_REPLICATION="true"
```

---

## 4. Disconnection & Failure Recovery

1. **Short-Term Disconnection (< 5 Minutes):** If a replica temporarily disconnects, the Master buffers incoming `DbChangeEvent` messages in memory for up to 300 seconds. Upon reconnection, buffered changesets are flushed to the replica immediately.
2. **Prolonged Disconnection (> 5 Minutes):** If a replica is disconnected for more than 5 minutes, the Master evicts its buffer to protect memory. Upon reconnection, the Master dispatches a `FULL_SYNC_REQUIRED` event, prompting the replica to download fresh database snapshots automatically.
3. **Automatic Fallback:** If gRPC channel connections fail, the replica automatically falls back to WebSocket streaming (`/replication/ws`) and HTTP polling.

---

## 5. Summary Matrix

| Metric | Master Node | Replica Node |
| :--- | :--- | :--- |
| **Write Consistency** | Strongly consistent (Immediate ACID commit). | Forwarded to Master. |
| **Read Consistency** | Immediate. | Eventually consistent (~1–10ms changeset latency). |
| **Cron Jobs / Scheduler** | **Active.** Runs all scheduled jobs. | **Disabled.** Tickers locked to prevent duplicate execution. |
| **Storage Uploads** | Authoritative persistence. | Local write + forward sync to Master. |
| **Tantivy Full-Text Search** | Indexed and maintained locally. | Replicated & searched locally. |
| **Vector Engine (HNSW)** | Loaded in RAM. | Loaded in RAM. |
