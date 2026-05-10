# REST API Reference

ApexKit automatically generates a RESTful API for every collection. This guide covers the standard endpoints and parameters.

## Base URL
All API requests are prefixed with `/api/v1`.

## Collection Endpoints

For a collection named `posts`:

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/collections/posts/records` | List records with pagination and filtering. |
| `GET` | `/collections/posts/records/:id` | Get a single record by ID. |
| `POST` | `/collections/posts/records` | Create a new record. |
| `PUT` | `/collections/posts/records/:id` | Update an existing record (full update). |
| `PATCH` | `/collections/posts/records/:id` | Partially update a record. |
| `DELETE` | `/collections/posts/records/:id` | Delete a record. |

## Query Parameters

### Pagination
- `page`: The page number (default: 1).
- `per_page`: Records per page (default: 20, max: 100).

### Sorting
Use `-` for descending order.
- `?sort=-created,title`

### Filtering
Filters can be simple key-value pairs or complex JSON objects.
- `?filter={"status": "active"}`
- `?filter={"price": {"$gt": 100}}` (Advanced comparison)

### Expansion
Fetch related records in one request.
- `?expand=author,comments.user`

## Authentication Header
Include the JWT in the `Authorization` header for protected routes.
```http
Authorization: Bearer <YOUR_TOKEN>
```

## Batch Operations
ApexKit supports atomic batch operations via the query endpoint.

**POST** `/api/v1/collections/posts/query`
```json
{
  "transaction": true,
  "ops": [
    { "type": "create", "data": { "title": "Batch 1" } },
    { "type": "create", "data": { "title": "Batch 2" } }
  ]
}
```

## Error Handling
ApexKit returns standard HTTP status codes:
- `200 OK`: Success.
- `201 Created`: Successfully created a record.
- `400 Bad Request`: Validation or syntax error.
- `401 Unauthorized`: Missing or invalid token.
- `403 Forbidden`: Policy violation.
- `404 Not Found`: Record or collection doesn't exist.
- `500 Internal Error`: Something went wrong on the server.
