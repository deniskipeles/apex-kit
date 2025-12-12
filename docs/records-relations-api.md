# 🕸️ TinyBase Relations & Expansion API

**Version:** 2.4
**Feature:** Relational Data Retrieval (Joins)

TinyBase solves the "N+1" query problem by allowing you to fetch related records, nested data, and reverse relationships in a single HTTP request using the `expand` query parameter.

---

## 1. Basic Syntax

To expand a relationship, add the `expand` parameter to your `GET` request.

**Endpoint:** `GET /api/v1/collections/{collection_id}/records`

**Syntax:** `?expand=relation_name`

**Example:**
Fetching `posts` and expanding the `author` relation.

```http
GET /api/v1/collections/posts/records?expand=author
```

**Response Structure:**
The original record data remains in `data`. The expanded relations are injected into a new `expand` object.

```json
{
  "id": 1,
  "data": {
    "title": "Hello World",
    "content": "...",
    "author": 55 // The ID stored in the DB
  },
  "expand": {
    "author": [
      {
        "id": 55,
        "data": { "name": "John Doe", "email": "john@example.com" }
      }
    ]
  }
}
```

---

## 2. Advanced Expansion Features

TinyBase supports powerful expansion logic including nesting, pagination, and reverse lookups.

### A. Nested Expansion (Deep Joins)
You can traverse the graph as deep as needed using **dot notation**.

**Syntax:** `field.sub_field`

**Example:**
Get **Posts**, expand their **Comments**, and expand the **Author** of those comments.

`?expand=comments.author`

```json
"expand": {
  "comments": [
    {
      "id": 101,
      "data": { "text": "Nice post!" },
      "expand": {
        "author": [ { "id": 20, "data": { "name": "Jane" } } ]
      }
    }
  ]
}
```

### B. Multiple Expansions
Expand multiple different fields by separating them with commas.

**Syntax:** `field1, field2`

**Example:**
`?expand=author, comments`

### C. Pagination (Limit & Offset)
You can limit the number of related records returned to improve performance. This is applied per record found.

**Syntax:** `field(limit, offset)`

*   `limit`: Max records to return.
*   `offset`: Number of records to skip.

**Example:**
Get posts and the **top 5 most recent comments**:

`?expand=comments(5, 0)`

**Example:**
Get the *next* 5 comments (pagination):

`?expand=comments(5, 5)`

---

## 3. Types of Relations

The system handles three types of expansion logic automatically based on your schema.

### 1. Forward Relations
*   **Definition:** Defined in `schema.relations` (e.g., `post` has `author_id`).
*   **Result:** Returns an **Array** of records (usually length 1 for 1:1, or length 0 if broken).

### 2. Owner Field (User System)
*   **Definition:** Defined in `schema.fields` with type `owner`.
*   **Behavior:** Links directly to the internal `users` authentication table.
*   **Result:** Returns a **Single Object** (not an array).

**Example:** `?expand=created_by`
```json
"expand": {
  "created_by": {
    "id": 1,
    "email": "admin@tinybase.io",
    "role": "admin"
  }
}
```

### 3. Reverse Relations (Back-References)
*   **Definition:** Implicit. If you are querying `posts` and request `?expand=comments`, the system checks if the `comments` collection has a relation pointing back to `posts`.
*   **Result:** Returns an **Array** of records.

**How it works:**
1.  You request `?expand=comments` on `posts`.
2.  System looks for a collection named "comments".
3.  System scans "comments" schema for a relation targeting "posts".
4.  If found, it executes a reverse join query.

---

## 4. Error Handling (Graceful Failures)

If you request an invalid expansion (e.g., a typo in the field name, or a relation that doesn't exist), the API **will not crash** or fail the main request.

Instead, it injects an error object into the specific expansion key.

**Request:** `?expand=non_existent_field`

**Response:**
```json
{
  "id": 1,
  "data": { "title": "My Post" },
  "expand": {
    "non_existent_field": {
      "error": "Relation \"non_existent_field\" not defined in schema for \"posts\" or valid reverse lookup found"
    }
  }
}
```

---

## 5. Performance Tips

1.  **Use Limits:** Always use `(limit)` on reverse lookups (e.g., `comments(10)`) to prevent fetching thousands of child records accidentally.
2.  **Indexing:** Ensure columns used in relations are indexed in the database for speed.
3.  **Recursive Depth:** While technically unlimited, keep nesting depth reasonable (2-3 levels) to maintain response times. SQL Complexity grows with depth.

---

## 8. Expansion on Single Records

You can also expand relationships when fetching a specific record by ID. This is particularly powerful for "Detail Views" (e.g., fetching a Post and its top 5 Comments in one request).

**Endpoint:**
`GET /collections/{collection_id}/records/{record_id}?expand=relation_name`

### Syntax
The `expand` parameter supports the exact same syntax as the List endpoint:

1.  **Forward Relation:** `?expand=author` (Expands the `author` field).
2.  **Owner:** `?expand=created_by` (Expands the User object).
3.  **Reverse Relation:** `?expand=comments` (Finds records in the `comments` collection that point to this record).
4.  **Nested:** `?expand=comments.author` (Expands comments, then the author of those comments).
5.  **Pagination:** `?expand=comments(5,0)` (Fetches only the first 5 related records).

### Example: Fetch Post + Author + Top 5 Comments
**Request:**
`GET /api/v1/collections/posts/records/101?expand=author,comments(5)`

**Response:**
```json
{
  "id": 101,
  "data": {
    "title": "My Viral Post",
    "content": "...",
    "author": 55
  },
  "expand": {
    "author": [
      { 
        "id": 55, 
        "data": { "name": "Jane Doe", "email": "jane@example.com" } 
      }
    ],
    "comments": [
      { "id": 901, "data": { "text": "First!", "post_id": 101 } },
      { "id": 902, "data": { "text": "Great read.", "post_id": 101 } },
      { "id": 903, "data": { "text": "Thanks!", "post_id": 101 } },
      { "id": 904, "data": { "text": "Helpful.", "post_id": 101 } },
      { "id": 905, "data": { "text": "More please.", "post_id": 101 } }
    ]
  }
}
```

> **Performance Note:** When using `expand` on a single record, the response is **not cached** server-side to ensure the related data (which changes independently of the parent record) is always fresh.