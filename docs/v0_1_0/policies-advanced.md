# ApexKit Security Policies: The Definitive Developer Guide

Welcome to the definitive guide on ApexKit API Rules and Security Policies. Whether you are building a simple portfolio or a complex multi-tenant enterprise system, understanding how to secure your collections at the database level is your most critical line of defense.

In ApexKit, every collection has four distinct API rules: **Read**, **Create**, **Update**, and **Delete**. 

By default, ApexKit supports a simple, string-based syntax for quick setups (Legacy Mode), as well as an expressive, MongoDB-style JSON syntax for advanced, deep-relational security (Advanced Mode).

---

## Part 1: The Basics (Legacy String Policies)

String-based policies are the fastest way to secure your application. They use simple keywords and logical operators.

### Built-in Keywords
*   `public` : Open to the world. Anyone, even unauthenticated users, can access this.
*   `auth` : Only authenticated (logged-in) users can access this.
*   `admin` : Only users with the exact system role of `"admin"` can access this.

### The Owner Shortcut
Often, you want to restrict access to the person who created the record.
*   `owner:field_name` : Checks if the ID of the logged-in user matches the value stored in `field_name` on the record.
    * *Example:* `owner:user_id` ensures only the creator can edit or delete their post.

### Combining Rules (Logical Operators)
You can combine these keywords using `&&` (AND) and `||` (OR).

**Examples:**
*   **Public Blog Posts (Read):** `public`
*   **Create a Post (Create):** `auth` *(Any logged-in user can create)*
*   **Edit a Post (Update):** `admin || owner:author_id` *(Only an admin OR the original author can edit it)*

---

## Part 2: Advanced JSON-Based Policies

While string policies are great, they fall short when you need to validate complex data structures, inspect incoming request payloads, or do cross-table relational lookups.

To solve this, ApexKit supports **JSON-Based Policies**. Instead of writing a string, you write a JSON object representing a logic tree.

### 1. The Structure
JSON policies use a nested, document-oriented structure. To check if a field named `status` equals `published`, you write:
```json
{ "status": "published" }
```

### 2. Operators
You can use operators by nesting them with a `$` prefixed key inside an object:
*   **`$eq`** / **`$neq`**: Equals / Not Equals
*   **`$gt`** / **`$gte`**: Greater Than / Greater or Equal
*   **`$lt`** / **`$lte`**: Less Than / Less or Equal
*   **`$in`** / **`$nin`**: In Array / Not In Array
*   **`$contains`** / **`$like`**: Text matching (contains or wildcard like)

**Example:** *Status is active AND price is greater than 50.*
```json
{
  "status": "active",
  "price": { "$gt": 50 }
}
```

### 3. Logical Grouping (`$and`, `$or`)
To evaluate multiple complex conditions, group them in arrays:
```json
{
  "$or": [
    { "role": "admin" },
    { "$and": [
        { "department": "sales" },
        { "budget": { "$lt": 1000 } }
    ]}
  ]
}
```

---

## Part 3: Dynamic Context Variables (The `@` Syntax)

The true power of JSON policies comes from Context Variables. By prefixing a string with `@`, you tell ApexKit to dynamically inject live data from the current HTTP Request or Database state.

### 1. User Context (`@request.auth`)
Access the currently authenticated user making the request.
*   `@request.auth.id` : The User's ID.
*   `@request.auth.email` : The User's Email.
*   `@request.auth.role` : The User's Role.

### 2. Incoming Payload Context (`@request.record`)
*Crucial for `Create` and `Update` rules.*
This represents the data the client (frontend) is trying to send to the server.
*   `@request.record.data.status` : The new status the user is trying to save.
*   `@request.record.data.price` : The new price the user is trying to set.

### 3. Existing Database Context (`@record`)
*Crucial for `Read`, `Update`, and `Delete` rules.*
This represents the data that *already exists* safely in the database table.
*   `@record.id` : The database ID of the record.
*   `@record.data.author_id` : The original author's ID currently saved in the DB.

