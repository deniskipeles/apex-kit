# Example: Community Forum with AI Moderation

A discussion board where posts are automatically scanned for toxicity before being published.

## 1. Database Collections

### `threads`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `title` | Text | Required | |
| `author_id` | Relation | Required | Collection: `users` |

### `posts`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `thread_id` | Relation | Required | Collection: `threads` |
| `content` | Rich Text | Required | |
| `is_flagged` | Bool | Default: `false` | |
| `status` | Select | Default: `pending` | `pending`, `published`, `removed` |

## 2. Security Policies

- **`posts`**:
  - `read`: `record.status == 'published'`
  - `create`: `auth`
  - `update`: `admin`

## 3. AI Moderation (Edge Function)

### `moderate-post`
**Trigger**: Before Create on `posts`
```javascript
export default async function(req) {
    const content = req.data.content;

    // Call AI Action to check toxicity
    const analysis = await $ai.run('check-toxicity', { text: content });

    if (analysis.is_toxic) {
        req.data.is_flagged = true;
        req.data.status = 'pending'; // Requires manual review
    } else {
        req.data.status = 'published';
    }

    return req.data;
}
```

## 4. Frontend Subscription

Users get notified when their post is approved.

```javascript
realtime.subscribe({
    collectionId: 'posts',
    dataFilter: { author_id: myUserId, status: 'published' }
});

realtime.onEvent((msg) => {
    alert("Your post has been approved and is now live!");
});
```
