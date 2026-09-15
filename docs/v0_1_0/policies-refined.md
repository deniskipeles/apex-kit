# 🛡️ ApexKit Security Policies & Access Control (RLS & ABAC)

**Version:** 0.1.0  
**Architecture:** 100% SQL Pushdown (Row-Level Security) & Declarative In-Memory AST

ApexKit features a unified access control engine that enforces security boundaries consistently across REST APIs, GraphQL operations, and internal query layers. Access control rules are defined per collection for the four fundamental operations: **`read`**, **`create`**, **`update`**, and **`delete`**.

---

## 1. Core Policy Syntax & Modes

ApexKit supports two policy syntax formats:

### A. String Expression Syntax (Concise)
Uses logical operators (`&&`, `||`), keywords, and variable comparisons:

```ini
# Admins have full access; authors can only edit their own records
admin || owner:author_id

# Authenticated users can view records marked as public or their own drafts
auth && (field:status == "published" || owner:author_id)

# Role check
auth.role == "editor" || admin
```

#### String Keywords & Context Variables
* **`public`**: Accessible by anyone (unauthenticated or authenticated).
* **`auth`**: Requires a valid JWT token or API key.
* **`admin`**: Caller must hold the `admin` role in their token claims.
* **`owner:{field_name}`**: Shorthand matching `record.data[field_name] == auth.id`.
* **`auth.id`**: The authenticated user's numeric ID (`uid`).
* **`auth.role`**: The authenticated user's role string (`"user"`, `"editor"`).
* **`auth.email`**: The authenticated user's email address.
* **`field:{name}`**: Evaluates the current value of `{name}` on the existing database record.

---

### B. JSON-Based Policy Syntax (Advanced ABAC / RBAC)
For structured validation, array containment (`$in`/`$nin`), subqueries (`@get()`), and incoming payload inspections:

```json
{
  "$or": [
    { "@request.auth.role": "admin" },
    {
      "$and": [
        { "author_id": "@request.auth.id" },
        { "@request.record.status": { "$neq": "archived" } }
      ]
    }
  ]
}
```

#### JSON Context Variables (`@` Prefix)
* **`@request.auth`**: Dynamic claims from the active caller (`@request.auth.id`, `@request.auth.role`, `@request.auth.email`).
* **`@request.record`**: Incoming client payload submitted in `create` or `update` requests (e.g. `@request.record.price`).
* **`@record`**: Existing record data currently committed in the database.
* **`@get()`**: Executes an isolated database subquery to retrieve related collections or profile identifiers dynamically.

---

## 2. Evaluation Lifecycle

```
[Incoming Request]
        │
        ├── Phase 1: Pre-Flight Access Check
        │     - Evaluates public access and admin tokens.
        │     - If unauthorized at table level -> Rejects with 403 Forbidden.
        │
        ├── Phase 2: Database SQL Pushdown (RLS)
        │     - Compiles policy expression directly into SQLite WHERE clause.
        │     - Ensures pagination, limits, and offsets are 100% accurate.
        │
        └── Phase 3: Recursive Deep Expansion Sanitization
              - Traverses related entities and user profiles in memory.
              - Redacts unauthorized nested records before response dispatch.
```

---

## 3. Phase Details

### Phase 1: Pre-Flight Check
Before executing queries, ApexKit validates whether the requester is permitted to interact with the collection in general (`record_data = None`):
* If the rule is `public`, access is permitted immediately.
* If caller holds `admin` role, the operation bypasses restrictions.
* If a table-level requirement fails (e.g. non-admin attempting an `admin`-only mutation), the request is rejected immediately with `403 Forbidden`.

### Phase 2: SQL RLS Pushdown
For queries (such as listing records or executing GraphQL collections), evaluating policies in memory across thousands of records is slow and breaks pagination. ApexKit compiles policies directly into SQLite SQL clauses:

```rust
// Policy: "admin || owner:author_id"
// Caller: Claims { uid: 42, role: "user" }
```
Compiles directly into:
```sql
(1=0 OR (json_extract(records.data, '$.author_id') = '42' OR CAST(json_extract(records.data, '$.author_id') AS TEXT) = '42'))
```

#### Type-Coercion Resilience
SQLite stores data in dynamic JSONB fields without strict type affinity. To prevent mismatches where an ID is stored as a string (`"42"`) while a query checks for a number (`42`), the compiler automatically wraps JSON equality checks with `CAST` fallbacks to ensure reliable evaluation.

---

### Phase 3: Recursive Deep Expansion Sanitization
When using `?expand=author_id,comments.user_id` in REST or fetching nested relations in GraphQL, related data is populated in memory.

To prevent unauthorized data exposure:
1. ApexKit parses the requested relationship expansion tree.
2. It inspects all expanded entities containing `owner` fields (`FieldType::Owner`).
3. It evaluates the global user policy (`policy_users`) against the expanded profile.
4. If unauthorized, the nested user profile is redacted to `null` before sending the HTTP response.

---

## 4. Subqueries with `@get()`

You can query external collections to validate relational permissions:

```json
{
  "workspace_id": {
    "$in": {
      "@get()": {
        "from": "workspace_members",
        "select": ["workspace_id"],
        "where": {
          "user_id": "@request.auth.id"
        }
      }
    }
  }
}
```

* The `@get()` block executes within the current tenant context.
* The query engine flattens the selected column into an array for `$in` / `$nin` comparison.

---

## 5. Policy Debugger (`@log`)

Wrap any policy in `@log()` to inspect how rules are resolved:

* **`@log(JSON)`**: Prints the preprocessed JSON structure with `@request` and `@get()` variables resolved.
* **`@log(SQL)`**: Prints both the resolved JSON and the compiled SQLite `WHERE` clause.

All debug logs are written to stdout and recorded in `_system_logs` under the `policy_debugger` namespace.

```json
{
  "@log(SQL)": {
    "status": "published",
    "views": { "$gt": 10 }
  }
}
```

---

## 6. Global User Policies (`policy_users`)

The system `_AuthUser` table is governed by the `policy_users` configuration setting:

```json
{
  "read": "admin || owner:id",
  "create": "public",
  "update": "admin || owner:id",
  "delete": "admin"
}
```

You can configure these rules under **Admin Dashboard > Settings > Security > User Data Policies** or by modifying the `policy_users` key via `POST /api/v1/admin/config`.

---

## 7. Best Practices

1. **Use `owner:fieldName` for Clean Row Ownership:** Prefer `owner:author_id` over manual variable comparisons; it compiles to clean, index-friendly SQL.
2. **Combine RLS with Indexed Fields:** Mark fields referenced in frequent query policies as `sql_indexed: true` in your collection schema to accelerate SQLite `WHERE` evaluation.
3. **Lock Immutable Fields on Update:** Use `@request.record.field == @record.field` in JSON policies to prevent users from altering restricted fields (like `role` or `is_verified`).
