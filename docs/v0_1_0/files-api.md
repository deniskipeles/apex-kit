# ☁️ Files & Storage API Documentation

**Version:** 0.1.0
**Base URL:** `https://api.your-app.com/api/v1`

ApexKit provides a unified Storage API that abstracts file management. Whether your instance is configured for **Local Storage** (disk) or **Cloud Storage** (AWS S3/R2), the API endpoints and script methods remain identical.

---

## 1. The File Object

When you upload or list files, ApexKit returns a metadata object. The `filename` is a generated UUID to prevent collisions and should be used as the reference in your database records.

```json
{
  "id": 55,
  "filename": "f47ac10b-58cc-4372-a567-0e02b2c3d479.png",
  "original_name": "profile-pic.png",
  "mime_type": "image/png",
  "size": 204800,
  "url": "https://api.your-app.com/api/v1/storage/file/f47ac10b...png",
  "created_at": "2024-02-14T10:00:00Z"
}
```

---

## 2. Standard API Endpoints

### Upload File
Upload a binary file using `multipart/form-data`.
*   **POST** `/storage/upload`
*   **Auth**: Required (Valid JWT)

### Serve File (Public)
Retrieve raw file content. This endpoint is public and supports range requests for streaming.
*   **GET** `/storage/file/{filename}`

### Image Processing (Thumbnails)
ApexKit can dynamically resize images on the fly and cache the results.
*   **Query Param**: `thumb={Width}x{Height}`
*   **Example**: `GET /storage/file/image.png?thumb=200x200`

### List Files
Retrieve a paginated list of file metadata.
*   **GET** `/storage/files?page=1&per_page=20`

---

## 3. Integration with Edge Functions (Scripts)

Scripts have powerful, scope-aware access to the storage system. They can read, manipulate, and generate files without needing external libraries.

### Reading Files
Use `$fs` for text or `$zip` for binary-to-base64 conversion.

```javascript
export default async function(req) {
    // 1. Read a text file (e.g. a config or log)
    const logData = await $fs.readText("app_logs.txt");

    // 2. Read a binary file as Base64 string
    const imageBase64 = await $zip.readFile("avatar.png");
    
    return new Response({ size: imageBase64.length });
}
```

### Saving Generated Files
The `$zip.saveFile` method is the most robust way to save data. It writes the file to the current scope's storage (S3/Local) **and** registers it in the database metadata table automatically.

```javascript
export default async function(req) {
    // Create a text file content
    const content = "User report generated at " + new Date();
    const b64 = $util.base64Encode(content);

    // Save directly to scoped storage
    const fileMeta = await $zip.saveFile("report.txt", b64, "text/plain");

    return new Response({ 
        msg: "File saved", 
        downloadUrl: fileMeta.url 
    });
}
```

### In-Memory Archiving
Use the `$zip` object to bundle multiple files or extract uploaded archives.

```javascript
export default async function(req) {
    // 1. Bundle existing uploads into a ZIP
    const fileA = await $zip.readFile("photo1.jpg");
    const fileB = await $zip.readFile("document.pdf");

    const zipB64 = await $zip.create({
        "assets/image.jpg": fileA,
        "docs/ref.pdf": fileB,
        "info.txt": "Exported via ApexKit"
    });

    // 2. Save the new ZIP
    const archive = await $zip.saveFile("bundle.zip", zipB64);
    
    return new Response({ archiveUrl: archive.url });
}
```

---

## 4. Multi-Tenancy & Sandboxes

Storage is strictly isolated based on the execution context:

1.  **Root App**: Files reside in `storage/system/uploads`.
2.  **Tenants**: Files reside in `storage/tenants/{tenant_id}/uploads`.
3.  **Sandboxes**: Files reside in `storage/sandboxes/session_{uuid}/uploads`.

**Behavior**:
*   A script running in **Tenant A** cannot use `$zip.readFile` to access files belonging to **Tenant B**.
*   Public URLs are automatically prefixed: `/tenant/{id}/api/v1/storage/file/...`.

---

## 5. JavaScript SDK Usage

The `pb.files` namespace provides convenient methods for frontend integration.

```javascript
import { pb } from './apiClient';

// 1. Upload from a file input
const fileInput = document.querySelector('input[type="file"]');
const uploaded = await pb.files.upload(fileInput.files[0]);

// 2. List with pagination
const { items, total } = await pb.files.list(1, 20);

// 3. Delete
await pb.files.delete(uploaded.id);

// 4. Generate URL (Tenant-Aware)
// Returns correct path regardless of whether you are in root or tenant context
const url = pb.files.getFileUrl(uploaded.filename);
```

---

## 6. Limits & Configuration

*   **Default Limit**: 10MB per file (Adjustable via `ARCHIVE_LIMIT` env variable).
*   **Image Formats**: Supports PNG, JPEG, WEBP, and SVG. (SVG is served raw without resizing).
*   **S3 Proxies**: If S3 is enabled, the `/storage/file` endpoint acts as a secure proxy to hide your bucket's private credentials.