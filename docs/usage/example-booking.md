# Example: Event Booking System

Manage venue availability, ticket sales, and automated email confirmations.

## 1. Database Collections

### `events`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `title` | Text | Required | |
| `date` | Date | Required | |
| `capacity` | Number | Required | |
| `price` | Number | | |

### `bookings`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `event_id` | Relation | Required | Collection: `events` |
| `user_id` | Relation | Required | Collection: `users` |
| `status` | Select | Default: `confirmed` | `confirmed`, `cancelled` |

## 2. Security Policies

- **`events`**:
  - `read`: `public`
- **`bookings`**:
  - `create`: `auth`
  - `read`: `owner:user_id`

## 3. Atomic Booking Logic (Edge Function)

Ensure we don't overbook an event.

### `create-booking`
**Trigger**: HTTP POST
```javascript
export default async function(req) {
    const { eventId } = await req.json();
    const userId = req.user.id;

    // Atomic check and increment
    return await $db.transaction(async (tx) => {
        const event = await tx.records.get('events', eventId);
        const count = await tx.records.count('bookings', { event_id: eventId, status: 'confirmed' });

        if (count >= event.capacity) {
            return new Response({ error: "Sold out!" }, { status: 400 });
        }

        const booking = await tx.records.create('bookings', {
            event_id: eventId,
            user_id: userId
        });

        return booking;
    });
}
```

## 4. Email Confirmation

### `send-confirmation`
**Trigger**: After Create on `bookings`
```javascript
export default async function(req) {
    const user = await $db.records.get('users', req.record.user_id);
    const event = await $db.records.get('events', req.record.event_id);

    await $util.sendEmail({
        to: user.email,
        subject: `Booking Confirmed: ${event.title}`,
        body: `See you on ${event.date}!`
    });
}
```
