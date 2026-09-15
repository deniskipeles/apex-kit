# 📚 Collections API Documentation

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

In ApexKit, a **Collection** represents a database table schema and its associated access boundaries. Collections define field types, constraints, relational links, search indexing modes, and security policies (Row-Level Security / RLS).

Each collection maintains structural metadata and indexes while records are stored in high-performance SQLite JSONB format with automatic migration tracking.

---

## 1. The Collection Object

Collections are identified by both a numeric ID (`id`) and a unique system name (`name`). They also carry an immutable `index` UUID to preserve relationship integrity across deployments (e.g. migrating from a Sandbox to Production).

```json
{
  "id": 5,
  "name": "blog_posts",
  "index": "cbaa8fa3-85db-4a69-b7d2-dcda99dbd4d8",
  "schema": {
    "fields": {
      "title": {
        "type": "string",
        "required": true,
        "ose_indexed": true,
        "sql_indexed": true,
        "min_length": 3,
        "max_length": 150,
        "uid": "a1b2c3d4"
      },
      "content": {
        "type": "text",
        "vectorize": true,
        "uid": "e5f6g7h8"
      },
      "author_id": {
        "type": "owner",
        "required": true,
        "auto": true,
        "uid": "i9j0k1l2"
      }
    },
    "relations": {
      "category": {
        "target_collection": "categories",
        "relation_type": "one",
        "required": false,
        "sql_indexed": true,
        "cascade_on_target_delete": false,
        "uid": "m3n4o5p6"
      }
    },
    "policies": {
      "read": "public",
      "create": "auth",
      "update": "admin || owner:author_id",
      "delete": "admin"
    },
    "composite_unique": [
      ["title", "author_id"]
    ]
  }
}
```

### Core Properties
* **`id`**: Numeric primary key (`INTEGER`).
* **`name`**: Alphanumeric identifier (`^[a-zA-Z0-9_]+$`) used in REST/GraphQL routes.
* **`index`**: Stable UUID used for cross-environment schema syncing and relational imports.
* **`schema.fields`**: Map of standard field definitions.
* **`schema.relations`**: Explicit graph links to other collections.
* **`schema.policies`**: Access control rules for `read`, `create`, `update`, and `delete`.
* **`schema.composite_unique`**: Arrays of field names that must be unique as a combination.

---

## 2. API Endpoints

