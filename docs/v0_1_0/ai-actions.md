# 🤖 AI Actions Documentation

**Version:** 0.1.0
**Base URL:** `https://api.your-app.com/api/v1`

ApexKit **AI Actions** transform Generative AI prompt templates into standard REST API endpoints. They act as a secure middle layer between your frontend and AI providers like Google Gemini.

### Why use AI Actions?
1.  **Security**: Your API Keys are encrypted in the database and never exposed to the client.
2.  **Abstraction**: Frontend developers call `run/summarize` instead of constructing complex LLM payloads.
3.  **Prompt Engineering**: Tweak system prompts and logic in the Admin UI without redeploying code.
4.  **Multimodality**: Built-in support for Text-to-Text, Vision (Image analysis), and Image Generation.

---

## 1. Defining an Action

Actions are configured in **Admin UI > AI Actions**.

| Field | Description | Example |
| :--- | :--- | :--- |
| **Name** | Human-readable label. | `Product Description Generator` |
| **Slug** | The unique URL identifier. | `gen-desc` |
| **Model** | The underlying AI model. | `gemini-2.0-flash`, `gemini-3-pro`, `imagen-4` |
| **System Prompt** | The AI's persona and rules. | `You are a professional copywriter. Respond in JSON.` |
| **Template** | The prompt with variables. | `Write a 50-word description for: {{product_name}}` |

---

## 2. Using Variables

ApexKit uses double-curly braces `{{variable}}` for dynamic substitution. When you call an action, you pass a `variables` object in the request body.

**Request Body:**
```json
{
  "variables": {
    "product_name": "Wireless Noise-Cancelling Headphones"
  }
}
```

---

## 3. Multimodal Features (Vision & Images)

### A. Image Analysis (Vision)
If you pass a variable containing a **Base64 Data URI** (e.g., `data:image/png;base64,...`), ApexKit automatically extracts the binary data and sends it to the Gemini Vision model as an inline media attachment.

**Template:** `Describe the objects found in this image: {{context}}`
**Request:**
```json
{
  "variables": {
    "context": "Inventory Check",
    "image_data": "data:image/jpeg;base64,/9j/4AAQ..."
  }
}
```
*Note: The template does not need to explicitly reference `{{image_data}}`; the presence of the data URI attaches it to the prompt context.*

### B. Image Generation
When using a generation model like **Imagen 4**, the response automatically returns a Data URI.

**Response:**
```json
{
  "result": "data:image/png;base64,iVBORw0KGgo..."
}
```

---

## 4. API Reference

### Run an Action
`POST /api/v1/ai/run/{slug}`

**Headers:**
```http
Authorization: Bearer <JWT_TOKEN>
Content-Type: application/json
```

---

## 5. JavaScript SDK Usage

The ApexKit SDK simplifies AI interaction with typed methods.

```javascript
import { pb } from './apiClient';

// 1. Text Generation
const summary = await pb.ai.run('summarize', { 
    text: "Long article content..." 
});
console.log(summary.result);

// 2. Vision (Image to Text)
const description = await pb.ai.run('describe-image', {
    image: await getBase64(fileInput.files[0]),
    prompt: "What color is the car?"
});

// 3. Grounding Metadata (Search Citations)
// For models with search enabled, citations are in the metadata
if (summary.metadata?.groundingChunks) {
    renderSources(summary.metadata.groundingChunks);
}
```

---

## 6. Supported Models

| Model | Capability | Best For |
| :--- | :--- | :--- |
| **`gemini-2.0-flash`** | Text + Vision | Speed, low latency, grounding. |
| **`gemini-3-pro`** | Text + Vision | Complex reasoning, large context. |
| **`imagen-4`** | Image Gen | High-quality visual generation. |
| **`gemma-3-27b`** | Text | Open-weights, efficient instructions. |

> **Pro Tip**: Enable **Google Search Grounding** in the Admin UI for an Action to ensure the AI uses real-time information and provides source URLs in the `metadata` field.