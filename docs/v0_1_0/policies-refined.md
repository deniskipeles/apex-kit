# 🛡️ ApexKit Security Policies & RLS 

**Version:** 0.1.0  
**Architecture:** 100% SQL Pushdown (Row-Level Security)

ApexKit uses a high-performance **Policy Engine** that translates logical access rules directly into SQLite SQL. This ensures that security is enforced at the database level, maintaining perfect consistency for **Pagination, Limits, and Offsets**.

---

## ApexKit Policy & Access Control Manual

This document outlines the architecture, syntax, and execution flow of ApexKit's declarative access control system. It is designed to help developers write secure policies, understand how they are evaluated across different boundaries (REST, SQL, GraphQL, and Scripting), and avoid common pitfalls.

---

## 1. Core Concepts

ApexKit uses a unified, declarative policy engine to enforce access control across all operations. Policies are defined as string expressions on a per-collection basis for four actions:
*   **Read (List/Get):** Determines who can query or fetch records.
*   **Create (Register/Insert):** Determines who can write new records.
*   **Update (Edit):** Determines who can modify existing records.
*   **Delete:** Determines who can remove records.

System-level entities (like the `_AuthUser` table) are governed by global settings (e.g., `policy_users`) but evaluate through the identical expression engine.

---

## 2. Policy Language Syntax

The policy engine parses logical expressions containing literals, variables, and comparison operators.

### Standard Keywords
*   `public`: Anyone can perform the action (unauthenticated or authenticated).
*   `auth`: The requester must provide a valid JWT token (authenticated).
*   `admin`: The requester must have the `admin` role in their claims.

### Context Variables
You can reference properties of the active requester (`auth`) or attributes of the record being evaluated (`field`):

| Variable | Description | Example |
| :--- | :--- | :--- |
| `auth.id` | The database ID (`uid`) of the authenticated user. | `owner:auth.id` |
| `auth.role` | The role string of the authenticated user (e.g., `"editor"`). | `auth.role == "editor"` |
| `auth.email` | The email address of the authenticated user. | `auth.email == "admin@apexkit.io"` |
| `field:fieldName` | Accesses a specific JSON field on the record. | `field:status == "published"` |
| `owner:fieldName` | Shorthand helper matching `record.data[fieldName] == auth.id`. | `owner:author_id` |

### Supported Operators
*   **Logical:** `&&` (AND), `||` (OR), parenthesized groupings `()`
*   **Comparison:** `==` (Equality), `!=` (Inequality)

### Example Expressions

```ini
# Admins can do anything; owners can edit their own records
admin || owner:author_id

# Must be authenticated, and the record must be in "draft" status
auth && field:status == "draft"

# Only users with the "editor" role can edit, unless they are the owner
auth.role == "editor" || owner:manager_id

# Public can read if marked "public", otherwise must be the owner
field:visibility == "public" || owner:author_id
```

---

## 3. Architectural Evaluation Flow

When an API request (REST or GraphQL) is made, the policy is evaluated in three distinct phases:

### Phase A: Table-Level Pre-Flight
Before hitting the database, the API checks if the requester is allowed to perform the operation in general. This is done by evaluating the policy expression with no record context (`record_data = None`):
*   If the policy is `"public"`, access is granted.
*   If the policy contains `"owner:id"`, table-level check returns `false` (requiring a row-by-row check) but allows the request to proceed to the database phase.
*   If the policy evaluates to false (e.g. `"admin"` for a public user), the request is rejected immediately with a `403 Forbidden` status.

### Phase B: Database RLS (Row-Level Security) Pushdown
For read operations (such as listing records), evaluating the policy in-memory for millions of records is highly inefficient. Instead, ApexKit compiles the policy expression directly into a SQL `WHERE` clause.

```rust
// policies::compile_to_sql("admin || owner:author_id", Some(Claims { uid: 42, role: "user" }))
```
Compiles to:
```sql
(1=0 OR (json_extract(records.data, '$.author_id') = '42' OR CAST(json_extract(records.data, '$.author_id') AS TEXT) = '42'))
```

