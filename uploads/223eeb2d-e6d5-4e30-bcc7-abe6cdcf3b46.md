# TinyBase Developer API Documentation

**Version:** v1.0
**Base URL:** `http://localhost:5000/api/v1` (Default)

TinyBase is a lightweight, high-performance Backend-as-a-Service (BaaS) providing a RESTful API, Real-time subscriptions, and Server-Side Scripting.

---

## 1. Authentication

Most endpoints require authentication based on the Collection's API Rules.

**Header:**
```http
Authorization: Bearer <YOUR_JWT_TOKEN>
```

### Authenticate (User Login)
**POST** `/auth/login`

**Body:**
```json
{
  "email": "user@example.com",
  "password": "secretpassword"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1Ni...",
  "user": {
    "id": 1,
    "email": "user@example.com",
    "role": "user"
  }
}
```

### Register User
**POST** `/auth/register`
*Same body structure as Login. Creates a user with role `user`.*

---

## 2. Records (CRUD)

Interact with your data collections. Replace `:collectionId` with the Collection ID (e.g., `1` or `posts`).

### List Records
**GET** `/collections/:collectionId/records`

**Query Parameters:**
| Param | Description | Example |
| :--- | :--- | :--- |
| `page` | Page number (default 1) | `?page=2` |
| `per_page` | Items per page (default 30) | `?per_page=50` |
| `sort` | Sort field. Prefix `-` for descending. | `?sort=-created` |
| `filter` | JSON string for filtering. | `?filter={"status":"active"}` |
| `expand` | Comma-separated relations to fetch. | `?expand=author,comments` |

**Example Request:**
`GET /collections/posts/records?filter={"published":true}&sort=-created&expand=author`

**Response:**
```json
[
  {
    "id": 101,
    "collectionId": 5,
    "created": "2023-10-27T10:00:00Z",
    "updated": "2023-10-27T10:00:00Z",
    "title": "My First Post",
    "published": true,
    "author": 1,
    "expand": {
      "author": [{ "id": 1, "email": "dave@example.com" }]
    }
  }
]
```

### Get Single Record
**GET** `/collections/:collectionId/records/:recordId`

### Create Record
**POST** `/collections/:collectionId/records`

**Body:**
```json
{
  "data": {
    "title": "New Article",
    "content": "Hello World",
    "status": "draft"
  }
}
```

### Update Record
**PATCH** `/collections/:collectionId/records/:recordId`

**Body:**
```json
{
  "data": {
    "status": "published"
  }
}
```

### Delete Record
**DELETE** `/collections/:collectionId/records/:recordId`

### Instant Search (Vector/Full-Text)
**GET** `/collections/:collectionId/instant-search?q=search_term`
*Returns lightweight results from the Tantivy search index (faster than SQL).*

---

## 3. Storage (Files)

### Upload File
**POST** `/storage/upload`
*Content-Type: `multipart/form-data`*

**Form Field:** `file` (Binary content)

**Response:**
```json
{
  "id": 55,
  "filename": "uuid-image.png",
  "url": "http://.../api/v1/storage/file/uuid-image.png"
}
```

### Serve File
**GET** `/storage/file/:filename`

---

## 4. Scripting (Server-Side Logic)

Execute server-side JavaScript functions defined in the Admin Dashboard.

**POST** `/run/:script_name`

**Body:** (Any JSON payload you want to pass to the script)
```json
{
  "order_id": 123,
  "action": "refund"
}
```

**Response:** (Whatever the script returns)
```json
{
  "success": true,
  "processed_at": "2023-10-27..."
}
```

---

## 5. AI Actions (LLM Integration)

Run predefined AI prompts (configured in Admin) with dynamic variables.

**POST** `/ai/run/:action_slug`

**Body:**
```json
{
  "variables": {
    "text": "Code needed for a binary search in Rust",
    "tone": "professional"
  }
}
```

**Response:**
```json
{
  "result": "Here is the Rust implementation for binary search..."
}
```

---

## 6. Real-time (WebSocket)

Subscribe to database changes instantly.

**Endpoint:** `ws://localhost:5000/ws`

**Messages Received:**
```json
{
  "event": "Insert",
  "payload": {
    "collection_id": 5,
    "record_id": 102,
    "data": { "title": "New Post" }
  }
}
```
*(Also supports `Update` and `Delete` events)*

---

## 7. GraphQL

A GraphQL endpoint is automatically generated based on your collection schema.

**Endpoint:** `POST /graphql`

**Example Query:**
```graphql
query {
  posts {
    id
    title
    author {
      email
    }
  }
}
```

---

## 8. Client SDK Example (JavaScript)

If using the official SDK (found in `sdk.js`), usage is simplified:

```javascript
import { PowerBase } from './sdk';

const pb = new PowerBase('http://localhost:5000');

// 1. Login
await pb.auth.login('user@example.com', 'password');

// 2. Fetch Records
const posts = await pb.collection('posts').list({ 
    page: 1, 
    sort: '-created',
    expand: 'author' 
});

// 3. Create Record
await pb.collection('todos').create({ 
    title: "Buy Milk", 
    completed: false 
});

// 4. Run Server Script
const result = await pb.scripts.run('calculate-stats', { userId: 123 });
```