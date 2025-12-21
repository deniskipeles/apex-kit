# TinyBase Multi-Tenancy & Sandbox Architecture

**Version:** 2.4
**Context:** Scaling, SaaS Architecture, and AI Prototyping.

TinyBase provides a powerful, built-in architecture for **Multi-Tenancy** and **Ephemeral Sandboxes**. This allows a single TinyBase instance to host thousands of isolated applications (Tenants) or temporary development sessions (Sandboxes) with strict data separation.

---

## 1. Concepts

| Entity | Purpose | Persistence | Isolation Level |
| :--- | :--- | :--- | :--- |
| **Root App** | The main application instance. Used for single-tenant apps or the "Platform" layer. | Permanent | Global |
| **Sandbox** | A temporary environment used by the **AI Architect** to build and test apps in real-time. | Ephemeral (Can be deleted) | High (Separate DBs & Files) |
| **Tenant** | A production-grade isolated environment for a specific customer or sub-application. | Permanent | High (Separate DBs & Files) |

---

## 2. Architecture & Isolation

TinyBase uses a **Physical Isolation** strategy (Database-per-Tenant) rather than Logical Isolation (Row-level `tenant_id` columns). This ensures maximum security and performance.

### File System Structure
When a Tenant or Sandbox is created, TinyBase provisions a dedicated directory structure:

```text
tinybase/
├── data.db (Root App)
├── uploads/ (Root App Files)
├── tenants/
│   ├── customer-abc/
│   │   ├── core.db       (Users & Auth)
│   │   ├── data.db       (Collections & Records)
│   │   ├── logs.db       (Audit Logs)
│   │   ├── vectors.db    (AI Embeddings)
│   │   ├── indexes/      (Tantivy Search Index)
│   │   └── uploads/      (Isolated File Storage)
└── sandboxes/
    └── session_uuid/
        ├── ... (Same structure as Tenant)
```

### Resource Management
*   **Database:** Each tenant gets its own set of SQLite files. This means a heavy query on Tenant A does not lock the database for Tenant B.
*   **Memory (LRU Cache):** TinyBase uses an LRU (Least Recently Used) cache to manage active database connections. If you have 10,000 tenants but only 50 active, only those 50 consume RAM.
*   **AI Models:** Heavy AI models (like BERT for embeddings) are **Shared** globally to save RAM, but the **Vector Indexes (HNSW)** are isolated per tenant.

---

## 3. Tenants (SaaS Mode)

Tenants are designed for SaaS applications where you need to give every customer their own database.

### Routing
TinyBase supports two routing strategies for tenants. The middleware automatically detects the context.

1.  **Subdomain Routing (Preferred for Production):**
    *   URL: `https://customer-a.myapp.com/api/v1/...`
    *   TinyBase extracts `customer-a` from the host header.
2.  **Path-Based Routing:**
    *   URL: `https://myapp.com/tenant/customer-a/api/v1/...`
    *   Useful for development or internal tools.

### Managing Tenants
Tenants **must be explicitly created** by an Admin via the Root API. Random access to non-existent tenants returns a `404`.

**Create a Tenant:**
`POST /api/v1/admin/tenants`
*Auth: Admin Only*
```json
{ "tenant_id": "client-google" }
```

**Accessing Tenant Data:**
Once created, you can access the tenant's API endpoints exactly like the root app.
*   `GET /tenant/client-google/api/v1/collections`
*   `GET /tenant/client-google/scalar` (Documentation)
*   `POST /tenant/client-google/graphql`

---

## 4. Sandboxes (AI Architect Mode)

Sandboxes are created automatically when starting a new **AI Architect Session**. They are designed to be "Playgrounds" where the AI can generate schemas, write code, and insert dummy data without affecting the main application.

*   **URL Pattern:** `/sandbox/{session_id}/...`
*   **Lifecycle:** Created via `POST /api/v1/admin/ai/sessions`.
*   **Publishing:** You can "Merge" a sandbox into the Main App (or a Tenant) using the `publish` endpoint.

---

## 5. Client SDK Usage

The TinyBase SDK (`sdk.js`) has been updated to support fluent context switching.

### Initialization
```javascript
import { PowerBase } from './sdk.js';

// 1. Connect to Root
const pb = new PowerBase('http://localhost:5000');
```

### Switching to a Tenant
Use the `.tenant(id)` method. This returns a *new* SDK instance configured for that tenant.

```javascript
// Switch context to 'client-a'
const clientA = pb.tenant('client-a');

// Login as a user BELONGING to Client A
await clientA.auth.login('admin@client-a.com', 'password');

// Create data inside Client A's database
await clientA.collection('products').create({ name: 'Widget A' });

// Upload file to Client A's isolated storage
await clientA.files.upload(myFile);
```

### Switching to a Sandbox
Use the `.sandbox(id)` method.

```javascript
const devSession = pb.sandbox('550e8400-e29b...');

// The AI Architect might have created a 'todos' collection here
const todos = await devSession.collection('todos').list();
```

---

## 6. Feature Parity Matrix

Every feature available in the Root App is available in Tenants and Sandboxes.

| Feature | Root App | Tenant | Sandbox | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **Auth** | ✅ | ✅ | ✅ | Users are isolated. `admin@root` cannot log into `tenant`. |
| **Collections** | ✅ | ✅ | ✅ | Dynamic schema per tenant. |
| **Storage** | ✅ | ✅ | ✅ | Files saved to `tenants/{id}/uploads`. |
| **Search** | ✅ | ✅ | ✅ | Independent Tantivy Indexes. |
| **Vector Search** | ✅ | ✅ | ✅ | Independent HNSW Indexes. |
| **GraphQL** | ✅ | ✅ | ✅ | Dynamic Schema generated per tenant. |
| **Scalar Docs** | ✅ | ✅ | ✅ | Available at `/tenant/{id}/scalar`. |
| **Scripting** | ✅ | ✅ | ✅ | Scripts run in isolated context. |
| **Templates** | ✅ | ✅ | ✅ | `render` endpoint works per tenant. |

---

## 7. Migration & Maintenance

### Database Migrations
Since every tenant has its own SQLite files, schema changes must be applied to **all** active tenants.
*   *Current Strategy:* The `TenantManager` runs standard initialization (`setup_schema`) when loading a tenant.
*   *Future Update:* A "Broadcast Schema Update" feature will be added to apply a `AppManifest` to all tenants.

### Backups
To backup a tenant, you simply need to zip the `tenants/{id}/` folder. This contains the data, logs, vectors, and uploaded files.

### Resource Limits
Check `main.rs` to configure the `TenantManager` capacity.
```rust
// Keep max 500 tenants in memory. Evict after 1 hour of idleness.
let tenant_manager = Arc::new(TenantManager::new(..., 500));
```