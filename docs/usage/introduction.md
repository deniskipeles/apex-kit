# Introduction to ApexKit

Welcome to the **ApexKit** documentation. ApexKit is an AI-native, all-in-one Backend-as-a-Service (BaaS) designed to accelerate development for modern applications.

## What is ApexKit?

ApexKit is a single-binary backend solution built with **Rust**. It provides everything you need to build a scalable application, including:

- **Database**: High-performance SQLite with automatic REST and GraphQL APIs.
- **Authentication**: Built-in JWT-based auth with support for social providers.
- **AI-Native**: Native support for LLMs, Vector search, and AI-driven schema generation.
- **Edge Scripting**: Run JavaScript logic on the server using a sandboxed engine.
- **Realtime**: WebSockets and SSE for instant data updates.
- **Multi-tenancy**: First-class support for isolated tenant environments.
- **Admin Dashboard**: A beautiful, React-based dashboard to manage your entire stack.

## Why ApexKit?

While tools like Firebase and Supabase are excellent, ApexKit focuses on a few key differentiators:

1. **Portability**: The entire backend compiles into a single binary. No complex setup or multiple services to manage.
2. **AI First**: Unlike other platforms where AI is an afterthought, ApexKit integrates Vector search and LLM orchestration into the core.
3. **Rust Performance**: Built on the Axum framework and SQLite, providing incredible speed and low memory footprint.
4. **Local-First, Cloud-Ready**: Perfect for local development and edge deployments.

## Core Concepts

### Collections
Collections are the heart of ApexKit. They define the structure of your data. When you create a collection, ApexKit automatically generates:
- RESTful endpoints (`/api/v1/collections/posts/records`)
- A GraphQL schema
- Search indexes (Full-text and Vector)

### Sandboxes and Tenants
ApexKit allows you to create isolated environments within a single instance.
- **Tenants**: Perfect for B2B SaaS apps where each customer needs their own isolated database.
- **Sandboxes**: Temporary environments for testing features or letting AI draft new schemas without affecting production.

### The AI Architect
The built-in AI Architect allows you to describe your application in natural language. It can generate schemas, write edge functions, and even create frontend templates for you.

## Next Steps

- **[Getting Started](./getting-started.md)**: Install and run your first ApexKit instance.
- **[Framework Guides](./framework-react.md)**: Learn how to integrate ApexKit with your favorite frontend framework.
- **[App Examples](./examples-overview.md)**: Explore 10+ real-world app architectures built on ApexKit.
