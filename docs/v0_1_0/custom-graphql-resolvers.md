# 🔮 Custom GraphQL Resolvers

**Version:** 0.1.0  
**Context:** Server-Side TypeScript/JavaScript Scripting & GraphQL Extensions

While ApexKit automatically generates CRUD GraphQL queries and mutations for all collections, real-world applications frequently require custom resolvers (e.g. analytical aggregations, third-party integrations, or computed fields on records and user types).

ApexKit uses a **Code-First** approach: you define the GraphQL schema metadata directly inside a standard TypeScript or JavaScript script.

---

## 1. Defining a Resolver

To create a custom GraphQL resolver, create a script with `trigger_type: "graphql"` in the Admin Dashboard or sync it via the ApexKit CLI.

### The Metadata Definition (`export const graphql`)
Every resolver must export a constant named `graphql` defining the target parent object, field name, input arguments, and return type:

```typescript
export const graphql = {
  parent: "Query",      // "Query", "Mutation", "User" (or "_AuthUser"), or {CollectionName}
  name: "getWeather",   // Field name in GraphQL operations
  args: {               // Optional input arguments
    city: "String!",    // '!' denotes non-nullable (required)
    unit: "String"
  },
  returnType: "JSON"    // Scalar type, collection type name, or list [Type]
};
```

### The Resolver Implementation
The default exported async function serves as the execution handler. Passed arguments are accessible by reading `await req.json()`:

```typescript
export default async function (req: Request) {
  const args = await req.json();
  const { city, unit = "C" } = args;

  // External API integration using native fetch
  const response = await fetch(`https://api.weather.com/v1/${encodeURIComponent(city)}`);
  const data = await response.json();

  return new Response({
    city,
    temp: data.temp,
    unit
  });
}
```

---

## 2. Supported Resolver Types & Examples

### A. Analytical Queries (`Query`)
Use the analytical `$db.query` engine to compute multi-column aggregations and expose them through a single GraphQL field.

```typescript
// Script Name: sales-summary
// Trigger: graphql | Path: ./webhooks/sales-summary.ts

export const graphql = {
  parent: "Query",
  name: "salesSummary",
  args: {
    category: "String"
  },
  returnType: "JSON"
};

export default async function (req: Request) {
  const { category } = await req.json();

  const report = await $db.query({
    from: "orders",
    select: [
      { fn: "sum", field: "total", as: "revenue" },
      { fn: "count", field: "id", as: "order_count" },
      { fn: "avg", field: "total", as: "average_order" }
    ],
    where: category ? { category } : {}
  });

  return new Response(report[0] || {});
}
```

**GraphQL Query:**
```graphql
query {
  salesSummary(category: "electronics")
}
```

---

### B. Action Mutations (`Mutation`)
Target the `Mutation` root to execute operations with side-effects, such as triggering notifications or transactions.

```typescript
// Script Name: send-contact-form
// Trigger: graphql | Path: ./webhooks/send-contact-form.ts

export const graphql = {
  parent: "Mutation",
  name: "sendContactForm",
  args: {
    email: "String!",
    message: "String!"
  },
  returnType: "Boolean"
};

export default async function (req: Request) {
  const { email, message } = await req.json();

  await $mail.send("support@app.com", `Contact Inquiry from ${email}`, message);

  return new Response(true);
}
```

**GraphQL Mutation:**
```graphql
mutation {
  sendContactForm(email: "visitor@example.com", message: "Need assistance with plans")
}
```

---

### C. Type Extensions & Computed Fields (Parent Context)
Attach computed properties directly to standard entity types (such as `User` / `_AuthUser` or collection types like `Posts`).

When attached to an entity, the resolver automatically receives the underlying entity record in `parent`:

```typescript
// Script Name: user-display-name
// Trigger: graphql | Path: ./webhooks/user-display-name.ts

export const graphql = {
  parent: "User", // Attaches to the system User type
  name: "displayName",
  returnType: "String"
};

export default async function (req: Request) {
  const body = await req.json();
  const parentUser = body.parent; // { id, email, ... }

  const profile = await $db.records.list("profiles", {
    filter: { user_id: parentUser.id },
    limit: 1
  });

  const customName = profile.items[0]?.data?.full_name;
  return new Response(customName || parentUser.email);
}
```

**GraphQL Query:**
```graphql
query {
  _users {
    items {
      id
      email
      displayName # Custom computed field
    }
  }
}
```

---

## 3. GraphQL Type System Mapping

When declaring `args` or `returnType` in `export const graphql`, use the following type names:

| Identifier | Description | GraphQL Mapping |
| :--- | :--- | :--- |
| **`"String"`** | Textual UTF-8 string. | `String` |
| **`"Int"`** | Signed 32-bit integer. | `Int` |
| **`"Float"`** | 64-bit floating point number. | `Float` |
| **`"Boolean"`** | Logical boolean (`true` / `false`). | `Boolean` |
| **`"ID"`** | Unique entity identifier string. | `ID` |
| **`"JSON"`** | Arbitrary object, map, or array. | `JSON` |
| **`"{CollectionName}"`** | Maps to a registered collection type (e.g. `"Posts"`). | `Posts` |

### Modifiers
* **Non-Nullable (`!`):** Append `!` to mark arguments or return types as required (e.g., `"String!"`, `"ID!"`).
* **List Arrays (`[...]`):** Wrap type names in brackets to return arrays (e.g., `"[String]"`, `"[JSON]"`).
* **Non-Nullable List (`[Type!]!`):** Wrap and append exclamation marks for strict arrays (e.g., `"[String!]!"`).

---

## 4. Multi-Tenancy & Hot-Reloading

1. **Scope Isolation:** Custom resolvers are scoped to the environment in which they are created:
   - Resolvers created in **Root** extend the root schema (`POST /graphql`).
   - Resolvers created in a **Tenant** extend only that tenant's schema (`POST /tenant/{id}/graphql`).
   - When calling `$db` inside a tenant's resolver, queries execute strictly against that tenant's database partition.
2. **Dynamic In-Memory Rebuilding:** ApexKit automatically detects changes to scripts with `trigger_type: "graphql"` and triggers an in-memory schema recompilation without dropping connections.
3. **Manual Reload:** To force a schema reload via the API:
   ```bash
   curl -X POST "https://api.your-app.com/api/v1/admin/system/reload" \
     -H "Authorization: Bearer <MASTER_KEY_OR_ADMIN_JWT>" \
     -H "Content-Type: application/json" \
     -d '{"target": "root"}'
   ```
4. **Interactive Testing:** Open the browser GraphQL Playground at `/graphql` (or `/tenant/{id}/graphql` for tenant contexts) to test your custom queries and mutations with full type autocomplete and schema introspection.