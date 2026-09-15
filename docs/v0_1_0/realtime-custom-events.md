# ⚡ Real-Time Custom Events & Signaling Guide

**Version:** 0.1.0  
**Context:** Server-Side Scripting, Ephemeral Signaling & Real-Time Client Integration

In addition to broadcasting database lifecycle mutations (`Insert`, `Update`, `Delete`), ApexKit provides **Custom Ephemeral Events**. These allow developers to stream high-frequency messages (such as typing indicators, cursor coordinates, and long-running job progress updates) across connected clients without writing rows to SQLite or inflating disk I/O.

---

## 1. Emitting Custom Events from Server-Side Scripts (`$realtime`)

You can emit real-time signals from any serverless script, webhook, database hook, or cron job using the global `$realtime` API.

### The `$realtime.send` Method

```typescript
await $realtime.send(channel: string, eventName: string, payload: any): Promise<boolean>;
```

* **`channel`** (`string`): Logical broadcast channel (e.g. `"room_101"`, `"user_notifications_5"`).
* **`eventName`** (`string`): Action or event descriptor (e.g. `"UserTyping"`, `"JobProgress"`, `"Alert"`).
* **`payload`** (`any`): JSON-serializable data object.

### Example: Emitting a Progress Update

```typescript
// Script Name: transcode-media
// Trigger: manual | Path: ./webhooks/transcode-media.ts

export default async function (req: Request) {
    const { videoId } = await req.json();

    // 1. Offload heavy work to background queue
    await $queue.spawn(async (pid, jobReq) => {
        const { id } = await jobReq.json();

        for (let percent = 10; percent <= 100; percent += 20) {
            await $util.sleep(1000);

            // Broadcast real-time progress update to listeners
            await $realtime.send(`video_${id}`, "ProcessingProgress", {
                videoId: id,
                percentage: percent,
                status: percent === 100 ? "completed" : "encoding"
            });
        }
    }, { args: { id: videoId } });

    return new Response({
        success: true,
        message: "Processing started",
        channel: `video_${videoId}`
    }, { status: 202 });
}
```

---

## 2. Consuming Events on the Frontend

### Option A: WebSockets (`ApexKitRealtimeWSClient`) — Recommended

WebSockets provide a full-duplex connection for bidirectional messaging, live filtering, and client-to-client signaling.

```typescript
import { ApexKit, ApexKitRealtimeWSClient } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// 1. Subscribe to a custom channel
realtime.subscribe({
  channel: 'video_42',
  customEvent: 'ProcessingProgress' // Optional: filter for specific event name
});

// 2. Listen for incoming messages
const unsubscribe = realtime.onEvent((msg) => {
  if (msg.type === 'Custom' && msg.payload.event === 'ProcessingProgress') {
    const { percentage, status } = msg.payload.data;
    console.log(`Video 42 progress: ${percentage}% (${status})`);
    
    document.getElementById('progress-bar')!.style.width = `${percentage}%`;
  }
});

// Cleanup when component unmounts
// unsubscribe();
// realtime.disconnect();
```

---

### Option B: Server-Sent Events (`ApexKitRealtimeSSEClient`)

For read-only streams over standard HTTP (e.g. notifications or live progress bars without a WebSocket dependency):

```typescript
import { ApexKitRealtimeSSEClient } from '@apexkit/sdk';

const sse = new ApexKitRealtimeSSEClient('https://api.your-app.com', token);

// Connect with specific channel & event filters
sse.connect({
  channel: 'video_42',
  eventName: 'ProcessingProgress'
});

const unsubscribe = sse.onEvent((eventData) => {
  if (eventData.type === 'Custom') {
    console.log('Progress received via SSE:', eventData.payload.data);
  }
});

// Cleanup
// unsubscribe();
// sse.disconnect();
```

---

## 3. Client-to-Client Ephemeral Signaling (`sendSignal`)

When clients need to communicate directly (e.g. for chat presence, live collaborative cursors, or drawing coordinates), they can broadcast ephemeral messages without routing through a server-side script:

```typescript
// Client A: Broadcast cursor coordinates
function onMouseMove(x: number, y: number) {
  realtime.sendSignal('canvas_room_1', 'CursorMoved', {
    userId: currentUser.id,
    x,
    y
  });
}

// Client B: Listen for remote cursor movements
realtime.subscribe({
  channel: 'canvas_room_1',
  customEvent: 'CursorMoved'
});

realtime.onEvent((msg) => {
  if (msg.type === 'Custom' && msg.payload.event === 'CursorMoved') {
    const { userId, x, y } = msg.payload.data;
    renderRemoteCursor(userId, x, y);
  }
});
```

---

## 4. Multi-Tenant Scoping & Channel Namespacing

ApexKit automatically namespaces channels based on the active execution scope to prevent cross-tenant message leakage:

* **Root App:** `channel_name` ➔ `root::channel_name`
* **Tenant `customer-a`:** `channel_name` ➔ `tenant_customer-a::channel_name`
* **Sandbox `session-101`:** `channel_name` ➔ `sandbox_session-101::channel_name`

### Security Guarantees:
1. **Zero Data Leakage:** Clients connected to `tenant_customer-a` cannot receive signals broadcast from `tenant_customer-b`, even if both use the channel name `"chat"`.
2. **Transparent In-Script API:** Inside your server-side scripts, call `$realtime.send("room_1", ...)` normally; ApexKit automatically resolves the caller's scope prefix.
