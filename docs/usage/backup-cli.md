# Developer Guide: ApexKit Backup & Restoration CLI

This document outlines how to use the built-in command-line interface (CLI) to backup, migrate, and restore data across both your **Root (System)** and **Tenant** environments. 

Under the hood, the Backup CLI compresses data using `tar` and `gzip`, preserving directories, permissions, and database files.

---

## 1. Storage Layout Recap

To understand what is being backed up, remember the ApexKit storage architecture:
*   **Root Data**: Located at `storage/system/`
*   **Tenant Data**: Located at `storage/tenants/<tenant-id>/`

Inside any environment directory, files are categorized as **Default** or **Optional**:

| Category | File / Folder | Included by Default? | Description |
| :--- | :--- | :--- | :--- |
| **Default** | `core.db` | Yes | Users, Auth tokens, Config registry |
| **Default** | `data.db` | Yes | User Collections, Records, Relations, File Metadata |
| **Default** | `system.db` | Yes | Scripts, HTML Templates, AI Actions, Sessions |
| **Default** | `public/` | Yes | Static Web Hosting files (deployed frontend assets) |
| **Default** | `uploads/` | Yes | Media library (images, documents, blobs) |
| **Optional** | `vectors.db` | No | Stored AI Embeddings (can become very large) |
| **Optional** | `logs.db` | No | System and audit logs (can generate high I/O / churn) |
| **Optional** | `indexes/` | No | Tantivy full-text search indexes (can be rebuilt easily) |

---

## 2. Creating Backups (`apexkit backup`)

The `backup` command allows you to selectively back up your Root environment, specific Tenants, or any granular combination thereof.

### Options:
*   `--root`: Includes the Root (System) environment. You can optionally specify a comma-separated list of optional files, or `*` to include everything.
*   `--tenants`: A comma-separated list of tenant IDs. You can specify granular options for individual tenants using parentheses `()`.
*   `-o, --out`: Custom output path. Defaults to `storage/backups/backup_<timestamp>.tar.gz`.

### Granular Syntax Rules:
1.  **No parameters specified**: Backs up only the **Default** files (Core DB, Data DB, System DB, Public folder, and Uploads folder).
2.  **Asterisk `(*)`**: Backs up **Default + All Optional** files (including Vectors, Logs, and Search Indexes).
3.  **Parentheses `(item1,item2)`**: Backs up **Default + specified Optional items**.

---

### Command Examples

#### 1. Standard Minimal Backup (Default files only)
Backs up only the essential databases, uploads, and static sites for both Root and two specific tenants:
```bash
./apexkit backup --root --tenants="client-a,client-b"
```

#### 2. Full Deep Backup (Includes everything)
Using the `*` wildcard, this backs up absolutely everything—including vectors, logs, and compiled indexes for both Root and the specified tenants:
```bash
./apexkit backup --root="*" --tenants="client-a(*),client-b(*)"
```

#### 3. Granular Mixed Backup (Custom configuration)
This backs up:
*   **Root**: Default files only.
*   **client-a**: Default files + Search Indexes.
*   **client-b**: Default files + Vectors + Logs.
```bash
./apexkit backup --root --tenants="client-a(indexes),client-b(vectors.db,logs.db)"
```

#### 4. Save to Custom Output Path
```bash
./apexkit backup --root --out="/home/user/deployments/root_backup.tar.gz"
```

---

## 3. Restoring Backups (`apexkit restore`)

The `restore` command extracts your backup archive, inspects its contents, checks which environments (Root or Tenants) are present in the archive, and safely restores them.

### Safety Net: Automated `.bak` Backups
Before overwriting any live database or directory, the CLI automatically renames the existing live directory by appending a `_bak_<timestamp>` suffix (e.g., `storage/system_bak_20260603_094407`). If the restore fails mid-way, your original live data is preserved untouched.

### Syntax:
```bash
./apexkit restore <path-to-archive> [flags]
```

### Flags:
*   `-y, --yes`: Bypasses the interactive "Are you sure?" safety prompt. Useful for CI/CD pipelines and automated scripts.

---

### Interactive Walkthrough Example

```bash
./apexkit restore storage/backups/backup_20260603_094407.tar.gz
```

**Output:**
```
📦 Extracting archive...

Archive contains the following scopes:
  - Root (System Data)
  - Tenant: client-a
  - Tenant: client-b

⚠️  WARNING: Proceeding will OVERWRITE live data. Existing folders will be moved to a .bak suffix. Continue? [y/N]: y

🚀 Restoring data...
  [Backing up] Root (System Data) -> storage/system_bak_20260603_094407
  ✅ Restored Root (System Data)
  [Backing up] Tenant: client-a -> storage/tenants/client-a_bak_20260603_094407
  ✅ Restored Tenant: client-a
  [Backing up] Tenant: client-b -> storage/tenants/client-b_bak_20260603_094407
  ✅ Restored Tenant: client-b

🎉 Restoration complete! Please restart the ApexKit server to reload databases into memory.
```

### Automation / Non-Interactive Example
To run the restore inside an automated script or cron job, append the `--yes` flag:
```bash
./apexkit restore storage/backups/backup_20260603_094407.tar.gz --yes
```