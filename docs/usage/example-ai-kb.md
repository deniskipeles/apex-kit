# Example: AI-Powered SaaS Knowledge Base

Build a documentation platform where users can ask questions in natural language and get answers based on your content.

## 1. Database Collections

### `articles`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `title` | Text | Required | |
| `content` | Rich Text | Required | |
| `category` | Select | | `General`, `Technical`, `Billing` |
| `embedding` | Vector | | Model: `BGE-Small`, Dim: 384 |
| `slug` | Text | Unique | |

### `feedback`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `article_id` | Relation | Required | Collection: `articles` |
| `rating` | Number | | 1-5 |
| `comment` | Text | | |

## 2. Security Policies

- **`articles`**:
  - `read`: `public`
  - `create/update/delete`: `admin`
- **`feedback`**:
  - `create`: `public`
  - `read/update/delete`: `admin`

## 3. Edge Functions

### `auto-embed-article`
**Trigger**: After Create/Update on `articles`
```javascript
export default async function(req) {
    const text = `${req.record.title} ${req.record.content}`;
    const vector = await $ai.embed(text);
    await $db.records.patch('articles', req.record.id, {
        embedding: vector
    });
}
```

## 4. Connecting the Frontend

### Searching for articles
```javascript
const query = "How do I reset my password?";
const results = await apex.collection('articles').searchTextVector(query, 5);

// Display results
results.forEach(article => {
    console.log(article.title);
});
```

### Using an AI Action for Q&A
Define an AI Action `kb-ask` with prompt: `Answer based on this context: {{context}}. Question: {{question}}`

```javascript
const context = results.map(r => r.content).join("\n");
const answer = await apex.ai.run('kb-ask', {
    context: context,
    question: query
});
console.log(answer.result);
```
