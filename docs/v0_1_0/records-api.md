# 📄 Records API Documentation

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

The Records API provides the primary interface for managing data stored within ApexKit Collections. It supports full CRUD operations, MongoDB-style JSON filtering, recursive relational expansion (joins), on-the-fly Tantivy full-text search, and multi-modal vector similarity lookups.

---

## 1. Authentication & Scoping

All requests must be authenticated using a JWT token or an API Key. Scoping is handled automatically based on the request URL:

* **Authentication Headers:**
  ```http
  Authorization: Bearer <JWT_TOKEN>
  # OR
  x-api-key: <YOUR_API_KEY>
  Content-Type: application/json
  ```

* **URL Context Patterns:**
  * **Root Scope:** `/api/v1/collections/...`
  * **Tenant Scope:** `/tenant/{tenant_id}/api/v1/collections/...`
  * **Sandbox Scope:** `/sandbox/{session_id}/api/v1/collections/...`

---

## 2. The Record Object

Records contain system metadata (`id`, `created`, `updated`) and a dynamic user payload stored in SQLite's native `JSONB` format (`data`). Relational expansions appear in the separate `expand` map.

```json
{
  "id": 105,
  "data": {
    "title": "Getting Started with ApexKit",
    "status": "published",
    "views": 1420,
    "author_id": 42
  },
  "created": "2026-06-15T10:00:00Z",
  "updated": "2026-06-15T11:30:00Z",
  "expand": {
    "author_id": {
      "id": 42,
      "email": "author@example.com",
      "role": "user"
    }
  }
}
```

---

## 3. CRUD Endpoints

Path parameters for collections accept either the collection **numeric ID** (e.g. `5`) or the **name** (e.g. `posts`).

### A. List Records
Retrieve a paginated list of records matching filters, sorting, and relational expansions:
* **Endpoint:** `GET /collections/{id_or_name}/records`
* **Query Parameters:**
  * `page`: Page number (default: `1`).
  * `per_page`: Records per page (default: `30`, max: `100`).
  * `sort`: Comma-separated sort keys. Prefix with `-` for descending (e.g. `?sort=-created,title`).
  * `filter`: URL-encoded MongoDB-style JSON filter.
  * `expand`: Comma-separated relationship paths.
  * `fields`: Comma-separated projection list of fields to include or exclude.

---

### B. Get Single Record
* **Endpoint:** `GET /collections/{id_or_name}/records/{record_id}`
* **Query Parameters:**
  * `expand`: Comma-separated relationship paths (e.g. `?expand=author_id,comments(5,0).user_id`).

---

### C. Create Record
* **Endpoint:** `POST /collections/{id_or_name}/records`
* **Behavior:**
  * System keys (`id`, `_id`, `created`, `updated`, `expand`) in the payload are automatically stripped.
  * `owner` and `date` fields with `auto: true` are automatically populated if omitted.
  * Unique and composite constraints are evaluated atomically.
  * Trigger hooks (`before_create_record`, `after_create_record`) are executed.
  * If the schema contains fields marked with `vectorize: true` or `ose_indexed: true`, background index jobs are enqueued automatically.

#### Request Body
```json
{
  "title": "High Performance SQLite Storage",
  "content": "Exploring WAL mode and memory-mapped files.",
  "status": "published",
  "category_id": 3
}
```

---

### D. Update Record (Partial Update)
* **Endpoint:** `PATCH /collections/{id_or_name}/records/{record_id}` (or `PUT`)
* **Behavior:** Merges submitted fields with existing record data, validates against schema constraints, updates the `updated` timestamp, and triggers `before_update_record` / `after_update_record` hooks.

#### Request Body
```json
{
  "status": "archived",
  "views": 1500
}
```

---

### E. Delete Record
* **Endpoint:** `DELETE /collections/{id_or_name}/records/{record_id}`
* **Behavior:**
  * Checks cascading rules (`cascade_on_target_delete`).
  * Deletes record row from `records`, unique index values from `_unique_values`, graph edges from `_relations`, and embedding vectors from `vectors`.
  * Removes document from the Tantivy full-text index.
  * Emits `DbEvent::Delete` over real-time WebSockets and SSE.

---

## 4. Advanced Analytical SQL Query Engine

For complex reporting, multi-column group-bys, aggregations, and post-processing pipelines:

* **Endpoint:** `POST /collections/{id_or_name}/query`

