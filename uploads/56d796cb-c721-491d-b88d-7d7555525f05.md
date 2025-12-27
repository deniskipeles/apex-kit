# TinyBase Files & Storage API Documentation

**Version:** 2.1
**Base URL:** `http://localhost:5000/api/v1`

The Storage API abstracts file management. Whether you are running TinyBase locally (storing files on disk) or in the cloud (using AWS S3), the API endpoints remain exactly the same.

TinyBase maintains a separate database table (`_storage_files`) to track file metadata (original name, size, uploader) while storing the actual binary data in the configured backend.

---

## 1. The File Object

When listing or uploading files, the API returns a metadata object.

```json
{
  "id": 55,
  "filename": "f47ac10b-58cc-4372-a567-0e02b2c3d479.png",
  "original_name": "profile-pic.png",
  "mime_type": "image/png",
  "size": 204800,
  "url": "http://localhost:5000/api/v1/storage/file/f47ac10b-58cc-4372-a567-0e02b2c3d479.png",
  "created_at": "2023-10-27T10:00:00Z"
}
```

*   **id**: (Integer) Internal System ID. Used for management/deletion.
*   **filename**: (UUID String) The sanitized, unique name of the file on disk/bucket. **This is what you store in your Record `file` fields.**
*   **url**: The fully qualified public URL to access the file.

---

## 2. API Endpoints

### Upload File
Upload a binary file. TinyBase automatically renames the file to a UUID to prevent collisions and sanitizes the extension.

*   **POST** `/storage/upload`
*   **Auth:** Required (Valid User Token).
*   **Content-Type:** `multipart/form-data`

**Form Fields:**

| Field | Type | Description |
| :--- | :--- | :--- |
| `file` | Binary | **Required.** The file content to upload. |

**Example (JavaScript):**
```javascript
const formData = new FormData();
formData.append('file', fileInput.files[0]);

const response = await fetch('http://localhost:5000/api/v1/storage/upload', {
  method: 'POST',
  headers: {
    'Authorization': 'Bearer <TOKEN>'
  },
  body: formData
});
```

**Response:** `201 Created`
Returns the [File Object](#1-the-file-object).

---

### Serve File (Public Access)
Retrieve the raw file content. This endpoint is public and includes appropriate `Content-Type` headers based on the file extension.

*   **GET** `/storage/file/{filename}`
*   **Auth:** Public.

**Parameters:**
*   `filename`: The UUID filename returned during upload (e.g., `f47ac...png`).

**Headers Returned:**
*   `Content-Type`: e.g., `image/png`, `application/pdf`.
*   `Cache-Control`: `public, max-age=...` (Configurable caching).

---

### List Files
Retrieve a paginated list of uploaded files and their metadata.

*   **GET** `/storage/files`
*   **Auth:** Public (Metadata listing).

**Query Parameters:**

| Parameter | Default | Description |
| :--- | :--- | :--- |
| `page` | `1` | Page number. |
| `per_page` | `20` | Items per page. |

**Response:**
```json
{
  "items": [ ...FileObjects... ],
  "total": 150
}
```

---

### Delete File
Permanently remove a file from both the database metadata and the physical storage (Disk/S3).

*   **DELETE** `/storage/files/{id}`
*   **Auth:** **Admin Only.**

**Parameters:**
*   `id`: The **Integer ID** of the file (not the filename).

**Response:** `204 No Content`

---

## 3. Linking Files to Data Records

To associate a file with a data record (e.g., a User Profile or a Blog Post cover image), use the **Record API**.

1.  **Schema Definition:**
    Ensure your collection has a field of type `file` or `string`.
    ```json
    "avatar": { "type": "file" }
    ```

2.  **Workflow:**
    1.  **Upload:** Call `POST /storage/upload`. Get the `filename` from the response (e.g., `abc-123.jpg`).
    2.  **Link:** Create or Update the Record, saving the filename string.

**Example Request (Update User Profile):**
`PATCH /collections/profiles/records/5`

```json
{
  "data": {
    "full_name": "Alice",
    "avatar": "abc-123.jpg" 
  }
}
```

3.  **Retrieving:**
    When you fetch the record, you get the filename string.
    **Frontend Logic:**
    ```javascript
    const imageUrl = `http://localhost:5000/api/v1/storage/file/${record.data.avatar}`;
    ```

---

## 4. Configuration (Server Side)

The storage backend is defined at the environment level (`.env`) when running the TinyBase binary.

### Local Storage (Default)
Files are stored in the `./uploads` directory relative to the binary.
```bash
STORAGE_TYPE="local"
```

### AWS S3 (or Compatible)
Offload storage to the cloud (AWS, DigitalOcean Spaces, MinIO, Google Cloud Storage).

```bash
STORAGE_TYPE="s3"
S3_BUCKET="my-app-assets"
S3_REGION="us-east-1"
S3_PUBLIC_URL="https://my-app-assets.s3.amazonaws.com/" 
# Credentials usually picked up from AWS_ACCESS_KEY_ID env vars or IAM roles
```

When `s3` is enabled:
1.  **Uploads:** Stream directly to the bucket.
2.  **Serving:** The `/storage/file/{filename}` endpoint effectively proxies the data (or you can construct the `S3_PUBLIC_URL` + `filename` on the client to bypass the server completely).