#### Type-Coercion Resilience
SQLite does not enforce strict type affinity on JSON extractions. To prevent failures where a database record stores an ID as a string (`"42"`) but a filter queries for a number (`42`), the compiler automatically wraps JSON equality checks with a `CAST` fallback:
```sql
(col = ? OR CAST(col AS TEXT) = CAST(? AS TEXT))
```
This guarantees that numeric-to-string comparisons succeed seamlessly and provides compatibility for future UUID transitions.

### Phase C: Recursive Deep Expansion Sanitization
When using the REST API's `expand` parameter (e.g. `?expand=author_id,likes.user_id`) or querying relationships in GraphQL, related data is populated in memory. 

To prevent data leaks, the API layer implements a **Deep Recursive Security Sanitizer** that traverses down the returned JSON tree alongside the schema:
1. It parses the requested `expand` path into a structured tree.
2. It walks through each JSON object and checks if any `Owner` fields (`FieldType::Owner`) are populated.
3. It evaluates the user-level policy (`policy_users`) against the expanded profile.
4. If the requester is unauthorized, it redacts the nested record entirely (setting it to `null`) before sending the response back to the client.

```
Request ?expand=author_id,likes.user_id
   │
   ├── [DB Layer] Fetches Records & Hydrates JSON data
   │
   └── [API Layer] Recursive Sanitization
         ├── Is author_id readable? Yes -> Keep
         └── Is likes[0].user_id readable? No -> Set user_id to null
```

---

## 4. GraphQL Integration

The dynamic GraphQL schema automatically respects your collection policies and system configuration:

*   **List Queries:** When you query a collection, the resolver compiles the read policy into SQL RLS parameters and executes it against the database. If the policy results in `1=0` (completely blocked), a `Forbidden` error is returned.
*   **Bidirectional Traversal:** Reverse relationships (e.g., querying `likes` inside `pins`, or `pins` inside `_AuthUser`) are bound to the exact same SQL RLS filters, ensuring that nested lists are automatically filtered to show only records the requester has the right to see.
*   **Auto-rebuilds:** To prevent stale configurations, whenever you update a collection schema or policies, the system automatically invalidates the cache and schedules a safe, async reload of the GraphQL schema.

---

## 5. System Fields and the `_AuthUser` Table

To prevent namespace collisions with your custom tables, all system-level GraphQL objects, queries, and mutations are prefixed with an underscore (`_`):

*   The user table type is `_AuthUser` (instead of `User`).
*   The query field to list users is `_users` (instead of `users`).
*   Standard system mutations like ping are `_ping`.

To configure read, create, update, or delete rules for the user table, navigate to **Settings -> Security -> User Data Policies** in the Dashboard, or modify the `"policy_users"` key in the Config Registry.

```json
// Example "policy_users" JSON Value
{
  "read": "public",
  "create": "public",
  "update": "admin || owner:id",
  "delete": "admin"
}
```

---

## 6. Developers Best Practices

1.  **Use `owner:fieldName` Shorthand:** For simple ownership checks, prefer `owner:author_id` rather than writing `field:author_id == auth.id`. It compiles to highly optimized SQL.
2.  **Avoid Mixed Operations in Single Policies:** Keep expressions readable. If a policy is too complex, write a server-side script hook (e.g., `before_create_record`) to handle the security validation procedurally.
3.  **Remember Sandbox Ephemerality:** Workspaces spawned inside a Sandbox (`/sandbox/{session_id}`) copy schemas and data but run completely isolated in-memory indexes and local databases. Ensure you publish the sandbox to persist changes back to the production environment.
4.  **Cooperative Yielding:** If you are writing a custom background script that processes database records in a loop, insert a small sleep interval (`await $util.sleep(50)`) between iterations. This releases the JS runtime thread and allows live API requests to interleave smoothly.
