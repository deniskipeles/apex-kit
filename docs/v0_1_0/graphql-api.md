
# ApexKit GraphQL API Documentation

**Version:** 0.1.0
**Endpoint:** `/graphql`
**Playground:** Open `/graphql` in your browser to access the interactive GraphiQL playground.

ApexKit provides a dynamic, **Read-Only GraphQL API**. The schema is automatically generated based on your Collections and their Relationships defined in the Admin Dashboard.

---

## 1. Overview

The GraphQL API is designed for efficient data retrieval. It solves the "N+1" query problem automatically using Dataloaders, allowing you to fetch deeply nested relational data in a single network request.

**Current Limitations:**
*   **Read-Only:** Mutations (Create, Update, Delete) are currently handled via the REST API. GraphQL is for querying only.
*   **Dynamic Schema:** Adding a new Collection immediately makes it available in the GraphQL schema.

---

## 2. The Query Structure

For every collection you create (e.g., `posts`), the API generates a top-level field with the same name.

### Basic Fetch
Fetch a list of records.

```graphql
query {
  posts {
    id
    title
    is_published
  }
}
```

---

## 3. Pagination

The API supports `limit` and `offset` arguments on all collection root fields.

*   `limit`: (Int) Max number of records to return (Default: 100).
*   `offset`: (Int) Number of records to skip.

```graphql
query GetPageTwo {
  posts(limit: 10, offset: 10) {
    id
    title
  }
}
```

---

## 4. Filtering (`where`)

To filter data, use the `where` argument. This argument accepts a **JSON Scalar**. The syntax follows the **ApexKit Filters API** (MongoDB-style).

**Syntax Reference:**
*   `{ "field": "value" }` (Equality)
*   `{ "field": { "$gt": 10 } }` (Operators: `$eq`, `$neq`, `$gt`, `$gte`, `$lt`, `$lte`, `$like`, `$in`)
*   `{ "$or": [...] }` (Logic)

**Example Query:**
Fetch electronics under $500.

```graphql
query FilteredProducts {
  products(
    where: {
      category: "electronics",
      price: { $lt: 500 }
    }
  ) {
    id
    name
    price
  }
}
```

**Example Complex Logic:**
Fetch posts that are either "featured" OR have more than 1000 views.

```graphql
query PopularPosts {
  posts(
    where: {
      $or: [
        { is_featured: true },
        { views: { $gt: 1000 } }
      ]
    }
  ) {
    title
    views
  }
}
```

---

## 5. Relationships & Expansion

The power of GraphQL lies in fetching related data. ApexKit automatically maps:
1.  **Forward Relations** (e.g., `post.author`).
2.  **Reverse Relations** (e.g., `author.posts`).

*Note: Relations must be defined in your Collection Schema for them to appear here.*

**Example:**
Fetch Users, their Posts, and the Comments on those posts.

```graphql
query UserActivity {
  users {
    id
    email
    
    # Reverse Relation: Fetch posts where author_id == user.id
    posts {
      id
      title
      
      # Reverse Relation: Fetch comments where post_id == post.id
      comments {
        id
        text
      }
    }
  }
}
```

**Performance:**
The backend uses **Dataloaders**. Even if you fetch 50 users, the system will only execute **3 SQL queries** total (one for users, one for posts, one for comments) instead of 50+ queries.

---

## 6. Type Mapping

ApexKit Schema types map to GraphQL types as follows:

| ApexKit Type | GraphQL Type | Notes |
| :--- | :--- | :--- |
| `string`, `text`, `email`, `url`, `file` | `String` | |
| `number` | `Float` | |
| `bool` | `Boolean` | |
| `json` | `String` | Returns raw JSON string |
| `relation` | `Object` or `[Object]` | Depends on One-to-One vs Many-to-Many |
| `owner` | `User` (Object) | Links to system user |

---

## 7. Example: React / Apollo Client

```javascript
import { gql, useQuery } from '@apollo/client';

const GET_DASHBOARD = gql`
  query GetDashboard {
    # 1. Fetch Users
    users(limit: 5) {
      email
    }
    
    # 2. Fetch Recent High-Priority Tickets
    tickets(
      limit: 10, 
      where: { priority: "high", status: { $neq: "closed" } }
    ) {
      id
      subject
      created_at
      
      # Expand assigned user
      assigned_to {
        email
      }
    }
  }
`;

function Dashboard() {
  const { loading, error, data } = useQuery(GET_DASHBOARD);

  if (loading) return <p>Loading...</p>;
  if (error) return <p>Error :(</p>;

  return (
    <div>
      <h2>High Priority Tickets</h2>
      {data.tickets.map(ticket => (
        <div key={ticket.id}>
           <b>{ticket.subject}</b> - {ticket.assigned_to?.email}
        </div>
      ))}
    </div>
  );
}
```