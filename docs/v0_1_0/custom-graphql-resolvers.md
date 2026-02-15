# 🔮 Custom GraphQL Resolvers

**Version:** 0.1.0
**Context:** Server-Side Scripting & API Extension

While ApexKit automatically generates CRUD GraphQL schemas for all your collections, real-world applications often need custom logic (e.g., "Aggregate Sales by Region", "Trigger External Email", or "Calculate User Level"). 

ApexKit uses a **Code-First** approach. You define the GraphQL schema metadata **inside** a standard JavaScript script.

---

## 1. The Anatomy of a Resolver

To create a resolver, create a script in the **Admin UI > Scripts** with the trigger type set to **`graphql`**.

### The Configuration Object
You must export a constant named `graphql`. This object tells the system where to attach the field in the GraphQL graph and what types it accepts/returns.

```javascript
// Metadata for the GraphQL Schema Builder
export const graphql = {
  "parent": "Query",      // Options: Query, Mutation, User, or {CollectionName}
  "name": "getWeather",   // The name of the field in your GQL query
  "args": {               // Input arguments (optional)
    "city": "String!",    // '!' denotes non-nullable (required)
    "unit": "String"
  },
  "returnType": "JSON"    // Options: String, Int, Float, Boolean, ID, JSON, or [Type]
};
```

### The Logic Handler
The default exported function is the execution logic. Arguments passed in the GraphQL query are available via `await req.json()`.

```javascript
export default async function(req) {
  const args = await req.json(); // Arguments from GQL
  const { city, unit } = args; 
  
  // Logic: Fetch from external API
  const response = await $http.get(`https://api.weather.com/v1/${city}`);
  const data = JSON.parse(response);

  return new Response({
    city: city,
    temp: data.temp,
    unit: unit || "C"
  });
}
```

---

## 2. Common Resolver Patterns

### A. Analytical Queries (Aggregation)
Use the `$db.query` engine to perform complex calculations and expose them as a single GraphQL field.

```javascript
export const graphql = {
  "parent": "Query",
  "name": "salesSummary",
  "args": { "category": "String" },
  "returnType": "JSON"
};

export default async function(req) {
  const { category } = await req.json();
  
  const stats = await $db.query(null, {
    "from": "orders",
    "select": [
       { "fn": "sum", "field": "total", "as": "revenue" },
       { "fn": "count", "field": "id", "as": "count" }
    ],
    "where": category ? { "category": category } : {}
  });

  return new Response(stats[0]); // Return the single row of stats
}
```

### B. Mutations (Side Effects)
Use the `Mutation` parent to create endpoints that perform actions.

```javascript
export const graphql = {
  "parent": "Mutation",
  "name": "sendContactForm",
  "args": { "email": "String!", "msg": "String!" },
  "returnType": "Boolean"
};

export default async function(req) {
  const { email, msg } = await req.json();
  await $mail.send("admin@app.com", "Contact", `${email} says: ${msg}`);
  return new Response(true);
}
```

### C. Type Extensions (Computed Fields)
Attach fields to existing types. For example, add a `fullName` field to the `User` object.

```javascript
export const graphql = {
  "parent": "User", // This attaches the field to every User object
  "name": "fullName",
  "returnType": "String"
};

export default async function(req) {
  // For extensions, the current object is passed as 'parent' in the body
  const body = await req.json();
  const user = body.parent; 
  
  // Logic: Combine fields or fetch from a profile collection
  return new Response(`${user.metadata.first_name} ${user.metadata.last_name}`);
}
```

---

## 3. Type Mapping Reference

When defining `args` or `returnType`, use these string identifiers:

| GQL Type | Description |
| :--- | :--- |
| **`String`** | Text data. |
| **`Int`** | Whole numbers. |
| **`Float`** | Decimal numbers. |
| **`Boolean`** | `true` or `false`. |
| **`ID`** | Unique identifier string. |
| **`JSON`** | Dynamic object/array. Use this for complex responses. |

**Modifiers:**
*   `String!` : Required (Non-null).
*   `[String]` : A list of strings.
*   `[String!]!` : A required list of non-null strings.

---

## 4. Deployment & Context

### Scoping
Resolvers are **Tenant-Aware**. 
*   If a script is created in **Tenant A**, it only appears in Tenant A's GraphQL schema (`/tenant/A/graphql`).
*   The script automatically uses Tenant A's database when calling `$db`.

### Reloading
GraphQL schemas are compiled into the Rust runtime for performance. If you create or update a `graphql` script, you must reload the system to see the changes:
1.  Click the **"Restart App"** button in the Admin UI Topbar.
2.  Or perform a `POST /api/v1/admin/system/reload`.

### Testing
Use the **GraphQL Playground** available at `/graphql` or the scoped URL `/tenant/{id}/graphql` to test your new fields with full autocomplete support.