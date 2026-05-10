# Realtime Capabilities

ApexKit makes building reactive applications easy with built-in WebSockets and Server-Sent Events (SSE).

## WebSockets vs SSE

- **WebSockets (`ApexKitRealtimeWSClient`)**: Bi-directional. Best for interactive features like chat, collaborative editing, or instant search.
- **SSE (`ApexKitRealtimeSSEClient`)**: One-way (Server to Client). Best for live feeds, stock prices, or notification systems where the client doesn't need to send messages back over the same socket.

## Realtime Database Events

You can subscribe to changes in your data. Whenever a record is `Inserted`, `Updated`, or `Deleted`, ApexKit broadcasts an event to subscribers.

### Subscription Filters
You can narrow down what you listen for:
- `collectionId`: Only events from a specific collection.
- `recordId`: Only events for a specific record.
- `eventType`: Only `Insert`, `Update`, or `Delete`.
- `dataFilter`: Only if the data matches certain criteria (e.g., `status == 'urgent'`).

```javascript
realtime.subscribe({
    collectionId: 'orders',
    dataFilter: { status: 'shipped' }
});
```

## Custom Ephemeral Events (Signals)

Sometimes you want to send data that shouldn't be stored in the database, like "User X is typing" or "Cursor Position".

### Sending a Signal
```javascript
realtime.sendSignal('chat_room_1', 'typing', { user: 'Alice' });
```

### Listening for Signals
```javascript
realtime.subscribe({ channel: 'chat_room_1' });

realtime.onEvent((msg) => {
    if (msg.event === 'Custom' && msg.payload.event === 'typing') {
        console.log(`${msg.payload.data.user} is typing...`);
    }
});
```

## Instant Search over WebSockets

ApexKit provides a dedicated WebSocket message for ultra-low latency search.

```javascript
const results = await realtime.search('products', 'nike air', 5);
```
By performing the search over the already-open WebSocket, you avoid the overhead of HTTP handshakes and TLS negotiation for every keystroke.

## Server-Side Broadcasts

You can also trigger realtime events from within **Edge Functions**.

```javascript
// Inside an Edge Function
await $realtime.broadcast('notifications', 'alert', {
    message: "System maintenance in 5 minutes!"
});
```
