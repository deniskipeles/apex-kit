# API Reference (Types)

Detailed interface and type definitions for `@apexkit/sdk`.

---

## Core Types

### ScopeType
```typescript
type ScopeType = 'root' | 'tenant' | 'sandbox';
```

### Scope
```typescript
interface Scope {
    type: ScopeType;
    id: string;
}
```

### User
```typescript
interface User {
    id: string;
    email: string;
    role: string;
    scope: string;
    metadata?: Record<string, any>;
    last_active?: string;
    [key: string]: any;
}
```

---

## Records & Queries

### BaseRecord
```typescript
interface BaseRecord {
    id: string;
    created: string;
    updated: string;
    [key: string]: any;
}
```

### ListResult<T>
```typescript
interface ListResult<T> {
    items: T[];
    total: number;
    page?: number;
    per_page?: number;
}
```

### QueryOptions
```typescript
interface QueryOptions {
    page?: number;
    per_page?: number;
    sort?: string;
    filter?: string | Record<string, any>;
    expand?: string;
    fields?: string;
    [key: string]: any;
}
```

---

## Collections & Schema

### Collection
```typescript
interface Collection {
    id: string;
    name: string;
    type: string;
    schema: {
        fields: Record<string, SchemaField>;
        relations?: Record<string, any>;
        policies?: {
            read: string;
            create: string;
            update: string;
            delete: string;
        };
    };
    created: string;
    updated: string;
}
```

### SchemaField
```typescript
interface SchemaField {
    name: string;
    type: string;
    required: boolean;
    unique?: boolean;
    options?: string[];
    [key: string]: any;
}
```

---

## Real-time Types

### SubscriptionFilter
```typescript
interface SubscriptionFilter {
    collectionId?: number;
    recordId?: number;
    eventType?: string;
    dataFilter?: Record<string, any>;
    channel?: string;
    customEvent?: string;
}
```

---

## Storage & Files

### StoredFile
```typescript
interface StoredFile {
    id: string;
    filename: string;
    original_name: string;
    mime_type: string;
    size: number;
    url: string;
    created_at: string;
}
```

### SiteFile
```typescript
interface SiteFile {
    path: string;
    size: number;
}
```

---

## AI & Scripts

### AiAction
```typescript
interface AiAction {
    id: string;
    slug: string;
    name: string;
    model: string;
    system_prompt?: string;
    template: string;
    config?: any;
}
```

### AiSession
```typescript
interface AiSession {
    id: string;
    name: string;
    messages: Array<{ role: 'user' | 'assistant'; content: string }>;
    current_manifest?: any;
    diff_summary?: string;
    last_error?: string;
    created_at: string;
}
```

### Plugin
```typescript
interface Plugin {
    id: string;
    name: string;
    version: string;
    description?: string;
    manifest: any;
    created_at: string;
}
```

### Script
```typescript
interface Script {
    id: string;
    name: string;
    trigger_type: string;
    code: string;
    active: boolean;
    target_collection?: string;
}
```

---

## Admin & System

### ApiKey
```typescript
interface ApiKey {
    id: string;
    name: string;
    prefix: string;
    role: string;
    scope: string;
    bypass_cors: boolean;
    created_at: string;
}
```

### SystemLog
```typescript
interface SystemLog {
    id: string;
    level: 'info' | 'warning' | 'error' | 'success';
    message: string;
    source: string;
    timestamp: string;
    meta?: any;
}
```
