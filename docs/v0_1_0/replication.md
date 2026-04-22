# Replication (High Availability & Multi-Node)

ApexKit supports a Master-Replica replication model, allowing you to scale read operations across multiple nodes and ensure data availability.

> **Note:** Replication is currently under heavy development.

## Architecture

ApexKit replication uses a single **Master** node and one or more **Replica** nodes.

- **Master Node:** The source of truth. Handles all write operations and broadcasts changes to replicas.
- **Replica Nodes:** Maintain a local copy of the database. They forward all write operations to the Master via gRPC and receive real-time updates (changesets) to keep their local state in sync.

### Communication
The Master and Replicas communicate over **gRPC**.
- **Snapshots:** When a replica starts or falls too far behind, it fetches a full SQLite database snapshot from the Master.
- **Event Streaming:** Replicas subscribe to a stream of `DbChangeEvent` messages containing binary changesets produced by SQLite's session extension.
- **Write Forwarding:** Replicas transparently forward SQL write commands to the Master.

## Configuration

### Master Node
To run a node as a Master, you must define a secret key that replicas will use for authentication.

```bash
# Set the master key for authentication
export APEXKIT_MASTER_KEY="your-super-secret-key"

# Run ApexKit
./apexkit-api
```

### Replica Node
To run a node as a Replica, you need to provide the Master's URL and the secret key.

```bash
# URL of the Master node
export APEX_MASTER_URL="http://master-node:3000"

# Must match the Master's key
export APEXKIT_MASTER_KEY="your-super-secret-key"

# Optional: Path to CA certificate if using HTTPS with self-signed certs
export APEX_TLS_CA_PATH="/path/to/ca.pem"

# Run ApexKit
./apexkit-api
```

## How it Works

1.  **Initial Sync:** When a Replica starts, it checks for the existence of local database files (e.g., `system.db`, `core.db`). If they are missing, it downloads them from the Master.
2.  **Streaming:** The Replica connects to the Master's `/replication.Replication/StreamEvents` gRPC endpoint. It registers its unique `replica_id` and the "scopes" (tenants/sandboxes) it wants to track.
3.  **Real-time Updates:** Whenever a change occurs on the Master, it records the changeset and sends it to all Replicas subscribed to that scope. The Replica applies this changeset locally.
4.  **Write Forwarding:** If an application sends a write request (POST/PUT/DELETE) to a Replica, the Replica detects it is in replica mode and forwards the raw SQL/params to the Master via `ExecuteWrite`. The Master executes it and the resulting change eventually streams back to the Replica.

## Limitations & Edge Cases

- **Full Sync:** If a replica is disconnected for more than 5 minutes, the Master may drop its buffer. Upon reconnection, the Master will signal a `FULL_SYNC_REQUIRED`, and the replica will re-download the entire database state.
- **Latency:** Replicas are eventually consistent. There is a small delay between a write on the Master and its appearance on a Replica.
- **Binary Format:** Replication uses SQLite's binary changeset format, which is highly efficient but requires compatible SQLite versions on both ends.
