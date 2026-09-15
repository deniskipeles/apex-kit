# ☁️ Files & Storage API Documentation

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

ApexKit provides a unified storage engine that abstracts file persistence across **Local Filesystem** (disk) and **S3-Compatible Storage** (AWS S3, Cloudflare R2, MinIO, DigitalOcean Spaces). It features on-the-fly image transformations, resumable uploads via the **Tus 1.0.0 protocol**, dynamic OpenGraph image rendering, and scope-isolated storage buckets for Multi-Tenant and Sandbox architectures.

---

## 1. Storage Architecture & Scope Isolation

Files are organized and isolated by execution context:

| Scope | Physical Storage Location | Public URL Pattern |
| :--- | :--- | :--- |
| **Root** | `storage/system/uploads/` | `/api/v1/storage/file/{filename}` |
| **Tenant** | `storage/tenants/{tenant_id}/uploads/` | `/tenant/{tenant_id}/api/v1/storage/file/{filename}` |
| **Sandbox** | `storage/sandboxes/session_{id}/uploads/` | `/sandbox/{id}/api/v1/storage/file/{filename}` |

When files are uploaded, ApexKit persists binary objects to storage, registers structured metadata in the `_storage_files` table, and assigns a UUID-based filename to prevent namespace collisions.

### The Stored File Metadata Schema
```json
{
  "id": 55,
  "filename": "f47ac10b-58cc-4372-a567-0e02b2c3d479.png",
  "original_name": "profile-pic.png",
  "mime_type": "image/png",
  "size": 204800,
  "created_at": "2026-06-15T10:00:00Z"
}
```

---

## 2. API Endpoints

### A. Multipart File Upload
Upload binary files using `multipart/form-data`.
* **Endpoint:** `POST /storage/upload`
* **Authentication:** Required (Valid JWT or API Key)
* **Request Body:** Form field `file` containing binary data.
* **Response:**
  ```json
  {
    "id": 55,
    "url": "/api/v1/storage/file/f47ac10b-58cc-4372-a567-0e02b2c3d479.png",
    "filename": "f47ac10b-58cc-4372-a567-0e02b2c3d479.png"
  }
  ```

---

### B. Tus 1.0.0 Resumable Uploads
For large files (up to 10 GB), ApexKit implements the **Tus 1.0.0** protocol with chunk assembly and verification.

* **Create Upload Session:** `POST /storage/upload/tus`
  * Header: `Upload-Length: <total_bytes>`
  * Header: `Upload-Metadata: filename <base64>,filetype <base64>`
  * Response: `201 Created` with `Location: .../tus/{upload_id}`
* **Query Offset:** `HEAD /storage/upload/tus/{upload_id}`
  * Returns `Upload-Offset` and `Upload-Length`.
* **Upload Chunk:** `PATCH /storage/upload/tus/{upload_id}`
  * Header: `Upload-Offset: <current_bytes>`
  * Header: `Content-Type: application/offset+octet-stream`
  * Body: Binary chunk stream.
* **Terminate / Abort:** `DELETE /storage/upload/tus/{upload_id}`

---

### C. Serve File & On-The-Fly Image Transformations
* **Endpoint:** `GET /storage/file/{filename}`
* **Access:** Public (Cache-controlled)
* **Conditional Headers:** Supports `ETag` and `If-None-Match` (`304 Not Modified` return when cached).

#### Image Transformation Query Parameters
| Parameter | Type | Description | Example |
| :--- | :--- | :--- | :--- |
| **`thumb`** | `string` | Dimensions as `{width}x{height}`. | `thumb=300x300`, `thumb=800x0` |
| **`format`** | `string` | Target format: `webp`, `png`, `jpg`, `avif`, `gif`. | `format=webp` |
| **`quality`** | `number` | Compression quality (`1` to `100`). | `quality=85` |
| **`blur`** | `number` | Gaussian blur sigma radius (`0.1` to `50.0`). | `blur=4.5` |

```http
GET /api/v1/storage/file/hero-banner.jpg?thumb=800x450&format=webp&quality=80&blur=2
```

---

### D. File Management
* **List Files:** `GET /storage/files?page=1&per_page=20`
* **Get File Metadata & Pre-Signed URL:** `GET /storage/files/{id_or_filename}?expires_in=3600`
* **Delete File:** `DELETE /storage/files/{id}`

---

### E. Dynamic OpenGraph (OG) Generator
ApexKit renders dynamic SVG templates to high-resolution PNG/WebP images using an integrated rasterization engine.

