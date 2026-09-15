# ⏰ Understanding Cron Jobs & Scheduled Tasks in ApexKit

ApexKit features an integrated, multi-tenant background scheduler powered by `tokio-cron-scheduler` and Rust's `cron` crate. Unlike traditional standalone cron daemons or external queue workers, ApexKit executes scheduled tasks directly inside the server binary while enforcing strict tenant and sandbox database boundaries.

---

## 1. How Scheduled Tasks Work

ApexKit's scheduler runs as an asynchronous background worker tied to the server process:

1. **Master Node Exclusive**: If the server runs in Replica mode (`APEXKIT_MASTER_URL` is set), the scheduler is automatically disabled. Cron jobs run exclusively on the Master node to prevent duplicate job executions across replica clusters.
2. **Periodic Tick Loop**: The scheduler evaluates active schedules every minute using 6-part cron expressions (`sec min hour day_of_month month day_of_week`).
3. **Context Scanning**: During each tick, the scheduler scans:
   - **Root Scope**: Global system jobs and root-configured tasks.
   - **Tenant Scopes**: Iterates through registered tenants on disk and executes jobs scoped strictly to their isolated databases.
   - **Sandbox Scopes**: Evaluates jobs only for sandboxes that are currently loaded and active in memory.
4. **Fast-Fail Minute Locking**: To prevent duplicate runs from timer drift or long-running tasks, each job execution is locked in cache using a minute stamp (`YYYYMMDDHHmm`).

---

## 2. Cron Schedule Syntax

ApexKit requires standard **6-field cron expressions** (specifying seconds first):

$$\text{Seconds} \quad \text{Minutes} \quad \text{Hours} \quad \text{DayOfMonth} \quad \text{Month} \quad \text{DayOfWeek}$$

### Common Expressions

| Schedule Expression | Meaning |
| :--- | :--- |
| `0 0 * * * *` | Every hour on the hour (at second 00) |
| `0 0 0 * * *` | Once a day at midnight UTC (`00:00:00`) |
| `0 0 2 * * *` | Once a day at 2:00 AM UTC |
| `0 */15 * * * *` | Every 15 minutes |
| `0 */5 * * * *` | Every 5 minutes |
| `0 0 9 * * MON` | Every Monday at 9:00 AM UTC |

---

## 3. Supported Payload Types

When configuring a job's `payload`, ApexKit supports two distinct execution patterns:

### A. Script Payload (Direct Execution)
If the payload is a plain string without a leading slash (e.g., `"auto-backup"`), the scheduler executes a serverless script matching that name within the current tenant or root context.

* **Trigger Payload**: The script receives `{ "trigger": "cron", "job": "<job_name>" }` in its request body.
* **Environment**: All script globals (`$db`, `$files`, `$fs`, `$env`) automatically bind to the executing tenant or root database.

```typescript
// Script: auto-backup
// Trigger: cron | Path: ./webhooks/auto-backup.ts

export default async function (req: Request) {
    const { trigger, job } = await req.json();
    console.log(`[Cron] ${job} started via ${trigger}`);

    // Call external backup endpoint or execute database archiving
    const res = await fetch("http://127.0.0.1:5000/backup", { method: "POST" });
    const data = await res.json();

    if (!res.ok) {
        console.error("Backup failed: " + (data.message || res.statusText));
        return new Response({ error: data.message }, { status: 500 });
    }

    console.log("✅ Backup succeeded: " + JSON.stringify(data));
    return new Response({ success: true, details: data });
}
```

### B. Internal Webhook Payload (Loopback)
If the payload starts with a forward slash (e.g., `"/api/v1/admin/backup"`), the scheduler performs an internal loopback HTTP request.

* **URL Resolution**:
  - **Root Scope**: `http://127.0.0.1:{PORT}{payload}`
  - **Tenant Scope**: `http://127.0.0.1:{PORT}/tenant/{tenant_id}{payload}`
  - **Sandbox Scope**: `http://127.0.0.1:{PORT}/sandbox/{session_id}{payload}`
* **Automated Admin JWT**: ApexKit generates an internal admin JWT (`scheduler@system.internal`, role: `admin`, scope: `root`) and injects it into the `Authorization: Bearer` header.
* **Request**: An HTTP `POST` request with body `{}`.

---

## 4. Registering a Cron Job

Cron jobs are managed within the `cron_jobs` array in system settings.

### Option A: Using the TypeScript SDK (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');
apex.setToken('ADMIN_JWT_OR_API_KEY');

// 1. Fetch current settings
const settings = await apex.admins.getSettings();
const existingJobs = settings.cron_jobs || [];

// 2. Add or update scheduled job
const updatedJobs = [
  ...existingJobs.filter(j => j.id !== 'hourly-backup-job'),
  {
    id: 'hourly-backup-job',
    name: 'Hourly Cloudflare Backup',
    schedule: '0 0 * * * *',
    payload: 'auto-backup',
    active: true
  }
];

// 3. Patch settings
await apex.admins.patchSettings({
  cron_jobs: updatedJobs
});

// 4. Reload system scheduler in-memory without restarting server
await apex.admins.reloadSystem('root');
```

### Option B: Using cURL

```bash
curl -X PATCH "https://api.your-app.com/api/v1/admin/settings" \
     -H "Authorization: Bearer <MASTER_KEY_OR_ADMIN_JWT>" \
     -H "Content-Type: application/json" \
     -d '{
           "cron_jobs": [
             {
               "id": "hourly-backup-job",
               "name": "Hourly Cloudflare Backup",
               "schedule": "0 0 * * * *",
               "payload": "auto-backup",
               "active": true
             }
           ]
         }'
```

---

## 5. Reloading the Scheduler

Because the scheduler registers job tables upon initial server boot, any additions or changes to `cron_jobs` must be synchronized into memory.

You can trigger an in-memory hot-reload without dropping active connections:

```bash
curl -X POST "https://api.your-app.com/api/v1/admin/system/reload" \
     -H "Authorization: Bearer <MASTER_KEY_OR_ADMIN_JWT>" \
     -H "Content-Type: application/json" \
     -d '{"target": "root"}'
```

---

## 6. Monitoring and Logs

* **Console Logging**: Any `console.log`, `console.info`, or `console.error` calls inside cron scripts are automatically saved to `_system_logs` in the database.
* **Audit Inspection**: Review execution outputs directly in the **Admin Dashboard > Logs** tab or by querying `GET /api/v1/admin/logs?type=system`.