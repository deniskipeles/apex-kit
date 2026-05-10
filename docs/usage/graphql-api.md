# GraphQL API

ApexKit provides a dynamic GraphQL API that reflects your collection schemas in real-time.

## Endpoint
The GraphQL endpoint is available at:
`/api/v1/graphql`

## Auto-generated Schema

For every collection (e.g., `posts`), ApexKit generates:
- **Queries**: `posts(filter, sort, limit, offset)` and `post(id)`.
- **Mutations**: `createPost`, `updatePost`, `deletePost`.
- **Types**: A `Post` type with all your defined fields.

## Example Queries

### Fetching Records
```graphql
query {
  posts(filter: { status: "published" }, sort: "-created") {
    id
    title
    content
    author {
      email
      name
    }
  }
}
```

### Fetching a Single Record
```graphql
query {
  post(id: "rec123") {
    title
  }
}
```

## Example Mutations

### Creating a Record
```graphql
mutation {
  createPost(data: { title: "New GraphQL Post", status: "draft" }) {
    id
    title
  }
}
```

## Custom Resolvers (Edge Functions)

You can extend the GraphQL schema with custom logic using Edge Functions.

1. Create a script in the dashboard.
2. Set the trigger to `GraphQL`.
3. Define your custom Query or Mutation.

```javascript
// Example: Custom resolver to calculate analytics
export default async function(args) {
    const total = await $db.records.count('sales', { status: 'paid' });
    return { total };
}
```

In your GraphQL schema, you can now call:
```graphql
query {
  salesAnalytics {
    total
  }
}
```

## Using GraphQL with the SDK

The ApexKit SDK makes it easy to run GraphQL queries.

```javascript
const query = `
  query GetPosts($status: String) {
    posts(filter: { status: $status }) {
      id
      title
    }
  }
`;

const result = await apex.graphql(query, { status: 'published' });
```

## Why use GraphQL?
- **Efficiency**: Fetch exactly the fields you need, avoiding over-fetching.
- **Relational Depth**: Easily traverse complex relationships in a single tree.
- **Strong Typing**: Integration with tools like GraphQL Code Generator for TypeScript safety.
