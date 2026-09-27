<p align="center">
  <img src="static/images/apexkit-logo.svg" width="250" alt="ApexKit Logo">
</p>

<h1 align="center">ApexKit</h1>

<p align="center">
  <strong>The AI-Native, Multi-Tenant Backend in a Single Binary.</strong><br>
  <em>Built with Rust, SQLite, and React.</em>
</p>

<p align="center">
  <a href="https://apexkit.pages.dev"><strong>Official Website & Docs</strong></a> • 
  <a href="#features">Features</a> • 
  <a href="#downloads">Downloads</a> • 
  <a href="#quickstart">Quickstart</a> • 
  <a href="#architecture">Architecture</a>
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

### ⚡ High-Performance Edge Runtime (QuickJS + TS/WASM)
- **Embedded QuickJS Engine:** Lightning-fast, standards-compliant JavaScript/TypeScript runtime powered by QuickJS (`rquickjs`) with full async/await and ES Module support.
- **Native TypeScript & TSX:** First-class support for `.ts` and `.tsx` scripts (via in-memory Oxc compiler). Run webhooks using [Hono](https://hono.dev/) with zero compile steps.
- **WASM & WASI Orchestration:** Call high-performance WebAssembly binaries and precompiled modules directly from scripts using `$wasm`.
- **Preemptible Task Scheduling:** Built-in quantum round-robin scheduler ensures long-running loops or tasks yield CPU slices without freezing Tokio worker threads.
- **Rich Standard Globals:** Built-in access to `$db`, `$http`, `$files`, `$fs`, `$cache`, `$mail`, `$realtime`, `$ai`, and `$env`.

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

<a id="downloads"></a>
Download pre-built standalone binaries for your target architecture from our global CDN:

| Operating System | Architecture | Build Flavor | Size | Download |
| :--- | :--- | :--- | :--- | :--- |
| **Linux (glibc / Ubuntu)** | `x86_64` | **Performant (ONNX)** | 34 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-unknown-linux-gnu-perf.tar.gz) |
| **Linux (glibc / Ubuntu)** | `x86_64` | **Small Size (ONNX)** | 28 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-unknown-linux-gnu-small.tar.gz) |
| **Linux (musl / Alpine)** | `x86_64` | **Performant (Candle)** | 25 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-unknown-linux-musl-perf.tar.gz) |
| **Linux (musl / Alpine)** | `x86_64` | **Small Size (Candle)** | 19 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-unknown-linux-musl-small.tar.gz) |
| **Linux (musl / ARM64)** | `aarch64` | **Performant (Candle)** | 24 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-aarch64-unknown-linux-musl-perf.tar.gz) |
| **Linux (musl / ARM64)** | `aarch64` | **Small Size (Candle)** | 18 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-aarch64-unknown-linux-musl-small.tar.gz) |
| **macOS (Apple Silicon)** | `arm64 (M1-M4)`| **Performant (Candle)** | 21 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-aarch64-apple-darwin-perf.tar.gz) |
| **macOS (Apple Silicon)** | `arm64 (M1-M4)`| **Small Size (Candle)** | 17 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-aarch64-apple-darwin-small.tar.gz) |
| **macOS (Intel)** | `x86_64` | **Performant (Candle)** | 23 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-apple-darwin-perf.tar.gz) |
| **macOS (Intel)** | `x86_64` | **Small Size (Candle)** | 18 MB | [Download `.tar.gz`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-apple-darwin-small.tar.gz) |
| **Windows** | `x86_64` | **Performant (Candle)** | 24 MB | [Download `.zip`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-pc-windows-gnu-perf.zip) |
| **Windows** | `x86_64` | **Small Size (Candle)** | 17 MB | [Download `.zip`](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/apexkit-x86_64-pc-windows-gnu-small.zip) |

*Verify download integrity using the official [SHA-256 Checksums](https://huggingface.co/datasets/kipeles/apexkit-releases/resolve/main/v0.1.0-beta.1/checksums.txt).*

```bash
# Linux / macOS
tar -xzf apexkit-*.tar.gz
chmod +x apexkit
./apexkit --port 5000

# Windows
# Extract the zip archive and run:
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

*   **Official Website:** [apexkit.pages.dev](https://apexkit.pages.dev)
*   **Core Backend:** [Rust](https://www.rust-lang.org/) & [Axum](https://github.com/tokio-rs/axum)
*   **Database Engine:** [SQLite](https://sqlite.org/) (via `rusqlite` with zero-copy JSONB and WAL mode)
*   **Search & Vector Engine:** [Tantivy](https://github.com/quickwit-oss/tantivy) (Full-text search), [Candle](https://github.com/huggingface/candle) & [ONNX Runtime](https://onnxruntime.ai/)
*   **JavaScript & TS Runtime:** [QuickJS](https://bellard.org/quickjs/) (via `rquickjs`) & [Oxc](https://oxc.rs/) (High-speed TypeScript transpiler)
*   **WebAssembly Engine:** [Wasmtime](https://wasmtime.dev/) (WASI Preview 1)
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

## ⚖️ License & The "Anti-Big-Tech" Guarantee

This engine is licensed under the Business Source License (BSL 1.1). See the full [LICENSE](LICENSE) file for legal details.

### The TL;DR for Startups & Devs:

*   ✅ **Building an App/SaaS?** 100% Free forever. Self-host it on your own servers, scale globally, and keep 100% of your revenue.
*   ✅ **Building a Competing Service?** If you want to use our engine to launch your own hosting platform or managed BaaS to compete with us, **go for it!** As long as your company makes under $5M/year or has raised under $10M, you have our full blessing. Work hard, innovate, and keep us on our toes.
*   ❌ **Are you a Tech Giant?** If you are a mega-corporation (like AWS, GCP, Azure) or a company making over $5M/year, you cannot bundle or host our engine as a managed service without a separate commercial agreement.

### No Vendor Lock-In

Every line of code we write automatically converts into pure, unrestricted **Apache 2.0 open-source** exactly 2 years after its release. If we disappear or stop maintaining the project, the community owns the code under Apache 2.0 forever.