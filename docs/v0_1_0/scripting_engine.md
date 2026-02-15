# ⚙️ Scripting Engine Documentation

**Version:** 0.1.0
**Runtime:** Boa JS (Rust-integrated)
**Execution Model:** Sandboxed, Async-first

The ApexKit Scripting Engine allows you to write server-side JavaScript to extend your application logic. It runs within a secure, isolated environment inside the Rust process, providing "Edge Function" performance with direct access to the database and system tools.

---

## 1. Script Structure

Every script must **default export an async function**. This function receives a `Request` object and must return a `Response` object.

```javascript
export default async function(req) {
    // 1. Parse input
    const body = await req.json();
    
    // 2. Perform logic
    log(`Hello request from ${body.name || 'Stranger'}`);

    // 3. Return response
    return new Response({ 
        message: "Action successful",
        timestamp: new Date().toISOString()
    }, { status: 200 });
}
```

---

## 2. Global API Reference

### `$db.records` (Database Access)
Provides scope-aware access to your collections. Operations automatically target the current Tenant or Sandbox.

| Method | Description |
| :--- | :--- |
| `find(col, filter)` | Returns an array of records matching the filter. |
| `find_one(col, id)`| Returns a single record or null. |
| `insert(col, data)` | Creates a record and returns the new ID. |
| `update(col, id, data)`| Performs a partial update. |
| `delete(col, id)` | Removes a record. |
| `query(null, queryObj)`| Executes the **Analytical Engine** for aggregations/grouping. |

### `$run` (Workflow Orchestration)
Allows calling other scripts. This is the primary way to reuse logic.

*   **`$run.script(name, payload)`**: Calls a script. If the script isn't found in the local scope, it looks for **Public** scripts in the **Root** scope (enabling shared system tools).

### `$zip` (File & Archive Manager)
A powerful tool for in-memory file manipulation and scoped storage access.

*   **`create({ path: data })`**: Creates a Base64 ZIP string from an object.
*   **`extract(base64)`**: Extracts a ZIP string into an object.
*   **`inspect(base64)`**: Returns archive metadata (sizes, file list, compression ratios).
*   **`readFile(filename)`**: Reads a file from the current scope's storage into Base64.
*   **`saveFile(filename, b64, mime)`**: Saves data to storage and registers it in the system file table.

### `$cmd` (Shell Execution)
**Security Note**: Only available to scripts running in the **Root App** scope.

*   **`run(cmd, args, options)`**: Executes a command and waits for output. Returns `{ stdout, stderr, status }`.
*   **`spawn(cmd, args, options)`**: Spawns a background process. Returns `{ pid }`.

### `$cache` (Ephemeral Store)
Scoped key-value storage for rate-limiting, session state, or temporary tokens.

*   **`get(key)`**, **`set(key, val, ttl?)`**, **`delete(key)`**
*   **`incr(key, delta)`**: Atomic increment. Essential for quotas.

### `$http` & `fetch`
Consolidated HTTP logic for external API calls.

*   **`fetch(url, options)`**: Standard Web API implementation (supports `redirect: "manual"`).
*   **`$http.get(url)`** / **`$http.post(url, body)`**: Legacy wrappers returning raw strings.

### `$util`
*   `uuid()`: Generates v4 UUID.
*   `slugify(text)`: URL-friendly conversion.
*   `hash(text, 'sha256'|'sha512')`: Secure hashing.
*   `base64Encode(text)` / `base64Decode(b64)`

---

## 3. Scoping & Visibility

### Context Awareness
Scripts are automatically "anchored" to the scope they are called from. When a script calls `$db.find()`, it only sees data from the current Tenant.

### Shared Scripts
Root Admins can create scripts with **Public Visibility**. These scripts act as "System Functions" that Tenants can invoke to perform heavy tasks (like FFmpeg processing) that require `$cmd` access, which Tenants lack for security reasons.

---

## 4. Examples

### Complex Analytics (Analytical Engine)
```javascript
export default async function(req) {
    const report = await $db.query(null, {
        "from": "sales",
        "select": [
            "category",
            { "fn": "sum", "field": "amount", "as": "revenue" }
        ],
        "group_by": ["category"]
    });
    return new Response(report);
}
```

### File Backup to ZIP
```javascript
export default async function(req) {
    const logo = await $zip.readFile("logo.png");
    const data = await $db.find("settings", {});

    const archive = await $zip.create({
        "assets/logo.png": logo,
        "backup_data.json": JSON.stringify(data)
    });

    const file = await $zip.saveFile("backup.zip", archive);
    return new Response({ download_url: file.url });
}
```

### Executing a Shell Command (Root Only)
```javascript
export default async function(req) {
    const res = await $cmd.run("ls", ["-lh", "storage/system"]);
    return new Response(res.stdout);
}
```

---

## 5. Limits & Constraints

*   **Memory**: Each script execution is limited by the global memory quota.
*   **Execution Time**: Default timeout is 30 seconds.
*   **ZIP Size**: Limited to 10MB (configurable via `ARCHIVE_LIMIT`).
*   **Safety**: Scripts cannot access the raw Node.js environment or standard Rust filesystem directly (outside of `$fs` and `$zip` helpers).