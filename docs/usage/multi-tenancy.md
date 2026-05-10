# Multi-Tenancy and Sandboxing

ApexKit is designed from the ground up to support multi-tenant applications, allowing you to isolate data for different customers or environments within a single server instance.

## Multi-Tenancy

In a multi-tenant architecture, multiple "Tenants" (e.g., different companies) share the same application but their data is strictly isolated.

### How it works in ApexKit:
- Every request is associated with a **Tenant ID**.
- Data for each tenant is logically or physically separated (depending on configuration).
- The SDK makes it easy to switch contexts.

```javascript
// Initialize for Tenant A
const companyA = apex.tenant('company_a_id');
await companyA.collection('users').list(); // Only users from Company A
```

### Benefits:
- **Scalability**: Manage thousands of customers on a single instance.
- **Security**: Hard isolation at the database query level.
- **Maintenance**: Update your application code once, and all tenants benefit.

## Sandboxing

Sandboxes are temporary, isolated environments. They are perfect for:
1. **Feature Development**: Test a new schema without touching production data.
2. **AI Experiments**: Let the AI Architect "draft" an entire backend in a sandbox.
3. **Demo Environments**: Create a "Fresh" environment for every sales demo that can be deleted afterward.

### Creating a Sandbox
```javascript
const sandbox = await apex.createSandbox();
console.log(`Sandbox ID: ${sandbox.id}`);

const client = apex.sandbox(sandbox.id);
// Run migrations, create records, etc.
```

## Scoped API Keys

You can generate API Keys that are locked to a specific tenant or sandbox. This ensures that even if a key is leaked, the impact is limited to a single tenant's data.

## Implementation Patterns

### 1. Subdomain-based Tenancy
Use the host header to determine the tenant.
- `customer1.yourapp.com` -> `apex.tenant('customer1')`
- `customer2.yourapp.com` -> `apex.tenant('customer2')`

### 2. User-based Tenancy
If a user belongs to multiple organizations, they can select one after login, and the SDK context can be updated.

```javascript
const user = await apex.auth.login(...);
const activeTenant = user.organizations[0].id;
const client = apex.tenant(activeTenant);
```

## Performance Considerations
ApexKit uses high-performance indexing to ensure that even with thousands of tenants, query speed remains consistent. For very large-scale deployments, ApexKit supports **Database Sharding**, where tenants can be distributed across multiple SQLite files.
