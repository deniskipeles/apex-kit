# File Storage

ApexKit provides a powerful file management system that abstracts away the complexities of local and cloud storage.

## Storage Backends

ApexKit supports two primary storage backends:
1. **Local File System**: Great for development and small deployments.
2. **S3-Compatible Storage**: Built-in support for AWS S3, Cloudflare R2, DigitalOcean Spaces, etc.

You can configure these in the **Settings > Storage** tab of the dashboard.

## Uploading Files

### Via the SDK
```javascript
const fileInput = document.getElementById('my-file');
const uploadedFile = await apex.files.upload(fileInput.files[0]);
console.log(uploadedFile.url);
```

### Via Multipart HTTP
**POST** `/api/v1/storage/upload`

## Image Processing (Thumbnails)

ApexKit can automatically generate thumbnails and resize images on the fly.

### Requesting a Thumbnail
```javascript
const url = apex.files.getFileUrl('photo.jpg') + '?thumb=200x200';
```
Parameters supported:
- `thumb=WxH`: Set width and height.
- `format=webp/avif`: Convert to a modern format automatically.
- `quality=1-100`: Adjust compression level.

## File Security

Files are subject to the same **Security Policies** as records. You can define a `read` policy on the "Files" system collection to control who can view uploaded assets.

### Private Files
To create private files:
1. Create a collection (e.g., `documents`) with a `File` field.
2. Set the collection policy to `owner`.
3. When the user requests the file, ApexKit verifies they have access to the record containing the file reference.

## Storage Migrations

ApexKit includes a built-in migration tool to move files between backends.
- Move from Local to S3 when you scale.
- Sync between two S3 buckets.
- Download all files for local backup.

Access this via **Admin > Storage > Migrate**.

## Best Practices
- **Use WebP/AVIF** for web assets to reduce bandwidth.
- **Set aggressive caching headers** if using a CDN in front of ApexKit.
- **Use S3 for production** to ensure durability and scalability.
