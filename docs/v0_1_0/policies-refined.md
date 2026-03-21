# 🛡️ ApexKit Security Policies & RLS Guide

**Version:** 0.1.0  
**Architecture:** 100% SQL Pushdown (Row-Level Security)

ApexKit uses a high-performance **Policy Engine** that translates logical access rules directly into SQLite SQL. This ensures that security is enforced at the database level, maintaining perfect consistency for **Pagination, Limits, and Offsets**.

---

## 1. How it Works
When you define a policy on a collection (e.g., `read: "auth.id == field:owner_id"`), ApexKit intercepts every request and compiles that string into a SQL `WHERE` clause.

*   **Requesting a List**: `SELECT * FROM records WHERE collection_id = 5 AND (data ->> 'owner_id' = '10')`
*   **Result**: Rows that you do not have permission to see are filtered out by the database engine itself. Your `total` count and `limit` remain accurate.

---

## 2. Available Keywords & Variables

The engine provides access to the **Authentication Context** (who is asking) and the **Record Context** (what is being asked for).

### Authentication Context (`auth`)
Extracted from the requester's JWT token.

| Variable | Type | Description |
| :--- | :--- | :--- |
| `public` | Shorthand | Always returns `true`. Use for guest access. |
| `auth` | Shorthand | Returns `true` if a valid token is present. |
| `admin` | Shorthand | Returns `true` if the user has the `admin` role. |
| `auth.id` | Number | The unique ID of the logged-in user. |
| `auth.role` | String | The role string (e.g., `'user'`, `'editor'`). |
| `auth.email`| String | The email address of the user. |

### Record Context (`field`)
Accesses the JSON data within the record.

| Variable | SQL equivalent | Description |
| :--- | :--- | :--- |
| `field:name` | `data ->> 'name'` | Specific field in the current record. |
| `status` | `data ->> 'status'` | (Implicit) same as `field:status`. |

---

## 3. Syntax & Operators

You can build complex logic using standard programming operators.

| Operator | Description | Example |
| :--- | :--- | :--- |
| `&&` | Logical AND | `auth && field:active == 'true'` |
| `||` | Logical OR | `admin || auth.id == field:user_id` |
| `==` | Equality | `auth.role == 'manager'` |
| `!=` | Inequality | `field:status != 'archived'` |
| `( )` | Grouping | `admin || (auth && field:is_public == 'true')` |

---

## 4. Common Patterns & Examples

### Shorthands (The "Standard" Way)
These cover 90% of use cases:
*   **`public`**: Anyone can access (Great for blog posts, products).
*   **`auth`**: Any logged-in user can access.
*   **`admin`**: Only system administrators can access.
*   **`owner:user_id`**: Only the user whose ID matches the `user_id` field can access.

### Custom Ownership
If your ownership field is named `author` or `creator_id`:
```javascript
"read": "auth.id == field:author"
```

### Role-Based Access (RBAC)
Allow access only to specific roles:
```javascript
"create": "auth.role == 'editor' || auth.role == 'admin'"
```

### Content-State Access
Allow anyone to read a post, but only if it's published:
```javascript
"read": "field:status == 'published' || admin"
```

### Multi-Tenant Ownership (Collaborative)
Allow access if the user is the owner **OR** if the record belongs to their organization:
```javascript
"read": "auth.id == field:owner_id || auth.org_id == field:org_id"
```

---

## 5. Security & Isolation

1.  **Admin Bypass**: Users with the `admin` role in the **Root App** automatically bypass all collection policies for maintenance.
2.  **Physical Isolation**: Policies are evaluated *within* a tenant's database. A user in `Tenant A` can never access `Tenant B` data, even if the policy is set to `public`.
3.  **Update/Delete Checks**: For `UPDATE` and `DELETE` operations, the policy is evaluated against the **existing record** in the database before the action is performed. This prevents users from "taking over" records they don't own.

---

## 6. Pro-Tips

*   **JSON Strings**: When comparing against a string in the database, use single quotes: `field:type == 'internal'`.
*   **Booleans**: ApexKit treats JSON booleans as literals. Use `field:is_active == true`.
*   **Performance**: Because policies are now compiled to SQL, they are extremely fast. However, avoid creating expressions with dozens of `||` chains; use a sidecar collection with roles if your logic gets too complex.