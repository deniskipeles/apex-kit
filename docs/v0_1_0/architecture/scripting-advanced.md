# Advanced Scripting Patterns

The ApexKit scripting engine, powered by Boa, allows you to extend your backend with custom logic. This document covers advanced patterns and real-world use cases.

## External API Integration

Using `$http` to interact with third-party services.

### Example: Stripe Webhook Handler

```javascript
export default async function(req) {
    const signature = req.headers.get('stripe-signature');
    const body = await req.text();

    // Verify webhook (Hypothetical util helper or manual check)
    // const isValid = $util.verifyStripe(body, signature, $env.STRIPE_SECRET);

    const event = JSON.parse(body);

    if (event.type === 'checkout.session.completed') {
        const session = event.data.object;
        const customerEmail = session.customer_details.email;

        // Update user record in database
        await $db.records.update('users', { email: customerEmail }, {
            status: 'premium',
            subscription_id: session.subscription
        });

        // Trigger welcome email
        await $mail.send(customerEmail, "Welcome to Premium!", "Thank you for subscribing.");
    }

    return new Response({ received: true });
}
```

## Complex Data Validation (Hooks)

Hooks run during the collection lifecycle. Use them to enforce rules that go beyond simple schema validation.

### Example: Preventing Duplicate Reservations

**Trigger:** `before-create` on `reservations` collection.

```javascript
export default async function(req) {
    const record = await req.json();

    // Check if the slot is already taken
    const existing = await $db.records.list('reservations', {
        filter: {
            room_id: record.room_id,
            date: record.date,
            status: 'confirmed'
        }
    });

    if (existing.items.length > 0) {
        // Throwing an error in a hook cancels the operation
        throw new Error("This room is already reserved for the selected date.");
    }

    // Return the record to continue the creation process
    return new Response(record);
}
```

## Background Jobs & Cron

Schedule tasks to run at specific intervals.

### Example: Daily Data Cleanup

**Trigger:** `cron` (Configured to run at `0 0 * * *`)

```javascript
export default async function(req) {
    console.log("Starting daily cleanup...");

    const thirtyDaysAgo = new Date();
    thirtyDaysAgo.setDate(thirtyDaysAgo.getDate() - 30);

    // Delete expired sessions or temporary logs
    const result = await $db.records.delete('logs', {
        created_at: { "$lt": thirtyDaysAgo.toISOString() }
    });

    console.log(`Cleaned up ${result.deleted_count} log entries.`);

    return new Response({ success: true });
}
```

## Advanced Database Queries ($db.query)

Use the full power of the query engine for reporting.

### Example: Monthly Sales Report

```javascript
export default async function(req) {
    const report = await $db.query(null, {
        from: 'orders',
        select: [
            { fn: 'strftime', args: ['%Y-%m', 'created_at'], as: 'month' },
            { fn: 'sum', field: 'total', as: 'revenue' },
            { fn: 'count', field: 'id', as: 'order_count' }
        ],
        group_by: ['month'],
        sort: '-month',
        limit: 12
    });

    return new Response(report);
}
```

## Script Communication ($run)

Scripts can call other scripts, allowing for modular logic.

```javascript
export default async function(req) {
    // Call a utility script to process an image
    const processed = await $run.script('image-optimizer', {
        file_id: 'file_123',
        target_size: 'thumb'
    });

    return new Response(processed);
}
```
