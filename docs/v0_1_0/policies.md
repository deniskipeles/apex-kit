# 🛡️ Security Policies & Access Control (ABAC & RLS)

**Version:** 0.1.0  
**Context:** Multi-Tenant Row-Level Security (RLS) & Role-Based Access Control (RBAC)

ApexKit uses an expression engine to enforce data access rules across all API endpoints, GraphQL queries, and database operations. Policies are defined per collection and evaluated in real time for `read`, `create`, `update`, and `delete` operations.

---

## 1. Defining Policies

Policies are configured inside the `policies` object of a collection schema:

```json
{
  "name": "posts",
  "schema": {
    "fields": {
      "title": { "type": "string" },
      "status": { "type": "select", "options": ["draft", "published"] },
      "author_id": { "type": "owner", "auto": true }
    },
    "policies": {
      "read": "public",
      "create": "auth",
      "update": "admin || (auth.id == field:author_id && field:status == 'draft')",
      "delete": "admin || owner:author_id"
    }
  }
}
```

---

## 2. Policy Syntax

ApexKit supports two formats:

1. **String Expressions (Concise):** Uses logical operators (`&&`, `||`), keywords, and variable comparisons.
2. **JSON Rules (Advanced):** Uses structured MongoDB-style syntax with support for subqueries (`@get()`) and request payload validation (`@request.record`).

### String Expression Operators & Keywords

| Keyword / Operator | Description | Example |
| :--- | :--- | :--- |
| **`public`** | Open to all callers (unauthenticated or authenticated). | `public` |
| **`auth`** | Requires a valid JWT token or API key. | `auth` |
| **`admin`** | Requires `role: "admin"` in caller claims. | `admin` |
| **`owner:{field}`** | Shorthand matching `record.data[field] == auth.id`. | `owner:author_id` |
| **`&&`** | Logical AND | `auth && field:status == 'published'` |
| **`||`** | Logical OR | `admin || owner:author_id` |
| **`==` / `!=`** | Equality / Inequality | `auth.role == 'editor'` |
| **`( )`** | Precedence grouping | `(auth || public) && field:active == 'true'` |

### Context Variables

| Variable | Description |
| :--- | :--- |
| **`auth.id`** | Numeric ID (`uid`) of the authenticated user. |
| **`auth.role`** | Role string of the authenticated user (`"admin"`, `"user"`). |
| **`auth.email`** | Email address of the authenticated user. |
| **`field:{name}`** | Current value of `{name}` stored in the database record. |

---

## 3. SQL Pushdown (Row-Level Security)

For read operations (such as listing records or resolving GraphQL collections), ApexKit compiles policy expressions directly into SQLite `WHERE` clauses:

```rust
// Policy: "admin || owner:author_id"
// Caller: Claims { uid: 42, role: "user" }
```
Compiles to:
```sql
(1=0 OR (json_extract(records.data, '$.author_id') = '42' OR CAST(json_extract(records.data, '$.author_id') AS TEXT) = '42'))
```

This guarantees:
* Accurate server-side pagination (`page`, `per_page`, `total`).
* Zero in-memory filtering overhead on large tables.
* Type-coercion compatibility between integer and string ID formats.

---

## 4. Common Security Patterns

### A. Record Ownership (Private User Data)
Only the creator can read, update, or delete their records:
```json
{
  "read": "owner:user_id",
  "create": "auth",
  "update": "owner:user_id",
  "delete": "admin || owner:user_id"
}
```

---

### B. Public Read with Authenticated Creation
Anyone can view published posts, but only logged-in users can author new ones:
```json
{
  "read": "public",
  "create": "auth",
  "update": "admin || owner:author_id",
  "delete": "admin"
}
```

---

### C. State-Dependent Workflow Locking
Users can edit their submissions only while in `"draft"` status:
```json
{
  "update": "admin || (owner:author_id && field:status == 'draft')"
}
```

---

### D. Multi-Role Permissions (RBAC)
Grant access based on custom user roles:
```json
{
  "read": "auth.role == 'auditor' || auth.role == 'manager' || admin",
  "update": "auth.role == 'manager' || admin"
}
```

---

## 5. Multi-Tenant Scope Hierarchy

ApexKit enforces physical database separation:

1. **Root Admin Access:** A JWT issued for the `root` scope with `role: "admin"` has global administrative access across all tenant databases.
2. **Tenant Admin Scope:** A user with `role: "admin"` inside `tenant:client-a` has administrative access **only** within `client-a`.
3. **Physical Isolation:** Policy checks are executed against the specific tenant's SQLite file (`storage/tenants/{tenant_id}/data.db`). Data cannot leak across tenant boundaries regardless of the policy expression.

---

## 6. Performance Best Practices

1. **Prefer `owner:{field}` Shorthand:** Compiles to optimized, index-friendly SQL expressions.
2. **Index Policy Fields:** If a policy frequently references a field (e.g. `field:status == 'published'`), add `sql_indexed: true` to that field's schema definition to maintain $O(\log N)$ query speeds.
3. **Keep Expressions Focused:** For complex conditional flows involving third-party checks, use a server-side `before_create_record` or `before_update_record` script hook instead of oversized policy strings.