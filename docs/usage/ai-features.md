# AI Features

ApexKit is built from the ground up to be **AI-Native**. It integrates machine learning capabilities directly into the core engine.

## 1. AI Architect

The AI Architect is your co-pilot for building backends. Accessible from the dashboard, it can:
- **Design Schemas**: Describe your app, and it will create the collections and fields.
- **Write Scripts**: Ask it to write a specific Edge Function.
- **Explain Logic**: Highlight a policy and ask "How does this work?".

## 2. Vector Semantic Search

Standard search looks for keywords. **Vector search** looks for meaning.

### How it works:
1. Enable `Vectorize` on a text field in your collection.
2. ApexKit automatically generates high-dimensional embeddings using local models (e.g., `BGE`).
3. Use the SDK to find relevant records.

```javascript
// Search for "healthy recipes" even if the text says "nutritious meals"
const results = await apex.collection('recipes').searchTextVector("healthy recipes", 10);
```

## 3. AI Actions (Prompts)

AI Actions are server-side prompt templates that you can expose as API endpoints.

### Defining an Action:
- **Slug**: `summarize-article`
- **System Prompt**: "You are a helpful assistant that summarizes long text."
- **Prompt Template**: "Please summarize this: {{input}}"

### Executing an Action:
```javascript
const res = await apex.ai.run('summarize-article', {
    input: "A very long text about technology..."
});
console.log(res.result);
```

## 4. Local Embeddings & Models

ApexKit uses the **Candle** library to run inference locally whenever possible. This means:
- **Privacy**: Your data doesn't always have to leave your server.
- **Speed**: No network latency for embedding generation.
- **Cost**: No per-token pricing for vector generation.

## 5. Instant AI Search

ApexKit combines **Tantivy** (keyword search) with **Vector search** to provide "Hybrid Search" results that are both accurate and fast.

### Instant Search Example (WebSocket):
```javascript
const results = await realtime.search('products', 'red sneakers', 5);
```
This returns results in milliseconds as the user types, using the combined power of full-text and semantic indexing.
