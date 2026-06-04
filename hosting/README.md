# 🚀 ApexKit Hosting Guide

ApexKit is designed to be fully portable. While standard hosting requires a persistent volume to store your SQLite databases and uploads, we have engineered a solution to run ApexKit on **ephemeral (stateless) environments** like Hugging Face Spaces, Koyeb, and Render Free Tiers.

By hooking into cloud storage (S3/R2) and OS lifecycle signals (`SIGTERM`), these environments pull your latest database into ultra-fast NVMe memory on boot, and push a safe backup back to cloud storage right before the container spins down.

---

## 📂 Directory Structure

```text
hosting/
├── apexkit-host-free/
│   ├── with-bash/          # Ultra-lightweight Alpine Linux deployment
│   └── with-python/        # Heavier deployment (for Hugging Face / Python ML)
└── docker-files/
    └── bookworm/           # Debian-based generic deployment (Persistent Disk needed)
```

---

## 🌩️ Ephemeral Hosting (Stateless Deployments)

Use these setups if you are deploying to **Render**, **Koyeb Free Tier**, **DigitalOcean App Platform**, or **Hugging Face Spaces**.

### How it Works
1. **Boot**: The container queries your S3 bucket. If it finds a `.tar.gz` backup, it downloads it and uses the built-in `apexkit restore` CLI to safely unpack your databases.
2. **Run**: ApexKit runs in the background. It reads/writes data to the local disk at blazing-fast NVMe speeds.
3. **Shutdown**: When the platform decides to put your app to sleep or redeploy it, it sends a `SIGTERM` signal. The wrapper script intercepts this, gracefully shuts down ApexKit, runs `apexkit backup`, uploads the new archive to S3, prunes old backups, and finally exits.

### 🔑 Required Environment Variables

Regardless of which environment you choose, you must configure your S3 bucket (Cloudflare R2, AWS S3, MinIO, or DigitalOcean Spaces) in your provider's dashboard:

| Variable | Example Value | Description |
| :--- | :--- | :--- |
| `APEXKIT_MASTER_KEY` | `your_32_byte_base64_string=` | The encryption key for your database. |
| `S3_ENDPOINT_URL` | `https://<ID>.r2.cloudflarestorage.com` | Your S3 provider endpoint. |
| `S3_BUCKET_NAME` | `apexkit-backups` | The name of your bucket. |
| `S3_ACCESS_KEY` | `abc123xyz` | S3 Access Key. |
| `S3_SECRET_KEY` | `super_secret_key` | S3 Secret Key. |
| `S3_REGION` | `auto` | Bucket region (use `auto` for Cloudflare R2). |
| `MAX_BACKUPS` | `5` | (Optional) How many backups to retain in the bucket. |

---

### Option 1: `with-bash` (Recommended for Koyeb / Render)
This is an ultra-lightweight Alpine Linux deployment. It uses `mc` (MinIO Client) to securely and robustly sync files with S3.

**Deployment Steps:**
1. Point your hosting provider to the `hosting/apexkit-host-free/with-bash/Dockerfile`.
2. Add the required environment variables.
3. Deploy! The Bash script will automatically detect the `$PORT` assigned by the platform and bind to it.

### Option 2: `with-python` (Required for Hugging Face Spaces)
Hugging Face Spaces enforce strict environment rules (running as UID 1000) and often prefer Python environments for Machine Learning workflows. 

Because HF Spaces can sleep randomly, this setup runs a tiny Flask server on port `5000` alongside ApexKit (running on HF's required port `7860`).

**Deployment Steps:**
1. Point Hugging Face to the `hosting/apexkit-host-free/with-python/Dockerfile`.
2. Ensure you add the S3 environment variables to your Space Secrets.
3. **Manual Backup Trigger:** Because Hugging Face Spaces do not always send a clean `SIGTERM` when pausing a space, you can hit the local Flask server to manually trigger a backup at any time.
    * Send a `POST` request to `https://your-space-url.hf.space/backup`
    * Or set up a cron job (like cron-job.org) to hit that endpoint every hour.

---

## 💾 Standard Hosting (Stateful Deployments)

If you are running ApexKit on a VPS (DigitalOcean Droplet, AWS EC2, Hetzner) or a platform with persistent volumes (Railway, Fly.io with Volumes), you do **not** need the S3 syncing scripts.

### Option 1: Bookworm / Debian
We provide a standard Debian Bookworm Dockerfile in `hosting/docker-files/bookworm/Dockerfile`.

**Deployment Steps:**
1. Mount a persistent volume to `/app/storage`.
2. Deploy the container.
3. That's it! ApexKit will write directly to your persistent volume.

### Automated Backups on Stateful Hosts
If you are using stateful hosting but still want off-site backups to S3, you can use ApexKit's built-in Cron Scheduler.

1. Go to **Settings -> System Backups** in your ApexKit Admin Dashboard.
2. Enable Backups, enter your S3 credentials, and set a Cron schedule (e.g., `0 2 * * *` for daily at 2 AM).
3. ApexKit will handle the rest natively!