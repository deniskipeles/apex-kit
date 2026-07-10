# Multi-Tenancy and Sandboxing in ApexKit

ApexKit is architected from the ground up for modern, high-scale SaaS platforms. It provides built-in multi-tenant data isolation and ephemeral developer sandbox environments. Rather than sharing a single monolithic database containing shared rows with complex `where tenant_id = ?` clauses, ApexKit provides **physical, file-level SQLite database isolation** per tenant/sandbox.

---

## 1. Physical Tenant Database Isolation

Every tenant registered under ApexKit receives its own dedicated filesystem directory structure:

```
storage/
  tenants/
    tenant_company_a/
      core.db       # Scoped configuration, users, and credentials
      data.db       # Scoped business collections and records
      logs.db       # Scoped audit and system event logs
      system.db     # Scoped AI actions, templates, and server scripts
      vectors.db    # Scoped coordinates and high-dimensional embeddings
      indexes/      # Scoped Tantivy full-text search index directory
      uploads/      # Scoped local file attachment storage
```

This guarantees:
- **Hard Data Isolation**: SQL queries can never bleed across tenants because they target completely distinct SQLite file descriptors.
- **Zero Shared Key Contention**: No shared table indexes or locks across tenants, maximizing write performance.
- **Easy Backups & Migration**: Backup, restore, or migrate individual tenants by copying their isolated directory.

---

## 2. Multi-Tenant JWT Scoping

To secure API access, ApexKit uses structured **scoped JSON Web Tokens (JWTs)**. When a user authenticates or registers, the issued JWT carries the active `scope` claims.

### JWT Scoping Claims Structure:

```json
{
  "sub": 12,
  "email": "admin@company-a.com",
  "role": "admin",
  "scope": "tenant:company_a",
  "exp": 1799834800
}
```

The `scope` claim values are strictly validated:
- `root`: The primary system administrator context. Accesses global metrics, system logs, master configuration settings, and tenant/sandbox provisioners.
- `tenant:<id>`: Locked to the specified tenant namespace (e.g. `tenant:company_a`). Attempts to access collections, files, or configs outside of this tenant scope return a `403 Forbidden` error.
- `sandbox:<session_id>`: Locked to a transient sandbox development environment.

The ApexKit API middleware automatically extracts the token scope and routes all database connection pools (`DatabaseConnection`) directly to that tenant's SQLite files.

---

## 3. Ephemeral Sandbox Environments

Sandboxes are transient, isolated developer spaces that duplicate your database structure. They allow developers, testing suites, or the **AI Architect** to edit schemas, run scripts, or test integrations without touching production data.

### Cloning Strategies (`CloneStrategy`)

When creating a sandbox, you can choose from four cloning strategies:

1. **SchemaOnly**: Duplicates all collections and schemas without any record data.
2. **Partial(N)**: Duplicates all schemas, scripts, templates, and copies up to `N` records per collection.
3. **Full (Fast Physical Copy)**: Performs a block-level file system copy of the source SQLite database files directly. This is extremely fast but subject to storage limits.
4. **Selected**: Selects specific collections, scripts, or templates to clone, with optional record limits and dependency resolution.

### Automatic Dependency Resolution

When cloning partial or selected records, the sandbox engine recursively walks relations (e.g., relation fields or owner fields) to pull in dependency records and users into the sandbox so that your test sandbox data has perfect integrity and no orphaned foreign key reference errors.

---

## 4. Subdomain & Header Routing

By default, the server determines the current tenant scope using two methods:
1. **Subdomains**: E.g., `client-a.your-saas.com` maps automatically to tenant `client-a`.
2. **Paths**: E.g., `/tenant/client-a/api/v1` routes directly to tenant `client-a`.

---

## 5. SDK Context Switching Example

Using the `@apexkit/sdk` client, switching tenant and sandbox contexts is incredibly clean:

```typescript
import { ApexKit } from '@apexkit/sdk';

// Initialize the root client
const apex = new ApexKit('https://api.my-saas.com');
await apex.auth.login('admin@my-saas.com', 'password');

// 1. Switch to a tenant context
const tenantClient = apex.tenant('company_a');

// Lists records from `storage/tenants/company_a/data.db`
const posts = await tenantClient.collection('posts').list();
console.log(`Company A Posts:`, posts);

// 2. Provision and connect to a sandbox context
const sandboxMeta = await apex.admins.createSandbox(
  'Dev Test Sandbox',
  'Partial', // CloneStrategy
  10 // Clone N records
);

const sandboxClient = apex.sandbox(sandboxMeta.id);

// Any schema updates or writes are isolated inside the sandbox
await sandboxClient.admins.createCollection('new_feedback_col', {
  fields: {
    comment: { type: 'text', required: true }
  }
});
```
