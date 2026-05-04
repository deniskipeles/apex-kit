# Request Data Flow

This document describes the lifecycle of a request in ApexKit, from the client's HTTP call to the final response.

## Standard API Request Flow

For standard operations like fetching records or creating a collection, the flow is optimized for direct database interaction.

```mermaid
sequenceDiagram
    participant Client
    participant Axum as Axum API Layer
    participant Auth as Auth Middleware
    participant Policy as Policy Engine
    participant DB as Rusqlite (Core)
    participant Search as Tantivy

    Client->>Axum: GET /api/v1/collections/posts/records
    Axum->>Auth: Extract JWT & Validate
    Auth->>Axum: User Context (TenantID, Role)
    Axum->>Policy: Check 'read' policy for 'posts'
    Policy->>Axum: Authorized
    Axum->>DB: Query posts (filtered by TenantID)
    DB-->>Axum: Record List
    Axum-->>Client: 200 OK (JSON)

    Note over Client, Search: On Write Operations
    Client->>Axum: POST /api/v1/collections/posts/records
    Axum->>DB: Insert Record
    DB->>Search: Update Index (Async)
    DB-->>Axum: Success
    Axum-->>Client: 201 Created
```

## Scripted Request Flow

When a request hits a custom endpoint or a hook, ApexKit spins up a sandboxed JavaScript environment.

```mermaid
sequenceDiagram
    participant Client
    participant Axum as Axum API Layer
    participant Boa as Boa JS Engine
    participant DB as Rusqlite (Core)
    participant External as External API

    Client->>Axum: POST /api/v1/scripts/my-logic
    Axum->>Boa: Load & Execute Script
    Boa->>DB: $db.records.find('users', { id: 1 })
    DB-->>Boa: User Data
    Boa->>External: $http.post('https://api.thirdparty.com/notify')
    External-->>Boa: Response
    Boa-->>Axum: script return value
    Axum-->>Client: 200 OK (JSON)
```

## Hook Execution Flow

Hooks allow you to inject logic before or after standard database operations.

```mermaid
sequenceDiagram
    participant Axum as Axum API Layer
    participant Hook as Before-Create Hook (JS)
    participant DB as Rusqlite (Core)

    Axum->>Hook: Trigger with Record Data
    Hook->>Hook: Validate/Modify Data
    Hook-->>Axum: Return Modified Data
    Axum->>DB: Persist to Database
    DB-->>Axum: Success
    Axum-->>Client: 201 Created
```

## Internal Transaction Handling

ApexKit ensures data integrity by wrapping multiple operations in transactions, even when they involve the search index.

1. **Transaction Start:** A SQLite transaction is opened.
2. **Database Change:** Data is written to the JSONB columns.
3. **Index Notification:** The Search Engine is notified of the change.
4. **Transaction Commit:** If everything succeeds, the transaction is committed.
5. **Real-time Broadcast:** After commit, the change is broadcasted via WebSockets/SSE.
