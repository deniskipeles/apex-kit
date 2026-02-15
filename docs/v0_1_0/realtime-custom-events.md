# ⚡ Real-Time Custom Events Guide

**Version:** 0.1.0
**Context:** Server-Side Scripting & Client Integration

While ApexKit automatically broadcasts database changes (Insert/Update/Delete), many applications require **Ephemeral Events**—messages that need to be delivered instantly to connected clients but do not need to be permanently stored in the database.

**Common Use Cases:**
*   Chat "User is typing..." indicators.
*   Progress bars for long-running background tasks or media processing.
*   Live cursors or presence indicators.
*   Custom notifications triggered by specific logic.

---

## 1. Sending Events (Server-Side)

You can fire custom events from any **Script** (Manual Endpoint, Database Hook, or Cron Job) using the global `$realtime` object.

### The `$realtime` API

```javascript
await $realtime.send(channel, eventName, payload);
```

*   **`channel`** *(string)*: A logical grouping for listeners (e.g., `"room_1"`, `"notifications_user_5"`).
*   **`eventName`** *(string)*: A label to identify the type of message (e.g., `"Typing"`, `"NewMessage"`).
*   **`payload`** *(object)*: Any JSON-serializable data.

### Example: Broadcast a Progress Update

```javascript
// Script Name: process_video
// Trigger: manual

export default async function(req) {
    const { videoId } = await req.json();

    // ... processing logic ...

    // Notify listeners on the specific video channel
    await $realtime.send(`video_${videoId}`, "ProcessingProgress", {
        percent: 45,
        status: "Encoding frames..."
    });

    return new Response({ success: true });
}
```

---

## 2. Receiving Events (Client-Side)

ApexKit supports two methods for consuming these events: **WebSockets** (Bi-directional) and **Server-Sent Events** (Uni-directional).

### Option A: WebSockets (Recommended)

WebSockets allow you to subscribe/unsubscribe dynamically and send signals back.

**Endpoint:** `ws://your-api.com/ws` (Scoped automatically if using a Tenant URL).

#### 1. Subscribe
To listen to custom events, send a `Subscribe` message specifying the `channel`.

```javascript
const ws = new WebSocket("ws://localhost:5000/ws");

ws.onopen = () => {
    ws.send(JSON.stringify({
        type: "Subscribe",
        payload: {
            channel: "room_1",           // Listen to this channel
            custom_event: "ChatMessage"  // Optional: Filter for specific event name
        }
    }));
};
```

#### 2. Handle Messages
Incoming custom messages will have the type `Custom`.

```javascript
ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    if (msg.type === "Custom") {
        const { event: eventName, data } = msg.payload;
        console.log(`Received ${eventName}:`, data);
    }
};
```

---

### Option B: Server-Sent Events (SSE)

SSE is simpler for read-only scenarios (e.g., live feeds) as it uses standard HTTP.

**Endpoint:** `GET /sse`

#### Usage
Pass the `channel` and `event` as query parameters.

```javascript
// Listen to all events on "room_1"
const evtSource = new EventSource("http://localhost:5000/sse?channel=room_1");

evtSource.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.type === "Custom") {
        console.log("New Custom Event:", msg.payload.data);
    }
};
```

---

## 3. Client-to-Client Signaling

Sometimes you want to send a message directly from one Client to other Clients without a backend script (e.g., for "User is Typing" indicators). You can use the **`Signal`** command over WebSocket.

**Client Code:**
```javascript
ws.send(JSON.stringify({
    type: "Signal",
    payload: {
        channel: "room_1",
        event: "UserTyping",
        data: { username: "Alice" }
    }
}));
```
*Note: Signals are not stored. They are broadcast immediately to all other subscribers of that channel in the same scope.*

---

## 4. Security & Scoping

ApexKit automatically namespaces channels to prevent data leakage between tenants.

1.  **Root App**: Channel `general` becomes `root::general`.
2.  **Tenant A**: Channel `general` becomes `tenant_A::general`.
3.  **Sandbox B**: Channel `general` becomes `sandbox_B::general`.

**Impact:**
*   A user in **Tenant A** cannot listen to or send signals to **Tenant B**, even if they use the same channel name.
*   The Script Engine automatically applies the current execution's scope when calling `$realtime.send()`.
*   The API middleware applies the scope based on the URL (e.g., `/tenant/xyz/ws`) when a client connects.

---

## 5. Summary Checklist

| Feature | Method | Context |
| :--- | :--- | :--- |
| **Send from Backend** | `$realtime.send()` | Any Script |
| **Send from Frontend** | WS `Signal` | WebSocket Only |
| **Listen (Complex)** | WebSocket | `Subscribe` command |
| **Listen (Simple)** | SSE | `/sse?channel=...` |
| **Isolation** | Automatic | Handled by Scope system |