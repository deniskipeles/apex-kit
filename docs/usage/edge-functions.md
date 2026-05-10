# Edge Functions (Server-Side Logic)

Edge Functions allow you to run custom JavaScript logic directly on the ApexKit server. They are powered by the **Boa** engine, a high-performance, sandboxed JS engine written in Rust.

## Triggers

Scripts can be triggered in several ways:
1. **HTTP**: Expose a public or private URL endpoint.
2. **Database Hooks**: Run before or after record creation, update, or deletion.
3. **Cron Jobs**: Run at scheduled intervals (e.g., every hour).
4. **GraphQL**: Act as custom resolvers.

## Global Objects

Inside a script, you have access to powerful global objects:

| Object | Purpose |
| :--- | :--- |
| **`$db`** | Perform database operations (`list`, `create`, `update`, `delete`). |
| **`$http`** | Make external API calls (e.g., to Stripe or Slack). |
| **`$util`** | Utility functions for UUIDs, hashing, and formatting. |
| **`$ai`** | Access AI models for embeddings or generation. |
| **`$cache`** | High-speed, ephemeral Key-Value store. |
| **`$fs`** | Interact with the file storage system. |

## Example: Stripe Webhook Handler

```javascript
// Trigger: HTTP POST /run/stripe-webhook
export default async function(req) {
    const body = await req.json();

    if (body.type === 'checkout.session.completed') {
        const customerId = body.data.object.customer;

        // Update user status in DB
        await $db.records.update('users', { stripe_id: customerId }, {
            is_premium: true
        });
    }

    return new Response({ received: true });
}
```

## Example: Database Hook (Auto-Slugify)

```javascript
// Trigger: Before Create (Collection: posts)
export default async function(req) {
    const data = req.data;
    data.slug = $util.slugify(data.title);
    return data;
}
```

## Example: AI-Powered Image Tagging

```javascript
// Trigger: After Create (Collection: images)
export default async function(req) {
    const file = req.record.file_path;

    // Get AI generated labels
    const labels = await $ai.analyzeImage(file);

    await $db.records.update('images', req.record.id, {
        tags: labels
    });
}
```

## Sandbox Safety
Edge Functions run in a **completely isolated sandbox**. They cannot:
- Access the host filesystem directly.
- Execute arbitrary system commands (unless specifically enabled via `$cmd` for Root scripts).
- Access memory of other scripts or the main Rust process.

## Performance
Because scripts run in-process using the Boa engine, there is **zero cold start time**. Requests are handled with sub-millisecond overhead compared to native code.
