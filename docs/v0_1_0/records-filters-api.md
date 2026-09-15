# 🔍 Filtering API Documentation

**Version:** 0.1.0  
**Context:** REST API, GraphQL, Real-Time WebSockets, and Server-Side Scripting

ApexKit provides a unified **MongoDB-style JSON filtering engine**. This engine translates JSON logic trees directly into parameterized SQLite `WHERE` clauses for database queries and evaluates expressions in memory for real-time WebSocket subscriptions and script hooks.

---

## 1. Syntax Overview

Filters are structured JSON objects operating over the `data` properties of your collection records.

### Direct Field Equality
Provide the target field name and its expected value:

```json
{
  "status": "published",
  "category": "technology"
}
```
*Compiled SQL:* `(records.data ->> 'status' IN ('published', CAST('published' AS TEXT), CAST('published' AS INTEGER)) AND records.data ->> 'category' IN ('technology', CAST('technology' AS TEXT), CAST('technology' AS INTEGER)))`

---

### System Fields
You can filter directly on system metadata without special prefixes:
* `id`: Matches record primary key (`records.id`).
* `created`: Matches ISO creation timestamp (`records.created`).
* `updated`: Matches ISO update timestamp (`records.updated`).

```json
{
  "id": { "$in": [101, 102, 103] },
  "status": "active"
}
```

---

### Dot Notation (Nested Objects)
Query deeply nested JSON attributes within a record using dot notation:

```json
{
  "settings.notifications.email": true,
  "metadata.author.profile.verified": true
}
```
*Compiled SQL:* `records.data -> 'settings' -> 'notifications' ->> 'email' = 1`

---

## 2. Comparison Operators

To apply non-equality comparisons, wrap the condition in an operator object: `{ "field": { "$operator": value } }`.

| Operator | Aliases | Description | Example |
| :--- | :--- | :--- | :--- |
| **`$eq`** | `eq`, `_eq` | Exact equality (with type-coercion safety). | `{ "role": { "$eq": "admin" } }` |
| **`$neq`** | `neq`, `_neq` | Not equal to. | `{ "status": { "$neq": "archived" } }` |
| **`$gt`** | `gt`, `_gt` | Greater than. | `{ "price": { "$gt": 99.99 } }` |
| **`$gte`** | `gte`, `_gte` | Greater than or equal to. | `{ "views": { "$gte": 1000 } }` |
| **`$lt`** | `lt`, `_lt` | Less than. | `{ "inventory": { "$lt": 5 } }` |
| **`$lte`** | `lte`, `_lte` | Less than or equal to. | `{ "rating": { "$lte": 3.0 } }` |
| **`$in`** | `in`, `_in` | Matches any value within the provided array. | `{ "category": { "$in": ["tech", "ai"] } }` |
| **`$nin`** | `nin`, `_nin` | Value does not exist in the provided array. | `{ "status": { "$nin": ["banned", "deleted"] } }` |
| **`$like`** | `like`, `_like` | Case-insensitive SQL `LIKE` wildcard matching. | `{ "title": { "$like": "ApexKit%" } }` |
| **`$contains`** | `contains`, `_contains`| Substring match (automatically wraps query in `%...%`). | `{ "bio": { "$contains": "rust" } }` |

---

## 3. Logical Grouping (`$and`, `$or`)

Combine multiple conditions into logical arrays:

### `$and`
Every condition in the array must be true:
```json
{
  "$and": [
    { "is_published": true },
    { "stock": { "$gt": 0 } }
  ]
}
```

### `$or`
At least one condition in the array must be true:
```json
{
  "$or": [
    { "role": "admin" },
    { "role": "moderator" }
  ]
}
```

### Nested Complex Logic
Nest `$and` and `$or` groups to arbitrary depths:
```json
{
  "$and": [
    { "status": "active" },
    {
      "$or": [
        { "category": "electronics" },
        { "price": { "$lte": 50 } }
      ]
    }
  ]
}
```

---

## 4. Usage Contexts

### A. REST API
Pass the filter as a URL-encoded string inside the `filter` query parameter:

```http
GET /api/v1/collections/posts/records?filter=%7B%22status%22%3A%22published%22%2C%22views%22%3A%7B%22%24gt%22%3A100%7D%7D
```

---

### B. GraphQL API
Pass the raw filter JSON object directly into the `where` argument on any collection query:

```graphql
query GetFilteredArticles {
  articles(
    where: {
      status: "published",
      views: { "$gte": 500 },
      tags: { "$in": ["rust", "sqlite"] }
    }
  ) {
    total
    items {
      id
      title
      created
    }
  }
}
```

---

### C. Real-Time WebSockets (`ApexKitRealtimeWSClient`)
Filter the real-time event stream on the server so clients only receive events matching their criteria:

```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": "orders",
    "filter": {
      "total": { "$gte": 500 },
      "status": "pending"
    }
  }
}
```

---

### D. Server-Side Scripting (`$db`)
Pass filter objects directly into `$db.records.list()` or `$db.query()`:

```typescript
// Script: ./webhooks/fetch-active-users.ts
export default async function (req: Request) {
    const activeUsers = await $db.records.list("users_profiles", {
        filter: {
            "status": "active",
            "subscription.plan": { "$in": ["pro", "enterprise"] }
        },
        limit: 50
    });

    return new Response(activeUsers);
}
```

---

## 5. Type-Coercion & Implementation Notes

1. **Numeric vs. String ID Compatibility:** SQLite JSON functions return raw extracted text. When filtering by ID using `$eq` or `$in`, ApexKit's compiler generates fallback `CAST` expressions (`col IN (?, CAST(? AS TEXT), CAST(? AS INTEGER))`) to ensure number and string representations match seamlessly.
2. **Null Checks:**
   * Check for missing or `null` values: `{ "field_name": null }`
   * Check for field presence: `{ "field_name": { "$neq": null } }`
3. **Date Comparisons:** Dates are stored as ISO 8601 strings (`YYYY-MM-DDTHH:MM:SSZ`). Because ISO 8601 strings sort lexicographically, range operators (`$gt`, `$gte`, `$lt`, `$lte`) work accurately against date strings:
   ```json
   {
     "created": {
       "$gte": "2026-01-01T00:00:00Z",
       "$lte": "2026-12-31T23:59:59Z"
     }
   }
   ```
