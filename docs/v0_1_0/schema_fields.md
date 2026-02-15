# 📚 Field Types & Schema Reference

ApexKit provides a robust typing system allowing you to define the structure, validation, and relationships of your data.

## 💾 Basic Data Types

| Type | Description | Options |
| :--- | :--- | :--- |
| **`string`** | Short text. | `min_length`, `max_length`, `pattern`, `vectorize`, `ose_indexed` |
| **`text`** | Long text/HTML. | `min_length`, `max_length`, `vectorize`, `ose_indexed` |
| **`number`** | Integer/Float. | `min`, `max` |
| **`bool`** | Boolean toggle. | - |
| **`json`** | Structured object/array. | - |
| **`select`** | Enum options. | `options: ["A", "B"]` |
| **`date`** | ISO 8601 Date. | `auto` (Inject current time on create) |

## 🔗 Special Types

| Type | Description | Options |
| :--- | :--- | :--- |
| **`email`** | Validates email format. | - |
| **`url`** | Validates URL format. | - |
| **`file`** | File reference (filename). | `max_size`, `mime_types` |
| **`blob`** | Base64 binary data. | `max_size` |

## 🕸️ Relationships

| Type | Description | Logic |
| :--- | :--- | :--- |
| **`relation`** | Foreign Key. | `relationTo` (Collection Name/ID). Supports `expand` queries. |
| **`owner`** | Link to User ID. | `auto` (Inject current User ID on create). Used for RLS policies. |

## 🤖 AI & Search

| Type | Description | Logic |
| :--- | :--- | :--- |
| **`vector`** | Embedding array. | `dimension` (e.g. 1536). Used for vector search. |

To enable **Vector Search** on a `text` field, set `vectorize: true`. The system will automatically generate embeddings using the configured AI model when records are created or updated.

To enable **Full-Text Search** (Tantivy), set `ose_indexed: true`. This allows high-performance fuzzy searching via the `/search` and `/instant-search` endpoints.

## 🛡️ Validation & Constraints

Every field supports standard constraints to ensure data integrity.

| Constraint | Supported Types | Description |
| :--- | :--- | :--- |
| **Required** | All | The field cannot be `null` or `undefined`. |
| **Unique** | String, Email, Number | No two records can have the same value for this field. |
| **Indexed** | String, Text, Email | Adds the field to the Search Index for high-performance Instant Search. |
| **Min/Max** | Number | Enforces numerical range. |
| **Min/Max Len**| String, Text, Blob | Enforces character count limits. |
| **Pattern** | String, Text, Email | Enforces a custom **Regex** pattern (e.g., `^[A-Z]+$`). |