* **Endpoint:** `GET /storage/files/opengraph`
* **Query Parameters:**
  * `template`: Template slug or Base64-encoded SVG string (use `"default"` for built-in card).
  * `data`: URL-encoded JSON array of objects (`[{ "type": "text"|"image", "target": "KEY", "value": "..." }]`). Max 8 entries.
  * `format`: Target format (`png`, `webp`, `jpeg`). Default `png`.
  * `quality`: Quality level (`1`–`100`).

```html
<meta property="og:image" content="https://api.your-app.com/api/v1/storage/files/opengraph?template=default&data=%5B%7B%22type%22%3A%22text%22%2C%22target%22%3A%22TITLE%22%2C%22value%22%3A%22ApexKit%20BaaS%22%7D%5D" />
```

---

## 3. Server-Side Scripting API (`$files`)

Scripts in ApexKit interact with file storage through the global `$files` API:

```typescript
// Script: process-attachment
// Trigger: manual | Path: ./webhooks/process-attachment.ts

export default async function (req: Request) {
    const { filename } = await req.json();

    // 1. Read file as Base64 string from storage (Local or S3)
    const base64Data = await $files.read(filename);

    // 2. Decode into Uint8Array for processing
    const buffer = $util.base64DecodeBuffer(base64Data);

    // 3. Save a transformed or generated file back to storage
    const savedFile = await $files.save(
        "processed-report.pdf", 
        buffer, 
        "application/pdf"
    );

    // 4. Generate temporary signed URL (valid for 1 hour)
    const secureUrl = await $files.getSignedUrl(savedFile.filename, 3600);

    // 5. Delete temporary file
    await $files.delete(filename);

    return new Response({
        file_id: savedFile.id,
        url: savedFile.url,
        signed_url: secureUrl
    });
}
```

---

## 4. TypeScript SDK Usage (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('JWT_OR_API_KEY');

// 1. Standard Multipart Upload
const fileInput = document.getElementById('file-picker') as HTMLInputElement;
const uploadedFile = await apex.files.upload(fileInput.files![0]);

// 2. Resumable Chunked Upload (Tus 1.0.0)
const uploadTask = apex.files.uploadResumable(fileInput.files![0], {
  chunkSize: 1024 * 1024, // 1 MB chunks
  onProgress: ({ bytesUploaded, bytesTotal, percentage }) => {
    console.log(`Uploaded ${percentage}% (${bytesUploaded}/${bytesTotal} bytes)`);
  },
  onSuccess: (result) => {
    console.log('Upload complete:', result.url);
  },
  onError: (err) => {
    console.error('Upload failed:', err);
  }
});

// Controls: pause, resume, abort
// uploadTask.pause();
// await uploadTask.resume();
// await uploadTask.abort();

// 3. Public Transformed Image URL (Synchronous)
const thumbUrl = apex.files.getFileUrl(uploadedFile.filename, {
  thumb: '400x300',
  format: 'webp',
  quality: 85,
  blur: 1.5
});

// 4. Pre-Signed Private S3 URL (Asynchronous)
const signedUrl = await apex.files.getFileUrl(uploadedFile.filename, {
  signed: true,
  expiresIn: 1800 // 30 minutes
});

// 5. OpenGraph Meta URL Builder
const ogUrl = apex.files.getOpenGraphUrl('default', [
  { type: 'text', target: 'TITLE', value: 'Modern BaaS Architecture' },
  { type: 'text', target: 'SUBTITLE', value: 'Scale single-node applications effortlessly' },
  { type: 'image', target: 'IMAGE_URL', value: uploadedFile.filename }
], { format: 'png', quality: 90 });
```

---

## 5. Storage Administration & Orphan Management

ApexKit provides built-in reconciliation tools to detect discrepancies between registered database metadata and physical storage files.

```typescript
// 1. Audit orphans between database and storage
const report = await apex.files.listOrphans();
console.log(`Unregistered files in storage: ${report.storage_orphans.length}`);
console.log(`Broken metadata references: ${report.db_orphans.length}`);

// 2. Resolve discrepancies
// 'flush_storage': Delete unreferenced files from disk/S3
// 'register_storage': Auto-create metadata entries for unindexed files
// 'flush_db': Delete orphaned database rows whose files no longer exist
await apex.files.resolveOrphans('flush_storage');
```

---

## 6. Limits & Configuration

| Parameter | Default | Environment Variable / Setting |
| :--- | :--- | :--- |
| **Max Upload Size (Standard)** | 10 MB | `FILE_UPLOAD_LIMIT` (MB) |
| **Max Upload Size (Tus)** | 10 GB | `DEFAULT_MAX_UPLOAD_SIZE` |
| **Image Processing Quantization** | `10` | `image_processing_step` (Settings > General) |
| **Allowed File Formats** | Dynamic | Configured per `file` field definition in collection schema |