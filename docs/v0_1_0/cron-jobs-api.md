# ⏰ Scheduler & Cron Jobs Documentation

**Version:** 0.1.0  
**Context:** Background Automation, Periodic Scripts, and Automated Backups

ApexKit features a multi-tenant scheduler powered by `tokio-cron-scheduler` and `cron::Schedule`. It enables executing serverless scripts or internal webhooks at designated intervals, fully isolated to the database and filesystem boundaries of Root, Tenant, or Sandbox environments.

---

## 1. Architectural Overview

```
+-------------------------------------------------------------------------+
|                           Master Node Only                              |
|   (If APEXKIT_MASTER_URL is set, cron scheduler is locked on Replicas)  |
+-------------------------------------------------------------------------+
                                     │
           ┌─────────────────────────┼────────────────────────┐
           ▼                         ▼                        ▼
+---------------------+   +---------------------+   +---------------------+
|     Root Scope      |   |    Tenant Scope     |   |    Sandbox Scope    |
| - Daily Log Cleanup |   | - Scoped DB & Files |   | - Active in memory  |
| - Root Backups      |   | - Rate limited      |   | - Auto-paused when  |
| - Custom Scripts    |   | - Custom Scripts    |   |   idle/evicted      |
+---------------------+   +---------------------+   +---------------------+
```

1. **Master Node Execution Lock:** If running as a Replica (`APEXKIT_MASTER_URL` is set), the cron scheduler is disabled to prevent duplicate jobs across cluster nodes.
2. **Context-Aware Scoping:** When a job executes for `tenant-a`, all global script primitives (`$db`, `$files`, `$fs`, `$env`) automatically bind to `storage/tenants/tenant-a/`.
3. **Sandbox In-Memory Lifecycle:** Sandboxes only run scheduled jobs while active in memory. If a sandbox expires or is evicted from the memory cache, its ticker halts.
4. **Fast-Fail Minute Locking:** Each scheduled run is stamped against `root_script_cache` with key `cron_lock:{context_id}:{job_id}:{YYYYMMDDHHmm}` to prevent race conditions or duplicate execution within the same minute.

---

## 2. Cron Schedule Expression Format

ApexKit uses standard **6-field cron expressions** (including seconds):

$$\text{sec} \quad \text{min} \quad \text{hour} \quad \text{day\_of\_month} \quad \text{month} \quad \text{day\_of\_week}$$

### Schedule Reference

| Expression | Frequency |
| :--- | :--- |
| `0 * * * * *` | Every minute on second `00` |
| `0 */5 * * * *` | Every 5 minutes |
| `0 */15 * * * *` | Every 15 minutes |
| `0 0 * * * *` | Every hour on the hour |
| `0 0 0 * * *` | Daily at midnight (00:00:00 UTC) |
| `0 30 2 * * *` | Daily at 02:30:00 UTC |
| `0 0 9 * * MON` | Every Monday at 09:00:00 UTC |
| `0 0 0 1 * *` | First day of every month at midnight |

---

## 3. Job Configuration Schema

Jobs are defined within the `cron_jobs` array inside the system configuration:

| Field | Type | Required | Description | Example |
| :--- | :--- | :--- | :--- | :--- |
| **`id`** | `string` | Yes | Unique identifier string for tracking and locking. | `nightly-cleanup-1` |
| **`name`** | `string` | Yes | Human-readable label for monitoring and logs. | `Nightly Cleanup` |
| **`schedule`** | `string` | Yes | 6-part cron schedule expression. | `0 0 2 * * *` |
| **`payload`** | `string` | Yes | Target script name OR internal loopback path starting with `/`. | `process-invoices` or `/api/v1/admin/backup` |
| **`active`** | `boolean` | Yes | Enables or disables job execution. | `true` |

---

## 4. Payload Types

### A. Script Payload (Direct Execution)
If `payload` does not start with a slash `/`, ApexKit executes the script matching that name in the current scope.

* **Trigger Type:** The script receives `{ "trigger": "cron", "job": "<job_name>" }` in its request body.
* **Environment:** Executes within the current scope's database and filesystem.

