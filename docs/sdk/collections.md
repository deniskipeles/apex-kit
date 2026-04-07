# Collections & Records

The `apex.collection(id)` namespace provides access to CRUD, querying, and search operations for a specific collection.

## Methods

- `collection(id).list(options?)`
- `collection(id).get(recordId, options?)`
- `collection(id).create(data)`
- `collection(id).update(recordId, data)`
- `collection(id).patch(recordId, data)`
- `collection(id).delete(recordId)`
- `collection(id).searchRecordsWithSQL(query)`
- `collection(id).searchRecordsWithOSE(query)`
- `collection(id).searchRecordsInstantlyWithOSE(query)`
- `collection(id).searchVector(field, vector, limit?)`
- `collection(id).searchTextVector(queryText, limit?)`
- `collection(id).getVector(recordId)`
- `collection(id).addRelation(originRecordId, targetCollectionId, targetRecordId, relationName)`
- `collection(id).removeRelation(originRecordId, targetCollectionId, targetRecordId, relationName)`

### Listing Records

List and filter records with sorting, pagination, and joins.

```typescript
const result = await apex.collection('posts').list({
    page: 1,
    per_page: 20,
    sort: '-created', // Newest first
    filter: {
        "status": "published",
        "category": { "$in": ["tech", "news"] }
    },
    expand: 'author_id,comments(5).user_id' // Join relations + nested expansion
});

console.log(result.items, result.total);
```

The `QueryOptions` object supports:
- `page`: Page number (starting at 1).
- `per_page`: Number of items per page.
- `sort`: Sorting criteria (e.g., `"-created"`, `"title"`).
- `filter`: A string or record filter.
- `expand`: Comma-separated fields to expand (e.g., `"author_id"`).
- `fields`: Comma-separated fields to include in the response.

### CRUD Operations

```typescript
// Create
const record = await apex.collection('posts').create({
    title: "New Post",
    content: "Content..."
});

// Update (Full Replace)
await apex.collection('posts').update(record.id, {
    title: "Updated Title",
    content: "Updated Content...",
    status: "draft"
});

// Patch (Partial Update)
await apex.collection('posts').patch(record.id, {
    status: "published"
});

// Get a Single Record
const post = await apex.collection('posts').get(record.id, { expand: 'author_id' });

// Delete
await apex.collection('posts').delete(record.id);
```

### High-Performance Search

#### Instant Search (Tantivy/OSE)

Fast fuzzy search for autocomplete. Requires `ose_indexed: true` on the collection fields.

```typescript
const hits = await apex.collection('products').searchRecordsInstantlyWithOSE("iphne");
// Returns: [{ id: 1, score: 2.1, snippet: { name: "<b>iPhone</b> 15" } }]
```

#### Vector Search (Semantic/AI)

Search by meaning using vector embeddings. Requires `vectorize: true`.

```typescript
// Search using a query string
const matches = await apex.collection('docs').searchTextVector("How to reset password?");

// Search using a raw vector
const vectorMatches = await apex.collection('docs').searchVector('content_vector', [0.12, 0.55, ...]);

// Get the vectors for a record
const vectors = await apex.collection('docs').getVector(record.id);
```

### GraphQL

A dynamic GraphQL schema is automatically generated for every environment.

```typescript
const query = `
  query GetProfile($id: ID!) {
    users(where: { id: $id }) {
      items {
        email
        posts {
          title
        }
      }
    }
  }
`;

const data = await apex.graphql(query, { id: "10" });
```

### Relations

ApexKit supports complex many-to-many and one-to-many relations.

```typescript
// Add a relation
await apex.collection('posts').addRelation(
    postId,
    'tags',
    tagId,
    'post_tags'
);

// Remove a relation
await apex.collection('posts').removeRelation(
    postId,
    'tags',
    tagId,
    'post_tags'
);
```

### Analytical Query Engine

Execute complex aggregations and pipelines.

```typescript
const result = await apex.collection('sales').searchRecordsWithSQL({
    "select": [
        "category",
        { "fn": "sum", "field": "total", "as": "revenue" }
    ],
    "where": { "status": "completed" },
    "group_by": ["category"]
});
```

For more complex analysis, see the server-side query language documentation.
