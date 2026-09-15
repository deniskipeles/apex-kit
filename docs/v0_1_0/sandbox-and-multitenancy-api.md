# 🏗️ Multi-Tenancy & Sandbox Architecture

**Version:** 0.1.0  
**Context:** Scaling, SaaS Architecture, Ephemeral Sandboxes & AI Architect Workspaces

ApexKit provides a native architecture for **Physical Multi-Tenancy** and **Ephemeral Sandboxes**. Rather than relying exclusively on shared database tables with logical row-level `WHERE tenant_id = ?` filters, ApexKit provisions isolated filesystem partitions and dedicated SQLite database files for each tenant and sandbox.

---

## 1. Environment Comparison & Lifecycle

| Environment | Purpose | Persistence | Database Isolation | Storage Path |
| :--- | :--- | :--- | :--- | :--- |
| **Root Scope** | Master infrastructure, system metrics, global settings, and tenant provisioning. | Permanent | Global | `storage/system/` |
| **Tenant Scope** | Production workspace for an isolated customer, vendor, or sub-brand. | Permanent | Physical SQLite partitions | `storage/tenants/{tenant_id}/` |
| **Sandbox Scope** | Ephemeral development environment for AI Architect drafting, staging, and automated test runs. | Ephemeral (Auto-expires or evicted on idle) | Physical SQLite partitions | `storage/sandboxes/session_{id}/` |

---

## 2. Physical Database Partitioning

When a Tenant or Sandbox is provisioned, ApexKit creates a self-contained directory layout:

```text
storage/
├── system/                          # Root Scope (Master)
│   ├── core.db                      # Global users, system configs, API keys, tenant registry
│   ├── data.db                      # Root collections and records
│   ├── logs.db                      # System logs & audit trail
│   ├── system.db                    # AI actions, templates, serverless scripts
│   ├── vectors.db                   # Embedding coordinates & model indices
│   ├── indexes/                     # Tantivy full-text search indices
│   └── uploads/                     # Public and private assets
├── tenants/
│   └── client-alpha/                # Scoped Tenant Directory
│       ├── core.db                  # Tenant users, scoped auth tokens, tenant configs
│       ├── data.db                  # Tenant collections, records, relations
│       ├── logs.db                  # Tenant audit & system event logs
│       ├── system.db                # Tenant-specific scripts & templates
│       ├── vectors.db               # Tenant AI vectors
│       ├── indexes/                 # Scoped Tantivy full-text indices
│       └── uploads/                 # Scoped local/S3 asset storage
└── sandboxes/
    └── session_9f86d081.../         # Ephemeral Sandbox Directory (auto-cleared on expiry)
        ├── core.db
        ├── data.db
        ├── logs.db
        ├── system.db
        ├── vectors.db
        ├── indexes/
        └── uploads/
```

### Architectural Guarantees
* **Zero Cross-Tenant Contention:** Read and write queries target separate SQLite file handles, eliminating shared table locks and index bottlenecks.
* **Hard Data Boundaries:** A query running inside `tenant:client-alpha` cannot read or join tables belonging to `tenant:client-beta`.
* **Independent Search & Vector Indexes:** Each tenant and sandbox maintains an isolated Tantivy full-text index and in-memory HNSW vector index.

---

## 3. Ephemeral Sandboxes & AI Architect

Sandboxes allow developers or the **AI Architect** to generate schemas, write edge scripts, run tests, and insert sample data without touching production databases.

### Cloning Strategies (`CloneStrategy`)
When creating a sandbox, you can choose how much data to replicate from the parent context:

1. **`none`**: Provisions an empty sandbox environment.
2. **`schema` (`SchemaOnly`)**: Replicates all collections, field schemas, relations, and policies with zero records.
3. **`partial` (`Partial(limit)`)**: Clones all schemas, scripts, and templates, plus up to `N` records per collection (default: 100).
4. **`full` (`Full`)**: Executes a direct physical file-level block copy of SQLite database files (`core.db`, `data.db`, `system.db`) and asset folders.
5. **`selected`**: Granularly clones specified collections, scripts, and templates with optional record limits.

### Automated Dependency Resolution
When executing `partial` or `selected` clones, ApexKit recursively walks graph relationships:
* If a cloned record references an `owner` field (`FieldType::Owner`), the corresponding User profile is imported automatically.
* If a cloned record references a `relation` field, the linked foreign record in the target collection is cloned recursively to maintain graph integrity.

