# 🤖 AI Actions Documentation

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

ApexKit **AI Actions** turn Generative AI prompt templates into secure, reusable API endpoints. They act as a backend proxy between your client applications and LLM providers (Google Gemini, Groq, and OpenAI), handling encrypted credential storage, variable interpolation, multimodal vision inputs, RAG pipeline hooks, and real-time Server-Sent Events (SSE) streaming.

---

## 1. Key Architectural Capabilities

1. **Zero Client Secret Leakage:** Third-party provider API keys are encrypted at rest with AES-256-GCM via the Master Vault and never exposed to the frontend.
2. **Multi-Provider Support:** Run models across **Google Gemini**, **Groq**, or **OpenAI** with a unified API interface.
3. **Automatic Multimodality:** Pass Base64 data URIs (`data:image/jpeg;base64,...`) directly in the variables payload. ApexKit automatically detects and extracts the binary payload for vision models.
4. **Pre-Run Filter Hooks (RAG):** The `before_ai_run` filter hook lets you intercept prompts, execute vector or database lookups, and inject context into the prompt variables before the model executes.
5. **Real-Time Token Streaming (SSE):** Enable streaming to deliver chunks to the client over Server-Sent Events as tokens are generated.
6. **Grounding & Web Citations:** Enable Google Search grounding or URL context to retrieve live web sources and citations in the response metadata.

---

## 2. Defining an Action

AI Actions can be defined via the **Admin Dashboard > AI Actions** or created programmatically using the SDK.

### Action Configuration Schema

| Field | Type | Description | Example |
| :--- | :--- | :--- | :--- |
| **`name`** | `string` | Descriptive label. | `Content Editor` |
| **`slug`** | `string` | Unique URL route segment. | `content-editor` |
| **`model`** | `string` | Model identifier. | `gemini-2.5-flash`, `llama-3.3-70b-versatile`, `gpt-4o-mini` |
| **`system_prompt`** | `string?` | Optional system instruction/persona. | `You are an expert copywriter. Respond in Markdown.` |
| **`template`** | `string` | Prompt template containing `{{variables}}`. | `Topic: {{topic}}\n\nOriginal Text:\n{{text}}` |
| **`config`** | `object?` | Provider and execution parameters (see below). | `{ "provider": "gemini", "streaming": true }` |

### Provider Configuration Options (`config`)

```json
{
  "provider": "gemini",       // "gemini" | "groq" | "openai"
  "streaming": true,          // Enable SSE stream responses
  "grounding": true,          // (Gemini only) Enable Google Search grounding
  "url_context": false,       // (Gemini only) Enable URL fetching
  "temperature": 0.7,         // Sampling temperature (0.0 to 2.0)
  "max_tokens": 2048,         // Maximum completion tokens
  "top_p": 0.95               // Nucleus sampling probability
}
```

---

## 3. Template Variables & Dynamic Context

Variables are defined inside the template using double curly braces: `{{variable_name}}`.

```text
Summarize the following document for a {{audience}} audience:

{{document_text}}
```

### Automatic JSON & Array Stringification
When passing complex structures (such as arrays of records from a vector search or nested objects), ApexKit automatically serializes them into valid JSON strings so they cleanly interpolate into your prompt:

```json
{
  "variables": {
    "audience": "technical",
    "document_text": {
      "title": "Database Optimization",
      "tags": ["sqlite", "performance"],
      "metrics": { "latency_ms": 1.2 }
    }
  }
}
```

---

## 4. Multimodal Vision (Image Analysis)

For vision-capable models (such as `gemini-2.5-flash` or `gemini-2.5-flash-image`), pass any Base64 Data URI inside your `variables` map. 

ApexKit automatically strips the data URI prefix, detects the MIME type, and attaches it as an inline media part in the inference request:

```json
{
  "variables": {
    "prompt": "Analyze this invoice and extract the total amount and line items.",
    "invoice_image": "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEASABIAAD..."
  }
}
```

