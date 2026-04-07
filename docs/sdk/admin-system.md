# Admin & System

The `apex.admins`, `apex.files`, and `apex.logs` namespaces provide management and diagnostic tools for the ApexKit system.

---

## Collections Management

The `admins` namespace allows for defining and managing the schema of your collections.

### Methods

- `admins.listCollections()`
- `admins.getCollection(id)`
- `admins.createCollection(name, schema)`
- `admins.updateCollection(id, payload)`
- `admins.patchCollection(id, payload)`
- `admins.deleteCollection(id)`
- `admins.reIndex(collectionId?)`
- `admins.revectorizeCollection(collectionId)`
- `admins.importSchema(file, strategy?)`
- `admins.exportSchema()`

### Defining a Collection Schema

```typescript
const schema = {
    fields: {
        title: { name: "title", type: "string", required: true },
        content: { name: "content", type: "text", required: true },
        status: { name: "status", type: "string", options: ["draft", "published"], required: true }
    }
};

const collection = await apex.admins.createCollection("posts", schema);
```

---

## Configuration

Manage system-wide configuration keys.

### Methods

- `admins.listConfigs()`
- `admins.setConfig(key, value, encrypt)`
- `admins.deleteConfig(key)`

---

## User Management

Manage users and roles within the current scope.

### Methods

- `admins.listUsers(options?)`
- `admins.registerUser(email, password?, role?, metadata?)`
- `admins.updateUser(id, email?, password?, role?, metadata?)`
- `admins.deleteUser(id)`
- `admins.listRoles()`

```typescript
// Create a new user with a specific role
const newUser = await apex.admins.registerUser('admin@test.com', 'secure-password', 'admin');
```

---

## Storage & Files

The `files` namespace handles file uploads and retrievals across local and S3 storage.

### Methods

- `files.list(page?, perPage?)`
- `files.upload(file: File)`
- `files.delete(id)`
- `files.getFileUrl(filename)`

### Uploading and Retrieving Files

```typescript
// 1. Upload
const fileInput = document.getElementById('upload');
const storedFile = await apex.files.upload(fileInput.files[0]);

// 2. Get Scoped URL
const url = apex.files.getFileUrl(storedFile.filename);

// 3. Dynamic Transformations (Resizing)
const thumbUrl = `${url}?thumb=100x100`;
```

---

## Backups & API Keys

Manage system health, data exports, and programmatic access.

### Backups

- `admins.listBackups()`
- `admins.createBackup()`
- `admins.restoreBackup(file)`
- `admins.restoreFromFile(filename)`
- `admins.downloadBackup(filename)`

### API Keys

- `admins.listApiKeys()`
- `admins.createApiKey(name, role?, scope?, bypass_cors?)`
- `admins.updateApiKey(id, updates)`
- `admins.deleteApiKey(id)`

### System Utilities

- `admins.getSettings()`
- `admins.updateSettings(settings)`
- `admins.patchSettings(settings)`
- `admins.reloadSystem(target?)`
- `admins.testS3StorageConnection(config)`
- `admins.testEmail(email)`
- `admins.getDashboardStats()`
- `admins.importData(collectionName, file)`

---

## Logs

Monitor system activity and errors.

### Methods

- `logs.list()`

```typescript
const logs = await apex.logs.list();
logs.forEach(log => console.log(`[${log.level}] ${log.message}`));
```

---

## Sites (Deployment)

Deploy static websites to your ApexKit instance.

### Methods

- `sites.deploy(file: File)`
- `sites.listFiles()`
- `sites.delete(path)`

```typescript
// Deploy a zip archive of your site
await apex.sites.deploy(zipFile);
```

---

## Multi-Tenant Operations

These operations are typically available only in the `root` scope.

### Methods

- `admins.listTenants()`
- `admins.createTenant(tenantId)`
- `admins.updateTenant(id, data)`
- `admins.updateTenantStatus(id, status)`
- `admins.deleteTenant(id)`

```typescript
// Create a new tenant
await apex.admins.createTenant('client-beta');

// Suspend a tenant
await apex.admins.updateTenantStatus('client-beta', 'suspended');
```

---

## Utilities

- `utils.stripHtmlTags(html: string): string`

```typescript
const text = apex.utils.stripHtmlTags('<p>Hello <b>World</b></p>');
// returns "Hello World"
```
