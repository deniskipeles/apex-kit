**Version:** 0.1.0
**Base URL:** `https://api.your-app.com/api/v1`

In ApexKit, a **Collection** is the fundamental container for your data. It functions like a Table in SQL or a Collection in MongoDB, defining the **Schema** (fields), **Validation Rules**, and **Security Policies** for the records it holds.

---

## 1. The Collection Object

A collection is defined by its name and a configuration schema.

```json
{
  "id": "5",
  "name": "blog_posts",
  "index": "cbaa8fa3-85db-4a69-b7d2-dcda99dbd4d8",
  "schema": {
    "fields": {
      "title": { "type": "string", "required": true, "ose_indexed": true },
      "content": { "type": "text", "vectorize": true },
      "author_id": { "type": "owner", "required": true }
    },
    "policies": {
      "read": "public",
      "create": "auth",
      "update": "auth.id == field:author_id",
      "delete": "admin"
    },
    "relations": {
        "category": {
            "target_collection": "categories",
            "relation_type": "one"
        }
    },
    "composite_unique": [
        ["title", "author_id"]
    ]
  }
}
```

*   **name**: The unique identifier used in API URLs.
*   **index**: A stable UUID used to maintain relationship integrity across different environments (e.g., migrating from Sandbox to Production).
*   **schema.fields**: Definitions for standard data fields.
*   **schema.relations**: Explicit relational links to other collections.
*   **schema.policies**: Access control rules for CRUD operations.
*   **schema.composite_unique**: Arrays of field names that must be unique as a combination.

---

## 2. API Endpoints

All collection management endpoints require **Admin** privileges.

### List Collections
Retrieve all schemas in the current scope.
*   **GET** `/collections`

### Get Collection
*   **GET** `/collections/{id_or_name}`

### Create Collection
*   **POST** `/collections`
*   **Body**: `{ "name": "string", "schema": { ... } }`

### Update Collection
*   **PATCH** `/collections/{id_or_name}`
*   **Body**: Partial updates to name or schema.

### Delete Collection
Permanently removes the collection, all associated records, and search indexes.
*   **DELETE** `/collections/{id_or_name}`

---

## 3. Field Types & Validation

ApexKit enforces strict types at the API entry point.

| Type | Purpose | Key Options |
| :--- | :--- | :--- |
| **`string`** | Short text | `min_length`, `max_length`, `pattern` (Regex) |
| **`text`** | Long-form content | `vectorize` (Enable AI search) |
| **`number`** | Integers or Decimals | `min`, `max` |
| **`bool`** | True/False | - |
| **`email`** | Email validation | - |
| **`url`** | Web link validation | - |
| **`date`** | ISO 8601 Timestamp | `auto` (Set on create) |
| **`select`** | Enum selection | `options` (Array of strings) |
| **`json`** | Dynamic objects | - |
| **`file`** | Reference to storage | `max_size`, `mime_types` |
| **`relation`** | Link to another record | `relation_to` (Target collection) |
| **`owner`** | Link to system User | `auto` (Set to current user) |

---

## 4. Advanced Schemas

### Relational Integrity
When a field is defined as a `relation`, ApexKit manages an internal graph table (`_relations`). 
*   **Expanding**: Use `?expand=field_name` in Record queries to join data automatically.
*   **Cleanup**: When a record is deleted, all relationship edges pointing to it are automatically purged.

### Search Indexing
*   **OSE Index (`ose_indexed`)**: If enabled, the field is added to the Tantivy full-text index for fast fuzzy searching.
*   **Vector Search (`vectorize`)**: If enabled, ApexKit generates a 384+ dimension embedding for the field content, enabling semantic "meaning-based" search.

### Composite Uniqueness
You can prevent duplicate combinations across multiple fields. For example, to ensure a user can only like a post once:
```json
"composite_unique": [ ["user_id", "post_id"] ]
```

---

## 5. Multi-Tenancy Behavior

Collections are **physically isolated**. 
1.  **Root App**: Collections are stored in `storage/system/data.db`.
2.  **Tenants**: Each tenant gets a unique `data.db` in their own folder.
3.  **Cross-Access**: A script in Tenant A **cannot** access or query a collection in Tenant B.

---

## 6. JavaScript SDK Usage

Management of collections is handled via the `admins` namespace.

```javascript
import { apex } from './apiClient';

// 1. List all collections
const cols = await apex.admins.listCollections();

// 2. Create a new collection
const newCol = await apex.admins.createCollection("products", {
    fields: {
        sku: { type: "string", required: true, unique: true },
        price: { type: "number", min: 0 }
    }
});

// 3. Update a schema
await apex.admins.patchCollection(newCol.id, {
    schema: {
        fields: {
            ...newCol.schema.fields,
            stock: { type: "number", default: 0 }
        }
    }
});
```