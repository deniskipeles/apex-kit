# TinyBase Collections API Documentation

**Version:** 2.1
**Base URL:** `http://localhost:5000/api/v1`

In TinyBase, a **Collection** is the fundamental container for data. You can think of it as a Table in SQL or a Collection in MongoDB. It defines the **Structure (Schema)**, **Validation Rules**, and **Security Policies** for the records it holds.

---

## 1. The Collection Object

When creating or retrieving a collection, the JSON structure looks like this:

```json
{
  "id": 1,
  "name": "posts",
  "schema": {
    "fields": { ... },
    "policies": { ... },
    "relations": { ... }
  }
}
```

*   **name**: (String, Unique) The identifier used in API URLs and relation definitions. Should be lowercase, alphanumeric, snake_case recommended (e.g., `blog_posts`).
*   **schema**: The configuration object containing fields and rules.

---

## 2. Defining Fields (Schema)

The `fields` object maps field names to their definitions. TinyBase enforces strict typing and validation at the API level.

### Field Properties

| Property | Type | Description |
| :--- | :--- | :--- |
| `type` | String | **Required.** The data type (see list below). |
| `required` | Boolean | If `true`, the record cannot be saved without this value. |
| `unique` | Boolean | If `true`, duplicate values across the collection are rejected. |
| `indexed` | Boolean | If `true`, this field is added to the Tantivy Search Index for high-performance searching. |
| `default` | Any | The value to use if the payload is missing this field. |

### Supported Field Types

| Type Value | Description | Specific Options |
| :--- | :--- | :--- |
| `string` | Short text (Title, Name). | `min_length`, `max_length`, `pattern` (Regex) |
| `text` | Long text or HTML content. | `min_length`, `max_length` |
| `number` | Integer or Float. | `min`, `max` |
| `bool` | True or False. | - |
| `email` | Validates email format. | - |
| `url` | Validates URL format. | - |
| `date` | ISO 8601 Date String. | - |
| `select` | Enum-like restriction. | `options`: `["Draft", "Published"]` |
| `json` | unstructured JSON object/array. | - |
| `file` | Reference to a stored file path. | `max_size` (bytes), `mime_types`: `["image/png"]` |
| `blob` | Binary data (Base64 encoded). | `max_size` |
| `vector` | Array of floats for AI Embeddings. | `dimension`: `1536` (Required for Vector Search) |
| `relation` | Foreign Key to another collection. | `relation_to`: `"target_collection_name"` |
| `owner` | Links record to a User ID. | - |

---

## 3. Security Policies (API Rules)

TinyBase uses a granular permission system defined in `policies`. Each operation has a rule string.

**Operations:** `read`, `create`, `update`, `delete`.

**Rule Syntax:**

| Rule | Description |
| :--- | :--- |
| `"public"` | Accessible by anyone (including unauthenticated guests). |
| `"auth"` | Accessible by any logged-in user with a valid JWT. |
| `"admin"` | Accessible only by users with the `admin` role. |
| `"owner:{field}"` | **Row-Level Security.** The system checks if the value in the record's `{field}` matches the authenticated User's ID. |

**Example Policies Object:**
```json
"policies": {
  "read": "public",           // Everyone can see
  "create": "auth",           // Only logged in can create
  "update": "owner:user_id",  // Only the creator can edit
  "delete": "admin"           // Only admins can delete
}
```

---

## 4. API Endpoints

### List Collections
Retrieve all schema definitions.

*   **GET** `/collections`
*   **Auth:** Admin Only.

**Response:**
```json
[
  {
    "id": 1,
    "name": "users",
    "schema": { ... }
  },
  {
    "id": 2,
    "name": "posts",
    "schema": { ... }
  }
]
```

---

### Get Collection
Retrieve details for a specific collection.

*   **GET** `/collections/{id}`
*   **Auth:** Admin Only (or internal use).

---

### Create Collection
Define a new table/collection.

*   **POST** `/collections`
*   **Auth:** Admin Only.

**Body Payload:**
```json
{
  "name": "products",
  "schema": {
    "fields": {
      "title": {
        "type": "string",
        "required": true,
        "indexed": true
      },
      "price": {
        "type": "number",
        "min": 0
      },
      "category": {
        "type": "select",
        "options": ["Electronics", "Books", "Clothing"]
      },
      "supplier_id": {
        "type": "relation",
        "relation_to": "suppliers"
      }
    },
    "policies": {
      "read": "public",
      "create": "admin",
      "update": "admin",
      "delete": "admin"
    }
  }
}
```

**Response:** `201 Created`

---

### Update Collection
Modify the schema or name. Note that changing field types may require manual data migration or cause validation errors on existing data if they are incompatible.

*   **PATCH** `/collections/{id}`
*   **Auth:** Admin Only.

**Body:**
```json
{
  "schema": {
    "fields": {
       // ... updated field definitions
    }
  }
}
```

---

### Delete Collection
Permanently drops the collection, **ALL** its records, and removes any search indexes associated with it.

*   **DELETE** `/collections/{id}`
*   **Auth:** Admin Only.

**Response:** `204 No Content`

---

## 5. Working with Relations

TinyBase handles relations automatically if defined in the schema.

### Defining a Relation
When you create a collection, define a field with type `relation`:

```json
"author_id": {
  "type": "relation",
  "relation_to": "users"
}
```

### Automatic Linking
When you **Create** or **Update** a record in this collection:
1.  Pass the ID of the target record in the JSON payload: `{"author_id": 55}`.
2.  TinyBase detects the field type is `relation`.
3.  It automatically creates an edge in the internal `_relations` graph table linking the new record to User #55.

### Fetching (Expanding)
Because the schema knows about the link, you can easily fetch the data using the Records API:
`GET /collections/posts/records?expand=author_id`

---

## 6. Vector Search Setup (AI)

To enable semantic search (e.g., "Find products similar to this description"):

1.  **Define a Vector Field:**
    ```json
    "embedding": {
      "type": "vector",
      "dimension": 1536  // Must match your AI model (e.g., OpenAI ada-002)
    }
    ```
2.  **Indexing:** Ensure `indexed: true` is set on text fields you want to keyword search alongside vectors.
3.  **Usage:** Populate this field using a Server-Side Script that calls the AI Embedding API, then use the `instant-search` endpoint (future update will include vector similarity search endpoints).