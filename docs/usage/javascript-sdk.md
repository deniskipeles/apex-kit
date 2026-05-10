# JavaScript SDK Reference

The official ApexKit SDK is a lightweight, TypeScript-first library for interacting with the ApexKit API.

## Installation

```bash
npm install @apexkit/sdk
```

## Initialization

```javascript
import { ApexKit } from '@apexkit/sdk';

// Initialize with base URL
const apex = new ApexKit('http://localhost:5000');
```

## Authentication (`apex.auth`)

| Method | Description |
| :--- | :--- |
| `login(email, password)` | Authenticate and store the token. |
| `register(email, password, metadata)` | Create a new user account. |
| `getMe()` | Fetch current user profile. |
| `logout()` | Clear current session and token. |
| `loginWithGithub(redirect)` | Trigger GitHub OAuth flow. |
| `loginWithGoogle(redirect)` | Trigger Google OAuth flow. |

## Collections (`apex.collection(name)`)

| Method | Description |
| :--- | :--- |
| `list(options)` | Fetch records with pagination/filter/expand. |
| `get(id, options)` | Fetch a single record. |
| `create(data)` | Create a new record. |
| `update(id, data)` | Replace a record entirely. |
| `patch(id, data)` | Partially update fields. |
| `delete(id)` | Remove a record. |
| `searchTextVector(text, limit)` | Perform AI semantic search. |

## Realtime (`ApexKitRealtimeWSClient`)

The realtime client supports WebSockets for live updates.

```javascript
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

// Subscribe to collection changes
realtime.subscribe({ collectionId: 'posts' });

// Listen for events
realtime.onEvent((msg) => {
    console.log("New Event:", msg.event, msg.payload);
});
```

## GraphQL (`apex.graphql`)

Run raw GraphQL queries against the server.

```javascript
const res = await apex.graphql(`query { posts { id title } }`);
```

## File Storage (`apex.files`)

| Method | Description |
| :--- | :--- |
| `upload(file)` | Upload a file to the storage backend. |
| `list(page, per_page)` | List uploaded files. |
| `getFileUrl(filename)` | Generate a public URL for a file. |

## Multi-Tenancy Scoping

You can point the client to a specific tenant or sandbox dynamically.

```javascript
const tenantClient = apex.tenant('tenant_id_123');
// All requests via tenantClient are now scoped to that tenant
```
