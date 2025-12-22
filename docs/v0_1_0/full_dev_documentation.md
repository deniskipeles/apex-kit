# ApexKit Comprehensive Developer Documentation

**Version:** 0.1.0 (Architect Edition)
**System Architecture:** Rust (Axum) + SQLite (LibSQL) + Boa (JS Engine) + Tera (Templating)

---

## Table of Contents

1.  [Core Architecture](#1-core-architecture)
2.  [Authentication & Security Policies](#2-authentication--security-policies)
3.  [Data Modeling & Schema](#3-data-modeling--schema)
4.  [The Query Engine (Filtering & Expansion)](#4-the-query-engine)
5.  [Server-Side Scripting (The Edge Runtime)](#5-server-side-scripting)
6.  [The Rendering Engine (SSR & HTMX)](#6-the-rendering-engine)
7.  [AI Integration](#7-ai-integration)
8.  [Real-Time Subscriptions](#8-real-time-subscriptions)
9.  [System API Reference](#9-system-api-reference)

---

## 1. Core Architecture

ApexKit is a monolithic, single-binary Backend-as-a-Service. Unlike traditional frameworks, it combines the database, API server, and logic engine into one process.

*   **Database:** Uses **LibSQL** (SQLite fork) for data storage. It uses JSON columns (`data`) for flexibility while maintaining relational integrity via a separate `_relations` graph table.
*   **Search:** Integrated **Tantivy** engine provides full-text and vector search. It automatically syncs with SQLite transactions.
*   **Logic:** A v8-compatible JavaScript engine (**Boa**) runs inside the Rust process. This allows for "Edge Function" style logic without external cold starts.
*   **Storage:** Abstracts local disk and AWS S3-compatible storage transparently.

---

## 2. Authentication & Security Policies

Authentication is JWT (JSON Web Token) based.

### API Rules (Policies)
Every collection has four policy hooks: `read`, `create`, `update`, `delete`.
A policy string defines who can perform the action.

| Policy Rule | Description | Logic |
| :--- | :--- | :--- |
| `public` | Open to everyone | No checks performed. |
| `auth` | Authenticated users | Token must be valid. |
| `admin` | Administrators only | Token role must be `'admin'`. |
| `owner:{field}` | Record Ownership | The value of `record[{field}]` must match the User ID in the token. |

**Example:**
*   Collection `posts`:
    *   `read`: `"public"` (Anyone can see posts)
    *   `create`: `"auth"` (Only logged-in users can post)
    *   `update`: `"owner:author_id"` (Only the author can edit)
    *   `delete`: `"admin"` (Only admins can delete)

### Auth Headers
All protected requests must include:
```http
Authorization: Bearer <YOUR_JWT_TOKEN>
```

---

## 3. Data Modeling & Schema

ApexKit uses a strict schema definition that validates data *before* it hits the JSON storage.

### Supported Field Types

| Type | Backend Key | Description | Validation Params |
| :--- | :--- | :--- | :--- |
| **String** | `string` | Short text (Varchar equivalent). | `min_length`, `max_length`, `pattern` (Regex) |
| **Text** | `text` | Long text / HTML. | `min_length`, `max_length` |
| **Number** | `number` | Integer or Float. | `min`, `max` |
| **Boolean** | `bool` | True/False. | - |
| **Email** | `email` | Validates email format. | - |
| **URL** | `url` | Validates URL structure. | - |
| **Date** | `date` | ISO 8601 Timestamp. | - |
| **Select** | `select` | Enum-like restriction. | `options: ["A", "B"]` |
| **JSON** | `json` | Arbitrary JSON object/array. | - |
| **File** | `file` | Path string to stored file. | `max_size` (bytes), `mime_types` |
| **Vector** | `vector` | Array of floats for AI embeddings. | `dimension` (e.g., 1536) |
| **Relation** | `relation` | Foreign Key. | `relationTo` (Collection Name) |
| **Owner** | `owner` | Special relation to `users` table. | - |

---

## 4. The Query Engine

ApexKit allows complex filtering and relational expansion in a single HTTP request.

### Filtering (`filter`)
The `filter` parameter accepts a JSON string. It uses SQLite's JSON operators internally.

**Syntax:** `?filter={"field": "value"}`

*   **Exact Match:** `{"status": "active"}`
*   **Boolean:** `{"is_published": true}`
*   **Nested JSON:** `{"metadata.category": "tech"}` (Dot notation works for nested JSON objects)

### Expansion (`expand`)
Fetches related records in a single query to avoid the N+1 problem.

**Syntax:** `?expand=relation_field,relation_field.nested_relation`

**How it works:**
1.  ApexKit parses the expansion tree.
2.  It constructs a **Recursive CTE** (Common Table Expression) or correlated subquery in SQL.
3.  It fetches the related record from the `_relations` table.
4.  It injects the result into an `expand` property on the record.

**Example Request:**
`GET /api/v1/collections/comments/records?expand=user_id,post_id.author_id`

**Response Structure:**
```json
{
  "id": 105,
  "text": "Great post!",
  "user_id": 55,
  "expand": {
    "user_id": [{ "id": 55, "email": "bob@example.com", ... }],
    "post_id": [{ 
       "id": 200, 
       "title": "Hello World", 
       "expand": {
          "author_id": [{ "id": 1, "name": "Admin" }]
       }
    }]
  }
}
```

### Sorting & Pagination
*   `sort`: `-created` (Descending), `title` (Ascending).
*   `page`: Integer (1-based).
*   `per_page`: Integer (Max 100).

---

## 5. Server-Side Scripting

ApexKit includes a custom JavaScript runtime. Scripts run in a sandboxed environment on the server.

### The Runtime Environment
*   **Engine:** Boa (Rust-based JS engine).
*   **Execution Model:** Per-request isolation.
*   **Entry Point:** Scripts must export a default async function.

### Global Objects

#### `$db` (Database Access)
All DB operations are async and respect ACLs/Policies internally unless running as system script.
```javascript
// Find One
const user = await $db.find_one('users', 123);

// Find Many (with filter)
const active_todos = await $db.find('todos', { is_completed: false });

// Insert
const new_id = await $db.insert('logs', { message: "Script ran" });

// Update
const updated_doc = await $db.update('todos', 1, { title: "New Title" });

// Delete
const success = await $db.delete('todos', 1);
```

#### `$http` (External Requests)
Synchronous-style blocking HTTP requests (wrapped in async interface).
```javascript
// GET
const html = await $http.get("https://example.com");

// POST (JSON)
const response = await $http.post("https://api.slack.com/webhook", { text: "Hello" });
```

#### `$util`
```javascript
const id = $util.uuid(); // Generates v4 UUID
```

### Anatomy of a Script
**Endpoint:** `POST /api/v1/run/{script_name}`

```javascript
export default async function(req) {
    // 1. Parse Input
    const body = await req.json();
    
    // 2. Logic
    if (!body.email) {
        return new Response({ error: "Email required" }, { status: 400 });
    }
    
    const users = await $db.find('users', { email: body.email });
    
    // 3. Response
    return new Response({ 
        exists: users.length > 0,
        count: users.length 
    }, { status: 200 });
}
```

---

## 6. The Rendering Engine

ApexKit can render HTML on the server, acting as a web server, not just an API.

### Endpoint
`GET /render/{template_slug}`

### How Data Flows
1.  **Request**: Browser requests `/render/dashboard`.
2.  **Lookup**: Engine finds template `dashboard` in DB.
3.  **Loader Script**: If the template has a `script_id` attached, the engine runs that script *first*.
    *   The Script receives `req.params`, `req.headers`, `req.body`.
    *   The Script returns a JSON object.
4.  **Context Merging**: The JSON returned by the script is merged with the default context (`params`, `headers`).
5.  **Tera Render**: The template processes the HTML using the context.

### Template Syntax (Tera)
*   **Variables**: `{{ user.name }}`
*   **Control Flow**: `{% if is_htmx %}...{% endif %}`
*   **Loops**: `{% for item in items %}...{% endfor %}`
*   **Includes**: `{% include "components/navbar" %}`

### Database Helpers (Inside Templates)
You can fetch data directly in the view (useful for simple reads).
*   `db_find(col='collection', filter=null)` -> Returns Array.
*   `db_find_one(col='collection', id=1)` -> Returns Object.

**⚠️ Warning:** You **must** use keyword arguments in these helpers. `db_find('users')` will fail. Use `db_find(col='users')`.

---

## 7. AI Integration

ApexKit allows defining "AI Actions" which are prompt templates exposed as API endpoints.

### Configuration
1.  **Define Action:**
    *   **Slug:** `summarize`
    *   **Model:** `gemini-1.5-flash`
    *   **Template:** "Summarize the following text: {{ input_text }}"
2.  **Execute:**
    *   **POST** `/api/v1/ai/run/summarize`
    *   **Body:** `{"variables": {"input_text": "Long story..."}}`

The system handles API key encryption, template variable substitution, and interacting with the LLM provider.

---

## 8. Real-Time Subscriptions

ApexKit broadcasts database change events via WebSocket.

**Endpoint:** `ws://localhost:5000/ws`

**Protocol:**
The server sends JSON messages immediately upon data changes. No handshake required currently.

**Event Structure:**
```json
{
  "event": "Insert", 
  "payload": {
    "collection_id": 5,
    "record_id": 102,
    "data": { "title": "New Data" }
  }
}
```
*Events:* `Insert`, `Update`, `Delete`.

---

## 9. System API Reference

### System
*   `POST /api/v1/admin/system/reload`: Hot-reloads schema, crons, and caches without restarting the binary.

### Storage
*   `POST /api/v1/storage/upload`: Multipart form upload.
*   `GET /api/v1/storage/file/{filename}`: Public access to files.

### Search (Tantivy)
*   `GET /api/v1/collections/{id}/search?q=...`: Uses standard SQL search.
*   `GET /api/v1/collections/{id}/instant-search?q=...`: Uses the memory-mapped Tantivy index. This is orders of magnitude faster for full-text search and supports fuzzy matching.

### Scripts & Templates (CRUD)
*   `GET /api/v1/admin/scripts`
*   `POST /api/v1/admin/scripts`
*   `GET /api/v1/admin/templates`
*   `POST /api/v1/admin/templates`