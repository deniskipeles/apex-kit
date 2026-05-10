# Example: Customer Support Ticketing

A helpdesk system that uses AI to categorize tickets and suggest resolutions.

## 1. Database Collections

### `tickets`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `subject` | Text | Required | |
| `description` | Text | Required | |
| `status` | Select | Default: `open` | `open`, `pending`, `resolved` |
| `priority` | Select | | `low`, `medium`, `high` |
| `category` | Text | | |

### `comments`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `ticket_id` | Relation | Required | Collection: `tickets` |
| `author_id` | Relation | Required | Collection: `users` |
| `body` | Text | Required | |

## 2. Security Policies

- **`tickets`**:
  - `create`: `auth`
  - `read/update`: `owner:author_id` OR `admin`

## 3. AI-Powered Auto-categorization

### `process-new-ticket`
**Trigger**: After Create on `tickets`
```javascript
export default async function(req) {
    const ticket = req.record;

    // Call AI Action to suggest category and priority
    const suggestions = await $ai.run('analyze-ticket', {
        text: ticket.description
    });

    await $db.records.patch('tickets', ticket.id, {
        category: suggestions.category,
        priority: suggestions.priority
    });
}
```

## 4. Real-time Agent Dashboard

Agents see new tickets as they arrive.

```javascript
const agentClient = apex.auth.login('agent@company.com', '...');
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

realtime.subscribe({ collectionId: 'tickets' });

realtime.onEvent((msg) => {
    if (msg.event === 'Insert') {
        notifyAgent("New Ticket: " + msg.payload.data.subject);
    }
});
```
