# 🔮 GraphQL API Documentation

**Version:** 0.1.0  
**Endpoint:** `/graphql`  
**Playground:** Interactive GraphQL Playground available at `/graphql` (or scoped URLs: `/tenant/{tenant_id}/graphql`, `/sandbox/{session_id}/graphql`).

ApexKit features an automatically generated, dynamic GraphQL API that reflects your **Collections**, **Relationships**, and **Security Policies** in real-time. The engine eliminates the classic N+1 query problem through internal **Dataloaders**, allowing deeply nested graphs to resolve in minimal SQL operations.

---

## 1. Endpoints & Multi-Tenant Scoping

ApexKit routes GraphQL operations according to your current execution scope:

| Scope Context | GraphQL Execution Endpoint | Interactive Playground |
| :--- | :--- | :--- |
| **Root Application** | `POST /graphql` | `GET /graphql` |
| **Tenant** | `POST /tenant/{tenant_id}/graphql` | `GET /tenant/{tenant_id}/graphql` |
| **Sandbox** | `POST /sandbox/{session_id}/graphql` | `GET /sandbox/{session_id}/graphql` |

### Authentication
Include your JWT token in the HTTP request headers:
```http
Authorization: Bearer <YOUR_JWT_TOKEN>
# OR
x-api-key: <YOUR_API_KEY>
Content-Type: application/json
```

All collection-level policies (`read`, `create`, `update`, `delete`) and user access rules (`policy_users`) are strictly evaluated for every GraphQL operation.

---

## 2. Querying Collections

For every registered collection (e.g. `posts`, `products`), ApexKit creates a root query field returning a paginated list object (`{CollectionName}List`):

```graphql
query GetPublishedPosts {
  posts(
    limit: 10,
    offset: 0,
    where: {
      status: "published"
    }
  ) {
    total
    items {
      id
      title
      content
      status
    }
  }
}
```

### Pagination Arguments
* **`limit`** (`Int`): Maximum number of records to return (defaults to `100`).
* **`offset`** (`Int`): Number of records to skip from the beginning of the result set.

---

## 3. Advanced Filtering (`where`)

The `where` argument accepts a dynamic **JSON Scalar** adhering to the MongoDB-style filter syntax. Filter trees compile down to parameterized SQLite `WHERE` clauses alongside active Row-Level Security (RLS) constraints:

```graphql
query FilteredCatalog {
  products(
    where: {
      category: { "$in": ["electronics", "appliances"] },
      price: { "$gte": 100, "$lte": 1500 },
      stock: { "$gt": 0 }
    }
  ) {
    total
    items {
      id
      name
      price
      stock
    }
  }
}
```

---

## 4. Traversing Graph Relationships

### A. Forward Relationships (`relation` / `owner`)
When a record holds a foreign key field pointing to another collection or user, it resolves directly as a single entity:

```graphql
query GetPostWithAuthorAndCategory {
  posts {
    items {
      id
      title
      author_id {  # 'owner' field resolving to the _AuthUser type
        id
        email
        role
      }
      category {   # 'relation' field resolving to the Category type
        id
        name
      }
    }
  }
}
```

---

### B. Reverse Relationships & Collections
Collections referenced by other entities automatically expose reverse fields. When a parent collection expands multiple dependent children, it returns a list type with nested pagination and filtering:

```graphql
query GetPostWithComments {
  posts {
    items {
      id
      title
      comments(limit: 5, where: { approved: true }) {
        total
        items {
          id
          body
          created
        }
      }
    }
  }
}
```

---

## 5. System Fields & User Management (`_AuthUser`)

To avoid collisions with custom user collections, internal user queries, mutations, and types are prefixed with an underscore (`_`):

* **Type:** `_AuthUser`
* **List Query:** `_users(limit: Int, offset: Int, search: String): _AuthUserList`

```graphql
query SearchUsers {
  _users(limit: 20, search: "alex") {
    total
    items {
      id
      email
      role
    }
  }
}
```

---

## 6. Mutations (Create, Update, Delete)

ApexKit automatically compiles type-safe input objects for each collection (`Create{Type}Input` and `Update{Type}Input`):

### Create Record
```graphql
mutation CreateNewPost {
  createPosts(
    data: {
      title: "Building Modern Backends with ApexKit",
      content: "ApexKit combines Rust and SQLite for low-latency APIs.",
      status: "published"
    }
  ) {
    id
    title
    created
  }
}
```

### Update Record
```graphql
mutation UpdateExistingPost {
  updatePosts(
    id: "105",
    data: {
      status: "archived"
    }
  ) {
    id
    status
    updated
  }
}
```

### Delete Record
```graphql
mutation RemovePost {
  deletePosts(id: "105")
}
```

---

## 7. Extending with Custom Resolvers

You can attach custom queries, mutations, or computed fields to existing types by creating scripts with `trigger_type: "graphql"`:

```typescript
// Script: calculate-cart-total
// Trigger: graphql | Path: ./webhooks/calculate-cart-total.ts

export const graphql = {
  parent: "Query",
  name: "calculateCartTotal",
  args: {
    cartId: "ID!"
  },
  returnType: "JSON"
};

export default async function (req: Request) {
  const { cartId } = await req.json();

  const items = await $db.records.list("cart_items", {
    filter: { cart_id: cartId }
  });

  const total = items.items.reduce((sum, item) => sum + (item.data.price * item.data.quantity), 0);

  return new Response({
    cartId,
    itemCount: items.items.length,
    total
  });
}
```

**Querying the Custom Resolver:**
```graphql
query {
  calculateCartTotal(cartId: "cart_42")
}
```

---

## 8. Type Mapping Reference

| Collection Field Type | Generated GraphQL Type | Notes |
| :--- | :--- | :--- |
| `string`, `text`, `email`, `url`, `date` | `String` | UTF-8 encoded text / ISO 8601 strings. |
| `number` | `Float` | 64-bit IEEE floating-point numbers. |
| `boolean` | `Boolean` | `true` / `false`. |
| `json`, `geopoint` | `JSON` | Structured objects and dynamic arrays. |
| `owner` | `_AuthUser` | Populated user profile (subject to `policy_users`). |
| `relation` (`relation_type: "one"`) | `{TargetType}` | Single expanded entity map. |
| `relation` (`relation_type: "many"`) | `{TargetType}List` | Paginated list with `total` and `items`. |

---

## 9. Performance & Security Safeguards

1. **Automatic Batching (Dataloaders):** Resolving nested relationships across 100 records performs a single batched `WHERE id IN (...)` lookup, preventing N+1 database queries.
2. **Execution Safeguards:**
   * **Max Query Depth:** Limited to **32 levels** of nesting.
   * **Max Complexity:** Capped at **2000 complexity points**.
3. **Production Introspection Lock:** When running with `APP_ENV=production`, schema introspection is automatically disabled to protect API structure from public enumeration.