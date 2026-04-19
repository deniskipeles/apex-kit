### 1. `ai-actions-metadata.md`

```markdown
# 🧠 AI Grounding & Metadata Guide

**Context:** When using AI Models capable of Google Search (e.g., `gemini-2.0-flash` with Search enabled), ApexKit captures rich metadata about *how* the AI formulated its answer.

This data is returned in the `metadata` field of the AI response.

## 1. The Metadata Structure

When an AI Action runs via `POST /api/v1/ai/run/{slug}`, the JSON response looks like this:

```json
{
    "result": "Rust is a modern systems programming language...",
    "metadata": {
        "groundingChunks": [
            {
                "web": {
                    "title": "turing.com",
                    "uri": "https://vertexaisearch.cloud.google.com/grounding-api-redirect/..."
                }
            },
            {
                "web": {
                    "title": "itprotoday.com",
                    "uri": "https://vertexaisearch.cloud.google.com/grounding-api-redirect/..."
                }
            }
        ],
        "groundingSupports": [
            {
                "groundingChunkIndices": [0, 1],
                "segment": {
                    "endIndex": 583,
                    "startIndex": 435,
                    "text": "Its consistent ranking as the \"most loved\" programming language..."
                }
            }
        ],
        "webSearchQueries": [
            "Rust programming language features",
            "benefits of Rust programming language"
        ]
    }
}
```

## 2. Use Case: Automated SEO Tagging

Instead of manually thinking of tags for a blog post, you can use the **exact queries** the AI used to research the topic. These are statistically high-relevance keywords.

### Server-Side Implementation (Script)

Create a Script (Trigger: `manual` or `before_create`) to generate content and auto-tag it.

```javascript
// Script Name: generate-blog-post
export default async function(req) {
    const { topic } = await req.json();

    // 1. Call your AI Action
    // We use the internal AI helper available in scripts
    // (Note: $ai.runAction is a convenience wrapper around the internal API)
    // Or you can use $http to call the endpoint if not exposed directly.
    
    const aiResponse = await $http.post(`http://localhost:5000/api/v1/ai/run/content-editor`, {
        variables: { 
            prompt: `Write a blog post about ${topic}`,
            originalText: "" 
        }
    });
    
    const data = JSON.parse(aiResponse);
    const content = data.result;
    
    // 2. Extract Search Queries as Tags
    const rawTags = data.metadata?.webSearchQueries || [];
    const tags = [...new Set(rawTags)].slice(0, 5); // Deduplicate and slice

    // 3. Save to Database
    const newRecordId = await $db.records.create("posts", {
        title: topic,
        content: content,
        tags: tags, // Stores: ["history of js", "js frameworks", ...]
        status: "draft"
    });

    return new Response({ 
        success: true, 
        id: newRecordId, 
        generated_tags: tags 
    });
}
```

## 3. Use Case: Displaying Citations (Footnotes)

To build trust with your readers, you can render the `groundingChunks` as sources at the bottom of your UI.

### Frontend Example (React/HTML)

```jsx
const Citations = ({ metadata }) => {
  if (!metadata?.groundingChunks) return null;

  return (
    <div className="citations-box">
      <h3>Sources & References</h3>
      <ul>
        {metadata.groundingChunks.map((chunk, index) => (
          <li key={index}>
            <span className="source-index">[{index + 1}]</span>
            <a href={chunk.web.uri} target="_blank" rel="noopener noreferrer">
              {chunk.web.title}
            </a>
          </li>
        ))}
      </ul>
    </div>
  );
};
```

## 4. Enabling Metadata

By default, metadata is enabled for models that support grounding (Gemini 2.0 Flash / Pro). Ensure your AI Action definition uses a supported model.

1.  Go to **Admin UI > AI Actions**.
2.  Edit your specific Action.
3.  Ensure the Model is set to a version that supports Google Search Grounding (e.g., `gemini-2.0-flash`).
```
