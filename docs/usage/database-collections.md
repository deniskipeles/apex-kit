# Database and Collections

ApexKit uses a schema-driven approach to data management. This guide covers how to define, manage, and query your data.

## Collections Overview

A **Collection** is a grouping of records that share the same schema. ApexKit supports two main types of collections:
1. **Base Collections**: Standard data storage.
2. **Auth Collections**: Specialized collections for managing users and authentication.

## Schema Fields

ApexKit supports a wide variety of field types:

| Type | Description |
| :--- | :--- |
| `Text` | Standard string. |
| `Number` | Integers or floats. |
| `Bool` | Boolean values. |
| `Date` | ISO 8601 timestamps. |
| `JSON` | Arbitrary nested objects. |
| `Relation` | Links to records in other collections. |
| `File` | References to uploaded files. |
| `Vector` | High-dimensional arrays for AI semantic search. |

## Managing Collections

You can manage collections via the **Admin Dashboard** or the **CLI**.

### Via Dashboard
1. Navigate to **Collections**.
2. Click **New Collection**.
3. Define your fields and their constraints (e.g., `unique`, `required`).

### Schema Constraints
- **Required**: The field must be present in every record.
- **Unique**: No two records can have the same value for this field.
- **Default Value**: Automatically populated if not provided.

## Querying Data

ApexKit provides a powerful query engine that supports filtering, sorting, and relational expansion.

### Basic List
```javascript
const posts = await apex.collection('posts').list({
    page: 1,
    per_page: 20,
    sort: '-created' // Descending by creation date
});
```

### Filtering
Filters use a JSON-based syntax or a simple string format.
```javascript
const filtered = await apex.collection('posts').list({
    filter: { status: 'published' }
});
```

### Relational Expansion
If you have a `Relation` field (e.g., `author` linking to `users`), you can expand it in a single request.
```javascript
const posts = await apex.collection('posts').list({
    expand: 'author'
});
// result.items[0].expand.author will contain the user record
```

## Advanced SQL Queries
For complex use cases, ApexKit allows structured queries that resemble SQL but are safe and performant.
```javascript
const stats = await apex.collection('sales').searchRecordsWithSQL({
    select: [
        { fn: 'sum', field: 'amount', as: 'total_revenue' },
        'category'
    ],
    group_by: ['category']
});
```

## Vector Search
If a field is marked as `Vector`, you can perform semantic search.
```javascript
const similar = await apex.collection('articles').searchTextVector("AI in healthcare", 5);
```
This uses the embedded AI engine to find records based on meaning rather than keyword matching.
