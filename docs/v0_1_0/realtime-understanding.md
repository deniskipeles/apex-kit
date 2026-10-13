# ApexKit Realtime Architecture: Comprehensive Documentation

ApexKit features an event-driven, multi-tenant Realtime subsystem designed around high throughput, edge resilience, and strict data isolation. It natively supports **WebSockets (`/ws`)** and **Server-Sent Events (SSE) (`/sse`)**, supporting both low-latency bidirectional communication and lightweight read-only push streams.

---

## Table of Contents

1. [Architectural Overview](#1-architectural-overview)
2. [Dual Protocols: WebSockets vs. SSE](#2-dual-protocols-websockets-vs-sse)
3. [Scopes, Multi-Tenancy & Channel Namespacing](#3-scopes-multi-tenancy--channel-namespacing)
4. [Event Categories](#4-event-categories)
   - [Native Database Events (`Insert`, `Update`, `Delete`)](#native-database-events-insert-update-delete)
   - [Custom Ephemeral Events & Pub/Sub](#custom-ephemeral-events--pubsub)
5. [In-Memory Event Filtering Engine](#5-in-memory-event-filtering-engine)
6. [Connection Quotas & Resource Management](#6-connection-quotas--resource-management)
7. [Scripting Hooks: Intercepting Realtime](#7-scripting-hooks-intercepting-realtime)
   - [`before_realtime_connection_established`](#before_realtime_connection_established)
   - [`before_realtime_data_pushed`](#before_realtime_data_pushed)
   - [`after_realtime_connection_disconnected`](#after_realtime_connection_disconnected)
   - [Infinite Loop Protection](#infinite-loop-protection)
8. [Client SDK Usage (`@apexkit/sdk`)](#8-client-sdk-usage-apexkitsdk)
   - [WebSocket Client (`ApexKitRealtimeWSClient`)](#websocket-client-apexkitrealtimewsclient)
   - [SSE Client (`ApexKitRealtimeSSEClient`)](#sse-client-apexkitrealtimesseclient)

---

## 1. Architectural Overview

```
                      ┌────────────────────────────────────────┐
                      │    Client (Web / Mobile / Daemon)     │
                      └──────────────────┬─────────────────────┘
                                         │
                   1. TCP Handshake /ws or GET /sse
                   (Passes JWT Bearer or x-api-key)
                                         ▼
                      ┌────────────────────────────────────────┐
                      │            Axum Middleware             │
                      │  - Resolves Scope (Root/Tenant/Sandbox)│
                      │  - Verifies Auth (JWT or x-api-key)    │
                      └──────────────────┬─────────────────────┘
                                         │
            ┌────────────────────────────┴────────────────────────────┐
            ▼                                                         ▼
┌───────────────────────────────────────┐ ┌───────────────────────────────────────┐
│     WebSocket (/ws)                   │ │     Server-Sent Events (/sse)         │
│  - Bidirectional                      │ │  - Unidirectional (Server -> Client)  │
│  - Ephemeral Messaging (Signals)      │ │  - Low memory footprint               │
│  - Interactive Remote Search (OSE)    │ │  - Native auto-reconnect              │
└──────────────────┬────────────────────┘ └──────────────────┬────────────────────┘
                   │                                         │
                   ▼                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 1. Host Quota Check (Root DB _tenants Quota: max_ws / max_sse)                  │
│ 2. Hook: before_realtime_connection_established (Inspects Headers, IP, Protocol)│
└────────────────────────────────────────┬────────────────────────────────────────┘
                                         │ Accepted (RAII Guard Active)
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Tokio Broadcast Hub (state.tx)                                                  │
│ Receives Local SQLite CUD Events, Cross-Node Replica Sync, and Custom Signals   │
└────────────────────────────────────────┬────────────────────────────────────────┘
                                         │
            ┌────────────────────────────┴────────────────────────────┐
            │ Row-Level Security (RLS) Filter: Evaluates Collection Policy   │
            │ In-Memory FilterNode AST: Matches Query Filter Syntax   │
            ▼                                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Hook: before_realtime_data_pushed                                               │
│ (Inspects/Redacts outgoing JSON frame. Replaces with {} on Error)               │
│ ($realtime.send calls are hard-blocked to prevent infinite cascade loops)       │
└────────────────────────────────────────┬────────────────────────────────────────┘
                                         │ Outgoing Frame Dispatched
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Client Disconnect / Stream Termination                                          │
│ - RAII Guard Drops -> Decrements Active Counter                                 │
│ - Hook: after_realtime_connection_disconnected                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

ApexKit's Realtime engine bridges SQLite table mutations and arbitrary messaging channels directly to network clients without requiring third-party messaging queues like Redis. 

- **Tokio Broadcast Hub (`state.tx`)**: When any record modification is committed to SQLite or dispatched via scripts, a typed `DbEvent` is broadcast to a non-blocking in-memory ring buffer.
- **Cluster Awareness**: In multi-node setups (Master/Replica replication), `DbEvent` notifications crossing node boundaries are automatically tunneled across master streams and fanned out locally.
- **Strict Scope Boundaries**: Realtime connections are bound to an execution scope (`Root`, `Tenant`, or `Sandbox`), guaranteeing that clients cannot subscribe to or sniff events from outside their tenant or workspace environment.

---

## 2. Dual Protocols: WebSockets vs. SSE

ApexKit implements both WebSockets and Server-Sent Events to accommodate different architectural requirements:

| Metric / Feature | WebSockets (`/ws`) | Server-Sent Events (`/sse`) |
| :--- | :--- | :--- |
| **Communication Flow** | Bidirectional (Full Duplex) | Unidirectional (Server-to-Client only) |
| **Transport** | Upgraded HTTP/1.1 or HTTP/2 TCP Socket | Standard HTTP long-lived stream |
| **Default Tenant Quota** | **300** Concurrent Connections | **500** Concurrent Connections |
| **Default Sandbox Quota** | **30** Concurrent Connections | **50** Concurrent Connections |
| **Authentication Flow** | Handshake Header or `{ type: "Auth" }` message | Handshake Header or `?token=<JWT>` parameter |
| **Dynamic Filtering** | Dynamic on-the-fly (`Subscribe` message) | Connection query parameters (`?filter=...`) |
| **Interactive Search** | Supported (Requests index via socket) | Not supported (Streaming only) |
| **Client Reconnection** | Manual reconnect logic required | Native browser auto-reconnect |

---

## 3. Scopes, Multi-Tenancy & Channel Namespacing

Every realtime event is accompanied by an `EventScope`. The realtime hub strictly validates this scope against the connection:

*   **Root Scope (`EventScope::Root`)**: The master deployment space. Has access to all root databases and can broadcast globally.
*   **Tenant Scope (`EventScope::Tenant(id)`)**: Isolated multi-tenant client instances (`/tenant/{tenant_id}/ws` or `/tenant/{tenant_id}/sse`). A tenant client will **only** receive database operations originating from that specific tenant’s SQLite database.
*   **Sandbox Scope (`EventScope::Sandbox(id)`)**: Ephemeral development sandboxes (`/sandbox/{session_id}/ws`). Operations executed by the AI Architect or preview clients are confined solely to the active session.

### Channel Namespacing
To prevent channel name collisions between tenants on custom pub/sub topics (e.g., two tenants both listening on a channel called `chat`), ApexKit automatically applies namespacing under the hood:

*   Root Channel: `root::<channel_name>`
*   Tenant Channel: `tenant_<tenant_id>::<channel_name>`
*   Sandbox Channel: `sandbox_<session_id>::<channel_name>`

Clients and server-side scripts always refer to the clean channel name (e.g., `chat`); namespacing is handled automatically by the server.

---

## 4. Event Categories

### Native Database Events (`Insert`, `Update`, `Delete`)
When records are added, modified, or deleted within a collection, ApexKit automatically creates and dispatches a `DbEvent`.

#### Event Structure:
```json
{
  "type": "Insert",
  "payload": {
    "collection_id": 1,
    "record_id": 42,
    "data": {
      "title": "Realtime in ApexKit",
      "status": "published",
      "author_id": 5
    },
    "scope": "Root"
  }
}
```

#### Row-Level Security (RLS) Filtering
Database change events are evaluated against collection permissions before reaching the client:
1. When an event fires, ApexKit identifies the target collection and its `read` rule.
2. The read policy is evaluated against the authenticated user/API key of **each active connection**.
3. If access is rejected, the event frame is dropped for that specific client without notification.

### Custom Ephemeral Events & Pub/Sub
Custom events bypass SQLite entirely, serving as high-speed signals for notifications, live cursors, typing indicators, and presence updates.

```json
{
  "type": "Custom",
  "payload": {
    "event": "UserTyping",
    "data": {
      "username": "alex",
      "isTyping": true
    },
    "scope": {
      "Channel": "tenant_abc::chat_lobby"
    }
  }
}
```

---

## 5. In-Memory Event Filtering Engine

Clients can apply declarative, in-memory filters so they only receive events matching specific criteria, conserving mobile bandwidth and CPU resources.

Filtering uses MongoDB-style operators via ApexKit’s internal `FilterNode` AST:
*   `$eq` / `$neq` – Equal / Not Equal
*   `$gt` / `$gte` – Greater than / Greater than or equal to
*   `$lt` / `$lte` – Less than / Less than or equal to
*   `$in` / `$nin` – Matches any / Matches none in an array
*   `$contains` – Substring match

### Filtering in SSE
SSE filters are configured directly in the URL query string:

```
GET /sse?collection=orders&filter={"status":"completed","total":{"$gte":100}}
```

### Filtering in WebSockets
WebSocket connections can update their filter at any time using a `Subscribe` frame:

```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": "orders",
    "event_type": "Insert",
    "filter": {
      "status": "completed",
      "total": { "$gte": 100 }
    }
  }
}
```

---

## 6. Connection Quotas & Resource Management

To prevent resource exhaustion (file descriptors, sockets, thread memory) in multi-tenant environments, connection limits are enforced by the root system:

| Context | Max SSE Connections | Max WebSocket Connections | Managed By |
| :--- | :--- | :--- | :--- |
| **Root Space** | Unlimited (`usize::MAX`) | Unlimited (`usize::MAX`) | System Configuration |
| **Tenants** | **500** *(Default)* | **300** *(Default)* | Configurable in `_tenants` table |
| **Sandboxes**| **50** *(Default)* | **30** *(Default)* | Ephemeral Hard-Cap |

### Database Storage
Tenant quotas are managed in the `_tenants` table inside `storage/system/core.db`:

```sql
ALTER TABLE _tenants ADD COLUMN max_sse INTEGER DEFAULT 500;
ALTER TABLE _tenants ADD COLUMN max_ws INTEGER DEFAULT 300;
```

Root administrators can update these limits per tenant via the Admin UI or through the system API:

```http
PATCH /api/v1/admin/tenants/{id}
Content-Type: application/json

{
  "max_sse": 1500,
  "max_ws": 1000
}
```

If a client attempts to connect when their tenant's quota is exhausted, the server rejects the connection with **`HTTP 429 Too Many Requests`**.

---

## 7. Scripting Hooks: Intercepting Realtime

ApexKit provides server-side JavaScript hooks to intercept, validate, transform, and clean up realtime traffic.

### `before_realtime_connection_established`
Fires during the initial connection handshake before the client is accepted.

*   **Type**: Void / Interceptor Hook
*   **Behavior**: Throwing an error terminates the connection immediately (**`HTTP 403 Forbidden`**).
*   **Payload (`event.data`)**:
    *   `ip`: Client IP address.
    *   `protocol`: `"sse"` or `"websocket"`.
    *   `headers`: Dictionary of all incoming HTTP headers.

```javascript
/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "name": "enforce-realtime-guard",
  "extension": "js",
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "before_realtime_connection_established",
  "active": true
};

export default async function (event) {
  const { ip, protocol, headers } = event.data;
  const user = event.auth;

  // Block unauthorized origins
  const origin = headers["origin"] || "";
  if (origin && !origin.endsWith("mycompany.com")) {
    throw new Error(`Unauthorized origin: ${origin}`);
  }

  // Restrict WebSocket access for guest accounts
  if (protocol === "websocket" && (!user || user.role === "guest")) {
    throw new Error("Guests are only permitted to use SSE streaming.");
  }

  return true;
}
```

---

### `before_realtime_data_pushed`
Fires right before an event is written to a client's stream or socket.

*   **Type**: Filter Hook
*   **Behavior**:
    *   Return the modified data object to transform what the client receives.
    *   **If the hook throws an error, the payload is replaced with an empty object `{}`** to prevent data leaks.
*   **Payload (`event.data`)**: The complete event payload (`DbEvent` or `Custom` event object).

```javascript
/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "name": "redact-outgoing-events",
  "extension": "js",
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "before_realtime_data_pushed",
  "active": true
};

export default async function (event) {
  const payload = event.data;
  const auth = event.auth;

  // Redact customer fields for non-admin viewers
  if (payload.type === "Insert" || payload.type === "Update") {
    if (auth?.role !== "admin" && payload.payload?.data) {
      delete payload.payload.data.internal_notes;
      delete payload.payload.data.billing_address;
      delete payload.payload.data.ssn;
    }
  }

  return payload;
}
```

---

### `after_realtime_connection_disconnected`
Fires asynchronously when a connection closes, whether from a normal client disconnect, a network drop, or an aborted connection.

*   **Type**: Void Hook
*   **Behavior**: Executes via an internal RAII drop guard upon connection cleanup.
*   **Payload (`event.data`)**:
    *   `ip`: Client IP address.
    *   `protocol`: `"sse"` or `"websocket"`.
    *   `headers`: Dictionary of headers captured during connection initialization.

```javascript
/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "name": "presence-disconnect-handler",
  "extension": "js",
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "after_realtime_connection_disconnected",
  "active": true
};

export default async function (event) {
  const { ip, protocol } = event.data;
  const user = event.auth;

  console.log(`[Disconnect] ${user?.email || "Guest"} dropped ${protocol} connection from ${ip}`);

  // Broadcast user departure to a presence channel
  if (user) {
    await $realtime.send("presence", "UserLeft", {
      userId: user.id,
      email: user.email,
      timestamp: new Date().toISOString()
    });
  }

  return true;
}
```

---

### Infinite Loop Protection
To prevent runaway execution loops, **`$realtime.send(...)` is blocked inside `before_realtime_data_pushed`**.

```
[Record Inserted] 
       │
       ▼
[before_realtime_data_pushed] ──(Calls $realtime.send)──► [NEW Event Dispatched]
       ▲                                                           │
       └───────────────────(Causes Infinite Loop)─────────────────┘
```

If a script attempts to invoke `$realtime.send(...)` while processing a `before_realtime_data_pushed` hook, the runtime throws an exception and halts execution:

```
Runtime Exception: $realtime.send is blocked inside before_realtime_data_pushed to prevent infinite loops.
```

---

## 8. Client SDK Usage (`@apexkit/sdk`)

ApexKit includes client libraries for both WebSockets and Server-Sent Events with TypeScript typings, automatic channel filtering, and reconnection handling.

### WebSocket Client (`ApexKitRealtimeWSClient`)

Best for bidirectional features like live chat rooms, collaborative editing, and instant database subscriptions.

```typescript
import { ApexKit, ApexKitRealtimeWSClient } from '@apexkit/sdk';

const apex = new ApexKit('https://api.myapp.com');
await apex.auth.login('user@myapp.com', 'password123');

// 1. Initialize Client
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// 2. Subscribe to Database Changes with In-Memory Filter
realtime.subscribe({
  collectionId: 10,                          // Collection ID or Name
  eventType: 'Insert',                       // 'Insert' | 'Update' | 'Delete'
  dataFilter: {
    priority: 'critical',
    status: { $in: ['open', 'investigating'] }
  }
});

// 3. Subscribe to an Ephemeral Pub/Sub Topic
realtime.subscribe({
  channel: 'operations_room',
  customEvent: 'DeployAlert'                 // (Optional) Filter on specific event names
});

// 4. Send an Ephemeral Client-to-Client Signal (Bypasses DB storage)
realtime.sendSignal('operations_room', 'DeployAlert', {
  releaseVersion: 'v2.4.0',
  deployedBy: 'Alice'
});

// 5. Execute Instant Full-Text Searches over the Socket
const searchResults = await realtime.search('articles', 'rust database', 5);
console.log('Search hits:', searchResults);

// 6. Listen for Inbound Messages
const unsubscribe = realtime.onEvent((msg) => {
  if (msg.type === 'Insert') {
    console.log('New Record Created:', msg.payload.data);
  }
  
  if (msg.type === 'Custom' && msg.payload.event === 'DeployAlert') {
    console.log('Alert received:', msg.payload.data);
  }
});

// Disconnect cleanly when done
// unsubscribe();
// realtime.disconnect();
```

---

### SSE Client (`ApexKitRealtimeSSEClient`)

Ideal for read-only streams such as system logs, notifications, and analytics feeds.

```typescript
import { ApexKit, ApexKitRealtimeSSEClient } from '@apexkit/sdk';

const apex = new ApexKit('https://api.myapp.com');
await apex.auth.login('user@myapp.com', 'password123');

// 1. Initialize SSE Streamer
const sse = new ApexKitRealtimeSSEClient(apex.baseUrl, apex.getToken());

// 2. Connect with Server-Side Collection and Query Filtering
sse.connect({
  collection: 'system_logs',
  filter: {
    level: { $in: ['warn', 'error', 'critical'] }
  }
});

// 3. Consume Incoming Events
const unsubscribe = sse.onEvent((event) => {
  if (event.type === 'Insert') {
    console.warn(`[${event.payload.data.level.toUpperCase()}]`, event.payload.data.message);
  }
});

// 4. Disconnect stream
// sse.disconnect();
```