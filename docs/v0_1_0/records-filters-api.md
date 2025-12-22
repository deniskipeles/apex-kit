# ApexKit Filtering API Documentation

**Version:** 0.1.0
**Context:** REST API, GraphQL, and Real-time WebSockets.

ApexKit provides a unified, **MongoDB-style JSON filtering syntax**. This engine translates JSON logic into efficient SQL for database queries and performs high-speed in-memory evaluation for real-time WebSocket subscriptions.

---

## 1. Syntax Overview

Filters are defined as JSON objects. They operate on the JSON data stored within your records.

### Basic Equality
To filter by exact match, provide the field name and value.
```json
{
  "status": "published",
  "author_id": 55
}
```
*Implies: `status = 'published' AND author_id = 55`*

### Dot Notation (Nested Data)
Since ApexKit stores data as JSON, you can filter deeply nested properties using dot notation.
```json
{
  "metadata.seo.keywords": "tech",
  "settings.notifications.email": true
}
```

### Operators
To perform checks other than equality, use an operator object:
```json
{
  "field_name": { "$operator": value }
}
```

---

## 2. Comparison Operators

| Operator | SQL Equivalent | Description | Example |
| :--- | :--- | :--- | :--- |
| **`$eq`** | `=` | Equal to. | `{ "role": { "$eq": "admin" } }` |
| **`$neq`** | `!=` | Not equal to. | `{ "status": { "$neq": "deleted" } }` |
| **`$gt`** | `>` | Greater than. | `{ "price": { "$gt": 100 } }` |
| **`$gte`** | `>=` | Greater than or equal. | `{ "age": { "$gte": 18 } }` |
| **`$lt`** | `<` | Less than. | `{ "stock": { "$lt": 5 } }` |
| **`$lte`** | `<=` | Less than or equal. | `{ "rating": { "$lte": 3.5 } }` |
| **`$in`** | `IN (...)` | Value exists in array. | `{ "category": { "$in": ["news", "tech"] } }` |
| **`$nin`** | `NOT IN (...)` | Value not in array. | `{ "id": { "$nin": [1, 2, 3] } }` |
| **`$like`** | `LIKE` | SQL Wildcard matching (`%`). | `{ "title": { "$like": "The %" } }` |
| **`$contains`** | `LIKE %...%` | Substring match. | `{ "bio": { "$contains": "developer" } }` |

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
        { "category": "A" },
        { "price": { "$lt": 50 } }
    ]}
  ]
}
```

---

## 4. Usage in REST API

Pass the filter JSON as a URL-encoded string in the `filter` query parameter.

**Endpoint:** `GET /api/v1/collections/{id}/records`

**Example:**
*Filter:* `{ "status": "active", "views": { "$gt": 100 } }`

**Request:**
```http
GET /api/v1/collections/posts/records?filter=%7B%22status%22%3A%22active%22%2C%22views%22%3A%7B%22%24gt%22%3A100%7D%7D
```

**JavaScript Client Example:**
```javascript
const filter = {
  status: "active",
  $or: [{ category: "tech" }, { category: "news" }]
};

const params = new URLSearchParams({ 
    filter: JSON.stringify(filter) 
});

fetch(`/api/v1/collections/posts/records?${params}`);
```

---

## 5. Usage in GraphQL

The GraphQL API exposes a `where` argument on collection queries. This argument accepts the raw JSON scalar.

**Query:**
```graphql
query GetProducts {
  products(
    limit: 10,
    where: {
      category: "electronics",
      price: { $lt: 500 },
      $or: [
        { stock: { $gt: 0 } },
        { is_preorder: true }
      ]
    }
  ) {
    id
    title
    price
  }
}
```

---

## 6. Usage in Real-Time (WebSockets)

You can filter the stream of events sent to your client. This is performed **in-memory** on the server before the event is broadcast, saving bandwidth and client-side processing.

**Scenario:** Only receive updates when a ticket marked "URGENT" is created or updated.

**WebSocket Message:**
```json
{
  "type": "Subscribe",
  "payload": {
    "collection_id": 5,
    "filter": {
      "priority": "URGENT",
      "status": { "$neq": "closed" }
    }
  }
}
```

If a record is inserted with `priority: "LOW"`, the server **will not** send that event to this socket.

---

## 7. Data Types & Caveats

1.  **Type Sensitivity:**
    *   `"100"` (String) is not equal to `100` (Number).
    *   Ensure your filter values match the data types defined in your Schema.
2.  **Date Comparison:**
    *   Dates are stored as ISO 8601 Strings (`"2023-01-01T00:00:00Z"`).
    *   Use `$gt` / `$lt` with string comparisons for dates:
        `{ "created_at": { "$gt": "2023-01-01" } }`.
3.  **Boolean:**
    *   SQLite stores booleans as `0` or `1` internally, but the JSON extractor handles standard JSON `true`/`false`.
    *   Filter using: `{ "is_published": true }`.