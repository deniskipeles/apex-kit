# ⏰ ApexKit Scheduler & Cron Jobs

**Version:** 0.1.0
**Context:** Background Tasks, Automation, and Maintenance.

ApexKit includes a built-in scheduler (powered by `tokio-cron-scheduler`) that eliminates the need for external tools like Redis/Celery or OS-level crontabs.

---

## 1. How it Works

The scheduler is **Multi-Tenant Aware**.

1.  **Global Ticker:** A master job runs every minute.
2.  **Context Scanning:** It iterates through the Root App, all active Tenants, and active Sandboxes.
3.  **Local Execution:** For each context, it checks the configured `cron_jobs`. If a job is due, it executes the script **within that tenant's isolation scope**.

This means a script `clean_logs` running for Tenant A will only delete Tenant A's logs, even though the scheduler process is global.

---

## 2. Defining Cron Jobs

Cron jobs are defined in the **Admin UI > Settings > System**.

### Job Structure
| Field | Description | Example |
| :--- | :--- | :--- |
| **Name** | Human readable label. | `Daily Report` |
| **Schedule** | Standard Cron Expression (5 or 6 fields). | `0 0 9 * * *` (Daily at 9am UTC) |
| **Payload** | The target to execute. Can be a **Script Name** or a **Webhook URL**. | `generate-report` |
| **Active** | Toggle to enable/disable. | `true` |

### Schedule Examples
*   `0 * * * * *` -> Every Minute (Second: 0)
*   `0 */15 * * * *` -> Every 15 Minutes
*   `0 0 * * * *` -> Hourly
*   `0 0 0 * * *` -> Daily at Midnight
*   `0 0 9 * * MON` -> Every Monday at 9am

---

## 3. Creating a Job Script

To run logic on a schedule, create a Script with Trigger Type: **`cron`**.

**Script Name:** `daily-cleanup`

```javascript
// Trigger: cron
export default async function(req) {
    log("Starting daily cleanup...");

    // 1. Database Operations (Scoped to current Tenant)
    // Find old logs
    const oldLogs = await $db.find("_audit_logs", {}); 
    
    // ... logic to delete ...
    
    // 2. External API calls
    await $http.post("https://slack.com/webhook", { text: "Cleanup complete" });

    return new Response({ success: true });
}
```

**Then register it in Settings:**
*   **Name:** Cleanup
*   **Schedule:** `0 0 2 * * *` (2am)
*   **Payload:** `daily-cleanup`

---

## 4. Webhook Jobs

You can also use the scheduler to call internal or external API endpoints directly without writing a wrapper script.

If the **Payload** starts with `/`, it is treated as an internal webhook relative to the current tenant's API.

*   **Payload:** `/api/v1/run/my-script`
*   **Behavior:** The scheduler makes a POST request to `http://127.0.0.1:5000/tenant/{id}/api/v1/run/my-script`.

---

## 5. System Maintenance Jobs

ApexKit includes hardcoded maintenance jobs that run automatically:

*   **Log Retention:** Runs daily at 3 AM. Deletes rows from `_audit_logs` older than the configured retention period (default 7 days).
*   **Cache Cleanup:** (Internal) Evicts unused Tenant DB connections from memory every hour.

---

## 6. Debugging

Logs from cron jobs appear in the **Admin UI > Logs**.

*   **Source:** `scheduler` or `script`
*   **Message:** "Executing Cron: Daily Report"

To test a job immediately, use the **Run Script** button in the Scripts view, as the logic is identical.