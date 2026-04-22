# 🕸️ Relations & Expansion API Documentation

**Version:** 0.1.0  
**Feature:** Relational Data Retrieval (Joins)

ApexKit solves the "N+1" query problem by allowing you to fetch related records, nested data, and reverse relationships in a single HTTP request using the `expand` query parameter. This is powered by a high-performance recursive subquery engine in the Rust backend.

---

## 1. Basic Syntax

To expand a relationship, add the `expand` parameter to any `GET` request for records.

**Endpoint:** `GET /api/v1/collections/{collection_id}/records`  
**Syntax:** `?expand={field_name}`

**Example:**  
Fetching `posts` and expanding the `author_id` relation.  
`GET /api/v1/collections/posts/records?expand=author_id`

**Response Structure:**  
The expanded data is injected into a dedicated `expand` object, leaving the original `data` payload intact.

```json
{
  "id": 101,
  "data": {
    "title": "Hello ApexKit",
    "author_id": "user_55" 
  },
  "expand": {
    "author_id": {
      "id": "user_55",
      "email": "john@app.io",
      "role": "editor"
    }
  }
}
```

---

## 2. Expansion Types

ApexKit automatically detects the type of relationship and handles the data formatting accordingly.

### A. Forward Relations (Direct)
Fields defined as type `relation` in the collection schema.
*   **One-to-One**: Returns a single object.
*   **One-to-Many**: Returns an array of objects.

### B. Owner Fields (System Users)
Fields defined as type `owner`. These link directly to the internal `users` table.
*   **Result**: Always returns a **single object** containing the user's ID, email, role, and metadata.

### C. Reverse Relations (Back-references)
ApexKit scans other collections to find links pointing back to the current record.
*   **Example**: If you query `posts` and request `?expand=comments`, ApexKit finds the `comments` collection has a relation field (e.g., `post_id`) pointing to `posts`.
*   **Result**: Returns an **array** of matching records.

---

## 3. Advanced Expansion Features

### Nested Expansion (Deep Joins)
You can traverse the graph multiple levels deep using **dot notation**.

**Syntax**: `?expand=field.sub_field`  
**Example**: Get **Posts**, expand their **Comments**, and then expand the **Author** of each comment.  
`?expand=comments.user_id`

### Multiple Expansions
Expand multiple unrelated fields by separating them with commas.

**Syntax**: `?expand=field1,field2`  
**Example**: `?expand=author_id,categories,tags`

### Relational Pagination (Limits & Offsets)
Avoid large payloads by limiting the number of related items returned per record.

**Syntax**: `field_name(limit, offset)`  
**Example**: Get posts and only the **top 5** most recent comments for each.  
`?expand=comments(5,0)`

---

## 4. Single Record Expansion

Relational expansion is equally powerful when fetching a single specific record.

**Endpoint**: `GET /collections/{id}/records/{record_id}?expand=...`  
**Example**:  
`GET /api/v1/collections/projects/records/1?expand=members,tasks(10,0).assigned_to`

---

## 5. Error Handling

If you request a relation that does not exist or has a typo, the API will not crash. Instead, it injects an error message into the specific key within the `expand` object.

**Request**: `?expand=non_existent_field`  
**Response**:
```json
{
  "id": 1,
  "expand": {
    "non_existent_field": {
      "error": "Relation 'non_existent_field' not defined in schema or no reverse lookup found."
    }
  }
}
```

---

## 6. Performance Best Practices

1.  **Use Limits**: When expanding Many-to-Many or Reverse relations (like `comments`), always provide a limit (e.g., `comments(20)`) to prevent fetching thousands of rows.
2.  **Depth Control**: While ApexKit supports deep nesting, aim for 2-3 levels maximum for optimal performance. Each level adds complexity to the underlying SQL query.
3.  **Indexing**: Ensure that the fields used for relations (e.g., `author_id`) are marked as `sql_indexed: true` in your schema to speed up join operations.

---

## 7. JavaScript SDK Usage

Expansion is natively supported in the `apex.collection().list()` and `get()` methods.

```javascript
import { apex } from './apiClient';

const posts = await apex.collection('posts').list({
    page: 1,
    expand: 'author_id,comments(5).user_id'
});

posts.items.forEach(post => {
    console.log("Author:", post.expand.author_id.email);
    console.log("Latest Commenter:", post.expand.comments[0]?.expand.user_id.email);
});
```