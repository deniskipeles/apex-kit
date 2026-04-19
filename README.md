<p align="center">
  <img src="static/images/apexkit-logo.svg" width="250" alt="ApexKit Logo">
</p>

<h1 align="center">ApexKit</h1>

<p align="center">
  <strong>The AI-Native, Multi-Tenant Backend in a Single Binary.</strong><br>
  <em>Built with Rust, SQLite, and React.</em>
</p>

<p align="center">
  <a href="#features">Features</a> • 
  <a href="#quickstart">Quickstart</a> • 
  <a href="#architecture">Architecture</a> • 
  <a href="#documentation">Documentation</a>
</p>

---

## What is ApexKit?

**ApexKit** is a blazingly fast, all-in-one backend solution designed for the AI era. Inspired by tools like PocketBase and Supabase, ApexKit compiles your entire backend infrastructure—Database, Authentication, Storage, Real-time APIs, Serverless JS Scripting, and a fully featured Admin Dashboard—into a **single, portable Rust binary**.

Beyond standard BaaS features, ApexKit introduces natively integrated **Generative AI (Gemini/Gemma)**, **Vector Semantic Search**, and an **AI Architect** that can literally build your schemas and server scripts via natural language chat.

## ✨ Key Features

### 🗄️ Database & Auto-APIs
- **Embedded SQLite:** High-performance data storage with WAL mode, batching, and zero-copy JSONB processing.
- **Auto-generated APIs:** Instant REST and dynamic GraphQL endpoints for every collection you create.
- **Advanced Query Engine:** Deep filtering, sorting, joins (relations), and pipeline aggregations natively supported.

### 🧠 AI-Native Capabilities
- **AI Architect:** Chat with the built-in AI assistant from the dashboard to auto-generate collections, write server-side JavaScript, and design UI templates.
- **Vector Search Engine:** Natively embedded local vector database using `candle` and HNSW. Automatically embed text and images for semantic search.
- **Instant Search:** Typo-tolerant, ultra-fast full-text search powered by Tantivy.
- **AI Actions:** Securely define and execute LLM prompts on the server, exposing them as simple API endpoints to your frontend.

### ⚡ Edge Scripting (JavaScript in Rust)
- **V8-style Engine:** Write custom endpoints, cron jobs, and database triggers using JavaScript.
- **Sandboxed Execution:** Runs natively inside Rust using the `boa_engine`, providing access to `$db`, `$http`, `$ai`, and `$fs` without needing Node.js or external runtimes.

### 🏢 Built-in Multi-Tenancy & Sandboxes
- Designed for B2B/SaaS apps out of the box. Securely isolate data using **Tenants**.
- Instantly spin up **Sandboxes** to safely test schema changes or let the AI Architect draft features without touching production data.

### 📡 Real-time & Storage
- **WebSockets & SSE:** Subscribe to database changes or broadcast custom ephemeral events to channels.
- **S3 & Local Storage:** Upload files, generate thumbnails on the fly (AVIF/WebP), and seamlessly migrate data between local disks and AWS S3/Cloudflare Spaces.
- **Static Hosting:** Deploy your frontend (React, Vue, HTML/CSS) directly into ApexKit for a true all-in-one deployment.

---

## 🚀 Quickstart

### 1. Run via Docker (Recommended)

The easiest way to get started is using our lightweight Docker image (Alpine/Musl based).

```bash
docker run -p 5000:5000 \
  -v /path/to/local/data:/app/storage \
  deniskipeles/apexkit:latest
```

### 2. Run via Pre-compiled Binary

Download the latest binary for your OS from the [Releases page](https://github.com/deniskipeles/apexkit/releases), make it executable, and run it:

```bash
# Linux / macOS
chmod +x apexkit
./apexkit --port 5000

# Windows
apexkit.exe --port 5000
```

### 3. Access the Admin UI

Once the server is running, navigate to:
👉 **`http://localhost:5000/_dashboard`**

Log in using the default credentials:
- **Email:** `admin@apexkit.io`
- **Password:** `password` *(Change this immediately in the Users tab!)*

---

## 🏗️ Architecture & Tech Stack

ApexKit achieves its performance and small footprint by utilizing a modern, carefully selected tech stack:

*   **Core Backend:** [Rust](https://www.rust-lang.org/) & [Axum](https://github.com/tokio-rs/axum)
*   **Database:** [SQLite](https://sqlite.org/) (via `rusqlite`)
*   **Search Engine:** [Tantivy](https://github.com/quickwit-oss/tantivy) (Full-text) & [Candle](https://github.com/huggingface/candle) (Machine Learning / Vectors)
*   **JavaScript Engine:** [Boa](https://github.com/boa-dev/boa)
*   **Admin Dashboard:** [React 19](https://react.dev/), [Vite](https://vitejs.dev/), [Tailwind CSS](https://tailwindcss.com/), & [Monaco Editor](https://microsoft.github.io/monaco-editor/)

---

## 📚 Documentation

Detailed guides for interacting with the SDK, writing server-side JavaScript, and configuring AI models can be found inside the Admin UI by clicking the **"API Docs"** button on the Collections or Users pages, or by visiting the `/scalar` endpoint on your running instance for OpenAPI specifications.

### Basic SDK Usage Example (JS/TS)
```javascript
import { ApexKit } from '@apexkit/sdk';

const pb = new ApexKit('http://localhost:5000');

// Authenticate
await pb.auth.login('user@example.com', 'password123');

// Create a record
const post = await pb.collection('posts').create({
    title: "Hello World",
    content: "Building with ApexKit is awesome!"
});

// Semantic Vector Search
const similarPosts = await pb.collection('posts').searchTextVector("Greeting messages", 5);
```

---

## 🛠️ Building from Source

To build ApexKit from source, you will need Rust, Node.js (for the Admin UI), and a few system dependencies (like `cmake` and `nasm`).

```bash
# 1. Clone the repository
git clone https://github.com/deniskipeles/apexkit.git
cd apexkit

# 2. Build the Admin UI
cd admin-ui
npm install
npm run build
cd ..

# 3. Move UI assets to the static directory
mkdir -p static/dashboard
cp -r admin-ui/dist/* static/dashboard/

# 4. Build the Rust backend (Release mode)
cargo build --release -p apexkit-api

# 5. Run it!
./target/release/apexkit-api
```

---

## 📝 License

This project is licensed under the [MIT License](LICENSE).