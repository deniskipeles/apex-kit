# 🧠 AI Grounding & Metadata Guide

**Context:** When using AI Models capable of Google Search (e.g., `gemini-2.5-flash` with Search enabled), ApexKit captures rich metadata about *how* the AI formulated its answer.

This data is returned in the `metadata` field of the AI response.

## 1. The Metadata Structure

When an AI Action runs, the JSON response looks like this:

```json
{
    "metadata": {
        "groundingChunks": [
            {
                "web": {
                    "title": "turing.com",
                    "uri": "https://vertexaisearch.cloud.google.com/grounding-api-redirect/AUZIYQFS1whADpfFecStW8dt-NjAhGXksMEatwlt2nJAoffZrKTEtoKBwpngEwfFpYFrTdKwwpy5tDh8017ufMfoPmxfMFSsVISx4zMRw97kZQVCdTnOSkUgF0y7yMq8Ex0zf9lmUZ6I6b_aW5RIffRHy0w5UkMnYu6EuHZu7mO-SCBuRM-stjg="
                }
            },
            {
                "web": {
                    "title": "itprotoday.com",
                    "uri": "https://vertexaisearch.cloud.google.com/grounding-api-redirect/AUZIYQGTdut3_TcDH5R_O6YkX8BywE_DPSPTioNv1aKm4C0pk6L4NmLrAE69qTRg4uNmbwYZne4LRrHtlFFjd5JkXfN6-ksp0W617MxYv4imZdY6K_hsrfkpruaPi7JvIk2Rq0GrMCs37YyR21sDuPRASLj0vDgGxNsT10S3IHYUo2E2zGV_P55koHmja4_pj4Y7PJUS3RDQ7KXPJJU="
                }
            },
            ...others,
            {
                "web": {
                    "title": "quora.com",
                    "uri": "https://vertexaisearch.cloud.google.com/grounding-api-redirect/AUZIYQFPn7nxcRGedqNp2OtD_nmasaIGAofWzsDNylP7v1wvOwTx-_5PW2bRSdtCZgeRukiVzPr1ieCAwKw0FI4KuLbTTM--G55jbK3PkDduMdORecHYJ-GCLo2KGRO9PyioR-evOe9HblRDAESkOlD9ZlcncFM9S_Q5oJUGDkmnuI_0xvNUSrlOIJC8cDKFZWqKd53Z1vRxs3pmSzd9a7k9a8neLC6ewzv_OFDej_ZZTE-e8reFZ6SVrA6cHDGYtDoMSrM-89bJNhr3_K9caAoqTA6ZCxGZLZrmGbtNsKIlgGHWdEsf-8toCSeK4MhA16uq8EYBgoAcDWuYx5aIoa3lkQ=="
                }
            }
        ],
        "groundingSupports": [
            {
                "groundingChunkIndices": [
                    0,
                    1,
                    2,
                    3
                ],
                "segment": {
                    "endIndex": 583,
                    "startIndex": 435,
                    "text": "Its consistent ranking as the \"most loved\" programming language by Stack Overflow is a testament to its growing influence and developer satisfaction"
                }
            },
            {
                "groundingChunkIndices": [
                    4,
                    5
                ],
                "segment": {
                    "endIndex": 778,
                    "startIndex": 628,
                    "text": "Rust's design philosophy centers on providing high performance without compromising safety, a combination that sets it apart from many other languages"
                }
            },
            {
                "groundingChunkIndices": [
                    6,
                    7,
                    8,
                    5
                ],
                "segment": {
                    "endIndex": 1128,
                    "startIndex": 956,
                    "text": "This system allows for deterministic memory management, preventing common issues like memory leaks and dangling pointers without the runtime overhead of a garbage collector"
                }
            },
            ...others,
            {
                "groundingChunkIndices": [
                    17
                ],
                "segment": {
                    "endIndex": 4360,
                    "startIndex": 4250,
                    "text": "The U.S. government has also recommended Rust over C/C++ for security reasons, further bolstering its adoption"
                }
            },
            {
                "groundingChunkIndices": [
                    8,
                    18,
                    14
                ],
                "segment": {
                    "endIndex": 4600,
                    "startIndex": 4362,
                    "text": "While Rust has a steeper learning curve than some other languages, its growing community, extensive documentation, and the tangible benefits it offers in terms of safety and performance ensure its continued rise in popularity and adoption"
                }
            }
        ],
        "searchEntryPoint": {
            "renderedContent": "..."
        },
        "webSearchQueries": [
            "Rust programming language features",
            "benefits of Rust programming language",
            "Rust programming language use cases",
            "Rust programming language popularity"
        ]
    },
    "result": "## Unpacking the Appeal of Rust: A Deep Dive into its Features and Applications\n\nRust, a modern systems programming language, has rapidly ascended in popularity, earning accolades for its unique blend of speed, safety, and reliability. Developed initially as a hobby project by a Mozilla developer in 2006, it officially released its first public version in 2012 and has since garnered significant attention from developers worldwide. Its consistent ranking as the \"most loved\" programming language by Stack Overflow is a testament to its growing influence and developer satisfaction.\n\n### Core Features Driving Rust's Success\n\nRust's design philosophy centers on providing high performance without compromising safety, a combination that sets it apart from many other languages. Several key features contribute to its robust nature:\n\n*   **Memory Safety without Garbage Collection:** Rust's most distinctive feature is its ownership and borrowing system. This system allows for deterministic memory management, preventing common issues like memory leaks and dangling pointers without the runtime overhead of a garbage collector. This means that potential memory-related bugs are caught during compilation, leading to more reliable code.\n*   **\"Fearless Concurrency\":** Rust's compiler can detect potential data races at compile time, enabling developers to write concurrent code with greater confidence. This significantly reduces the likelihood of bugs that are notoriously difficult to track down in other languages.\n*   **Performance:** Rust is designed to be blazingly fast and memory-efficient. Its zero-cost abstractions mean that high-level code can be written without sacrificing performance, achieving speeds comparable to C and C++.\n*   **Helpful Compiler and Tooling:** Rust boasts a user-friendly compiler that provides exceptionally helpful error messages, aiding developers in identifying and fixing issues quickly. Furthermore, its integrated package manager and build system, Cargo, is highly praised for its ease of use and efficient dependency management.\n*   **Expressive Syntax and Strong Type System:** Rust features a modern and expressive syntax, including pattern matching and algebraic data types, which enhances code readability and reduces boilerplate. Its statically-typed nature also contributes to code maintainability and reduces runtime errors.\n*   **Cross-Platform Support:** Rust offers robust cross-platform capabilities, allowing applications to run seamlessly across various operating systems and architectures with minimal platform-specific code.\n\n### Where Rust Shines: Key Use Cases\n\nThe unique combination of features makes Rust a compelling choice for a wide array of applications, particularly in areas where performance, safety, and low-level control are paramount:\n\n*   **Systems Programming:** Rust is ideal for developing operating systems, embedded systems, kernel development, and other low-level software where memory safety and performance are critical. Its ability to manage memory efficiently without a garbage collector makes it suitable for resource-constrained environments.\n*   **Web Development:** Rust is increasingly being used for building high-performance web services and backend systems. Its WebAssembly support also allows for running Rust code directly in web browsers, enhancing web application performance.\n*   **Game Development:** The language's performance characteristics and memory safety make it an excellent choice for developing complex game engines and applications requiring low latency.\n*   **Network Programming:** Rust's safety guarantees make it a strong candidate for building reliable and secure network applications and services.\n*   **Command-Line Interface (CLI) Tools:** Rust's robust ecosystem and performance make it a top choice for creating fast and reliable CLI tools.\n*   **Other Emerging Areas:** Rust is also finding applications in areas such as data science backends, machine learning, virtual and augmented reality, and cryptography.\n\n### Industry Adoption and Future Outlook\n\nMajor tech companies like Amazon (AWS), Google, Microsoft, and Dropbox are embracing Rust for various projects, from core infrastructure to performance-critical components. The U.S. government has also recommended Rust over C/C++ for security reasons, further bolstering its adoption. While Rust has a steeper learning curve than some other languages, its growing community, extensive documentation, and the tangible benefits it offers in terms of safety and performance ensure its continued rise in popularity and adoption."
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

    // 1. Call your AI Action (defined in Admin > AI Actions)
    // We use $http to call our own internal API for simplicity
    const aiResponse = await $http.post("http://127.0.0.1:5000/api/v1/ai/run/content-editor", {
        variables: { 
            prompt: `Write a blog post about ${topic}`,
            originalText: "" 
        }
    });
    
    const data = JSON.parse(aiResponse);
    const content = data.result;
    
    // 2. Extract Search Queries as Tags
    // The AI might return 10 queries, let's take the top 5 unique ones
    const rawTags = data.metadata?.webSearchQueries || [];
    const tags = [...new Set(rawTags)].slice(0, 5); // Deduplicate and slice

    // 3. Save to Database
    const newRecordId = await $db.insert("posts", {
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

## 3. Use Case: Search Autocomplete Feeder

You can use the AI's research patterns to build a "Trending Searches" or "Autocomplete" database. If the AI thinks these terms are relevant to your content, your users probably will too.

### Architecture
1.  Create a Collection named `search_terms` (Fields: `term` (string, unique), `hits` (number)).
2.  Create a Script that runs after AI generation to populate this collection.

### Script Implementation

```javascript
// Script Name: ingest-search-terms
// Trigger: manual (or called by another script)
export default async function(req) {
    const { metadata } = await req.json();
    
    if (!metadata || !metadata.webSearchQueries) {
        return new Response({ msg: "No metadata" });
    }

    const queries = metadata.webSearchQueries;

    // Iterate and Upsert (Update if exists, Insert if new)
    for (const term of queries) {
        const cleanTerm = term.toLowerCase().trim();
        
        
        // Check if exists
        const existing = await $db.find("search_terms", { term: cleanTerm });
        
        if (existing.length > 0) {
            // Increment popularity
            await $db.update("search_terms", existing[0].id, {
                hits: existing[0].hits + 1
            });
        } else {
            // Create new
            await $db.insert("search_terms", {
                term: cleanTerm,
                hits: 1
            });
        }
    }

    return new Response({ success: true, processed: queries.length });
}
```

## 4. Use Case: Displaying Citations (Footnotes)

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

## 5. Enabling Metadata

By default, metadata might be trimmed to save space. To ensure you receive this data:

1.  Go to **Admin UI > AI Actions**.
2.  Edit your specific Action (e.g., `content-editor`).
3.  Look for the **"Audit & Storage"** section.
4.  Enable **"Save Metadata Logs"** or ensure your action logic passes the full response object back to your script/frontend.