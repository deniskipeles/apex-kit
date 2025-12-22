# ApexKit Scripting Engine Documentation

The ApexKit Scripting Engine allows you to write server-side JavaScript to extend your application logic. It runs in a secure, isolated Rust environment using the **Boa** engine.

The architecture mimics "Edge Functions" (like Cloudflare Workers or Deno), using standard Web APIs like `Request` and `Response`.

---

## 1. The Basics

Every script must **default export an async function**. This function receives a `Request` object and must return a `Response` object.

### Minimal Example
```javascript
export default async function(req) {
    // 1. Parse input
    const body = await req.json();
    
    // 2. Do logic
    const name = body.name || "World";
    log("Processing request for: " + name);

    // 3. Return response
    return new Response({ 
        message: "Hello " + name 
    }, { status: 200 });
}
```

---

## 2. Global APIs

The following objects are available globally in the script context.

### `$db` (Database Access)
Direct access to your ApexKit collections. All methods return **Promises**.

| Method | Description | Example |
| :--- | :--- | :--- |
| `find_one(collection, id)` | Fetch a single record by ID. Returns `Object` or `null`. | `await $db.find_one("users", 1)` |
| `find(collection, filter)` | Search records. `filter` is an optional object. Returns `Array`. | `await $db.find("users", { role: "admin" })` |
| `insert(collection, data)` | Create a new record. Returns the new `ID`. | `await $db.insert("logs", { msg: "Hi" })` |
| `update(collection, id, data)`| Update an existing record. Returns the updated object. | `await $db.update("users", 1, { active: true })` |
| `delete(collection, id)` | Delete a record. Returns `true` if successful. | `await $db.delete("users", 1)` |

### `$http` (External Requests)
Make HTTP requests to third-party APIs.

| Method | Description | Example |
| :--- | :--- | :--- |
| `get(url)` | Performs a GET request. Returns **String** (raw body). | `await $http.get("https://api.com")` |
| `post(url, body)` | Performs a POST request. `body` is a JS Object. Returns **String**. | `await $http.post("https://api.com", {a:1})` |

> **Note:** `$http` returns the raw body string. You usually need to use `JSON.parse()` on the result.

### `$util` (Utilities)
Helper functions.

*   `$util.uuid()`: Generates a v4 UUID string.

### `log(message)`
Prints a message to the server console (stdout).

---

## 3. The `Request` Object
The `req` argument passed to your function has the following methods:

*   `await req.json()`: Parses the request body as JSON.
*   `await req.text()`: Returns the request body as a string.
*   `req.method`: The HTTP method (e.g., "POST").
*   `req.headers`: A Headers object.

## 4. The `Response` Object
You must return a `new Response(body, init)`.

*   **body**: Can be a JSON Object or a String.
*   **init** (optional): `{ status: number, headers: object }`.

---

## 5. Common Patterns & Examples

### Example A: Toggle a Boolean in Database
This script reads a record, flips a boolean flag, and saves it.

```javascript
export default async function(req) {
    const { id } = await req.json();

    // 1. Fetch current state
    const todo = await $db.find_one("todos", id);

    if (!todo) {
        return new Response({ error: "Todo not found" }, { status: 404 });
    }

    // 2. Update logic
    const updated = await $db.update("todos", id, { 
        done: !todo.done 
    });

    return new Response({ success: true, data: updated });
}
```

### Example B: Call External Webhook on Insert
Create a user locally, then notify Slack/Discord.

```javascript
export default async function(req) {
    const input = await req.json();

    // 1. Insert into local DB
    const newId = await $db.insert("users", {
        email: input.email,
        created_at: new Date().toISOString()
    });

    // 2. Notify external API
    const webhookUrl = "https://hooks.slack.com/services/XYZ/ABC";
    const payload = { text: "New user registered: " + input.email };
    
    // Note: $http returns a string, we don't need the result here
    await $http.post(webhookUrl, payload);

    return new Response({ id: newId }, { status: 201 });
}
```

### Example C: Secure Proxy (Auth + CORS)
A robust example handling CORS preflight and Authorization headers.

```javascript
export default async function(req) {
  // 1. Handle CORS Preflight
  if (req.method === "OPTIONS") {
    return new Response(null, {
      headers: {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "GET, POST",
        "Access-Control-Allow-Headers": "*"
      },
      status: 204
    });
  }

  // 2. Check Auth Header
  const auth = req.headers.get("Authorization");
  if (!auth || auth !== "Bearer my-secret-token") {
      return new Response({ error: "Unauthorized" }, { status: 401 });
  }

  try {
    const body = await req.json();
    
    return new Response({ 
        message: "Secure data accessed", 
        user: body.username 
    }, {
      headers: { "Access-Control-Allow-Origin": "*" }
    });
  } catch (e) {
    return new Response({ error: e.toString() }, { status: 500 });
  }
}
```

## 6. How to Invoke
Scripts are exposed via the API.

**Endpoint:**
`POST /api/v1/run/{script_name}`

**Payload:**
The JSON body you send to this endpoint becomes `req.json()` inside the script.

**Bash Example:**
```bash
curl -X POST http://localhost:5000/api/v1/run/my-script \
  -H "Content-Type: application/json" \
  -d '{"id": 123, "action": "toggle"}'
```