```typescript
// Script: purge-temp-files
// Trigger: cron | Path: ./webhooks/purge-temp-files.ts

export default async function (req: Request) {
    const { trigger, job } = await req.json();
    console.log(`[Scheduler] ${job} started via ${trigger}`);

    // Read files in scoped temp folder
    const files = await $fs.list("");
    let deletedCount = 0;

    for (const f of files) {
        if (!f.isDir && f.name.endsWith(".tmp")) {
            await $fs.delete(f.name);
            deletedCount++;
        }
    }

    console.log(`[Scheduler] Purged ${deletedCount} temporary files.`);
    return new Response({ deleted: deletedCount });
}
```

### B. Internal Loopback Webhook (`/path`)
If `payload` begins with a forward slash `/`, ApexKit triggers an internal loopback HTTP `POST` request.

* **Target URL Resolution:**
  * **Root:** `http://127.0.0.1:{PORT}{payload}`
  * **Tenant:** `http://127.0.0.1:{PORT}/tenant/{tenant_id}{payload}`
  * **Sandbox:** `http://127.0.0.1:{PORT}/sandbox/{session_id}{payload}`
* **Automated Auth:** ApexKit generates an internal admin JWT token (`scheduler@system.internal`, `role: "admin"`) and injects it into the `Authorization: Bearer` header.
* **Payload Sent:** Empty JSON object `{}`.

*Example Payload:* `/api/v1/run/sync-inventory`

---

## 5. Built-in Automated Background Tasks

ApexKit runs several automated maintenance routines:

1. **System Log Retention Cleanup:** Runs daily at 03:00:00 UTC (`0 3 * * *`). Cleans logs in `storage/system/logs.db` older than `log_retention_days` (configured under `general` settings, default: 7 days).
2. **Scheduled Automated Backups:** Evaluated every minute (`0 * * * * *`) against the `backups` configuration schema. If a backup is due, an async task executes `perform_backup()` with safe minute locks.
3. **Resource Analytics:** Runs every 30 minutes (`0 */30 * * * *` by default, configurable via `RESOURCE_ANALYTICS_CRON`). Re-calculates disk sizes, vector counts, and AI request metrics for all tenants and sandboxes.
4. **Tenant Connection Eviction:** Hourly cleanup releases idle SQLite connections and in-memory caches.

---

## 6. Tenant Protection & Rate Limiting

To prevent tenant scripts from starving system threads:

* **Max Concurrent Jobs:** Non-root scopes are limited to **2** active jobs per window (`TENANT_MAX_CRONS`, default: 2).
* **Sliding Window:** Monitored over a 5-minute window (`TENANT_CRON_INTERVAL`, default: 5 minutes).
* **Skipped Executions:** If a tenant exceeds their allowance, jobs for that cycle are skipped and logged with `[Scheduler] Rate limit hit for {tenant_id}`.

---

## 7. Configuration Examples

### Via TypeScript SDK (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('ADMIN_JWT_OR_API_KEY');

// 1. Fetch current settings
const settings = await apex.admins.getSettings();
const cronJobs = settings.cron_jobs || [];

// 2. Add or update scheduled jobs
const updatedJobs = [
  ...cronJobs.filter(j => j.id !== 'hourly-report'),
  {
    id: 'hourly-report',
    name: 'Hourly Sales Aggregator',
    schedule: '0 0 * * * *', // Top of every hour
    payload: 'aggregate-sales',
    active: true
  }
];

// 3. Save updated configuration
await apex.admins.patchSettings({
  cron_jobs: updatedJobs
});

// 4. Hot-reload scheduler without restart
await apex.admins.reloadSystem('root');
```

### Via cURL

```bash
curl -X PATCH "https://api.your-app.com/api/v1/admin/settings" \
  -H "Authorization: Bearer <ADMIN_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{
    "cron_jobs": [
      {
        "id": "nightly-backup",
        "name": "Nightly System Backup",
        "schedule": "0 0 2 * * *",
        "payload": "/api/v1/admin/backup",
        "active": true
      }
    ]
  }'
```