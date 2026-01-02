
# ⚡ Real-Time Custom Events Guide

**Version:** 0.1.0
**Context:** Server-Side Scripting & Client Integration

While ApexKit automatically broadcasts database changes (Insert/Update/Delete), many applications require **Ephemeral Events**—messages that need to be delivered instantly to connected clients but do not need to be permanently stored in the database.

**Common Use Cases:**
*   Chat "User is typing..." indicators.
*   Progress bars for long-running background tasks.
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
*   **`payload`** *(object)*: The JSON data to send.

### Example: Broadcast a Chat Message

```javascript
// Script Name: send_message
// Trigger: manual

export default async function(req) {
    const { room, user, text } = await req.json();

    // 1. (Optional) Persist to DB if history is needed
    // await $db.insert("messages", { room, user, text });

    // 2. Broadcast immediately
    await $realtime.send(room, "ChatMessage", {
        user: user,
        text: text,
        timestamp: new Date().toISOString()
    });

    return new Response({ success: true });
}
```

---

## 2. Receiving Events (Client-Side)

ApexKit supports two methods for consuming these events: **WebSockets** (Bi-directional) and **Server-Sent Events** (Uni-directional).

### Option A: WebSockets (Recommended)

WebSockets allow you to subscribe/unsubscribe dynamically without reconnecting.

**Endpoint:** `ws://your-api.com/ws` (or `wss://`)

#### 1. Connect & Subscribe
To listen to custom events, send a `Subscribe` message specifying the `channel` and optionally a `custom_event` filter.

```javascript
const ws = new WebSocket("ws://localhost:5000/ws");

ws.onopen = () => {
    console.log("Connected!");
    
    // Subscribe to a specific channel
    ws.send(JSON.stringify({
        type: "Subscribe",
        payload: {
            channel: "room_1",       // Must match the channel used in script
            custom_event: "ChatMessage" // Optional: Filter by specific event name
        }
    }));
};
```

#### 2. Handle Messages
Incoming messages will have the type `Custom`.

```javascript
ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    if (msg.type === "Custom") {
        const { event: eventName, data } = msg.payload;
        
        console.log(`Received ${eventName} on channel ${msg.payload.scope.Channel}`);
        console.log("Data:", data);
        
        // Example: Update UI
        if (eventName === "ChatMessage") {
            appendMessageToChat(data.user, data.text);
        }
    }
};
```

---

### Option B: Server-Sent Events (SSE)

SSE is simpler if you only need to listen (read-only) and don't want to manage a complex WebSocket state.

**Endpoint:** `GET /sse`

#### Usage
Pass the `channel` and `event` as query parameters.

```javascript
// Listen to all events on "room_1"
const evtSource = new EventSource("http://localhost:5000/sse?channel=room_1");

// OR: Filter for specific events
// const evtSource = new EventSource("http://localhost:5000/sse?channel=room_1&event=ChatMessage");

evtSource.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    
    if (msg.type === "Custom") {
        console.log("New Event:", msg.payload.data);
    }
};
```

---

## 3. Client-to-Client Signaling (WebSockets Only)

Sometimes you want to send a message directly from the Client to other Clients without writing a specific backend script (e.g., for "User is Typing" indicators).

You can use the **`Signal`** command over WebSocket.

**Client Code:**
```javascript
// Send a transient signal
ws.send(JSON.stringify({
    type: "Signal",
    payload: {
        channel: "room_1",
        event: "UserTyping",
        data: { username: "Alice" }
    }
}));
```

*Note: Signals are not stored in the database. They are broadcast immediately to all other subscribers of that channel.*

---

## 4. Security & Isolation

ApexKit automatically namespaces channels based on the current environment.

1.  **Root App:** Channel `room_1` becomes `root::room_1`.
2.  **Tenant A:** Channel `room_1` becomes `tenant_A::room_1`.
3.  **Tenant B:** Channel `room_1` becomes `tenant_B::room_1`.

**What this means:**
*   A user in Tenant A **cannot** listen to messages from Tenant B, even if they both use the channel name "general".
*   The Script Engine automatically applies the current tenant's scope when you call `$realtime.send()`.
*   The API automatically applies the current tenant's scope when a client connects via WebSocket or SSE.

---

## 5. Summary Checklist

| Feature | Method | Key Parameters |
| :--- | :--- | :--- |
| **Send from Backend** | `$realtime.send()` | `channel`, `event`, `json_data` |
| **Send from Frontend** | WS `Signal` | `channel`, `event`, `json_data` |
| **Listen (Robust)** | WebSocket | Send `{ type: "Subscribe", payload: { channel: "..." } }` |
| **Listen (Simple)** | SSE | Connect to `/sse?channel=...` |