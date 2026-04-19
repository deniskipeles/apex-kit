# ApexKit Documentation (v0.1.0)

Welcome to the ApexKit documentation. ApexKit is a monolithic, single-binary Backend-as-a-Service designed for speed, simplicity, and scalability.

## Table of Contents

### Getting Started
- [Quick Start Guide](quick_dev_documentation.md) - Get up and running in minutes.
- [Full Developer Documentation](full_dev_documentation.md) - Deep dive into architecture and core concepts.
- [Sandbox & Multi-tenancy](sandbox-and-multitenancy-api.md) - Understanding isolated environments.

### Core API
- [Collections](collections-api.md) - Creating and managing collections.
- [Records](records-api.md) - Standard CRUD operations.
- [Filtering Records](records-filters-api.md) - Advanced query syntax and filtering.
- [Relations & Expansion](records-relations-api.md) - Working with related data.
- [Schema & Fields](schema_fields.md) - Field types and validation rules.

### Advanced Features
- [Authentication](users-api.md) - Login, registration, and user management.
- [Security Policies](policies.md) - Fine-grained access control (ABAC).
- [Files & Storage](files-api.md) - Uploading and managing assets.
- [Real-time Subscriptions](realtime-api.md) - WebSocket and SSE integration.
- [Custom Events](realtime-custom-events.md) - Broadcasting custom real-time messages.
- [GraphQL API](graphql-api.md) - Using the built-in GraphQL interface.
- [Cron Jobs](cron-jobs-api.md) - Scheduling background tasks.

### Logic & Scripting
- [Scripting Engine](scripting_engine.md) - Overview of the server-side JS runtime.
- [Database Tool ($db)](scripting-db-tool.md) - Interacting with data from scripts.
- [Edge Tooling ($http, $util)](scripting-public-edge-tooling.md) - External requests and utilities.
- [System Commands ($cmd)](scripting-tool-cmd.md) - Running shell commands (Root only).
- [Hooks Guide](hooks-guide.md) - Custom logic for collection lifecycle events.
- [Script Samples](scripts-samples.md) - Real-world examples of server-side logic.
- [Templating Engine](templating_engine.md) - Server-side rendering with Tera.

### AI Integration
- [AI Actions](ai-actions.md) - Executing LLM prompts and image analysis.
- [AI Metadata](ai-actions-metadata.md) - Configuring AI processing for collections.

### Infrastructure & Operations
- [Replication](replication.md) - Master-Replica setup for high availability.
- [Write Optimizations](write-optimizations.md) - Performance tuning for high-throughput apps.
- [Microframe JS](microframe-js-documentation.md) - Lightweight frontend integration.

## SDK Reference

ApexKit provides an official TypeScript/JavaScript SDK: `@apexkit/sdk`.

- [SDK README](../sdk/README.md)
- [Installation](../sdk/README.md#installation)
- [Authentication](../sdk/auth.md)
- [Collections](../sdk/collections.md)
- [Real-time](../sdk/realtime.md)
