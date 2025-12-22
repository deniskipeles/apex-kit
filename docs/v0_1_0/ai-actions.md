# ApexKit AI Actions Documentation

**Version:** 0.1.0
**Base URL:** `http://localhost:5000/api/v1`

ApexKit **AI Actions** allow you to turn Generative AI prompts (LLMs) into standard REST API endpoints. This creates a secure "middle layer" between your frontend and AI providers (like Google Gemini/Imagen).

### Why use AI Actions?
1.  **Security:** Your Gemini API Key is encrypted on the server. It is never exposed to the client.
2.  **Abstraction:** Frontend developers call `run/summarize` instead of constructing complex LLM payloads.
3.  **Prompt Engineering:** Prompts are managed in the database/Admin UI, allowing you to tweak logic without redeploying client code.
4.  **Multimodality:** Native support for Text, Images, and Vision tasks.

---

## 1. Creating an AI Action

Before you can use an endpoint, you must define it in the **Admin Dashboard > AI Actions**.

An Action consists of:

| Field | Description | Example |
| :--- | :--- | :--- |
| **Name** | Human-readable label. | `Content Summarizer` |
| **Slug** | The unique URL identifier. | `summarize-text` |
| **Model** | The underlying AI model. | `gemini-2.0-flash` (Fast), `gemini-2.5-pro` (Complex), `imagen-3.0` (Images) |
| **System Prompt** | Sets the AI's persona and constraints. Hidden from the user. | `You are a helpful editor. Respond in Markdown only.` |
| **Template** | The User Prompt with variable placeholders (`{{var}}`). | `Summarize this text: {{input_text}}` |

---

## 2. API Endpoint

To execute an action, make a POST request to the run endpoint.

**Endpoint:** `POST /api/v1/ai/run/{slug}`

**Headers:**
*   `Content-Type: application/json`
*   `Authorization: Bearer <TOKEN>` (Required if defined in collection rules, currently restricted to Admin or open based on app logic).

**Body Schema:**
```json
{
  "variables": {
    "variable_name": "Value to insert into template",
    "another_var": "More data"
  }
}
```

---

## 3. Usage Examples

### Scenario A: Text Generation
**Goal:** Create an endpoint to correct grammar.

1.  **Admin Config:**
    *   **Slug:** `grammar-fix`
    *   **System Prompt:** `You are a strict grammar checker. Output only the corrected text.`
    *   **Template:** `Correct this: {{text}}`

2.  **Client Request:**
    ```bash
    curl -X POST http://localhost:5000/api/v1/ai/run/grammar-fix \
      -H "Content-Type: application/json" \
      -d '{ "variables": { "text": "Me and him went to store." } }'
    ```

3.  **Response:**
    ```json
    {
      "result": "He and I went to the store.",
      "metadata": { ... } // Citations if applicable
    }
    ```

---

### Scenario B: Image Generation
**Goal:** Generate an image using Imagen or Gemini Image generation.

1.  **Admin Config:**
    *   **Slug:** `generate-image`
    *   **Model:** `gemini-2.5-flash-image` (or Imagen)
    *   **Template:** `{{prompt}}`

2.  **Client Request:**
    ```javascript
    const res = await fetch('/api/v1/ai/run/generate-image', {
        method: 'POST',
        body: JSON.stringify({
            variables: { prompt: "A cyberpunk cat neon city" }
        })
    });
    ```

3.  **Response:**
    The backend automatically detects image output and returns a Data URI.
    ```json
    {
      "result": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA..."
    }
    ```

---

### Scenario C: Vision & Image Editing (Multimodal)
**Goal:** Ask the AI to describe an image or edit it.

**How it works:** If you pass a variable containing a **Base64 Data URI** (e.g., `data:image/png;base64,...`), ApexKit automatically extracts the binary data and sends it to the model as an inline media attachment.

1.  **Admin Config:**
    *   **Slug:** `describe-image`
    *   **Model:** `gemini-2.0-flash`
    *   **Template:** `Describe what you see in this image. Context: {{context}}`

2.  **Client Request:**
    ```json
    {
      "variables": {
        "context": "For a blind user accessibility report.",
        "image_input": "data:image/jpeg;base64,/9j/4AAQSkZJRg..." 
      }
    }
    ```
    *Note: The template does NOT need to reference `{{image_input}}`. Simply sending the image in the variables object attaches it to the prompt context for the AI to see.*

3.  **Response:**
    ```json
    {
      "result": "The image shows a golden retriever playing in a park..."
    }
    ```

---

## 4. Frontend Integration (TypeScript/SDK)

If you are using the `apiClient` provided in the admin UI or the SDK, interacting with AI is typed and simple.

```typescript
import { apiClient } from './lib/apiClient';

// 1. Text
async function fixSpelling(text: string) {
    const res = await apiClient.ai.run('grammar-fix', { text });
    console.log(res.result);
}

// 2. Image Gen
async function makeArt() {
    const res = await apiClient.ai.run('generate-image', { 
        prompt: "Oil painting of a cottage" 
    });
    // Render the base64 string
    document.getElementById('my-img').src = res.result;
}

// 3. Vision / Edit
async function editPhoto(base64Str: string) {
    const res = await apiClient.ai.run('edit-image', { 
        image: base64Str,
        prompt: "Make it look like a sketch"
    });
    return res.result; 
}
```

## 5. Supported Models

The availability depends on your Google Cloud/AI Studio API key permissions, but ApexKit is configured to support:

*   **Gemini 2.0 Flash:** Best for speed and multimodal (Text/Image/Audio inputs).
*   **Gemini 3 Pro:** Best for complex reasoning and large context windows.
*   **Gemini 2.5 Flash/Lite:** Optimized for cost and latency.
*   **Imagen 3/4:** Dedicated image generation models.