# Real-time API

ApexKit provides a real-time stream of database events and custom signaling via WebSockets and Server-Sent Events (SSE).

## WebSocket Client

The `ApexKitRealtimeWSClient` class allows for high-performance, bidirectional real-time communication.

### Methods

- `connect()`
- `disconnect()`
- `subscribe(filter: SubscriptionFilter)`
- `sendSignal(channel, eventName, data)`
- `search(collectionId, query, limit?)`
- `onEvent(callback)`

### Initialization & Connection

```typescript
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// Listen for connection status
console.log(realtime.isConnected);
```

### Subscribing to Events

You can subscribe to database changes, custom channels, or both.

```typescript
// Subscribe to data changes
realtime.subscribe({
    collectionId: 5,
    eventType: "Update",
    dataFilter: { "priority": "high" }
});

// Subscribe to a custom channel
realtime.subscribe({
    channel: "chat_room_1",
    customEvent: "NewMessage"
});

// Handle events
const unsubscribe = realtime.onEvent((msg) => {
    // Handle DB Event
    if (msg.type === "Insert") {
        console.log("Record Created:", msg.payload.data);
    }

    // Handle Custom Signal
    if (msg.type === "Custom") {
        const { event, data } = msg.payload;
        if (event === "UserTyping") console.log(`${data.user} is typing...`);
    }
});

// Stop listening
unsubscribe();
```

### Signaling (Client-to-Client Broadcast)

Send ephemeral messages to other clients on a specific channel.

```typescript
realtime.sendSignal("chat_room_1", "UserTyping", { user: "Alice" });
```

### Instant Search over WebSocket

Perform searches with lower latency than REST.

```typescript
const results = await realtime.search(1, "search query", 5);
console.log(results); // [{ id: 1, score: 2.5, snippet: {...} }]
```

---

## SSE Client

`ApexKitRealtimeSSEClient` is a read-only stream for environments where WebSockets are not required.

### Methods

- `connect({ channel?, eventName? })`
- `disconnect()`
- `onEvent(callback)`

### Initialization & Connection

```typescript
import { ApexKitRealtimeSSEClient } from '@apexkit/sdk';

const sse = new ApexKitRealtimeSSEClient(apex.baseUrl);

// Connect and filter
sse.connect({
    channel: "notifications",
    eventName: "Alert"
});

// Handle events
const unsubscribe = sse.onEvent((msg) => {
    console.log("SSE Event Received:", msg);
});

// Stop listening
unsubscribe();
```
