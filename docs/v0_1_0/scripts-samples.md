Here are several sample JavaScript scripts you can run in ApexKit `ScriptEngine`.

These are divided into **Manual Endpoints** (executed via `POST /run/{name}`) and **Event Hooks** (executed automatically on DB actions).

---

### 📚 Category 1: Manual API Endpoints
*Create these with Trigger Type: `manual`*

#### 1. "Hello World" & Input Echo
A simple script to test input parsing and response formatting.

```javascript
export default async function(req) {
    // 1. Parse JSON body from the request
    const body = await req.json();
    const name = body.name || "Stranger";

    log("Received hello request for: " + name);

    // 2. Return a standard Response object
    return new Response({
        message: `Hello, ${name}!`,
        timestamp: new Date().toISOString(),
        received_data: body
    }, { status: 200 });
}
```

#### 2. External API Integration (Crypto Price Fetcher)
Demonstrates using `$http` to call 3rd party APIs and merging that data.

```javascript
export default async function(req) {
    const body = await req.json();
    const coin = body.coin || "bitcoin";

    // 1. Call external API (Coingecko)
    // Note: $http.get returns a raw string
    const responseStr = await $http.get(`https://api.coingecko.com/api/v3/simple/price?ids=${coin}&vs_currencies=usd`);
    const data = JSON.parse(responseStr);

    if (!data[coin]) {
        return new Response({ error: "Coin not found" }, { status: 404 });
    }

    const price = data[coin].usd;

    // 2. Log to server console
    log(`Fetched price for ${coin}: $${price}`);

    // 3. Return combined result
    return new Response({
        asset: coin,
        price_usd: price,
        formatted: `$${price.toFixed(2)}`
    });
}
```

#### 3. Custom Dashboard Aggregation
Fetching data from multiple collections to create a custom stats endpoint (avoids making multiple calls from frontend).

```javascript
export default async function(req) {
    // 1. Fetch data from multiple collections
    // (Assuming collections 'users' and 'orders' exist)
    const users = await $db.find("users", {}); // Empty filter = all
    const orders = await $db.find("orders", { status: "pending" });

    // 2. Perform logic (Aggregation)
    const totalUsers = users.length;
    const pendingOrders = orders.length;
    
    // Calculate total value of pending orders
    let totalValue = 0;
    for (let i = 0; i < orders.length; i++) {
        totalValue += (orders[i].total_amount || 0);
    }

    return new Response({
        stats: {
            user_count: totalUsers,
            pending_order_count: pendingOrders,
            pipeline_value: totalValue
        }
    });
}
```

---

### 🪝 Category 2: Database Event Hooks
*Create these with Trigger Type: `before_create`, `after_create`, etc.*

#### 4. Slug Generator & Validation (`before_create`)
Automatically generates a URL-friendly slug from a title and enforces validation.

```javascript
export default async function(e) {
    // 'e' contains: e.data, e.collection, e.auth

    // 1. Validation
    if (!e.data.title) {
        throw new Error("Title is required for this collection.");
    }

    // 2. Logic: Generate Slug if missing
    if (!e.data.slug) {
        e.data.slug = e.data.title
            .toLowerCase()
            .replace(/ /g, '-')
            .replace(/[^\w-]+/g, '');
        
        // Append random string to ensure uniqueness
        e.data.slug += "-" + $util.uuid().split('-')[0];
    }

    // 3. Force default fields
    e.data.view_count = 0;
    e.data.is_published = false;

    // 4. Return modified data to be saved
    return e.data; 
}
```

#### 5. Slack/Discord Notification (`after_create`)
Sends a notification to a chat channel when a new record is created.

```javascript
export default async function(e) {
    // We don't return data in 'after' hooks, just perform side effects
    
    const webhookUrl = "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_TOKEN";
    
    const message = {
        content: `🆕 **New Item Created!**\nCollection: ${e.collection}\nID: ${e.record.id}\nTitle: ${e.record.data.title}`
    };

    // Fire and forget
    await $http.post(webhookUrl, message);
    
    log("Notification sent for record " + e.record.id);
}
```

#### 6. Immutable Fields Check (`before_update`)
Prevents users from changing sensitive fields (like `role` or `subscription_status`) via the API.

```javascript
export default async function(e) {
    // 1. Fetch the EXISTING record from DB to compare
    const oldRecord = await $db.find_one(e.collection, e.record.id);

    // 2. Check if restricted fields are being changed
    if (e.data.role !== oldRecord.role) {
        // Only allow Admins to change role
        if (!e.auth || e.auth.role !== 'admin') {
            throw new Error("You are not authorized to change the User Role.");
        }
    }

    // 3. Return the data (allowed)
    return e.data;
}
```

---

### 🤖 Category 3: AI & Vectors
*Using the built-in `$ai` and `$db` features.*

#### 7. Semantic Search Endpoint
A custom endpoint that takes a query, converts it to a vector, and searches the DB.

```javascript
// Trigger: manual
// Name: semantic-search
export default async function(req) {
    const body = await req.json();
    const query = body.q;
    const collectionId = 1; // ID of your 'posts' collection
    const fieldName = "description";

    if (!query) return new Response({ error: "Query 'q' required" }, { status: 400 });

    // 1. Generate Embedding for the query string
    // Provider can be "local", "openai", "gemini", etc. based on config
    const vector = await $ai.embed(query, "gemini");

    // 2. Perform Vector Search via $db (This connects to HNSW index)
    // Note: You currently need to expose `search_vector` to $db in scripting.rs 
    // or use the HTTP API internally if $db doesn't expose it directly yet.
    
    // Assuming we added $db.search_vector(col_id, field, vector, limit)
    // If not, we can fetch all and compute cosine similarity (slow but works for small datasets)
    
    // Mocking return for demonstration if specific $db function is missing in current lib.rs:
    return new Response({
        message: "Vector generated",
        vector_sample: vector.slice(0, 5),
        note: "Implement $db.search_vector binding in Rust to finish this."
    });
}
```

### 🧹 Category 4: Cron Job
*Trigger Type: `cron`*

#### 8. Data Cleanup / Archiving
Runs automatically to archive old records.

```javascript
export default async function() {
    log("Running nightly cleanup...");

    // 1. Find old logs (simulated logic)
    // In a real scenario, you'd filter by date, but JSON filtering for dates is string based
    const logs = await $db.find("audit_logs", {});

    let deletedCount = 0;
    
    // 2. Iterate and Delete
    for (let i = 0; i < logs.length; i++) {
        const item = logs[i];
        // If older than 30 days (logic simplified)
        // ... date comparison logic ...
        
        // await $db.delete("audit_logs", item.id);
        // deletedCount++;
    }

    log(`Cleanup finished. Deleted ${deletedCount} records.`);
    return new Response({ success: true });
}
```