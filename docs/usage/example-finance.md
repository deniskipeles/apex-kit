# Example: Personal Finance Tracker

A privacy-focused app to track income, expenses, and monthly budgets.

## 1. Database Collections

### `categories`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `name` | Text | Required | `Food`, `Rent`, `Salary`, etc. |
| `color` | Text | | |

### `transactions`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `user_id` | Relation | Required | Collection: `users` |
| `category_id` | Relation | Required | Collection: `categories` |
| `amount` | Number | Required | |
| `type` | Select | Required | `income`, `expense` |
| `date` | Date | Required | |

## 2. Security Policies

- **`categories`**:
  - `read`: `public`
- **`transactions`**:
  - `read/create/update/delete`: `owner:user_id`

## 3. Reporting with GraphQL

Fetch a summary of expenses grouped by category for the current month.

```graphql
query GetMonthlyStats($userId: String, $startDate: String) {
  transactions(filter: {
    user_id: $userId,
    type: "expense",
    date: { "$gte": $startDate }
  }) {
    amount
    category {
      name
    }
  }
}
```

## 4. Recurring Transactions (Cron Job)

### `process-subscriptions`
**Trigger**: Cron (Every Day at 00:00)
```javascript
export default async function() {
    const today = new Date().getDate();

    // Find subscriptions due today
    const subs = await $db.records.list('subscriptions', {
        filter: { billing_day: today }
    });

    for (const sub of subs) {
        await $db.records.create('transactions', {
            user_id: sub.user_id,
            amount: sub.amount,
            category_id: sub.category_id,
            type: 'expense',
            date: new Date().toISOString()
        });
    }
}
```