> **Note:** Image binary strings are automatically excluded from textual `{{variable}}` substitution in the template body to prevent multi-megabyte Base64 strings from polluting the prompt text.

---

## 5. Pre-Run Hook: RAG Context Injection

You can attach a `before_ai_run` filter hook to automatically inject retrieved knowledge from your vector database into the action's variables before execution.

```typescript
// Script: auto-rag-injector
// Trigger: before_ai_run | Path: ./webhooks/auto-rag-injector.ts

export default async function (event) {
    const { slug, vars } = event.data;

    // Only inject RAG context for the 'support-bot' action
    if (slug === "support-bot" && vars.user_query) {
        // 1. Generate query embedding using local vector model
        const queryVector = await $ai.embed(vars.user_query);

        // 2. Search knowledge base collection
        const matches = await $db.records.searchVector("knowledge_base", "content", queryVector, 3);
        
        const contextArticles = matches.map(m => m.data.body).join("\n\n---\n\n");

        // 3. Inject into the 'context' variable expected by the template
        vars.context = contextArticles;
    }

    return { slug, vars };
}
```

---

## 6. API Reference

### Run an Action
`POST /api/v1/ai/run/{slug}`

#### Headers
```http
Authorization: Bearer <JWT_TOKEN>
# OR
x-api-key: <API_KEY>
Content-Type: application/json
```

#### Request Body
```json
{
  "variables": {
    "prompt": "Explain the advantages of Single-Node BaaS architectures.",
    "format": "bullet points"
  }
}
```

#### Standard JSON Response (`streaming: false`)
```json
{
  "result": "Single-node architectures provide:\n- Zero network latency between database and application logic\n- Simplified operational and backup workflows\n- Reduced hosting costs...",
  "metadata": {
    "groundingChunks": [
      { "web": { "title": "ApexKit Architecture Overview", "uri": "https://apexkit.dev/docs" } }
    ],
    "webSearchQueries": ["Single-Node BaaS benefits"]
  }
}
```

#### Streaming Response (`streaming: true`)
When streaming is enabled, the endpoint returns a `text/event-stream` (SSE) connection:
```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache

data: Single
data: -node
data:  architectures
data:  provide...
data: [DONE]
```

---

## 7. SDK Integration

### A. TypeScript / JavaScript SDK (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('YOUR_TOKEN_OR_API_KEY');

// 1. Standard Execution
const response = await apex.ai.run('content-editor', {
  prompt: 'Improve readability and fix grammatical errors.',
  originalText: 'Their is several issues here.'
});
console.log(response.result);

// 2. Real-Time Token Streaming (Pass onChunk callback)
const streamed = await apex.ai.run(
  'support-bot',
  { user_query: 'How do I configure S3 backups?' },
  (chunk) => {
    process.stdout.write(chunk); // Received chunk by chunk via SSE
  }
);
```

### B. In-Browser Client (`apex.js`)

```javascript
// Stream directly to a DOM element in your frontend
const outputEl = document.getElementById('chat-response');

await $apex.ai.run(
  'code-explainer',
  { code: 'const sum = (a, b) => a + b;' },
  (token) => {
    outputEl.innerText += token;
  }
);
```

---

## 8. Supported Providers & Models

| Provider | Recommended Models | Capabilities |
| :--- | :--- | :--- |
| **`gemini`** | `gemini-2.5-flash`<br>`gemini-2.5-flash-lite`<br>`gemini-2.5-flash-image` | High speed, large context windows, Vision attachments, Google Search grounding, URL context. |
| **`groq`** | `llama-3.3-70b-versatile`<br>`mixtral-8x7b-32768`<br>`gemma2-9b-it` | Ultra-low latency inference, high token throughput. |
| **`openai`** | `gpt-4o`<br>`gpt-4o-mini` | Complex reasoning, structured outputs, code generation. |

> **Setup Reminder**: Make sure your provider API key is saved and active under **Admin Dashboard > Settings > AI**. ApexKit encrypts keys using your `APEXKIT_MASTER_KEY` secret before writing them to the database.
