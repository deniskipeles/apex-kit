# Example: Real-time Collaborative Whiteboard

A canvas where multiple users can draw and see each other's changes and cursors in real-time.

## 1. Database Collections

### `whiteboards`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `name` | Text | Required | |
| `owner_id` | Relation | Required | Collection: `users` |

### `shapes`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `board_id` | Relation | Required | Collection: `whiteboards` |
| `type` | Select | Required | `rect`, `circle`, `path` |
| `data` | JSON | Required | Coordinates, colors, etc. |

## 2. Security Policies

- **`whiteboards`**:
  - `read/update`: `auth` (or specific collaborators)
  - `delete`: `owner:owner_id`
- **`shapes`**:
  - `read/create/update`: `auth`
  - `delete`: `auth`

## 3. Real-time Connection (SDK)

### Syncing Shapes
```javascript
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// Subscribe to shapes for a specific board
realtime.subscribe({
    collectionId: 'shapes',
    dataFilter: { board_id: currentBoardId }
});

realtime.onEvent((msg) => {
    if (msg.event === 'Insert') addShapeToCanvas(msg.payload.data);
    if (msg.event === 'Update') updateShapeOnCanvas(msg.payload.data);
    if (msg.event === 'Delete') removeShapeFromCanvas(msg.payload.id);
});
```

### Syncing Cursors (Ephemeral)
```javascript
// Sending cursor position
canvas.on('mousemove', (pos) => {
    realtime.sendSignal(`board_${currentBoardId}`, 'cursor', {
        userId: currentUser.id,
        x: pos.x,
        y: pos.y
    });
});

// Listening for other cursors
realtime.subscribe({ channel: `board_${currentBoardId}` });
realtime.onEvent((msg) => {
    if (msg.event === 'Custom' && msg.payload.event === 'cursor') {
        drawRemoteCursor(msg.payload.data);
    }
});
```