All collection schema modifications require **Admin** credentials (`role: "admin"`). Endpoint paths accept either the collection **numeric ID** or **name**.

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/collections` | List all collections in the current scope. |
| `GET` | `/collections/{id_or_name}` | Fetch a single collection schema. |
| `POST` | `/collections` | Create a new collection. |
| `PATCH` | `/collections/{id_or_name}` | Partially update schema or rename collection. |
| `PUT` | `/collections/{id_or_name}` | Replace schema definition. |
| `DELETE` | `/collections/{id_or_name}` | Delete collection, records, relations, and search index. |
| `POST` | `/admin/collections/{id_or_name}/reindex` | Enqueue background search index rebuild (Tantivy). |

---

## 3. Schema Fields (`schema.fields`)

Every field in `schema.fields` requires a unique 8-character hex `uid` (auto-generated if omitted) used to track renames and migrations.

### Supported Field Types

| Type | Description | Key Options |
| :--- | :--- | :--- |
| **`string`** | Short text. | `min_length`, `max_length`, `pattern` (Regex), `unique` |
| **`text`** | Multi-line text / Markdown. | `min_length`, `max_length`, `vectorize`, `ose_indexed` |
| **`number`** | Integer or float. | `min`, `max`, `unique` |
| **`boolean`** | True / false flag. | `default` |
| **`email`** | Formatted email address. | `unique` |
| **`url`** | Valid HTTP/HTTPS URL. | `unique` |
| **`date`** | ISO 8601 Timestamp. | `auto` (Injects UTC timestamp on create) |
| **`select`** | Enum string. | `options: ["draft", "published", "archived"]` |
| **`json`** | Arbitrary object or array. | `default` |
| **`file`** | File reference filename. | `max_size`, `mime_types`, `vectorize` |
| **`blob`** | Raw Base64 string. | `max_size` |
| **`geopoint`** | Geographic coordinate object. | `{ "lat": 40.7, "lng": -74.0 }` |
| **`owner`** | User ID (`auth.id`) reference. | `auto` (Injects current user ID on create) |
| **`vector`** | Embedding array (`Vec<f32>`). | `dimension` (e.g. `384` or `768`) |

> **Security Rule on `auto` Fields**: In the current engine, `auto: true` is strictly permitted **only** on `owner` and `date` field types. If passed on any other type, it is automatically neutralized to `false`.

---

## 4. Relationships (`schema.relations`)

Relationships define directional foreign-key edges to another collection and populate the internal `_relations` table.

```json
"relations": {
  "author": {
    "target_collection": "authors",
    "relation_type": "one",
    "required": true,
    "sql_indexed": true,
    "cascade_on_target_delete": true
  }
}
```

### Relational Configuration Options
* **`target_collection`**: Name or ID of the destination collection.
* **`relation_type`**:
  * `"one"`: Enforces a scalar ID (e.g. `101`) and expands to a single JSON Object.
  * `"many"`: Forward scalar ID or reverse lookup expanding to an Array of Objects.
* **`sql_indexed`**: Automatically builds a SQLite B-Tree index on the foreign key for instant queries.
* **`cascade_on_target_delete`**: When `true`, deleting a record in the target collection will recursively delete this record as well.

---

## 5. Indexing & Acceleration Options

Collections support three distinct indexing engines:

### A. SQL B-Tree Indexes (`sql_indexed: true`)
Creates an optimized SQLite index on `json_extract(data, '$.field')`:
```sql
CREATE INDEX idx_col_5_a1b2c3d4 ON records (json_extract(data, '$.title')) WHERE collection_id = 5;
```
*Best for: Exact filters, ranges, and sorting (`?sort=-price`, `?filter={"status":"active"}`).*

### B. Tantivy Search Engine (`ose_indexed: true`)
Indexes the field in the embedded **Tantivy** search engine with English stemming (`en_stem`).
*Best for: Fuzzy typo-tolerant search and instant autocomplete via `/instant-search`.*

### C. AI Vector Search (`vectorize: true`)
Automatically triggers background embedding generation when records are inserted or updated.
*Best for: Semantic similarity searches via `/search-vector-with-text` and `/search-image-vector-with-image`.*

---

## 6. Composite Constraints & Validation

### Multi-Field Unique Constraints (`composite_unique`)
To enforce uniqueness across multiple columns simultaneously:

```json
"composite_unique": [
  ["tenant_id", "slug"],
  ["organization_id", "user_id"]
]
```

ApexKit maintains an internal `_unique_values` table. Any transaction that violates a composite constraint returns an immediate `422 Unprocessable Entity` response.

---

## 7. Schema Migration Engine

When updating a collection schema with `PATCH /collections/{id}`, ApexKit inspects the `uid` of each field to perform automated migrations:

1. **Field Renaming**: If a field's `uid` matches an existing field but the key has changed, ApexKit executes an atomic SQLite JSON update:
   ```sql
   UPDATE records 
   SET data = json_remove(json_set(data, '$.new_name', json_extract(data, '$.old_name')), '$.old_name')
   WHERE collection_id = ?;
   ```
2. **Field Deletion**: Removed fields are stripped from the record JSON with `json_remove`.
3. **Index Reconciliation**: Drops obsolete B-Tree indexes and constructs newly requested indexes.
4. **Automatic Scope Reload**: Triggers an in-memory invalidation and hot-reloads the dynamic GraphQL schema for the current scope without server restarts.

---

## 8. SDK Examples

### TypeScript SDK (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('ADMIN_JWT_OR_API_KEY');

// 1. Create a Collection with strict validation & indexes
const articles = await apex.admins.createCollection('articles', {
  fields: {
    title: {
      type: 'string',
      required: true,
      sql_indexed: true,
      ose_indexed: true,
      min_length: 5
    },
    body: {
      type: 'text',
      vectorize: true
    },
    author_id: {
      type: 'owner',
      auto: true,
      required: true
    }
  },
  relations: {
    category_id: {
      target_collection: 'categories',
      relation_type: 'one',
      sql_indexed: true
    }
  },
  policies: {
    read: 'public',
    create: 'auth',
    update: 'admin || owner:author_id',
    delete: 'admin'
  }
});

console.log(`Created collection ID: ${articles.id}`);

// 2. Rebuild the search index in the background
await apex.admins.reIndex(articles.id);
```

### In-Script Usage (`$db` / `$apex`)

Inside edge scripts or webhooks:

```typescript
// Webhook: ./webhooks/init-setup.ts
export default async function (req: Request) {
    // List all collections in current tenant scope
    const collections = await $db.collections.list();
    
    return new Response({
        count: collections.length,
        collections: collections.map(c => c.name)
    });
}
```