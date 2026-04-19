# 🏗️ Multi-Tenancy & Sandbox Architecture

**Version:** 0.1.0
**Context:** Scaling, SaaS Architecture, and AI-Driven Development.

ApexKit features a built-in architecture for **Physical Multi-Tenancy** and **Ephemeral Sandboxes**. This allows a single instance to host thousands of isolated applications (Tenants) or temporary development environments (Sandboxes) with strict data separation and independent AI contexts.

---

## 1. Entity Lifecycle

| Entity | Purpose | Persistence | Isolation |
| :--- | :--- | :--- | :--- |
| **Root App** | The master environment. Used for platform management and global services. | Permanent | Global |
| **Tenant** | A production-grade isolated application for a specific client or sub-brand. | Permanent | High (Dedicated DBs & Files) |
| **Sandbox** | A temporary "playground" used by the AI Architect to build/test features. | Ephemeral | High (Auto-deleted on expiry) |

---

## 2. Physical Isolation Strategy

ApexKit uses a **Database-per-Tenant** strategy rather than logical row-level isolation. This ensures that a heavy query in one tenant cannot impact the performance of another, and data leaks are prevented at the filesystem level.

### Directory Structure
When a Tenant or Sandbox is provisioned, ApexKit creates a dedicated workspace:

```text
storage/
├── system/               # Root App Data
├── tenants/
│   └── client-alpha/     # Isolated Tenant Folder
│       ├── data.db       # Collections & Records
│       ├── core.db       # Tenant-specific Users
│       ├── vectors.db    # AI Embeddings
│       ├── uploads/      # Private Files
│       └── indexes/      # Tantivy Search Index
└── sandboxes/
    └── session_uuid/     # Isolated Playground
```

---

## 3. Sandboxes & AI Architect

Sandboxes are the foundation of the **AI Architect** flow. They allow you to generate schemas, write code, and insert dummy data safely without touching your production environment.

### Cloning Strategies
When starting an AI session, you can choose how much data to bring into the sandbox:

*   **`none`**: A completely empty environment.
*   **`schema`**: Copies collection structures and scripts but no records.
*   **`partial`**: Copies schema and the first **N** records from every collection (useful for testing logic with real data).
*   **`full`**: A 1:1 clone of the source environment.

### The Architect Flow
1.  **Draft**: Architect generates a "Pending Manifest" (JSON) based on your request.
2.  **Review**: You view the diff in the Admin UI.
3.  **Apply**: The manifest is deployed to the Sandbox database.
4.  **Publish**: Once satisfied, the sandbox manifest is committed to the Root App or a Production Tenant as a **Plugin**.

---

## 4. Multi-Tenancy Management

### Routing
ApexKit detects the scope automatically based on the incoming request:
1.  **Subdomain**: `client-alpha.yourapp.com` maps to Tenant `client-alpha`.
2.  **Path**: `yourapp.com/tenant/client-alpha/...` maps to Tenant `client-alpha`.
3.  **Header**: `X-Apex-Scope: tenant:client-alpha`.

### Status & Suspension
Root admins can manage tenant lifecycles via **Settings > Tenants**:
*   **Active**: Normal operation.
*   **Suspended**: API returns `403 Forbidden` for all tenant users. Root admins can still enter to fix issues.
*   **Archived**: Database is disconnected from memory but remains on disk.

---

## 5. Security & Scoping

### JWT & API Key Scopes
Authentication tokens are "pinned" to a scope.
*   A token issued by **Tenant A** cannot be used to access **Tenant B**.
*   **Root Admin Fallback**: Tokens issued by the Root App with the `admin` role are "Super Tokens" and can access any Tenant or Sandbox by switching the URL context.

### Cross-Scope Scripting
Tenants are isolated, but they can consume logic shared by the Root App.
*   **Private**: Script is only visible within its own scope.
*   **Public (Root Only)**: Script can be called by any tenant using `$run.script("name")`. This is ideal for sharing heavy tools like FFmpeg or global AI logic.

---

## 6. JavaScript SDK Usage

The `ApexKit` client supports fluent context switching.

```javascript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.myapp.com');

// 1. Root Context
await apex.auth.login('admin@root.com', 'pass');

// 2. Switch to Tenant
const tenant = apex.tenant('client-beta');
const orders = await tenant.collection('orders').list();

// 3. Switch to Sandbox
const sandbox = apex.sandbox('session-99-abc');
await sandbox.collection('todos').create({ title: "Try AI feature" });
```

---

## 7. Performance & Resource Management

*   **Connection Pooling**: ApexKit uses an LRU (Least Recently Used) cache for tenant database connections. Inactive tenants are evicted from RAM after 60 minutes.
*   **Shared AI Models**: Heavy LLM models and local embedding models are loaded once in RAM and shared across all tenants to minimize memory footprint.
*   **Quota Enforcement**: Use the `before_tenant_request` script hook to implement custom rate limits or billing checks per tenant.