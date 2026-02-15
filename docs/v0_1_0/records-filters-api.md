# 🔍 Filtering API Documentation

**Version:** 0.1.0
**Context:** REST API, GraphQL, Real-time WebSockets, and Scripting.

ApexKit provides a unified, **MongoDB-style JSON filtering syntax**. This engine translates JSON logic into efficient SQL for database queries and performs high-speed in-memory evaluation for real-time WebSocket subscriptions and script hooks.

---

## 1. Syntax Overview

Filters are defined as JSON objects. They operate on the JSON data stored within your records.

### Basic Equality
To filter by an exact match, provide the field name and value.
```json
{
  "status": "published",
  "category": "news"
}
```
*Implies: `status = 'published' AND category = 'news'`*

### Dot Notation (Nested Data)
Since ApexKit stores data as JSON, you can filter deeply nested properties using dot notation.
```json
{
  "metadata.seo.keywords": "tech",
  "settings.notifications.email": true
}
```

---

## 2. Comparison Operators

To perform checks other than equality, use an operator object: `{ "field": { "$operator": value } }`.

| Operator | SQL Equivalent | Description | Example |
| :--- | :--- | :--- | :--- |
| **`$eq`** | `=` | Equal to. | `{ "role": { "$eq": "admin" } }` |
| **`$neq`** | `!=` | Not equal to. | `{ "status": { "$neq": "deleted" } }` |
| **`$gt`** | `>` | Greater than. | `{ "price": { "$gt": 100 } }` |
| **`$gte`** | `>=` | Greater than or equal. | `{ "age": { "$gte": 18 } }` |
| **`$lt`** | `<` | Less than. | `{ "stock": { "$lt": 5 } }` |
| **`$lte`** | `<=` | Less than or equal. | `{ "rating": { "$lte": 3.5 } }` |
| **`$in`** | `IN (...)` | Value exists in array. | `{ "tag": { "$in": ["A", "B"] } }` |
| **`$nin`** | `NOT IN (...)` | Value not in array. | `{ "id": { "$nin": [1, 2] } }` |
| **`$like`** | `LIKE` | SQL Wildcard matching. | `{ "title": { "$like": "The %" } }` |
| **`$contains`** | `LIKE %...%` | Substring match. | `{ "bio": { "$contains": "dev" } }` |

---

## 3. Logical Operators

You can combine multiple conditions using logical groups.

### `$and`
All conditions in the array must be true.
```json
{
  "$and": [
    { "is_active": true },
    { "views": { "$gt": 1000 } }
  ]
}
```

### `$or`
At least one condition in the array must be true.
```json
{
  "$or": [
    { "role": "admin" },
    { "role": "editor" }
  ]
}
```

### Complex Nesting
You can nest logic arbitrarily deep.
```json
{
  "$and": [
    { "status": "active" },
    { "$or": [
        { "category": "tech" },
        { "price": { "$lt": 50 } }
    ]}
  ]
}
```

---

## 4. Usage Contexts

### REST API
Pass the filter as a URL-encoded JSON string in the `filter` query parameter.
`GET /collections/posts/records?filter={"status":"active"}`

### GraphQL
The GraphQL API exposes a `where` argument on collection queries. This argument accepts the raw JSON scalar.
```graphql
query {
  posts(where: { status: "published", views: { $gt: 100 } }) {
    items { title }
  }
}
```

### Real-Time (WebSockets)
Filter the stream of events sent to your client. This happens in-memory on the server before broadcast.
```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": 5,
    "filter": { "priority": "URGENT" }
  }
}
```

### Scripting Engine
Pass filter objects directly to the `$db` helper.
```javascript
const activeUsers = await $db.find("users", {
    "last_login": { "$gt": "2024-01-01" },
    "status": "active"
});
```

---

## 5. Data Types & Caveats

1.  **Strict Typing**: Filters are type-sensitive. `{ "age": "18" }` (string) will not match a record where `age` is `18` (number).
2.  **Dates**: Dates are stored as ISO 8601 strings. Use string comparisons: `{ "created_at": { "$gt": "2024-02-01T00:00:00Z" } }`.
3.  **Booleans**: Use JSON booleans `true` / `false`.
4.  **Nulls**: To check for missing or null fields, use `{ "field": null }`. To check for existence, use `{ "field": { "$neq": null } }`.