# 🛡️ Security Policies & Access Control

**Version:** 0.1.0
**Context:** Multi-Tenant Row-Level Security (RLS) & RBAC

ApexKit uses a high-performance **Expression Engine** to define who can access what data. Policies are defined directly in the Collection Schema and are evaluated in real-time for every API and GraphQL request.

---

## 1. Defining Policies

Policies are mapped to the four core CRUD operations in the `policies` object of a collection.

**Example Schema:**
```json
{
  "name": "posts",
  "schema": {
    "fields": {
      "title": { "type": "string" },
      "status": { "type": "select", "options": ["draft", "published"] },
      "owner_id": { "type": "owner" }
    },
    "policies": {
      "read": "public",
      "create": "auth",
      "update": "(auth.id == field:owner_id) && field:status == 'draft'",
      "delete": "admin"
    }
  }
}
```

---

## 2. Syntax Reference

The engine parses logical expressions. Spaces are ignored, but strings must be quoted.

### Operators
| Operator | Description | Example |
| :--- | :--- | :--- |
| `&&` | Logical AND | `auth && field:active == 'true'` |
| `||` | Logical OR | `admin || auth.id == field:user_id` |
| `==` | Equality | `auth.role == 'editor'` |
| `!=` | Inequality | `field:status != 'locked'` |
| `( )` | Grouping | `(A || B) && C` |

### Literals
*   **Strings**: `'published'`, `"admin"` (must be quoted).
*   **Booleans**: `true`, `false`.
*   **Numbers**: `101`, `5.5`.

---

## 3. Context Variables

You have access to the **Requester** (Auth) and the **Record** (Field).

### Authentication Context (`auth.*`)
These variables are extracted from the JWT token.

| Variable | Description |
| :--- | :--- |
| `auth` | Returns `true` if the user is logged in. |
| `admin` | Returns `true` if the user has the `admin` role. |
| `auth.id` | The unique ID of the logged-in user. |
| `auth.role` | The role string (e.g., `'manager'`, `'student'`). |
| `auth.email`| The email address of the user. |

### Record Context (`field:*`)
These variables access the JSON data of the record in the database.

| Variable | Description |
| :--- | :--- |
| `field:{name}` | The value of a specific field in the record. |

> **Crucial for Updates**: During `update` or `delete` operations, `field:*` refers to the **existing** data in the database *before* the changes are applied. This allows for logic like "you cannot edit a post if its current status is 'archived'".

---

## 4. Common Use Cases

### A. Ownership (Row-Level Security)
Only allow users to see or edit their own data.
```json
{
  "read": "auth.id == field:user_id",
  "update": "auth.id == field:user_id"
}
```

### B. Role-Based Access (RBAC)
Allow access based on specific custom roles.
```json
{
  "read": "auth.role == 'editor' || auth.role == 'viewer' || admin"
}
```

### C. Workflow Locking
Allow users to update their records, but only if the record is in a 'draft' state.
```json
{
  "update": "auth.id == field:owner_id && field:status == 'draft'"
}
```

### D. Public/Private Toggle
Allow any user to read a record if it is marked as public.
```json
{
  "read": "field:is_public == 'true' || auth.id == field:owner_id"
}
```

---

## 5. Multi-Tenancy & Admin Bypass

ApexKit enforces a strict hierarchy for security:

1.  **Root Admin Bypass**: Users with the `admin` role in the **Root App** context bypass all collection policies. They can see and edit any record in any tenant for support or maintenance.
2.  **Tenant Admin**: Users with the `admin` role inside a **Tenant** context bypass policies *only within that tenant*. They cannot see data in other tenants.
3.  **Physical Isolation**: Policies are evaluated *after* tenant resolution. A request to `tenant_A` can never leak data from `tenant_B`, regardless of how permissive the policy is.

---

## 6. Shorthands (Legacy Support)

ApexKit maintains support for standard shorthands for quick configuration:

| Shorthand | Equivalent Expression |
| :--- | :--- |
| `"public"` | Always returns `true`. |
| `"auth"` | Equivalent to `auth` (logged in). |
| `"admin"` | Equivalent to `auth.role == 'admin'`. |
| `"owner:X"` | Equivalent to `auth.id == field:X`. |

---

## 7. Performance Note

Policies are evaluated in a high-speed Rust-based virtual machine. However, for large `list` requests (e.g., thousands of records), complex expressions involving many `field:*` lookups may impact latency. 

**Best Practice**: Whenever possible, combine policies with **Filters** in your API requests (`?filter={...}`) to reduce the number of records the policy engine needs to process.