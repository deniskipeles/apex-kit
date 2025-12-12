

# 📚 Field Types & Schema Reference

TinyBase provides a robust typing system allowing you to define the structure, validation, and relationships of your data. This guide covers all supported field types, their validation options, and how to manage schema evolution.

## 💾 Basic Data Types

These are the fundamental building blocks for your data.

### `String`
*   **Description:** Short, single-line text. Ideal for titles, names, or identifiers.
*   **Storage:** Stored as efficient text.
*   **Validation Options:** Min Length, Max Length, Regex Pattern.

### `Text`
*   **Description:** Long-form, multi-line text. Use this for descriptions, blog content, or comments.
*   **Storage:** Stored as variable-length text.
*   **Validation Options:** Min Length, Max Length.

### `Number`
*   **Description:** Numeric values. Supports both Integers (e.g., `42`) and Floating point numbers (e.g., `3.14`).
*   **Validation Options:** Min Value, Max Value.

### `Boolean`
*   **Description:** A binary `true` or `false` toggle.
*   **Usage:** Flags like `is_published`, `is_verified`.

---

## 🔗 Formatted Strings

Specialized string types that include automatic validation logic.

### `Email`
*   **Description:** Validates that the input follows standard email formatting (e.g., `user@example.com`).
*   **Validation:** Automatic Regex check.

### `URL`
*   **Description:** Validates that the input is a properly formatted web link (e.g., `https://tinybase.io`).
*   **Validation:** Automatic URL parsing check.

### `Date`
*   **Description:** Stores date and time points.
*   **Format:** Strictly adheres to **ISO 8601** format (e.g., `2023-11-29T10:00:00Z`).

---

## 🗂 Structured & Complex Types

### `Select`
*   **Description:** Restricts the value to a specific set of predefined options (Enum).
*   **Usage:** Status fields (e.g., `Draft`, `Published`, `Archived`).
*   **Configuration:** Requires a list of **Options**.

### `JSON`
*   **Description:** Stores arbitrary structured data (Arrays or Objects).
*   **Usage:** Configuration settings, raw API responses, or flexible metadata.
*   **Validation:** Ensures the input is valid JSON syntax.

---

## ☁️ Storage & Binary

### `File`
*   **Description:** References a file stored in your configured Storage Backend (Local Disk or S3).
*   **Storage:** Stores the `filename` or storage key string.
*   **Validation Options:**
    *   **Max Size:** Limit file size in bytes.
    *   **MIME Types:** Restrict to specific types (e.g., `image/png`, `application/pdf`).

### `Blob`
*   **Description:** Stores binary data directly in the database record encoded as a **Base64** string.
*   **Usage:** Small thumbnails, cryptographic keys, or small binary payloads.
*   **Validation:** Max Length (character count of the Base64 string).

---

## 🤖 AI & Search

### `Vector`
*   **Description:** Stores an array of floating-point numbers representing a vector embedding.
*   **Usage:** Semantic search, recommendation systems, and AI contexts.
*   **Configuration:**
    *   **Dimension:** (Required) The fixed size of the vector (e.g., `1536` for OpenAI, `768` for HuggingFace models).

---

## 🕸️ Relationships & System

### `Relation`
*   **Description:** Creates a link to a record in another collection.
*   **Configuration:**
    *   **Related Collection:** The ID or Name of the target collection.
*   **Behavior:** When querying, you can expand this field to retrieve the full data of the related record.

### `Owner`
*   **Description:** A specialized relation that links a record to a **User ID**.
*   **Usage:** Security policies. Used in API Rules like `@request.auth.id = owner`.
*   **Validation:** Ensures the value is a valid User ID string.

---

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

---

## 📦 Schema Evolution & Field History

TinyBase includes a **Field History Tracker** to handle schema migrations safely.

### How Renaming Works
When you rename a field in the Admin UI (e.g., changing `username` to `display_name`):

1.  **Tracking:** The system records the change in the schema's `field_history`.
    *   *Example:* `display_name` -> points to history `["username"]`.
2.  **Migration:** This allows future migration scripts to understand that data stored under the key `username` should now be mapped to `display_name`, preventing data loss during schema evolution.

### JSON Schema Example
Here is how a schema definition looks in the API:

```json
{
  "fields": {
    "title": {
      "type": "string",
      "required": true,
      "indexed": true,
      "min_length": 3
    },
    "status": {
      "type": "select",
      "required": false,
      "options": ["draft", "published"]
    },
    "embedding": {
      "type": "vector",
      "required": false,
      "dimension": 1536
    }
  },
  "field_history": {
    "title": ["post_title", "header"]
  }
}
```