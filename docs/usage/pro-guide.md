# Pro Guide: Internals and Optimization

This guide is for developers who want to push ApexKit to its limits, understand its internals, and optimize performance for high-scale applications.

## 1. The Boa Engine (JavaScript Runtime)

ApexKit uses **Boa**, an embeddable JavaScript engine written in Rust.

### Why Boa?
- **Safety**: Unlike V8, Boa is written in 100% safe Rust. There are no memory-safety vulnerabilities.
- **In-process**: Scripts run directly in the Axum request handler, meaning zero context switching between the OS and a separate process (like Node.js).

### Limitations to Consider:
- **Event Loop**: Boa's event loop is currently single-threaded within a script execution context. While ApexKit scales by running many scripts in parallel across Rust threads, a single long-running script can block its own execution.
- **Library Support**: Boa does not support Node.js native modules (C++ add-ons) or the full NPM ecosystem. Use standard ES Modules whenever possible.
- **WASM**: WebAssembly support within Boa is experimental.

## 2. Tantivy Search Tuning

ApexKit uses **Tantivy** for full-text search. To get the best performance:

### Indexing Strategies:
- **Field Storing**: By default, ApexKit stores field values in the index for snippets. For very large text fields, disable `Stored` and only use `Indexed` to keep the index size small.
- **Tokenization**: Use the `Raw` tokenizer for IDs or emails, and the `Standard` (Stemming) tokenizer for body text to support "word variant" matching (e.g., "running" matches "run").

### Optimization:
ApexKit periodically runs a "Merge" operation on Tantivy segments. You can manually trigger a full compaction via the Admin API if you notice search latency increasing after massive bulk imports.

## 3. High Availability and Replication

ApexKit supports a **Master-Replica** replication model via gRPC.

### Custom gRPC Replication:
- **Snapshot Sync**: Replicas download a full SQLite snapshot on startup.
- **Streaming Changesets**: The Master streams every transaction (WAL frames) to connected Replicas in real-time.
- **Read-Scaling**: Use Replicas to handle 100% of `GET` requests, leaving the Master to focus on `POST/PUT/DELETE`.

### Setup:
```bash
# Master
APEXKIT_MASTER_KEY=secret ./apexkit --port 5000

# Replica
APEXKIT_MASTER_URL=http://master-ip:5000 APEXKIT_MASTER_KEY=secret ./apexkit --port 5001
```

## 4. SQLite Performance Optimization

### WAL Mode
ApexKit runs SQLite in **Write-Ahead Logging (WAL)** mode by default. This allows multiple readers to operate simultaneously with one writer.

### Busy Timeout
If you encounter "Database is locked" errors during high write concurrency, increase the `busy_timeout` in your settings. However, the best approach is to:
1. **Batch Writes**: Use the `/query` endpoint for bulk inserts.
2. **Move Logic to Rust**: If a script is doing too many individual DB calls, consider writing a custom Rust plugin for that specific high-load path.

## 5. Vector Search Tuning (HNSW)

The Vector Search uses the **HNSW (Hierarchical Navigable Small Worlds)** algorithm.
- **M (Max Links)**: Increasing `M` improves search accuracy but increases memory usage and build time.
- **efConstruction**: Controls the tradeoff between index speed and search quality.

For datasets > 1M vectors, we recommend externalizing the Vector index to a dedicated service or using ApexKit's "Quantization" settings to reduce vector size.
