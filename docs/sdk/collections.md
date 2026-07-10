# Collections & Records SDK Reference

The `apex.collection(id)` namespace provides access to CRUD, querying, and multi-modal vector/text search operations for a specific collection.

## Method Signatures

- `collection(id).list(options?)`
- `collection(id).get(recordId, options?)`
- `collection(id).create(data)`
- `collection(id).update(recordId, data)`
- `collection(id).patch(recordId, data)`
- `collection(id).delete(recordId)`
- `collection(id).searchRecordsWithSQLQueryEngine(query)`
- `collection(id).searchRecordsWithOSE(query, options?)`
- `collection(id).searchRecordsInstantlyWithOSE(query)`
- `collection(id).searchVectorWithVector(field, vector, options?)`
- `collection(id).searchVectorWithText(queryText, options?)`
- `collection(id).searchImageVectorWithImage(imageData, limit?)`
- `collection(id).searchImageVectorWithText(queryText, limit?)`
- `collection(id).getVector(recordId)`
- `collection(id).addRelation(originRecordId, targetCollectionId, targetRecordId, relationName)`
- `collection(id).removeRelation(originRecordId, targetCollectionId, targetRecordId, relationName)`

---

## 1. Listing Records

List and filter records with sorting, pagination, and multi-level joins.

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

console.log(`Fetched ${result.items.length} of ${result.total} posts.`);
```

### `QueryOptions` Specification:
- `page`: Page index starting at 1.
- `per_page`: Number of elements to return.
- `sort`: Sort attributes. Prefix with `-` for descending order (e.g. `"-created"`, `"title"`).
- `filter`: SQL-like filter string or JSON evaluation criteria.
- `expand`: Comma-separated fields to expand (resolves linked relations automatically).
- `fields`: Comma-separated list of attributes to return, pruning payload size.

---

## 2. CRUD Operations

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

---

## 3. Full-Text Search (Tantivy / OSE)

To use full-text search, fields must have `ose_indexed: true` in their collection definition schema.

### Standard Full-Text Search
```typescript
const searchResults = await apex.collection('posts').searchRecordsWithOSE("tech tutorial", {
    page: 1,
    per_page: 10
});
```

### Autocomplete Instant Search
Fuzzy, low-latency search optimized for autocomplete search boxes:
```typescript
const hits = await apex.collection('products').searchRecordsInstantlyWithOSE("iphne");
// Returns: [{ id: 1, score: 2.1, snippet: { name: "<b>iPhone</b> 15" } }]
```

---

## 4. Multi-Modal Vector Search (Semantic/AI)

Search collection records by semantic meaning using high-dimensional vector embeddings.

### Search with Query Text (Converts Text to Embeddings on the fly)
```typescript
const matches = await apex.collection('docs').searchVectorWithText("How to configure S3?", {
    per_page: 5
});
```

### Search with raw Coordinates Vector
```typescript
const rawVector = [0.12, -0.45, 0.88, ...];
const matches = await apex.collection('docs').searchVectorWithVector('content_vector', rawVector, {
    per_page: 5
});
```

### Vision/Image Search with Base64 Image
```typescript
const base64Image = "data:image/png;base64,...";
const matches = await apex.collection('products').searchImageVectorWithImage(base64Image, 10);
```

### Vision/Image Search with Description Text
```typescript
const matches = await apex.collection('products').searchImageVectorWithText("red leather jacket", 10);
```

### Retrieve Record Vectors Coordinates
```typescript
const vectors = await apex.collection('docs').getVector(record.id);
// Returns: [{ field_name: "content", vector: [...], model: "bge-small" }]
```

---

## 5. GraphQL Queries & Mutations

Every environment generates a fully-compliant dynamic GraphQL Schema automatically.

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

const response = await apex.graphql(query, { id: "10" });
```

---

## 6. Graph Relationships

Add and remove directional tags or multi-directional links connecting arbitrary collection records.

```typescript
// Add relation
await apex.collection('posts').addRelation(
    postId,
    'tags',
    tagId,
    'post_tags'
);

// Remove relation
await apex.collection('posts').removeRelation(
    postId,
    'tags',
    tagId,
    'post_tags'
);
```

---

## 7. SQL Analytical Query Engine

Execute advanced analytical aggregation queries over collection data.

```typescript
const result = await apex.collection('sales').searchRecordsWithSQLQueryEngine({
    "select": [
        "category",
        { "fn": "sum", "field": "total", "as": "revenue" }
    ],
    "where": { "status": "completed" },
    "group_by": ["category"]
});
```
