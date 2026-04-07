# @apexkit/sdk Documentation

Welcome to the `@apexkit/sdk` documentation. This SDK provides a type-safe, powerful interface to interact with ApexKit's REST, GraphQL, and Real-time APIs.

## Table of Contents

- [Getting Started](README.md)
  - [Installation](README.md#installation)
  - [Initialization](README.md#initialization)
  - [Multi-Tenancy & Scoping](README.md#multi-tenancy--scoping)
- [Authentication](auth.md)
  - [Login / Register](auth.md#login--register)
  - [OAuth (GitHub/Google)](auth.md#oauth)
  - [Identity](auth.md#identity)
- [Collections & Records](collections.md)
  - [CRUD Operations](collections.md#crud-operations)
  - [Querying & Filtering](collections.md#querying--filtering)
  - [Instant Search](collections.md#instant-search)
  - [Vector Search (AI)](collections.md#vector-search)
- [Real-time API](realtime.md)
  - [WebSocket Client](realtime.md#websocket-client)
  - [SSE Client](realtime.md#sse-client)
- [AI & Scripts](ai-scripts.md)
  - [AI Actions](ai-scripts.md#ai-actions)
  - [Architect (AI Sessions)](ai-scripts.md#architect)
  - [Scripts & Templates](ai-scripts.md#scripts)
- [Admin & System](admin-system.md)
  - [Collection Management](admin-system.md#collections)
  - [User Management](admin-system.md#users)
  - [Storage & Files](admin-system.md#storage)
  - [Backups & API Keys](admin-system.md#backups)
- [API Reference (Types)](types.md)

## Installation

```bash
npm install @apexkit/sdk
# or
yarn add @apexkit/sdk
# or
pnpm add @apexkit/sdk
```

## Initialization

The main entry point is the `ApexKit` class.

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');

// Set an existing token if available
apex.setToken('YOUR_JWT_TOKEN');
```

### Constructor

`new ApexKit(baseUrl: string, scopeType: ScopeType = 'root', scopeId: string = '')`

- `baseUrl`: The base URL of your ApexKit instance.
- `scopeType`: Defaults to `'root'`. Can also be `'tenant'` or `'sandbox'`.
- `scopeId`: The identifier for the tenant or sandbox.

## Multi-Tenancy & Scoping

ApexKit is designed for multi-tenancy from the ground up. You can easily switch contexts using the `tenant()` and `sandbox()` methods.

```javascript
// Target a specific tenant
const tenant = apex.tenant('client-alpha');

// Target a sandbox session
const sandbox = apex.sandbox('session-uuid-123');

// List records within the tenant's context
const records = await tenant.collection('posts').list();
```

### Scoping Methods

- `apex.tenant(tenantId: string): ApexKit`: Returns a new `ApexKit` instance scoped to the specified tenant.
- `apex.sandbox(uuid: string): ApexKit`: Returns a new `ApexKit` instance scoped to the specified sandbox session.
- `apex.scope`: Getter that returns the current `Scope` object `{ type, id }`.