### Publishing Sandboxes to Production
Once changes in a sandbox are verified, you can commit the entire sandbox manifest into production as a reusable **Plugin**:

`POST /api/v1/admin/sandboxes/{session_id}/publish`

---

## 4. Multi-Tenant Request Routing

ApexKit resolves scope through multiple ingress mechanisms:

1. **Subdomains:** `client-alpha.your-app.com` routes to `tenant:client-alpha`.
2. **URL Path Prefixes:**
   * `/tenant/{tenant_id}/api/v1/...` routes to `tenant:{tenant_id}`.
   * `/sandbox/{session_id}/api/v1/...` routes to `sandbox:{session_id}`.
3. **Composite API Keys:** Presenting a key with prefix `tnt_client-alpha_sk_...` automatically locks the execution thread to `tenant:client-alpha`.

---

## 5. Scope-Aware Scripting & Cross-Scope Access

Serverless scripts automatically anchor to the scope in which they are invoked.

### A. Scoped Database API (`$db`)
```typescript
// Webhook: ./webhooks/fetch-orders.ts
// Automatically queries storage/tenants/{current_tenant}/data.db
export default async function (req: Request) {
    const orders = await $db.records.list("orders", { limit: 10 });
    return new Response(orders);
}
```

### B. Privileged Root Context Switching (`$root.db` / `ApexKit`)
Scripts running strictly in the **Root** scope can cross boundaries to manage or query tenants:

```typescript
// Root Script Only: ./webhooks/tenant-provisioner.ts
export default async function (req: Request) {
    const { newTenantId, initialAdminEmail } = await req.json();

    // 1. Provision new tenant workspace
    await $root.createTenant(newTenantId, { tier: "pro" });

    // 2. Switch context to the new tenant and seed initial data
    const tenantApp = new ApexKit().tenant(newTenantId);
    
    await tenantApp.users.create(initialAdminEmail, "temporaryPassword123!", "admin");
    await tenantApp.collection("settings").create({ onboarding_complete: false });

    return new Response({ success: true, tenant: newTenantId });
}
```

---

## 6. Tenant Status & Lifecycle Management

Root Administrators can manage tenant operational states via the API or Admin Dashboard:

* **`active`**: Normal read and write operations permitted.
* **`suspended`**: API requests from tenant users return `403 Forbidden`. Root administrators can still access the tenant for maintenance.
* **`archived`**: Tenant database connections are evicted from RAM and kept cold on disk.

```bash
# Suspend a tenant immediately
curl -X PATCH "https://api.your-app.com/api/v1/admin/tenants/client-alpha/status" \
  -H "Authorization: Bearer <ROOT_ADMIN_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"status": "suspended"}'
```

---

## 7. TypeScript SDK Usage (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

// 1. Initialize Root Client
const rootApex = new ApexKit('https://api.your-app.com');
await rootApex.auth.login('admin@system.internal', 'superSecretMasterPassword');

// 2. Create a new Customer Tenant
await rootApex.admins.createTenant('client-beta');

// 3. Switch context to the tenant (all operations route to /tenant/client-beta/)
const clientBeta = rootApex.tenant('client-beta');

const products = await clientBeta.collection('products').list({
  filter: { in_stock: true }
});

// 4. Create an Ephemeral Sandbox for testing
const sandboxMeta = await rootApex.admins.createSandbox(
  'AI Staging Sandbox',
  'partial',
  25 // Clone 25 records per collection
);

const sandboxClient = rootApex.sandbox(sandboxMeta.id);

// Mutations executed on sandboxClient do not affect production
await sandboxClient.collection('products').create({
  title: 'Experimental Test Item',
  price: 99.00
});

// 5. Cleanup Sandbox when done
await rootApex.admins.deleteSandbox(sandboxMeta.id);
```

---

## 8. Resource Quotas & Limits

Configure per-tenant and sandbox limits to ensure noisy-neighbor protection:

| Setting Key | Default | Description |
| :--- | :--- | :--- |
| `max_storage_mb` | `500` MB (Tenants) / `100` MB (Sandboxes) | Maximum allowable physical disk footprint before blocking file uploads. |
| `max_vectors` | `10000` | Maximum high-dimensional vector embeddings permitted. |
| `max_ai_requests` | `100` per 30m | Rate cap for model embedding and AI Action requests. |
| `TENANT_MAX_CRONS` | `2` | Maximum concurrent cron jobs a tenant can execute per 5-minute window. |
