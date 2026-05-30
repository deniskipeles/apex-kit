Setting up a Cron Job in ApexKit is straightforward. ApexKit’s internal scheduler can automatically trigger **Server-Side Scripts** or **Internal Webhooks** on a predefined schedule.

Here is the exact step-by-step process, using the **Cloudflare R2 Backup** we just built as the perfect practical example!

---

### Step 1: Create the Target Script
First, we need a script for the Cron Job to execute. 

1. Open your ApexKit **Admin Dashboard**.
2. Go to **Scripts** and click **Create Script**.
3. Fill in the details:
   * **Name:** `auto-backup`
   * **Trigger Type:** `cron` *(This is just for your own organization, the scheduler ignores this field).*
   * **Code:**
```javascript
export default async function(req) {
    try {
        console.log("⏰ Cron Triggered: Starting auto-backup...");
        
        // Call the internal Python Flask server
        const res = await fetch("http://127.0.0.1:5000/backup", { method: "POST" });
        const data = await res.json();
        
        if (!res.ok) {
            console.error("Backup failed: " + data.message);
            throw new Error(data.message);
        }
        
        console.log("✅ Backup Success: " + data.message);
        return new Response({ success: true, details: data });
    } catch (e) {
        return new Response({ success: false, error: e.message }, { status: 500 });
    }
}
```

---

### Step 2: Register the Cron Job in Settings

Cron Jobs are stored in ApexKit's global settings. If your frontend dashboard has a **Cron Jobs** section under Settings, you can add it there. 

If your UI doesn't expose it yet, you can easily set it via the API using the **System Tools (Test Backend RPC)** area in the ApexApp Settings, or via a simple `cURL` command:

**Option A: Via cURL (Terminal)**
Run this to update your settings and schedule the script. *(Replace `your_master_key` and the URL)*.

```bash
curl -X PATCH https://your-space-name.hf.space/api/v1/admin/settings \
     -H "Authorization: Bearer your_master_key" \
     -H "Content-Type: application/json" \
     -d '{
           "cron_jobs": [
             {
               "id": "backup-job-1",
               "name": "Hourly Cloudflare Backup",
               "schedule": "0 0 * * * *",
               "payload": "auto-backup",
               "active": true
             }
           ]
         }'
```

### Understanding the Configuration:
* **`schedule`**: ApexKit uses standard 6-part cron expressions `(Seconds Minutes Hours DayOfMonth Month DayOfWeek)`. 
   * `0 0 * * * *` = Top of every hour.
   * `0 0 0 * * *` = Midnight every day.
   * `0 */15 * * * *` = Every 15 minutes.
* **`payload`**: This is the "Action". If you type a string without a slash (e.g., `"auto-backup"`), ApexKit will look for a **Script** with that name and execute it.
* **Internal Webhooks (Alternative)**: If your payload starts with a `/` (e.g., `"/api/v1/admin/some-endpoint"`), ApexKit will generate an internal Admin JWT and make a secure `POST` HTTP request to that URL instead of running a script!

---

### Step 3: Reload the System

Because the Cron Scheduler loads its timetable into memory when the server boots, you need to tell ApexKit to refresh its schedules.

You can either:
1. Restart your Hugging Face space / Docker container.
2. **Or**, trigger a seamless hot-reload via the API without dropping the server:

```bash
curl -X POST https://your-space-name.hf.space/api/v1/admin/system/reload \
     -H "Authorization: Bearer your_master_key" \
     -H "Content-Type: application/json" \
     -d '{"target": "root"}'
```

### How to Monitor Your Cron Jobs
You can verify your cron jobs are running by checking the **System Console** in the ApexApp settings, or by going to the `/api/v1/admin/logs` endpoint. Every time the scheduler triggers a script, it will print your `console.log` statements directly to the system logs!