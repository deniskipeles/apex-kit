# Admin, System & Files SDK Reference

The `apex.admins`, `apex.files`, `apex.logs`, and `apex.sites` namespaces provide complete administrative, system diagnostics, S3/Local file attachments, and static site deployment features.

---

## 1. Collection & Schema Management

The `admins` namespace allows platform administrators to programmatically manipulate schemas and trigger system-wide index maintenance.

### Method Signatures
- `admins.listCollections()`
- `admins.getCollection(id)`
- `admins.createCollection(name, schema)`
- `admins.updateCollection(id, payload)`
- `admins.patchCollection(id, payload)`
- `admins.deleteCollection(id)`
- `admins.reIndex(collectionId?)`
- `admins.revectorizeCollection(collectionId, force?)`
- `admins.importSchema(file, strategy?)`
- `admins.exportSchema()`

---

## 2. Configuration Store

Manage persistent server settings securely.

### Method Signatures
- `admins.listConfigs()`
- `admins.setConfig(key, value, encrypt)`
- `admins.deleteConfig(key)`

---

## 3. User & Role Management

Query, create, update, or revoke user accounts.

### Method Signatures
- `admins.listUsers(options?)`
- `admins.registerUser(email, password?, role?, metadata?)`
- `admins.updateUser(id, email?, password?, role?, metadata?)`
- `admins.deleteUser(id)`

```typescript
// Create a new Administrator user
const user = await apex.admins.registerUser(
  'staff@my-saas.com',
  'ultraSecurePass1!',
  'admin',
  { department: 'finance' }
);
```

---

## 4. Multi-Tenant Operations (Root Scoped)

Create and manage isolated customer databases. Typically requires a `root` authenticated JWT.

### Method Signatures
- `admins.listTenants()`
- `admins.createTenant(tenantId)`
- `admins.updateTenant(id, data)`
- `admins.updateTenantStatus(id, status)`
- `admins.deleteTenant(id)`

```typescript
// Register new tenant "tenant-beta"
await apex.admins.createTenant('tenant-beta');

// Suspend a tenant instantly (Disables database access)
await apex.admins.updateTenantStatus('tenant-beta', 'suspended');
```

---

## 5. Ephemeral Sandboxes (Scout & Create)

Create, delete, or publish isolated sandboxes programmatically.

### Method Signatures
- `admins.listSandboxes()`
- `admins.createSandbox(name, cloneStrategy, cloneRecordLimit?, model?, initialPrompt?, collections?, scripts?, templates?)`
- `admins.deleteSandbox(id)`
- `admins.publishSandbox(id)`

```typescript
// Spin up a sandbox, copying schemas and up to 20 records per collection
const sandbox = await apex.admins.createSandbox(
  'Feature-Test-Sandbox',
  'Partial',
  20
);
console.log(`Sandbox session created with ID: ${sandbox.id}`);
```

---

## 6. S3 & Local Storage Files

Upload and manage assets. ApexKit seamlessly handles public and signed URL generation, plus on-the-fly thumbnail transformations.

### Method Signatures
- `files.list(page?, perPage?)`
- `files.upload(file)`
- `files.delete(id)`
- `files.getFileUrl(filename, options?)`

### File Handling with Thumbnails & Signed URLs:

```typescript
// 1. Upload a binary file
const fileInput = document.getElementById('file-picker') as HTMLInputElement;
const uploadedFile = await apex.files.upload(fileInput.files![0]);

// 2. Resolve Public URL synchronously
const publicUrl = apex.files.getFileUrl(uploadedFile.filename);

// 3. Resolve Public Thumbnail with width resizing (synchronous)
const thumbUrl = apex.files.getFileUrl(uploadedFile.filename, {
    thumb: '300x300',
    format: 'webp',
    quality: 85
});

// 4. Resolve Private/Secure S3 URL with pre-signature (asynchronous)
const signedUrl = await apex.files.getFileUrl(uploadedFile.filename, {
    signed: true,
    expiresIn: 3600 // 1 hour expiry
});
```

---

## 7. API Keys (Composite Scopes)

Programmatically provision, update, or delete composite scoped API keys.

### Method Signatures
- `admins.listApiKeys()`
- `admins.createApiKey(name, role?, scope?, bypass_cors?, env_type?, roles?, target_tenant?)`
- `admins.updateApiKey(id, updates)`
- `admins.deleteApiKey(id)`

```typescript
// Provision an API Key for direct client integration (PK environment) for tenant-alpha
const { key, info } = await apex.admins.createApiKey(
  "React Web Key",
  "user",          // Role
  "tenant",        // Scope
  true,            // Bypass CORS
  "pk",            // Key environment type (pk, sk, tnnt, sys)
  ["user"],        // Roles array
  "tenant-alpha"   // Target tenant
);
console.log(`Your client API Key: ${key}`);
```

---

## 8. Backups & Restores

```typescript
// Create system backup
await apex.admins.createBackup();

// Restore from a backup archive file
await apex.admins.restoreFromFile("backup_2026_01_01.zip");
```

---

## 9. Site Deployment (Static Hosting)

Deploy fully functional Single Page Apps (SPAs) directly on your ApexKit server node.

### Method Signatures
- `sites.deploy(zipFile)`
- `sites.listFiles()`
- `sites.delete(path)`

```typescript
// Deploy static HTML site bundle
const zipFile = fileInput.files![0];
await apex.sites.deploy(zipFile);
```
