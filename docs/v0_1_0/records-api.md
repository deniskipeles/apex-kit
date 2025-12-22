# ApexKit Records API Documentation

**Version:** 0.1.0
**Base URL:** `http://localhost:5000/api/v1`

The Records API is the core interface for interacting with data stored in ApexKit. It allows you to Perform CRUD (Create, Read, Update, Delete) operations, complex filtering, relational expansion, and high-performance search.

---

## 1. Authentication & Headers

Almost all record operations require authentication, depending on the **API Rules** configured for the specific collection.

**Headers:**
```http
Content-Type: application/json
Authorization: Bearer <YOUR_ACCESS_TOKEN>
```

---

## 2. The Record Object

A generic record in ApexKit consists of a system ID and a flexible JSON payload.

```json
{
  "id": 105,
  "data": {
    "title": "My Awesome Post",
    "slug": "my-awesome-post",
    "is_published": true,
    "views": 42,
    "author_id": 5
  }
}
```

*   **id** `(integer)`: The unique, auto-incrementing primary key of the record.
*   **data** `(object)`: The schema-defined content of the record.

---

## 3. CRUD Endpoints

### List Records
Fetch a paginated list of records from a collection.

**Endpoint:**
`GET /collections/{collection_id}/records`

**Query Parameters:**

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `page` | `int` | `1` | The page number to fetch. |
| `per_page` | `int` | `30` | Number of items per page (Max 100). |
| `sort` | `string` | `-id` | Field to sort by. Use `-` for descending order. |
| `filter` | `json` | `null` | JSON object for exact matching. |
| `expand` | `string` | `null` | Comma-separated list of relations to resolve. |

**Example Request:**
`GET /collections/1/records?page=1&per_page=10&sort=-created`

**Response:**
```json
[
  { "id": 1, "data": { "title": "Post A", ... } },
  { "id": 2, "data": { "title": "Post B", ... } }
]
```

---

### Get Single Record
Fetch a specific record by its ID.

**Endpoint:**
`GET /collections/{collection_id}/records/{record_id}`

**Response:**
```json
{
  "id": 1,
  "data": {
    "title": "Post A",
    "content": "..."
  }
}
```

---

### Create Record
Add a new record to a collection.

**Endpoint:**
`POST /collections/{collection_id}/records`

**Body:**
You must wrap your fields inside a `data` object.

```json
{
  "data": {
    "title": "New Project",
    "status": "active",
    "budget": 5000
  }
}
```

**Behavior:**
1.  **Validation:** The input is validated against the Collection Schema. If a field is `required` but missing, or has the wrong type, a `422 Unprocessable Entity` error is returned.
2.  **Relations:** If the schema defines a `relation`, ApexKit automatically updates the internal graph table.
3.  **Search:** If fields are marked as `indexed`, the full-text search index is updated immediately.

**Response:** `201 Created`
```json
{
  "id": 55,
  "data": { ... }
}
```

---

### Update Record
Modify an existing record. This is a partial update; fields not sent will remain unchanged.

**Endpoint:**
`PATCH /collections/{collection_id}/records/{record_id}`

**Body:**
```json
{
  "data": {
    "status": "archived"
  }
}
```

**Permissions:**
If the collection has an update rule like `"owner:author_id"`, the system verifies that `record.data.author_id` matches the ID of the currently logged-in user.

**Response:** `200 OK` (Returns the updated record)

---

### Delete Record
Permanently remove a record.

**Endpoint:**
`DELETE /collections/{collection_id}/records/{record_id}`

**Behavior:**
*   Deletes the record from the database.
*   **Cascading Cleanup:** Removes all relationship links pointing *to* or *from* this record in the graph table to ensure data integrity.
*   Removes the entry from the Search Index.

**Response:** `204 No Content`

---

## 4. Advanced Querying

### Filtering
ApexKit supports JSON-based filtering using the `filter` query parameter.

**Syntax:** `?filter={"field_name": value}`

*   **Exact Match:** `filter={"status": "active"}`
*   **Boolean:** `filter={"is_published": true}`
*   **Nested Data:** `filter={"meta.category": "tech"}`

*Note: Currently supports equality checks. Range queries (>, <) are available via SQL/Scripting but not the REST filter parameter yet.*

### Sorting
Use the `sort` parameter.
*   `sort=price` (Ascending)
*   `sort=-price` (Descending)
*   `sort=-created` (Newest first)

### Relationship Expansion (Joins)
ApexKit solves the "N+1" problem using the `expand` parameter. It fetches related records in a single HTTP request using optimized Recursive CTEs (Common Table Expressions).

**Prerequisite:** The collection schema must define a field with type `relation`.

**Syntax:** `?expand=field_name`

**Example:**
Fetching `comments` and expanding the `user_id` field to get the user's details.

`GET /collections/5/records?expand=user_id`

**Response:**
```json
{
  "id": 101,
  "data": {
    "text": "Great article!",
    "user_id": 12
  },
  "expand": {
    "user_id": [
      {
        "id": 12,
        "data": { "email": "alice@example.com", "role": "editor" }
      }
    ]
  }
}
```

**Nested Expansion:**
You can go deeper using dot notation: `?expand=post_id.author_id`

---

## 5. Search API

ApexKit offers two types of search mechanisms.

### A. SQL Search (Standard)
Searches directly against the database using `LIKE` queries on the JSON blob. Good for simple lookups on non-indexed fields.

**Endpoint:**
`GET /collections/{collection_id}/search?q={query}`

### B. Instant Search (Tantivy / High-Performance)
Uses a memory-mapped inverted index (similar to Elasticsearch/Solr) stored on disk. This allows for ultra-fast full-text search, typo tolerance, and ranking.

**Prerequisite:** Fields must have `indexed: true` in the Collection Schema.

**Endpoint:**
`GET /collections/{collection_id}/instant-search?q={query}`

**Response:**
Returns a lightweight array of hits with relevance scores.
```json
[
  {
    "id": 55,
    "score": 4.25,
    "snippet": {
      "title": "Introduction to Rust",
      "summary": "Rust is a systems programming language..."
    }
  }
]
```

---

## 6. Error Handling

The API returns standard HTTP status codes and a JSON error object.

**Structure:**
```json
{
  "error": "error_code",
  "message": "Human readable description",
  "details": { ... }, 
  "status": 4xx
}
```

| Status | Code | Meaning |
| :--- | :--- | :--- |
| `400` | `input_validation` | Invalid JSON or query parameters. |
| `401` | `unauthorized` | Missing or invalid JWT token. |
| `403` | `forbidden` | Authenticated, but Policy Rule (e.g., `admin` or `owner`) denied access. |
| `404` | `not_found` | Collection or Record ID does not exist. |
| `422` | `schema_validation_error` | Data payload violates schema (e.g., missing required field, wrong type). |
| `500` | `database_error` | Internal server or storage error. |

---

## 7. Relations (Manual Linking)

While schema-based relations are handled automatically, you can manually create graph edges between arbitrary records (useful for many-to-many tags or social follows).

### Create Relation edge
`POST /collections/{origin_col}/records/{origin_id}/relations`

**Body:**
```json
{
  "target_collection_id": 2,
  "target_record_id": 50,
  "relation_name": "liked_by"
}
```

### Delete Relation edge
`DELETE /collections/{origin_col}/records/{origin_id}/relations`

*(Body same as create)*