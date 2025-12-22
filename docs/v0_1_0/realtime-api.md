

# ApexKit Real-Time API (WebSocket)

**Version:** 0.1.0
**Endpoint:** `ws://localhost:5000/ws`

ApexKit provides a high-performance, persistent WebSocket connection for listening to database changes in real-time. Unlike polling, this pushes data to your client immediately when a record is **Inserted**, **Updated**, or **Deleted**.

The API supports **Server-Side Filtering**, allowing you to subscribe only to specific events or data conditions to save bandwidth.

---

## 1. Connection Lifecycle

### Connecting
Connect using a standard WebSocket client.

```javascript
const ws = new WebSocket("ws://localhost:5000/ws");

ws.onopen = () => {
    console.log("Connected to ApexKit Real-time Stream");
};
```

### Authentication
Currently, the WebSocket endpoint is public. However, if your instance is behind a proxy or requires auth cookies, the browser automatically handles standard headers. *Token-based handshake authentication is planned for v2.5.*

---

## 2. Client Commands (Sending Messages)

Once connected, you can send JSON messages to control your subscription.

### A. Subscribe
By default, a connection receives **no events** until subscribed. You can send a subscription object to define what you want to hear.

**Structure:**
```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": 5,          // Optional: Only events for this collection
    "record_id": 100,            // Optional: Only events for this specific record
    "event_type": "Insert",      // Optional: "Insert", "Update", or "Delete"
    "filter": { ... }            // Optional: Complex Data Filter (See Section 3)
  }
}
```

*Note: Sending a new Subscribe message overwrites the previous subscription for that socket.*

### B. Unsubscribe
Stop receiving events without closing the socket.

```json
{ "type": "Unsubscribe" }
```

### C. Ping/Pong
Keep the connection alive.
**Send:** `{ "type": "Ping" }`
**Receive:** `"Pong"`

---

## 3. Filtering Capabilities

The `filter` payload accepts the **ApexKit JSON Filter Syntax** (MongoDB-style). This filtering happens **in-memory** on the server, ensuring extremely low latency.

**Supported Operators:** `$eq`, `$neq`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`, `$contains`, `$and`, `$or`.

### Example: High Priority Tickets
Subscribe only when a ticket is created or updated with "High" priority.

```javascript
ws.send(JSON.stringify({
  type: "Subscribe",
  payload: {
    collection_id: 12, // 'tickets' collection
    filter: {
      "priority": "high",
      "status": { "$neq": "closed" }
    }
  }
}));
```

---

## 4. Receiving Events

The server pushes JSON messages when a database event matches your subscription.

### Event Structure

```json
{
  "event": "Insert", 
  "payload": {
    "collection_id": 5,
    "record_id": 102,
    "data": { 
      "title": "New Post",
      "status": "published"
    }
  }
}
```

### Event Types

| Event | Payload Contains | Triggered When |
| :--- | :--- | :--- |
| **`Insert`** | `collection_id`, `record_id`, `data` | A new record is created. |
| **`Update`** | `collection_id`, `record_id`, `data` | An existing record is modified. `data` contains the **new** state. |
| **`Delete`** | `collection_id`, `record_id` | A record is removed. **Note:** `data` is not available in delete events. |

---

## 5. JavaScript Implementation Guide

Here is a robust class for handling ApexKit subscriptions with auto-reconnection.

```javascript
class ApexKitSubscriber {
    constructor(url) {
        this.url = url;
        this.socket = null;
        this.listeners = [];
        this.connect();
    }

    connect() {
        this.socket = new WebSocket(this.url);

        this.socket.onopen = () => {
            console.log("ApexKit Connected");
            // Re-subscribe logic would go here
        };

        this.socket.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                this.listeners.forEach(cb => cb(msg));
            } catch (e) {
                // Handle non-JSON messages (like "Pong")
            }
        };

        this.socket.onclose = () => {
            console.log("Disconnected. Retrying in 3s...");
            setTimeout(() => this.connect(), 3000);
        };
    }

    subscribe(config) {
        if (this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({
                type: "Subscribe",
                payload: config
            }));
        } else {
            // Wait for open
            this.socket.addEventListener('open', () => {
                this.subscribe(config);
            }, { once: true });
        }
    }

    onEvent(callback) {
        this.listeners.push(callback);
    }
}

// --- Usage ---

const stream = new ApexKitSubscriber("ws://localhost:5000/ws");

// 1. Subscribe to 'orders' collection where amount > $100
stream.subscribe({
    collection_id: 1, 
    filter: { "total": { "$gt": 100 } }
});

// 2. React to data
stream.onEvent((msg) => {
    if (msg.event === "Insert") {
        alert(`New Big Order! ID: ${msg.payload.record_id}`);
    }
});
```

---

## 6. Caveats & Limits

1.  **Filter Limitations on Delete:**
    *   The `Delete` event payload does not contain the record data (it's gone).
    *   Therefore, if you subscribe with a content filter (e.g., `status == 'active'`), you **will not** receive Delete events for those records.
    *   **Best Practice:** If you need to handle deletions, subscribe using only `collection_id`, or handle the logic client-side.
2.  **Security:**
    *   Currently, WebSocket subscriptions do not check Row-Level Security (RLS) policies (like `owner:id`).
    *   Do not expose sensitive collections via WebSocket if they require strict user-level partitioning. (This is scheduled for update v2.5).