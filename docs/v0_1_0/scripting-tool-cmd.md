# 🐚 System Command Tool (`$cmd`)

**Version:** 0.1.0  
**Context:** Root-Scoped Scripting only  
**Runtime:** Scoped process execution with concurrency management

The `$cmd` object provides a robust interface for executing shell commands and managing system processes directly from your JavaScript scripts. 

> [!CAUTION]
> **Security Restriction**: This tool is **strictly restricted** to scripts running in the **Root App** scope. Tenant and Sandbox scripts cannot access `$cmd` directly. To allow tenants to use specific system tools, Root Admins must create a "Public" visibility script that acts as a safe wrapper.

---

## 1. Concurrency & Safety
ApexKit manages system resources to prevent scripts from overwhelming the server:
*   **Concurrency Limit**: Only **5** background processes can run simultaneously (global limit).
*   **Execution Timeouts**: Synchronous runs default to **30s**. Background spawns default to **60s**.
*   **Process Isolation**: Each command can specify its own working directory (`cwd`) and environment variables.

---

## 2. API Reference

### `$cmd.run(program, args, options?)`
Executes a program and waits for it to complete. 
*   **Returns**: `Promise<{ stdout: string, stderr: string, status: number }>`
*   **Options**:
    *   `timeout`: Max time to wait in milliseconds.
    *   `cwd`: Working directory path.
    *   `env`: Object containing environment variables.

### `$cmd.spawn(program, args, options?)`
Spawns a program in the background and returns immediately.
*   **Returns**: `Promise<{ pid: number, status: "running" }>`
*   **Note**: The process is automatically killed if it exceeds the `timeout`.

### `$cmd.status(pid)`
Checks the current state of a process started via `.spawn()`.
*   **Returns**: `Promise<{ pid, status, exit_code, runtime_ms }>`
*   **Statuses**: `running`, `completed`, `failed`, `timed_out`, `unknown`.

---

## 3. Script Samples

### Sample 1: Media Processing (ffmpeg)
This is the most common use case for `$cmd`. It offloads heavy CPU tasks to native binaries.

```javascript
// Script: convert-to-gif (Root Only)
export default async function(req) {
    const { video_file } = await req.json();
    
    log(`Starting conversion for ${video_file}...`);

    const result = await $cmd.run("ffmpeg", [
        "-i", `storage/system/uploads/${video_file}`,
        "-t", "5", // only first 5 seconds
        "-vf", "scale=320:-1",
        "-f", "gif",
        "pipe:1" // output to stdout
    ], { 
        timeout: 15000,
        env: { "FFREPORT": "level=32" } 
    });

    if (result.status !== 0) {
        return new Response({ error: "Conversion failed", logs: result.stderr }, { status: 500 });
    }

    // saveFile registers it in the DB automatically
    const saved = await $zip.saveFile("preview.gif", $util.base64Encode(result.stdout), "image/gif");
    
    return new Response({ success: true, url: saved.url });
}
```

### Sample 2: Background Database Backup (git)
Using `.spawn()` for tasks that shouldn't block the API response.

```javascript
// Script: git-sync-backups
export default async function(req) {
    const repoPath = "/app/backups/git_repo";

    // Start a background sync
    const job = await $cmd.spawn("git", ["push", "origin", "main"], {
        cwd: repoPath,
        timeout: 120000 // Allow 2 minutes for network ops
    });

    return new Response({ 
        message: "Git sync started in background", 
        job_id: job.pid 
    });
}
```

### Sample 3: Process Monitor
A script that checks the status of a previously spawned background task.

```javascript
// Script: check-job-status
export default async function(req) {
    const { pid } = await req.json();
    
    if (!pid) return new Response({ error: "Missing PID" }, { status: 400 });

    const info = await $cmd.status(pid);

    if (info.status === "running") {
        return new Response({ 
            busy: true, 
            runtime: (info.runtime_ms / 1000).toFixed(1) + "s" 
        });
    }

    return new Response({
        busy: false,
        status: info.status,
        exit_code: info.exit_code
    });
}
```

### Set Concurrency Limits
A script that sets the number of cli processes that runs concurrently.

```javascript
// Initialization Script (Run on startup)
export default async function(req) {
    // Heavy tasks: strict limit
    await $cmd.setLimit("ffmpeg", 2);
    
    // Light tasks: higher limit
    await $cmd.setLimit("curl", 20);
    await $cmd.setLimit("ls", 50);
    
    // Global fallback for everything else
    await $cmd.setLimit("*", 5);
    
    return new Response({ success: true });
}
```

---

## 4. Best Practices

1.  **Always use Absolute Paths**: When accessing files outside of the `$zip` or `$fs` abstraction, use full paths to avoid confusion regarding the current working directory.
2.  **Sanitize Input**: Never pass raw user input into the `args` array without validation. Although ApexKit passes arguments safely (not via a raw shell string), malicious flags can still be dangerous.
3.  **Handle Timeouts**: Always specify a `timeout` for long-running operations like video encoding or external network calls to prevent the 5-process concurrency limit from being exhausted by "stuck" jobs.
4.  **Monitor Logs**: Use `log()` inside your scripts to capture the `result.stderr` if a command fails; it usually contains the reason why the binary exited with a non-zero status.