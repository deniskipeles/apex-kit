# 🔮 GraphQL API Documentation

**Version:** 0.1.0
**Endpoint:** `/graphql`
**Playground:** Access `/graphql` or the scoped URL (e.g., `/tenant/{id}/graphql`) for interactive testing.

ApexKit provides a dynamic, high-performance GraphQL API that is automatically generated based on your **Collections** and **Relationships**. It is designed to solve the "N+1" problem using efficient **Dataloaders**, allowing you to fetch deeply nested data in a single network request.

---

## 1. Endpoint & Scoping

The GraphQL API respects ApexKit's multi-tenancy architecture. Use the appropriate URL to target your environment:

| Scope | GraphQL Endpoint |
| :--- | :--- |
| **Root App** | `POST /graphql` |
| **Tenant** | `POST /tenant/{tenant_id}/graphql` |
| **Sandbox** | `POST /sandbox/{session_id}/graphql` |

> **Authentication**: All requests must include the `Authorization: Bearer <TOKEN>` header. API Policies (Read/Update/etc.) defined in your collections are strictly enforced.

---

## 2. Querying Collections

For every collection you create (e.g., `posts`), the API generates a top-level field and a paginated list type.

### Basic Fetch
Fetch a list of records. The collection name is lowercase.

```graphql
query {
  posts {
    total
    items {
      id
      title
      status
    }
  }
}
```

### Pagination
Use `limit` and `offset` to handle large datasets.

*   `limit`: (Int) Max records (Default 100).
*   `offset`: (Int) Records to skip.

```graphql
query GetPageTwo {
  posts(limit: 10, offset: 10) {
    items {
      id
      title
    }
  }
}
```

---

## 3. Advanced Filtering (`where`)

The `where` argument accepts a **JSON Scalar** using the MongoDB-style **Filters API**.

### Syntax Examples
*   **Equality**: `{ "status": "published" }`
*   **Comparison**: `{ "price": { "$gt": 100 } }`
*   **Logic**: `{ "$or": [{ "category": "A" }, { "featured": true }] }`
*   **Containment**: `{ "tags": { "$in": ["news", "tech"] } }`

```graphql
query FilteredProducts {
  products(
    where: {
      category: "electronics",
      price: { "$lte": 500 },
      stock: { "$gt": 0 }
    }
  ) {
    items {
      name
      price
    }
  }
}
```

---

## 4. Relationships & Deep Expansion

One of the primary benefits of the GraphQL API is fetching related data without multiple round-trips.

### Forward Relations
Fields defined as `relation` or `owner` in your schema.

```graphql
query GetPostWithAuthor {
  posts {
    items {
      title
      author_id { # This is an 'owner' field
        email
        role
      }
    }
  }
}
```

### Reverse Relations
ApexKit automatically discovers collections that point *to* the current one. If `comments` has a relation to `posts`, you can query comments from within a post.

```graphql
query GetBlogFeed {
  posts {
    items {
      title
      comments { # Auto-discovered reverse relation
        text
        created_at
      }
    }
  }
}
```

---

## 5. Custom Resolvers (Scripts)

You can extend the GraphQL schema with custom logic by creating a script with the `graphql` trigger. These resolvers can perform aggregations, call external APIs, or run system commands via `$cmd`.

**Example Query for a custom resolver:**
```graphql
query {
  calculateSystemHealth(detailed: true) # Custom field from a Script
}
```
*See the [Custom GraphQL Resolvers Guide](./custom-graphql-resolvers.md) for implementation details.*

---

## 6. Type Mapping Reference

| ApexKit Type | GraphQL Type | Notes |
| :--- | :--- | :--- |
| `string`, `text`, `email`, `url`, `date` | `String` | |
| `number` | `Float` | |
| `bool` | `Boolean` | |
| `json` | `JSON` | Returns a structured dynamic object/array. |
| `relation` (One) | `Object` | Returns the related record. |
| `relation` (Many) | `[Object]` | Returns an array of related records. |
| `owner` | `User` | Returns the system User object. |

---

## 7. Performance & Security

1.  **Dataloaders**: ApexKit uses an internal batching mechanism. If you fetch 50 `posts` and expand their `authors`, the backend only executes **2 SQL queries** (one for all posts, one for all unique authors) instead of 51.
2.  **Complexity Limits**: To prevent DoS attacks, queries are limited to a depth of **32 levels** and a complexity score of **2000**.
3.  **Policy Injection**: When a query is executed, the `auth` context of the requester is injected into the engine. Records that fail the collection's `read` policy are automatically filtered out of the results.