#### Request Body
```json
{
  "select": [
    "category",
    { "fn": "sum", "field": "price", "as": "total_sales" },
    { "fn": "avg", "field": "price", "as": "avg_price" },
    { "fn": "count", "field": "id", "as": "order_count" }
  ],
  "where": {
    "status": "completed",
    "price": { "$gt": 0 }
  },
  "group_by": ["category"],
  "sort": "-total_sales",
  "limit": 10,
  "pipeline": [
    {
      "op": "cumulative",
      "args": { "field": "total_sales", "output_field": "running_revenue" }
    }
  ]
}
```

---

## 5. Relationships & Expansion (`expand`)

ApexKit eliminates the N+1 query problem by resolving relational joins in a single request.

* **Forward Relations:** Resolves foreign keys to target record maps.
* **Owner Relations:** Resolves user IDs to `_AuthUser` profiles.
* **Reverse Relations:** Resolves collections pointing back to the current entity (e.g. `posts.comments`).
* **Pagination on Relations:** Limit nested arrays with `comments(5,0)`.

```http
GET /api/v1/collections/posts/records?expand=author_id,category_id,comments(5,0).user_id
```

---

## 6. Search Endpoints

### A. OSE Full-Text Search (Tantivy)
Fuzzy, typo-tolerant keyword search with pagination (requires `ose_indexed: true` in schema):
* **Endpoint:** `GET /collections/{id_or_name}/search?q=query_text&page=1&per_page=20`

### B. OSE Instant Autocomplete Search
Ultra-low-latency prefix matching with highlight snippets:
* **Endpoint:** `GET /collections/{id_or_name}/instant-search?q=query_text&limit=10`

### C. Multimodal AI Vector Search
* **Text-to-Text Vector Search:** `POST /collections/{id_or_name}/search-vector-with-text`
  ```json
  { "query_text": "distributed consensus algorithms", "per_page": 5 }
  ```
* **Text-to-Image Cross-Modal Search:** `POST /collections/{id_or_name}/search-image-vector-with-text`
  ```json
  { "query_text": "red sports coupe at night", "limit": 10 }
  ```
* **Image-to-Image Similarity Search:** `POST /collections/{id_or_name}/search-image-vector-with-image`
  ```json
  { "image_data": "data:image/jpeg;base64,...", "limit": 10 }
  ```
* **Raw Vector Search:** `POST /collections/{id_or_name}/search-vector-with-vector`
  ```json
  { "field": "content", "vector": [0.12, -0.45, 0.88], "limit": 10 }
  ```
* **Get Stored Vector Coordinates:** `GET /collections/{id_or_name}/get-vector/{record_id}`

---

## 7. TypeScript SDK Usage (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('JWT_OR_API_KEY');

// 1. Create a record
const record = await apex.collection('posts').create({
  title: 'Modern BaaS Architecture',
  content: 'Single-node design patterns and SQLite WAL optimization.',
  status: 'published'
});

// 2. Fetch with Joins and Complex Filters
const result = await apex.collection('posts').list({
  page: 1,
  per_page: 20,
  sort: '-created',
  filter: {
    status: 'published',
    views: { $gt: 100 }
  },
  expand: 'author_id,comments(5,0).user_id'
});

// 3. Instant Search (Tantivy)
const hits = await apex.collection('posts').searchRecordsInstantlyWithOSE('sqlite');

// 4. Semantic Vector Search
const aiMatches = await apex.collection('posts').searchVectorWithText('database performance', {
  per_page: 5
});
```

---

## 8. HTTP Status & Error Codes

| Status Code | Error Code | Meaning |
| :--- | :--- | :--- |
| `200 OK` | — | Query / fetch / update operation succeeded. |
| `201 Created` | — | Record successfully inserted. |
| `204 No Content` | — | Record successfully deleted. |
| `400 Bad Request` | `input_validation` | Malformed JSON syntax or invalid query parameters. |
| `401 Unauthorized` | `unauthorized` | Missing, expired, or invalid JWT / API Key. |
| `403 Forbidden` | `forbidden` | Request rejected by Row-Level Security (RLS) policy. |
| `404 Not Found` | `not_found` | Collection or Record ID does not exist. |
| `422 Unprocessable Entity` | `validation_error` | Data violates schema validation rules or unique constraints. |
| `500 Internal Server Error` | `database_error` | Storage engine error or uncaught exception. |