### ⚠️ Important Distinction for Updates
When writing an `Update` policy, understanding the difference between `@request` and `@record` is how you build unbreakable security.

*   `@record.data.status` = What the status is right now.
*   `@request.record.data.status` = What the user is trying to change the status to.

---

## Part 4: Relational Lookups with `@get()`

Sometimes, the data you need to verify isn't on the user or the record—it's in an entirely different table.

The `@get()` block allows your policy to execute an isolated, in-memory SQL query using the ApexKit Query Engine to fetch external data to compare against.

### The Clean Object Syntax
To make writing policies as clean as possible, `@get()` is written as a native JSON key pointing to an object containing your query variables.

**Syntax:**
```json
{
  "field_name": {
    "$in": {
      "@get()": {
        "from": "collection_to_query",
        "select": ["field_to_extract"],
        "where": {
          "field": "value"
        }
      }
    }
  }
}
```

Because `@get()` is processed recursively, **you can use other `@` variables inside the `@get()` query itself**! ApexKit will resolve the inner variables first, execute the sub-query, and swap the `@get()` block with the resulting flat array.

---

## Part 5: Admin UI Configuration

In the ApexKit Admin dashboard, you can write policies directly inside the **User Data Policies** card (in Settings) or the **API Rules** sidebar (in Collections).

Every input has a toggler in the top right to switch modes:
1.  **Legacy:** Provides a simple input line for writing quick, string-based expressions (`public`, `auth`, `admin || owner:id`).
2.  **JSON (Advanced):** Provides a fully formatted, color-coded JSON Editor with syntax validation. If your JSON is malformed, the UI will warn you and prevent you from saving corrupt settings.

---

## Part 6: Practical Recipes & Real-World Examples

Here are common security patterns you can copy and paste directly into your projects.

### Recipe 1: The "Immutable Field" (Preventing unauthorized edits)
**Goal:** Users can update their own posts, but they are absolutely NOT allowed to change the `is_verified` boolean field. Only admins can change that.

*   **Collection:** `posts`
*   **Rule:** `Update`
```json
{
  "$or": [
    { "@request.auth.role": "admin" },
    {
      "$and": [
        { "author_id": "@request.auth.id" },
        { "@request.record.data.is_verified": "@record.data.is_verified" }
      ]
    }
  ]
}
```
*How it works: If the user is not an admin, they must be the author, AND the `is_verified` value they are sending (`@request.record.data.is_verified`) must match what is already stored in the database (`@record.data.is_verified`).*

---

### Recipe 2: Workspace & Organization Membership
**Goal:** You have a `documents` collection and a `workspace_members` collection. A user can only READ a document if they are a registered member of the workspace that owns that document.

*   **Collection:** `documents` (Fields: `title`, `workspace_id`)
*   **Rule:** `Read`
```json
{
  "$or": [
    { "@request.auth.role": "admin" },
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
  ]
}
```
*How it works: It looks at the document's `workspace_id`. It then queries the `workspace_members` table to find all `workspace_id`s where the `user_id` matches the logged-in user. If the document's workspace is in that returned array, access is granted.*

---

### Recipe 3: Enforcing Public Registration Constraints
**Goal:** Users can register themselves, but they are forbidden from assigning themselves the `"admin"` role, nor can they assign themselves any system role prefixed with an underscore `_`.

*   **Collection:** `users`
*   **Rule:** `Create`
```json
{
  "$and": [
    { "@request.record.data.role": { "$neq": "admin" } },
    { "@request.record.data.role": { "$like": "^[^_].*$" } }
  ]
}
```
*How it works: The database verifies the incoming payload. If the requested role is "admin" or begins with an underscore, the creation is rejected.*

---

### Recipe 4: Drafts vs. Published Visibility
**Goal:** Draft posts can only be read by their authors. Published posts can be read by anyone.

*   **Collection:** `posts`
*   **Rule:** `Read`
```json
{
  "$or": [
    { "status": "published" },
    { "author_id": "@request.auth.id" }
  ]
}
```
*How it works: If the record has `status` set to "published", it's public. Otherwise, it is only returned if the reader is the original author.*