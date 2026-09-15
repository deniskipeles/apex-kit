# 🚀 ApexKit Developer Quick Reference

**Version:** 0.1.0  
**Base URL:** `http://localhost:5000/api/v1`  
**SDK Package:** `@apexkit/sdk`

ApexKit is an all-in-one Backend-as-a-Service delivering type-safe REST APIs, automated GraphQL schemas, Tantivy full-text search, local and ONNX vector search, real-time WebSocket subscriptions, and a sandboxed QuickJS/TypeScript scripting runtime in a single binary.

---

## 1. SDK Initialization

Install the official SDK:

```bash
npm install @apexkit/sdk
```

Initialize the client and configure authentication:

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');

// Authenticate via JWT or set an API Key
apex.setToken('YOUR_JWT_TOKEN');
// Or set custom headers: apex.setHeader('x-api-key', 'YOUR_API_KEY');
```

---

## 2. Multi-Tenant Scoping

ApexKit provides physical database isolation per tenant and sandbox. Switch execution contexts on the client:

```typescript
// Target a customer tenant database
const tenant = apex.tenant('client-alpha');

// Target an ephemeral sandbox session
const sandbox = apex.sandbox('session-uuid-123');

// Calls executed on scoped instances target isolated SQLite databases
const tenantPosts = await tenant.collection('posts').list();
```

---

## 3. Authentication

```typescript
// 1. Password Login
const auth = await apex.auth.login('user@example.com', 'password123');
console.log(`Logged in: ${auth.user.email} (Scope: ${auth.user.scope})`);

// 2. Registration
const newUser = await apex.auth.register('new@example.com', 'password123', {
  displayName: 'Alex'
});

// 3. Current User Profile
const me = await apex.auth.getMe();

// 4. Update Profile Metadata
await apex.auth.updateMeMetadata({ theme: 'dark' });

// 5. Social OAuth Redirects (Browser)
apex.auth.loginWithGithub('https://your-app.com/callback');
apex.auth.loginWithGoogle('https://your-app.com/callback');

// 6. Password Reset Flow
await apex.auth.requestPasswordReset('user@example.com');
await apex.auth.confirmPasswordReset('RESET_TOKEN', 'newPassword123');
```

---

## 4. Collections & Records

### Listing Records (with Filters, Sorting & Joins)
```typescript
const result = await apex.collection('posts').list({
  page: 1,
  per_page: 20,
  sort: '-created', // Newest first
  filter: {
    status: 'published',
    category: { $in: ['tech', 'news'] }
  },
  expand: 'author_id,comments(5,0).user_id' // Multi-level relation joins
});

console.log(`Loaded ${result.items.length} of ${result.total} records.`);
```

### CRUD Operations
```typescript
// Create
const post = await apex.collection('posts').create({
  title: 'Getting Started with ApexKit',
  content: 'Single-binary BaaS architecture overview.',
  status: 'published'
});

// Partial Update (PATCH)
await apex.collection('posts').patch(post.id, {
  views: 42
});

// Full Replace (PUT)
await apex.collection('posts').update(post.id, {
  title: 'Updated Title',
  content: 'Updated Content...',
  status: 'draft'
});

// Get Single Record with Expansion
const single = await apex.collection('posts').get(post.id, {
  expand: 'author_id'
});

// Delete
await apex.collection('posts').delete(post.id);
```

---

## 5. Search Engines

### A. Instant Autocomplete Search (Tantivy / OSE)
Fuzzy, typo-tolerant full-text search (requires `ose_indexed: true` in schema):

```typescript
const hits = await apex.collection('products').searchRecordsInstantlyWithOSE('iphne');
// Returns: [{ id: 101, score: 2.4, snippet: { name: 'iPhone 15' } }]
```

### B. Full-Text Search with Pagination
```typescript
const searchResults = await apex.collection('articles').searchRecordsWithOSE('database indexing', {
  page: 1,
  per_page: 10
});
```

### C. Multimodal Vector Similarity Search
Semantic search across high-dimensional embeddings:

```typescript
// Text-to-Text Semantic Search
const similarArticles = await apex.collection('articles').searchVectorWithText('machine learning systems', {
  per_page: 5
});

