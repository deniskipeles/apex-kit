# 📚 Field Types & Schema Reference

**Version:** 0.1.0  
**Context:** Data Modeling, Schema Constraints, Validation & Indexing Rules

ApexKit utilizes a strongly typed, schema-driven validation engine. Schemas define the structure of the JSONB data payload, enforce validation constraints at the API gateway, configure indexing strategies across SQLite, Tantivy, and HNSW vector engines, and establish relational graph edges.

---

## 1. Schema Structure

A collection schema is composed of standard fields (`fields`), graph relations (`relations`), composite unique constraints (`composite_unique`), and access control policies (`policies`):

```json
{
  "fields": {
    "title": {
      "type": "string",
      "required": true,
      "min_length": 3,
      "max_length": 120,
      "sql_indexed": true,
      "ose_indexed": true,
      "uid": "f1a2b3c4"
    },
    "content": {
      "type": "text",
      "vectorize": true,
      "uid": "d5e6f7a8"
    },
    "author_id": {
      "type": "owner",
      "required": true,
      "auto": true,
      "uid": "b9c0d1e2"
    }
  },
  "relations": {
    "category_id": {
      "target_collection": "categories",
      "relation_type": "one",
      "sql_indexed": true,
      "cascade_on_target_delete": true,
      "uid": "r3e4w5q6"
    }
  },
  "composite_unique": [
    ["title", "author_id"]
  ],
  "policies": {
    "read": "public",
    "create": "auth",
    "update": "admin || owner:author_id",
    "delete": "admin"
  }
}
```

---

## 2. Standard Field Types (`schema.fields`)

### A. Primitive Data Types

| Type | Stored Format | Description | Supported Validation Constraints |
| :--- | :--- | :--- | :--- |
| **`string`** | String (UTF-8) | Short text (e.g. titles, slugs, names). | `required`, `unique`, `min_length`, `max_length`, `pattern`, `sql_indexed`, `ose_indexed` |
| **`text`** | String (UTF-8) | Long-form text or Markdown content. | `required`, `min_length`, `max_length`, `vectorize`, `ose_indexed` |
| **`number`** | Number (f64) | 64-bit integer or floating point number. | `required`, `unique`, `min`, `max`, `sql_indexed` |
| **`boolean`** | Boolean | `true` or `false` toggle. | `required`, `default`, `sql_indexed` |
| **`date`** | String (ISO 8601) | Timestamps (e.g. `2026-06-15T12:00:00Z`). | `required`, `auto` (Injects current UTC ISO time on create), `sql_indexed` |
| **`json`** | Object / Array | Arbitrary dynamic JSON objects or arrays. | `required`, `default` |

---

### B. Extended & Validated Formats

| Type | Validation Rule | Description |
| :--- | :--- | :--- |
| **`email`** | Regex `^[\w\-\.]+@([\w-]+\.)+[\w-]{2,4}$` | Validates standard email addresses. |
| **`url`** | `url::Url` Parser | Validates structured HTTP or HTTPS URLs. |
| **`select`** | `options: string[]` | Enforces that values must match one of the predefined string options. |
| **`blob`** | Base64 Decoder | Validates raw Base64 data strings (supports `max_size` in bytes). |
| **`geopoint`** | `{ "lat": f64, "lng": f64 }` | Enforces latitude (`-90.0` to `90.0`) and longitude (`-180.0` to `180.0`). Accepts `lon` or `lng`. |

---

### C. Relational & Identity Types

| Type | Stored Format | Description |
| :--- | :--- | :--- |
| **`owner`** | Number / String ID | Links directly to an authenticated user ID (`auth.id`). Supports `auto: true` to inject current user ID on create. |
| **`relation`** | Number / String ID | Direct foreign key referencing an entity in another collection. Supports `relation_to`. |

---

### D. AI & Vector Embeddings

| Type | Configuration | Description |
| :--- | :--- | :--- |
| **`vector`** | `dimension: number` | Pre-computed floating point embedding array (`number[]`). Validates array length against `dimension`. |
| **`vectorize: true`** | Attribute flag | When placed on a `text` or `file` field, ApexKit automatically generates high-dimensional vector embeddings on record mutations. |

---

## 3. Relationships (`schema.relations`)

Relationships define graph edges between collections and populate the internal `_relations` table for `?expand=` joins:

```json
"relations": {
  "post_comments": {
    "target_collection": "comments",
    "relation_type": "many",
    "required": false,
    "sql_indexed": true,
    "cascade_on_target_delete": true,
    "uid": "rel_7a8b9c"
  }
}
```

### Configuration Attributes
* **`target_collection`**: Name or numeric ID of the destination collection.
* **`relation_type`**:
  * `"one"`: Enforces a scalar ID and expands as a single JSON Object.
  * `"many"`: Resolves reverse lookups as an Array of Objects.
* **`sql_indexed`**: Creates a SQLite B-Tree index on the relation key.
* **`cascade_on_target_delete`**: Automatically deletes dependent records when the target record is deleted.
* **`target_index`**: Immutable UUID of the target collection for cross-environment migrations.

---

## 4. Indexing Configurations

| Index Flag | Target Engine | Purpose & Use Cases |
| :--- | :--- | :--- |
| **`sql_indexed: true`** | SQLite B-Tree | Speeds up exact equality checks, range queries, and sorting (`?sort=-price`, `?filter={"status":"active"}`). |
| **`ose_indexed: true`** | Tantivy Full-Text Engine | Enables fast, typo-tolerant search and prefix autocomplete (`/instant-search?q=query`). Uses `en_stem` analyzer. |
| **`vectorize: true`** | HNSW Vector Engine | Computes embeddings for semantic similarity (`/search-vector-with-text`, `/search-image-vector-with-image`). |

---

## 5. Validation Constraints Reference

```json
{
  "price": {
    "type": "number",
    "required": true,
    "unique": false,
    "min": 0.01,
    "max": 10000.00
  },
  "slug": {
    "type": "string",
    "required": true,
    "unique": true,
    "min_length": 3,
    "max_length": 100,
    "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$"
  }
}
```

### Constraints Checklist:
* **`required`** (`boolean`): Field cannot be omitted or `null`.
* **`unique`** (`boolean`): Enforces single-field uniqueness via `_unique_values`.
* **`auto`** (`boolean`): Strictly allowed **only** on `owner` (current user ID) and `date` (current UTC ISO timestamp).
* **`min` / `max`** (`number`): Numerical boundary range.
* **`min_length` / `max_length`** (`number`): String or text character limits.
* **`pattern`** (`string`): Validates string against a custom regular expression pattern.
* **`options`** (`string[]`): Permitted values for `select` fields.
* **`mime_types`** (`string[]`): Allowed MIME types for `file` fields.
* **`max_size`** (`number`): Maximum size limit in bytes for `file` and `blob` fields.

---

## 6. Multi-Field Unique Constraints (`composite_unique`)

Enforce uniqueness across combinations of fields:

```json
"composite_unique": [
  ["workspace_id", "slug"],
  ["user_id", "post_id"]
]
```

Any operation attempting to insert or update a duplicate combination is rejected with a `422 Unprocessable Entity` error.

---

## 7. Migration & Field History (`field_history`)

Every field maintains a unique 8-character hex string (`uid`). When a field is renamed in the schema, ApexKit matches the `uid` against the previous schema version and automatically runs SQLite JSON migrations (`json_set` and `json_remove`) across all existing records without data loss.
