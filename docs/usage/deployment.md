# Production Deployment Guide

Moving from local development to a production environment requires careful consideration of security, persistence, and performance.

## 1. Environment Variables

Configure your ApexKit instance using environment variables.

| Variable | Description |
| :--- | :--- |
| `APEXKIT_PORT` | The port the server listens on (default: 5000). |
| `APEXKIT_STORAGE_PATH` | Directory for SQLite and files (default: `./storage`). |
| `APEXKIT_MASTER_KEY` | Secret key for gRPC replication. |
| `APEXKIT_JWT_SECRET` | Custom secret for signing tokens. |
| `STRIPE_SECRET`, etc. | Any custom variables for your Edge Functions. |

## 2. Persistence

Ensure the `APEXKIT_STORAGE_PATH` is mounted to a persistent volume. If you are using Docker:

```bash
docker run -p 5000:5000 \
  -v /mnt/data/apexkit:/app/storage \
  deniskipeles/apexkit:latest
```

## 3. Reverse Proxy (Nginx / Caddy)

We recommend running ApexKit behind a reverse proxy for SSL termination and request logging.

### Example Caddyfile:
```caddy
api.yourdomain.com {
    reverse_proxy localhost:5000
}
```

## 4. Database Backups

ApexKit includes an automated backup system.

- **Manual Backup**: Go to **Admin > Backups > Create Backup**.
- **Automated**: Set the `APEXKIT_BACKUP_INTERVAL` (e.g., `daily`) and `APEXKIT_BACKUP_S3_BUCKET` to stream backups to the cloud.

## 5. Security Checklist

- [ ] **Change the Admin Password**: Ensure you are not using the default `password`.
- [ ] **Enable HTTPS**: Never run an API in production over plain HTTP.
- [ ] **Configure CORS**: Limit which domains can access your API.
- [ ] **Set Security Policies**: Ensure collections are not `public` unless intended.
- [ ] **Use API Keys**: Use scoped keys for internal services instead of the master admin account.

## 6. Scaling with Replicas

For high-traffic applications, use **Replicas** to scale read operations.

1. Deploy a Master node.
2. Deploy multiple Replica nodes pointing to the Master.
3. Use a Load Balancer (like Cloudflare or AWS ALB) to distribute traffic.

## 7. Monitoring

ApexKit exposes system logs via the dashboard and an internal API.
- **Logs**: Monitor `/api/v1/admin/logs`.
- **Metrics**: ApexKit is compatible with standard Prometheus exporters via specialized plugins.