// Text-to-Image Cross-Modal Search
const matchingImages = await apex.collection('gallery').searchImageVectorWithText('red sports car', 10);

// Image-to-Image Search (Base64)
const similarImages = await apex.collection('gallery').searchImageVectorWithImage('data:image/jpeg;base64,...', 5);
```

---

## 6. Analytical SQL Query Engine

Execute multi-column aggregations directly against the database:

```typescript
const stats = await apex.collection('orders').searchRecordsWithSQLQueryEngine({
  select: [
    'category',
    { fn: 'sum', field: 'total_amount', as: 'revenue' },
    { fn: 'count', field: 'id', as: 'order_count' }
  ],
  where: { status: 'completed' },
  group_by: ['category'],
  sort: '-revenue'
});
```

---

## 7. Webhooks & Serverless Scripts

Invoke server-side JavaScript / TypeScript endpoints:

```typescript
// 1. Calling via Webhook Client (Supports all HTTP verbs)
const charge = await apex.webhook('process-payment').post({
  amount: 99.95,
  currency: 'USD'
});

// 2. GET with subpath & query parameters
const profile = await apex.webhook('users').get('/summary', { id: 42 });
```

---

## 8. AI Prompt Actions (LLMs)

Run server-side prompt templates with optional Server-Sent Events (SSE) streaming:

```typescript
// 1. Standard Response
const response = await apex.ai.run('content-editor', {
  prompt: 'Fix grammar and summarize',
  originalText: 'Here is some text...'
});
console.log(response.result);

// 2. Real-Time Token Streaming (Pass onChunk callback)
await apex.ai.run(
  'assistant',
  { user_query: 'Explain SQL WAL mode' },
  (chunk) => {
    process.stdout.write(chunk);
  }
);
```

---

## 9. Storage & Resumable Uploads

```typescript
// 1. Standard Multipart Upload
const fileInput = document.getElementById('upload') as HTMLInputElement;
const file = await apex.files.upload(fileInput.files![0]);

// 2. Resumable Upload for Large Files (Tus 1.0.0)
const uploadTask = apex.files.uploadResumable(fileInput.files![0], {
  chunkSize: 1024 * 1024,
  onProgress: ({ percentage }) => console.log(`Upload: ${percentage}%`),
  onSuccess: (res) => console.log('File uploaded:', res.url)
});

// 3. Dynamic Thumbnail URL (Resizing & WebP conversion)
const thumbUrl = apex.files.getFileUrl(file.filename, {
  thumb: '400x300',
  format: 'webp',
  quality: 85,
  blur: 1.5
});

// 4. Pre-Signed Private URL (Asynchronous)
const signedUrl = await apex.files.getFileUrl(file.filename, {
  signed: true,
  expiresIn: 3600
});
```

---

## 10. Real-Time Subscriptions (WebSockets)

```typescript
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// 1. Subscribe to database changes
realtime.subscribe({
  collectionId: 5,
  eventType: 'Insert',
  dataFilter: { priority: 'high' }
});

// 2. Subscribe to custom signaling channel
realtime.subscribe({ channel: 'room_101' });

// 3. Send ephemeral client-to-client signal
realtime.sendSignal('room_101', 'UserTyping', { username: 'Alice' });

// 4. Handle incoming messages
realtime.onEvent((msg) => {
  console.log('Realtime message:', msg);
});
```

---

## 11. GraphQL Queries & Mutations

Execute queries against the dynamic, auto-generated GraphQL schema:

```typescript
const query = `
  query GetPosts($status: JSON) {
    posts(where: $status, limit: 10) {
      total
      items {
        id
        title
        author_id {
          email
        }
      }
    }
  }
`;

const result = await apex.graphql(query, {
  status: { status: 'published' }
});
```

---

## 12. Error Handling

ApexKit returns standardized error structures via `ApexError`:

```typescript
try {
  await apex.collection('products').create({ price: -10 });
} catch (err: any) {
  console.error('HTTP Status:', err.status); // 422
  console.error('Error Code:', err.code);     // "validation_error"
  console.error('Details:', err.details);     // [{ field: "price", message: "..." }]
}
```