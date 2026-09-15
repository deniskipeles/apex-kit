# ⚡ Real-Time API Documentation

**Version:** 0.1.0  
**Transports:** WebSocket (`ws://` / `wss://`) & Server-Sent Events (SSE)

ApexKit provides a unified real-time stream that delivers database change mutations (`Insert`, `Update`, `Delete`), client-to-client ephemeral signaling, and low-latency full-text search over persistent socket connections. All real-time channels and streams enforce multi-tenant scoping and evaluate Row-Level Security (RLS) policies against authenticated caller claims.

---

## 1. Transport Comparison

| Feature | WebSocket (`ApexKitRealtimeWSClient`) | Server-Sent Events (`ApexKitRealtimeSSEClient`) |
| :--- | :--- | :--- |
| **Communication** | Bi-directional (Full duplex). | Uni-directional (Server to client). |
| **Endpoint** | `ws://host/ws` (or `/tenant/{id}/ws`, `/sandbox/{id}/ws`) | `GET /sse` |
| **Use Cases** | Interactive chat, collaborative whiteboards, instant search-as-you-type, cursor presence. | Live dashboard stats, notifications, read-only feeds, progress monitors. |
| **Authentication** | `{"type": "Auth", "payload": { "token": "..." }}` message. | `?token=...` query parameter or `Authorization` header. |
| **Dynamic Subscriptions** | Add or alter subscription filters without reconnecting. | Bounded to query parameters set at connection time. |

---

## 2. WebSocket Protocol Specification

### Connection URL
* **Root Application:** `ws://localhost:5000/ws`
* **Tenant Scope:** `ws://localhost:5000/tenant/{tenant_id}/ws`
* **Sandbox Scope:** `ws://localhost:5000/sandbox/{session_id}/ws`

---

### Client Inbound Messages (Client ➔ Server)

All client commands are serialized as JSON objects with `type` and `payload` fields:

#### A. Authentication (`Auth`)
Authenticates the open socket connection with a JWT token to evaluate Row-Level Security (RLS) on incoming broadcasts and search requests:

```json
{
  "type": "Auth",
  "payload": {
    "token": "eyJhbGciOiJIUzI1NiIsIn..."
  }
}
```

*Server Response on Success:* `{"type": "AuthSuccess"}`  
*Server Response on Failure:* `{"type": "Error", "message": "Invalid token"}`

---

#### B. Subscription Filter (`Subscribe`)
Registers in-memory filters for database and custom events:

```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": "posts",
    "event_type": "Insert",
    "filter": {
      "priority": "urgent",
      "status": "published"
    },
    "channel": "chat_room_101",
    "custom_event": "NewMessage"
  }
}
```

* **`collection_id`**: Target collection numeric ID or string name (optional).
* **`record_id`**: Target specific numeric record ID (optional).
* **`event_type`**: `"Insert"`, `"Update"`, or `"Delete"` (optional).
* **`filter`**: MongoDB-style JSON filter evaluated in memory against the record's payload (optional).
* **`channel`**: Logical channel name for custom ephemeral signals (optional).
* **`custom_event`**: Filter for a specific custom signal name (optional).

---

#### C. Unsubscribe (`Unsubscribe`)
Clears all active subscription filters:

```json
{
  "type": "Unsubscribe"
}
```

---

#### D. Ephemeral Signaling (`Signal`)
Broadcasts a non-persisted message directly to all subscribers sharing the same scoped channel:

```json
{
  "type": "Signal",
  "payload": {
    "channel": "chat_room_101",
    "event": "UserTyping",
    "data": {
      "username": "Alice"
    }
  }
}
```

---

#### E. Low-Latency Instant Search (`Search`)
Performs a search over the open WebSocket, bypassing HTTP handshakes:

```json
{
  "type": "Search",
  "payload": {
    "collection_id": "products",
    "query": "running shoes",
    "limit": 5,
    "request_id": "req-987"
  }
}
```

*Server Response:*
```json
{
  "type": "SearchResult",
  "request_id": "req-987",
  "results": [
    {
      "id": 42,
      "score": 3.12,
      "snippet": { "name": "Trail Running Shoes", "price": 120 }
    }
  ]
}
```

