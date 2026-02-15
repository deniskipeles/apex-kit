# ⏰ Scheduler & Cron Jobs Documentation

**Version:** 0.1.0
**Context:** Background Automation, Maintenance, and Scheduled Tasks.

ApexKit includes a built-in, high-performance scheduler (powered by `tokio-cron-scheduler`) that allows you to execute backend logic at specific intervals. Unlike traditional setups, the ApexKit scheduler is **Multi-Tenant Aware**, ensuring that tasks run safely within the isolated context of specific tenants or sandboxes.

---

## 1. How it Works

The scheduler operates on a **Master-Worker** model:

1.  **Global Ticker**: A master process ticks every 60 seconds.
2.  **Context Scanning**: It scans the Root App and all active Tenants/Sandboxes for configured `cron_jobs`.
3.  **Isolated Execution**: When a job is due, it is executed within that specific tenant's scope.
    *   A script named `daily-report` running for **Tenant A** will only have access to **Tenant A's** database and files.
    *   Environmental globals (like `$db` or `$env`) are automatically mapped to the correct environment.

---

## 2. Job Configuration

Cron jobs are managed via **Admin UI > Settings > System** or the `admins` API.

| Field | Description | Example |
| :--- | :--- | :--- |
| **Name** | A human-readable label for the job. | `Cache Cleanup` |
| **Schedule** | Standard Cron expression (5 or 6 fields). | `0 0 * * *` (Daily at Midnight) |
| **Payload** | The target to execute (Script name or URL). | `archive_old_data` |
| **Active** | Toggle to enable/disable execution. | `true` |

### Schedule Cheat Sheet
*   `0 * * * *` -> Every Hour (top of the hour)
*   `*/15 * * * *` -> Every 15 Minutes
*   `0 9 * * MON` -> Every Monday at 9:00 AM
*   `0 0 1 * *` -> First day of every month at midnight

---

## 3. Payload Types

### A. Script Payload
If the payload is a simple string (e.g., `process-billing`), the scheduler looks for a **Script** with that name in the current scope with the `cron` trigger type.

```javascript
// Script Name: process-billing
// Trigger: cron
export default async function(req) {
    log("Starting billing cycle...");
    const overdue = await $db.find("invoices", { status: "pending" });
    // logic...
}
```

### B. Webhook Payload (Loopback)
If the payload starts with a forward slash (`/`), the scheduler treats it as an **internal API request**. This is useful for triggering existing manual scripts or system endpoints without writing a wrapper script.

*   **Payload**: `/api/v1/run/sync-external-data`
*   **Behavior**: The scheduler generates a temporary System Admin token and performs a `POST` request to that endpoint within the tenant's own URL space.

---

## 4. System Maintenance Jobs

ApexKit runs several hardcoded maintenance jobs automatically in the background:

1.  **Log Retention**: Runs daily at 3:00 AM. It deletes entries from `_system_logs` and `_audit_logs` older than the configured `log_retention_days` (Default: 7).
2.  **Connection Pruning**: Every hour, the `TenantManager` evicts database connections for tenants that have been idle for more than 60 minutes to reclaim memory.
3.  **Sandbox Expiry**: Checks for sandboxes that have passed their `expires_at` timestamp and deletes their physical storage.

---

## 5. JavaScript SDK Usage

Cron jobs are part of the system settings. You manage them by updating the `cron_jobs` array in the system configuration.

```javascript
import { pb } from './apiClient';

// 1. Get current jobs
const settings = await pb.admins.getSettings();
const currentJobs = settings.cron_jobs || [];

// 2. Add a new job
const updatedJobs = [
    ...currentJobs,
    {
        id: "job_unique_id",
        name: "Nightly Sync",
        schedule: "0 2 * * *", // 2 AM
        payload: "sync_script_name",
        active: true
    }
];

// 3. Save settings
await pb.admins.patchSettings({
    cron_jobs: updatedJobs
});
```

---

## 6. Best Practices & Limitations

*   **No Long-Running Tasks**: Scripts have a standard execution timeout (default 30s). For very heavy processing (e.g., processing 1 million rows), use the cron job to queue smaller "Job" objects or use the `$cmd` tool (Root only) to spawn a background process.
*   **Idempotency**: Ticker logic can occasionally drift by a few milliseconds. Ensure your scripts check if they have already run for the current period if duplicate execution is a concern.
*   **Error Logging**: Any errors thrown by a cron script are captured and logged in **Admin UI > Logs** with the source labeled as `scheduler`.