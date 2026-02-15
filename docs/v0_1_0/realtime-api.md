# ApexKit Real-Time API

**Version:** 0.1.0
**Transports:** WebSocket & Server-Sent Events (SSE)

ApexKit provides a real-time stream of database events.

---

## 1. WebSocket API

**Endpoint:** `ws://localhost:5000/ws` (or `wss://`)
**Context:** Automatically scoped to current Tenant if connecting via tenant URL.

### Client Commands
Send JSON messages to control the connection.

#### Subscribe
Start listening to specific events.
```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": 5,        // Optional
    "event_type": "Insert",    // Optional: Insert, Update, Delete
    "filter": { "status": "active" }, // Optional: Data filter
    "channel": "chat_room_1"   // Optional: For Custom Events
  }
}
```

#### Signal (Client-to-Client)
Broadcast a message to other clients on the same channel without storing it.
```json
{
  "type": "Signal",
  "payload": {
    "channel": "chat_room_1",
    "event": "UserTyping",
    "data": { "user": "Alice" }
  }
}
```

#### Instant Search
Perform a search over WebSocket.
```json
{
  "type": "Search",
  "payload": {
    "collection_id": 1,
    "query": "search term",
    "request_id": "req-123"
  }
}
```

---

## 2. Server-Sent Events (SSE)

**Endpoint:** `GET /sse`

### Query Parameters
*   `channel`: Subscribe to a specific custom channel.
*   `event`: Filter by specific event name.

**Example:**
`GET /sse?channel=notifications`

---

## 3. JavaScript Client SDK

Use the `ApexKitRealtimeWSClient` class provided in the SDK.

```javascript
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

// 1. Connect
const realtime = new ApexKitRealtimeWSClient()
realtime.connect();

// 2. Subscribe
realtime.subscribe({
    collectionId: 1,
    filter: { priority: "high" }
});

// 3. Listen
realtime.onEvent((msg) => {
    console.log("New Event:", msg);
});
```