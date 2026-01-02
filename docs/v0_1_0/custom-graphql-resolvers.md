
# 🔮 Custom GraphQL Resolvers

**Version:** 0.1.0
**Context:** Server-Side Scripting

While ApexKit automatically generates CRUD APIs for your collections, real-world applications often need custom logic (e.g., "Calculate Total Revenue", "Send Email", or "Get Weather").

ApexKit uses a **Code-First** approach. You define the GraphQL schema definition **inside** your JavaScript script.

---

## 1. The Anatomy of a Resolver

To create a resolver, you create a standard **Script** in the Admin UI, but you must export a specific configuration constant.

### The Configuration Object
You must export a constant named `graphql` containing the schema metadata.

```javascript
export const graphql = {
  parent: "Query",      // Where to attach this field (Query, Mutation, User, or CollectionName)
  name: "getWeather",   // The name of the field in the API
  args: {               // Input arguments
    city: "String!",    // '!' denotes required
    unit: "String"
  },
  returnType: "JSON"    // The return type (String, Int, Float, Boolean, JSON)
};
```

### The Logic Handler
The default exported function receives the request. The GraphQL arguments are available in `req.args`.

```javascript
export default async function(req) {
  const { city, unit } = req.args; 
  
  // Your Logic Here...
  const temp = Math.floor(Math.random() * 30);

  return {
    city: city,
    temperature: temp,
    unit: unit || "C"
  };
}
```

---

## 2. Usage Examples

### Example A: A Simple Query
**Goal:** Create a query `hello(name: String)` that returns a greeting.

1.  **Create Script:** `hello-world`
2.  **Code:**
    ```javascript
    export const graphql = {
      parent: "Query",
      name: "hello",
      args: { name: "String!" },
      returnType: "String"
    };

    export default async function(req) {
      return `Hello, ${req.args.name}!`;
    }
    ```
3.  **GraphQL Query:**
    ```graphql
    query {
      hello(name: "Developer")
    }
    ```

### Example B: A Mutation (Side Effect)
**Goal:** Send a contact email and return success status.

1.  **Create Script:** `contact-form`
2.  **Code:**
    ```javascript
    export const graphql = {
      parent: "Mutation",
      name: "sendContactEmail",
      args: {
        email: "String!",
        message: "String!"
      },
      returnType: "Boolean"
    };

    export default async function(req) {
      const { email, message } = req.args;

      // Use internal mailer
      await $mail.send("admin@myapp.com", "New Contact", `${email} says: ${message}`);
      
      return true;
    }
    ```
3.  **GraphQL Mutation:**
    ```graphql
    mutation {
      sendContactEmail(email: "user@test.com", message: "Help!")
    }
    ```

### Example C: Computed Field (Type Extension)
**Goal:** Add a `fullName` field to the `User` type, combining metadata.

1.  **Create Script:** `user-fullname`
2.  **Code:**
    ```javascript
    export const graphql = {
      parent: "User",      // Attach to the User object
      name: "fullName",
      args: {},
      returnType: "String"
    };

    export default async function(req) {
      // For type extensions, the parent object is passed as 'parent'
      const user = req.args.parent;
      
      // Assume we store profile data in a separate collection
      const profile = await $db.find_one("profiles", { user_id: user.id });
      
      return profile ? `${profile.first_name} ${profile.last_name}` : "Unknown";
    }
    ```
4.  **GraphQL Query:**
    ```graphql
    query {
      users {
        items {
          email
          fullName # <--- This is your calculated field
        }
      }
    }
    ```

---

## 3. Supported Types

When defining `args` or `returnType`, use these string values:

| Type | Description |
| :--- | :--- |
| **`String`** | Textual data. |
| **`Int`** | Integer numbers. |
| **`Float`** | Decimal numbers. |
| **`Boolean`** | `true` or `false`. |
| **`ID`** | Unique identifier strings. |
| **`JSON`** | A dynamic object or array. This is the most flexible return type. |

**Modifiers:**
*   `String!` : Required (Non-Null).
*   `[String]` : List of Strings.
*   `[String!]!` : Required List of Required Strings.

---

## 4. Workflow & Deployment

1.  **Write Script:** Create your script in the Admin UI with the `graphql` trigger type (or just select it in the dropdown).
2.  **Save:** Saving the script persists it to the database.
3.  **Reload:** For the GraphQL Schema to pick up the new definition, you must trigger a **System Reload**.
    *   *Via UI:* Click "Restart App" in the top bar.
    *   *Via API:* `POST /api/v1/admin/system/reload`
4.  **Query:** Go to the GraphQL Playground and test your new field.

## 5. Limitations

*   **Recursion:** Be careful attaching a computed field to a Collection that queries *that same collection*, as it can cause infinite loops if logic isn't careful.
*   **Performance:** Scripts run in the Boa engine. While fast, heavy computation in a resolver (especially one attached to a List type like `User.fullName` running on 50 users) can impact query latency. Use `JSON` return types to fetch bulk data at the root `Query` level when possible.