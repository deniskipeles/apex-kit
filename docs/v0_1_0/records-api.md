# 📄 Records API Documentation

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

The Records API is the primary interface for managing data stored in ApexKit Collections. It supports full CRUD operations, complex MongoDB-style filtering, relational expansion (joins), and high-performance full-text search.

---

## 1. Authentication & Scoping

Requests must be authenticated using a JWT token or an API Key. Scoping is handled automatically via the URL path.

*   **Headers:**
    ```http
    Authorization: Bearer <JWT_TOKEN>
    # OR
    x-api-key: <YOUR_API_KEY>
    ```

*   **URL Contexts:**
    *   **Root**: `/api/v1/collections/...`
    *   **Tenant**: `/tenant/{tenant_id}/api/v1/collections/...`
    *   **Sandbox**: `/sandbox/{session_id}/api/v1/collections/...`

---

## 2. The Record Object

A record consists of system metadata and a flexible user-defined JSON payload stored in the `data` field.

```json
{
  "id": 105,
  "data": {
    "title": "My Awesome Post",
    "status": "published",
    "views": 42,
    "author_id": "user_77"
  },
  "created": "2024-02-14T10:00:00Z",
  "updated": "2024-02-14T11:30:00Z",
  "expand": {
    "author_id": { "id": "user_77", "email": "alice@app.com" }
  }
}
```

---

## 3. CRUD Endpoints

### List Records
Fetch a paginated list of records.
*   **GET** `/collections/{id}/records`
*   **Query Parameters:**
    *   `page`: (Int) Default 1.
    *   `per_page`: (Int) Default 30, Max 100.
    *   `sort`: (String) e.g., `-created,title`. Use `-` for descending.
    *   `filter`: (JSON String) MongoDB-style filter.
    *   `expand`: (String) Comma-separated relations to resolve.

### Get Single Record
*   **GET** `/collections/{id}/records/{record_id}`
*   **Params:** Supports `expand` query parameter.

### Create Record
*   **POST** `/collections/{id}/records`
*   **Body:** Wrap your fields in a `data` object.
*   **Behavior:** Fields of type `owner` or `date` with `auto: true` will be injected automatically if missing.

### Update Record (Partial)
*   **PATCH** `/collections/{id}/records/{record_id}`
*   **Body:** Only include fields you wish to change.

### Delete Record
*   **DELETE** `/collections/{id}/records/{record_id}`
*   **Behavior**: Cascades to the `_relations` table, deleting all links pointing to or from this record.

---

## 4. Analytical Query Engine

For complex reporting and aggregations, use the advanced query endpoint.

*   **POST** `/collections/{id}/query`
*   **Body Schema:**
```json
{
  "select": [
    "category",
    { "fn": "sum", "field": "price", "as": "total_sales" },
    { "fn": "avg", "field": "price", "as": "average_price" },
    { "fn": "count", "field": "id", "as": "count" }
  ],
  "filter": { "status": "completed" },
  "group_by": ["category"],
  "sort": "-total_sales",
  "pipeline": [
    { "op": "cumulative", "args": { "field": "total_sales", "output_field": "running_total" } }
  ]
}
```

---

## 5. Relationships & Expansion

ApexKit solves the "N+1" problem by allowing you to fetch related records in a single request.

*   **Syntax**: `?expand=author_id,comments.user_id`
*   **Pagination on Relations**: `?expand=comments(5,0)` (Fetch first 5 comments).

**Types of Relations:**
1.  **Forward**: Link stored in the record (e.g., `post.author_id`).
2.  **Reverse**: Back-references (e.g., `author.posts`).
3.  **Owner**: Direct link to the system `users` table.

---

## 6. Search API

### OSE Instant Search (Recommended)
Uses the high-performance **Tantivy** index. Supports fuzzy matching, typo-tolerance, and high-speed autocomplete.
*   **Requirement**: Fields must have `ose_indexed: true` in schema.
*   **GET** `/collections/{id}/instant-search?q=query&limit=10`

### SQL Search
Standard `LIKE` queries against the database JSON blob.
*   **GET** `/collections/{id}/search?q=query`

### Vector Search (AI)
Semantic search based on meaning rather than keywords.
*   **Requirement**: Fields must have `vectorize: true`.
*   **POST** `/collections/{id}/search-text-vector`
*   **Body**: `{ "query_text": "Items about science", "limit": 5 }`

---

## 7. Data Import / Export

### Import
Upload a CSV or JSON array to create records in bulk.
*   **POST** `/admin/import-data` (Multipart)
*   **Params**: `collection_name`, `file`.

### Export
Download an entire collection.
*   **GET** `/admin/export-data/{id}?format=json|csv`

---

## 8. Error Codes

| Status | Code | Meaning |
| :--- | :--- | :--- |
| `400` | `input_validation` | Malformed JSON or invalid parameters. |
| `403` | `forbidden` | Policy Rule (RLS) denied access. |
| `404` | `not_found` | Record or Collection does not exist. |
| `422` | `validation_error` | Data violates schema constraints (e.g., unique, required). |
| `500` | `database_error` | Internal storage failure. |