# ApexKit Security Policies & Access Control

**Version:** 0.1.0 (Updated for Expression Engine)
**Context:** Policies define **who** can perform **what** action (`read`, `create`, `update`, `delete`) on a Collection.

ApexKit uses a robust **Expression Engine** allowing for complex logic, role-based access control (RBAC), and row-level security (RLS) by chaining conditions.

---

## 1. Defining Policies

Policies are defined in the `schema` object of a Collection.

**JSON Structure:**
```json
{
  "name": "posts",
  "schema": {
    "fields": {
      "title": { "type": "string", "required": true },
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

The policy engine parses logical expressions.

### Operators
| Operator | Description | Example |
| :--- | :--- | :--- |
| `&&` | Logical AND | `auth && field:published == 'true'` |
| `||` | Logical OR | `admin || auth.id == field:owner_id` |
| `==` | Equality | `auth.role == 'editor'` |
| `!=` | Inequality | `field:status != 'locked'` |
| `( )` | Grouping | `(A || B) && C` |

### Literals
*   **Strings**: Must be quoted using `'` or `"`. (e.g., `'published'`, `"admin"`).
*   **Booleans**: Represented as string comparisons usually, or implicit existence (e.g., `field:is_active == 'true'`).

---

## 3. Context Variables

You have access to the **User** (Requester) and the **Record** (Data).

### Authentication Context (`auth.*`)
These variables are populated from the JWT token passed in the `Authorization` header.

| Variable | Description |
| :--- | :--- |
| `auth` | Boolean. Returns `true` if the user is logged in. |
| `admin` | Boolean. Returns `true` if the user has the `admin` role. |
| `auth.id` | The User ID (integer) of the requester. |
| `auth.role` | The Role string (e.g., `'user'`, `'manager'`, `'student'`). |
| `auth.email` | The Email address of the requester. |

### Record Context (`field:*`)
These variables access the JSON data of the record being acted upon.

| Variable | Description |
| :--- | :--- |
| `field:{name}` | The value of a specific field in the record. |

> **Note on Updates:** During an `update` or `delete` operation, `field:*` refers to the **existing** data in the database, not the new incoming data. This allows for "Locking" logic (e.g., "You cannot delete if status is 'archived'").

---

## 4. Common Use Cases & Examples

### A. Public Read, Authenticated Write
Standard for blogs or forums.
```json
{
  "read": "public",
  "create": "auth",
  "update": "admin",
  "delete": "admin"
}
```

### B. Ownership (Row-Level Security)
Only the user who created the record can modify it.
```json
{
  "read": "public",
  "create": "auth",
  "update": "auth.id == field:owner_id",
  "delete": "auth.id == field:owner_id"
}
```
*Note: Legacy syntax `owner:owner_id` is still supported and equivalent to the update rule above.*

### C. Role-Based Access (RBAC)
Allow multiple specific roles to access data.
```json
{
  "read": "auth.role == 'manager' || auth.role == 'auditor' || admin"
}
```

### D. Workflow Locking
Users can edit their own records, **but only if** the record is in 'draft' mode. Once published, only admins can touch it.
```json
{
  "update": "(auth.id == field:owner_id && field:status == 'draft') || admin"
}
```

### E. Department Isolation
Assuming you store a `department_id` on the User object (custom JWT claims support coming soon, currently assumes mapped via role or lookup scripts).
*Currently, you would model this via roles:*
```json
{
  "read": "auth.role == field:required_role"
}
```

---

## 5. Legacy Shorthands

ApexKit 2.4 maintains backward compatibility with v1.0 shorthands.

| Shorthand | Equivalent Expression |
| :--- | :--- |
| `"public"` | `true` (Always allowed) |
| `"auth"` | `auth` (Checks if JWT exists) |
| `"admin"` | `admin` (Checks if role == 'admin') |
| `"owner:X"` | `auth.id == field:X` |

---

## 6. Testing Policies

You can test policies using the **Script Runner** before applying them to a schema.

```javascript
// Pseudo-code for testing logic manually in a script
export default async function(req) {
    const user = { role: "student", uid: 55 };
    const record = { owner_id: 55, status: "active" };
    
    // Simulate logic
    const can_update = (user.uid == record.owner_id) || (user.role == "admin");
    
    return new Response({ allowed: can_update });
}
```