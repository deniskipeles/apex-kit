# Contributing to ApexKit 🚀

Welcome to the ApexKit project! We are thrilled that you want to contribute. ApexKit is a modern, Rust-powered Backend-as-a-Service (BaaS) and AI-Architect platform featuring multi-tenancy, built-in vector search, server-side JS scripting, and edge replication.

This document serves as the single source of truth for setting up your environment, understanding the architecture, and submitting high-quality contributions.

## Table of Contents
1. [Architecture Overview](#1-architecture-overview)
2. [Prerequisites](#2-prerequisites)
3. [Local Development Setup](#3-local-development-setup)
4. [Branching & Versioning Strategy](#4-branching--versioning-strategy)
5. [Database & Migrations (UUID v7)](#5-database--migrations-uuid-v7)
6. [Coding Guidelines](#6-coding-guidelines)
7. [Submitting a Pull Request](#7-submitting-a-pull-request)

---

## 1. Architecture Overview

ApexKit is structured as a Cargo Workspace Monorepo:

*   **`apexkit-api/`**: The Axum web server, routing, gRPC/WebSocket replication layer, and HTTP endpoints.
*   **`apexkit-core/`**: The core logic. Contains the database traits (Rusqlite), Server-side JS engine (Boa), Tantivy search engine, caching (Moka), and security/cryptography.
*   **`apex-vector/`**: The AI embedding generation (Candle) and in-memory vector indexing (HNSW) module.
*   **`admin-ui/`**: The React/Vite/TypeScript frontend dashboard.

**Key Technologies:**
*   **Backend**: Rust, Axum, Tonic (gRPC), Rusqlite (SQLite WAL mode), Boa (JavaScript engine).
*   **Frontend**: React 19, Zustand, Tailwind CSS, Monaco Editor.

---

## 2. Prerequisites

Before you begin, ensure you have the following installed on your system:

*   **Rust**: Latest stable version (`rustup update`).
*   **Node.js**: v18.0.0 or higher.
*   **Package Manager**: `npm` (or `pnpm`).
*   **C Compiler**: `build-essential` (Linux) or Xcode Command Line Tools (macOS) for compiling SQLite and Candle dependencies.
*   **Protobuf Compiler**: While the project uses `protobuf-src` to compile locally, having `protoc` installed in your system PATH is highly recommended.

---

## 3. Local Development Setup

### Step 1: Clone the Repository
```bash
git clone https://github.com/your-org/apexkit.git
cd apexkit
```

### Step 2: Environment Variables
Create a `.env` file in the root directory:
```env
APEXKIT_MASTER_KEY="<generate-a-32-byte-base64-string>"
APEX_VECTOR_MODEL="gemma-300m"
HF_TOKEN="your_huggingface_token" # Optional, for downloading models
```
*(If you do not provide a master key, the server will generate one on its first boot and print it to the terminal).*

### Step 3: Build the Admin UI
ApexKit serves its dashboard directly from the Rust binary. You must build the UI first so the Rust `rust-embed` macro can bundle it.

```bash
cd admin-ui
npm install
npm run build
cd ..
```
*(For frontend-only development, you can run `npm run dev` in the `admin-ui` folder while the Rust backend runs on port 5000).*

### Step 4: Run the Backend
Run the backend in development mode:
```bash
cargo run --bin apexkit-api
```
The API will be available at `http://localhost:5000`, and the admin dashboard at `http://localhost:5000/_dashboard`.

---

## 4. Branching & Versioning Strategy

ApexKit recently underwent a major migration from Auto-Incrementing Integers to **UUID v7** for database primary keys. 

*   **`main` branch**: The bleeding-edge branch. All new development, features, and UUID v7 implementations happen here. (Version `0.2.x+`).
*   **`v0.1-legacy` branch**: The older Integer-based version of ApexKit. We only accept **critical security patches and bug fixes** to this branch. No new features.

### Creating a Branch
Always branch off `main` for new features:
```bash
git checkout main
git pull origin main
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

---

## 5. Database & Migrations (UUID v7)

Because ApexKit uses local SQLite files rather than a centralized DB server, migrations are handled **in code at application startup**.

### Working with IDs
As of v0.2.0, **all database IDs are `String` types containing UUID v7s**. 
*   Do NOT use `conn.last_insert_rowid()`.
*   Always generate IDs in the application layer and pass them to SQL:
    ```rust
    let new_id = uuid::Uuid::now_v7().to_string();
    ```

### Adding Schema Changes
If you need to change the internal SQLite tables (e.g., adding a column to `_users` or `_tenants`), you must add a migration block in **`apexkit-core/src/migrations.rs`**.

1. Increment the target `user_version` check.
2. Write raw SQL `ALTER TABLE` or `CREATE TABLE` commands.
3. Update the `setup_*` initialization functions in `apexkit-core/src/lib.rs` for *new* users.

---

## 6. Coding Guidelines

### Rust (Backend)
1. **Formatting**: Always format your code before committing.
   ```bash
   cargo fmt
   ```
2. **Linting**: Keep the code clean of warnings.
   ```bash
   cargo clippy -- -D warnings
   ```
3. **Error Handling**: Use the internal `AppError` enum for API routes. Do not `.unwrap()` or `panic!()` in production paths; use `?` and bubble errors up gracefully.
4. **Async Blocking**: If you are calling heavy CPU operations (like Candle ML generation or heavy file I/O), wrap it in `tokio::task::spawn_blocking` to prevent starving the async runtime.

### React (Frontend)
1. **Component Structure**: Place generic UI elements in `components/ui/` and feature-specific components in `features/<name>/components/`.
2. **State Management**: Use `Zustand` for global state. Do not overuse React Context unless absolutely necessary.
3. **Styling**: Use Tailwind CSS. Stick to CSS variables for colors (e.g., `bg-background`, `text-primary`) to maintain Dark/Light mode compatibility.

---

## 7. Submitting a Pull Request

When you are ready to share your code:

1. **Commit your changes** using conventional commit messages:
   * `feat: added custom model support`
   * `fix: resolved gRPC disconnection panic`
   * `docs: updated API swagger documentation`
2. **Push your branch** to your fork.
3. **Open a Pull Request** against the `main` branch.
4. **PR Checklist**:
   * [ ] I have run `cargo fmt` and `cargo clippy`.
   * [ ] I have tested my changes locally.
   * [ ] If I changed the frontend, I ran `npm run build` and committed the resulting static assets (if required by the maintainer workflow).
   * [ ] I have updated relevant documentation / OpenAPI annotations (`utoipa`).

### Review Process
A maintainer will review your PR. If changes are requested, please push them to the same branch. Once approved, the maintainer will squash and merge your contribution!

***

**Thank you for helping make ApexKit better! ⚡️**