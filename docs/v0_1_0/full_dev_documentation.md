# ApexKit Comprehensive Developer Documentation

**Version:** 0.1.0
**System Architecture:** Rust (Axum) + SQLite (LibSQL) + Boa (JS Engine) + Tera (Templating)

---

## Table of Contents

1.  [Core Architecture](#1-core-architecture)
2.  [Authentication & Security Policies](#2-authentication--security-policies)
3.  [Data Modeling & Schema](#3-data-modeling--schema)
4.  [The Query Engine (Filtering & Expansion)](#4-the-query-engine)
5.  [Server-Side Scripting (The Edge Runtime)](#5-server-side-scripting)
6.  [AI Integration](#6-ai-integration)
7.  [Real-Time Subscriptions](#7-real-time-subscriptions)
8.  [Storage & Files](#8-storage--files)

---

## 1. Core Architecture

ApexKit is a monolithic, single-binary Backend-as-a-Service. Unlike traditional frameworks, it combines the database, API server, and logic engine into one process.

*   **Multi-Tenancy:** ApexKit supports multiple isolated environments (Tenants/Sandboxes) within a single instance.
*   **Database:** Uses **LibSQL** (SQLite fork) for data storage. It uses JSON columns (`data`) for flexibility while maintaining relational integrity.
*   **Search:** Integrated **Tantivy** engine provides full-text and vector search. It automatically syncs with SQLite transactions.
*   **Logic:** A v8-compatible JavaScript engine (**Boa**) runs inside the Rust process.
*   **Storage:** Abstracts local disk and AWS S3-compatible storage transparently.

---

## 2. Authentication & Security Policies

Authentication is JWT (JSON Web Token) based. Scopes are enforced strictly (Root vs Tenant).

### API Rules (Policies)
Every collection has four policy hooks: `read`, `create`, `update`, `delete`.
A policy string defines who can perform the action.

| Policy Rule | Description | Logic |
| :--- | :--- | :--- |
| `public` | Open to everyone | No checks performed. |
| `auth` | Authenticated users | Token must be valid. |
| `admin` | Administrators only | Token role must be `'admin'`. |
| `owner:{field}` | Record Ownership | The value of `record[{field}]` must match the User ID in the token. |
| `auth.id == field:owner` | Expression | Advanced expression logic. |

### Auth Headers
All protected requests must include:
```http
Authorization: Bearer <YOUR_JWT_TOKEN>
```

---

## 3. Data Modeling & Schema

ApexKit uses a strict schema definition that validates data *before* it hits the JSON storage. See `schema_fields.md` for details.

---

## 4. The Query Engine

ApexKit allows complex filtering, aggregation, and relational expansion in a single HTTP request.

### Advanced Query Endpoint
**POST** `/api/v1/collections/{id}/query`

**Body:**
```json
{
  "from": "sales",
  "select": [
    "customerName",
    { "fn": "sum", "field": "totalAmount", "as": "revenue" }
  ],
  "where": { "status": "completed" },
  "group_by": ["customerName"],
  "sort": "-revenue"
}
```

### Standard List (GET)
**GET** `/api/v1/collections/{id}/records?filter={"status":"active"}&expand=author`

---

## 5. Server-Side Scripting

Scripts run in a sandboxed environment on the server.

### Global Objects

| Object | Description |
| :--- | :--- |
| **`$db`** | Database Access (`find`, `insert`, `update`, `delete`, `query`). Context-aware (Tenant/Root). |
| **`$http`** | Make external HTTP requests (`get`, `post`). |
| **`$util`** | Utilities (`uuid`, `slugify`, `hash`, `hmac`). |
| **`$zip`** | In-memory Zip creation/extraction (`create`, `extract`, `inspect`). |
| **`$cmd`** | **Root Only.** Execute system shell commands (`run`, `spawn`). |
| **`$run`** | Execute other scripts (`script`). Can call Public Root scripts from Tenants. |
| **`$ai`** | Generate Embeddings (`embed`). |
| **`$cache`** | Key-Value ephemeral store (`get`, `set`, `incr`). |

### Example Script
```javascript
export default async function(req) {
    const { name } = await req.json();
    const id = await $db.records.create('users', { name });
    return new Response({ success: true, id });
}
```

---

## 6. AI Integration

ApexKit provides **AI Actions** (Prompt Templates) and **Vector Search**.

### Vector Search
Requires fields in schema to have `vectorize: true`.

**POST** `/api/v1/collections/{id}/search-text-vector`
```json
{ "query_text": "Find similar items...", "limit": 10 }
```

### AI Actions
**POST** `/api/v1/ai/run/{slug}`
```json
{ "variables": { "input": "Text to summarize..." } }
```

---

## 7. Real-Time Subscriptions

ApexKit broadcasts database change events via WebSocket or SSE.

**WebSocket Endpoint:** `ws://host/ws`
**SSE Endpoint:** `GET /sse`

**Event Structure:**
```json
{
  "type": "Insert", 
  "payload": {
    "collection_id": 5,
    "record_id": 102,
    "data": { "title": "New Data" }
  }
}
```

---

## 8. Storage & Files

Files are stored in `storage/tenants/{id}/uploads` (Local) or S3 bucket (if configured).

*   **Upload:** `POST /storage/upload` (Multipart)
*   **Get:** `GET /storage/file/{filename}`
*   **Resize:** `GET /storage/file/{filename}?thumb=100x100`
```