---

#### F. Ping (`Ping`)
Keeps socket connections active:

```json
{
  "type": "Ping"
}
```

*Server Response:* `Pong`

---

### Server Outbound Messages (Server ➔ Client)

#### 1. Database Mutation Events
```json
{
  "type": "Insert",
  "payload": {
    "collection_id": 5,
    "record_id": 102,
    "data": {
      "title": "New Article",
      "status": "published"
    }
  }
}
```
*(Types: `"Insert"`, `"Update"`, `"Delete"`)*

#### 2. Custom Ephemeral Events
```json
{
  "type": "Custom",
  "payload": {
    "event": "UserTyping",
    "data": {
      "username": "Alice"
    }
  }
}
```

---

## 3. Server-Sent Events (SSE)

For read-only streams over standard HTTP:

* **Endpoint:** `GET /sse`

### Query Parameters
* **`channel`**: Filter by custom channel name (e.g. `notifications`).
* **`event`**: Filter by custom event label (e.g. `Alert`).
* **`token`**: JWT token for evaluating Row-Level Security on streamed database records.

```http
GET /api/v1/sse?channel=alerts&event=ThresholdExceeded&token=eyJhbGci... HTTP/1.1
Host: api.your-app.com
Accept: text/event-stream
```

---

## 4. Server-Side Scripting Integration (`$realtime`)

You can emit real-time signals from any serverless script, webhook, database hook, or cron job:

```typescript
// Script: order-status-hook
// Trigger: after_update_record | Target: orders
// Path: ./webhooks/order-status-hook.ts

export default async function (event) {
    const { id, data } = event.record;

    // Emit custom signal to connected clients in the current tenant scope
    await $realtime.send(`order_${id}`, "OrderStatusChanged", {
        orderId: id,
        newStatus: data.status,
        updatedAt: new Date().toISOString()
    });
}
```

---

## 5. JavaScript / TypeScript SDK Usage (`@apexkit/sdk`)

### A. WebSocket Client (`ApexKitRealtimeWSClient`)

```typescript
import { ApexKit, ApexKitRealtimeWSClient } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
await apex.auth.login('user@example.com', 'password123');

// 1. Initialize & Connect
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// 2. Subscribe to Database Changes & Channels
realtime.subscribe({
  collectionId: 'orders',
  eventType: 'Insert',
  dataFilter: { status: 'urgent' },
  channel: 'orders_feed'
});

// 3. Send Client-to-Client Signal
realtime.sendSignal('orders_feed', 'AgentViewing', { agentId: 42 });

// 4. Instant Search over Socket
const results = await realtime.search('products', 'sneakers', 5);
console.log('Search hits:', results);

// 5. Listen for Events
const unsubscribe = realtime.onEvent((msg) => {
  if (msg.type === 'Insert') {
    console.log('New Order Created:', msg.payload.data);
  }
  if (msg.type === 'Custom' && msg.payload.event === 'AgentViewing') {
    console.log('Agent is viewing feed:', msg.payload.data);
  }
});

// 6. Cleanup
// unsubscribe();
// realtime.disconnect();
```

---

### B. SSE Client (`ApexKitRealtimeSSEClient`)

```typescript
import { ApexKitRealtimeSSEClient } from '@apexkit/sdk';

const sse = new ApexKitRealtimeSSEClient('https://api.your-app.com', token);

// Connect with optional channel and event filters
sse.connect({
  channel: 'notifications',
  eventName: 'TaskCompleted'
});

const unsubscribe = sse.onEvent((eventData) => {
  console.log('Received notification:', eventData);
});

// Cleanup
// unsubscribe();
// sse.disconnect();
```

---

## 6. Channel Namespacing & Tenant Isolation

ApexKit automatically prefixes channel names with the active execution scope to prevent cross-tenant message leakage:

* **Root App:** `channel_name` ➔ `root::channel_name`
* **Tenant `client-a`:** `channel_name` ➔ `tenant_client-a::channel_name`
* **Sandbox `session-123`:** `channel_name` ➔ `sandbox_session-123::channel_name`

Clients connected to `tenant_client-a` cannot listen to or broadcast signals into channels belonging to `tenant_client-b`.
