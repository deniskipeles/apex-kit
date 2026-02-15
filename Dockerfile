FROM debian:bookworm-slim

# 1. Install dependencies
RUN apt-get update && \
    apt-get install -y curl ca-certificates jq sendmail && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 2. This argument is populated by Koyeb's Build Environment Variables
ARG GITHUB_TOKEN

# 3. Download Logic
#    - Hits the GitHub API using the Token
#    - Finds the Asset ID for the 'linux-musl' binary
#    - Downloads via the /assets endpoint (required for private repos)
RUN if [ -z "$GITHUB_TOKEN" ]; then echo "❌ ERROR: GITHUB_TOKEN not found in Build Env"; exit 1; fi && \
    echo "Resolving private asset ID..." && \
    ASSET_ID=$(curl -s -H "Authorization: token ${GITHUB_TOKEN}" \
    "https://api.github.com/repos/deniskipeles/apex-kit/releases?per_page=1" | \
    jq -r '.[0].assets[] | select(.name | contains("linux-musl")) | .id') && \
    echo "Downloading Asset ID: ${ASSET_ID}..." && \
    curl -L -H "Authorization: token ${GITHUB_TOKEN}" \
    -H "Accept: application/octet-stream" \
    -o /usr/local/bin/apexkit \
    "https://api.github.com/repos/deniskipeles/apex-kit/releases/assets/${ASSET_ID}" && \
    chmod +x /usr/local/bin/apexkit

# 4. Prepare storage
RUN mkdir -p storage/system storage/tenants storage/sandboxes storage/backups

# 5. Run with dynamic port
#    - Koyeb provides the $PORT variable automatically.
#    - Defaults to 5000 if not provided.
CMD ["sh", "-c", "apexkit --port ${PORT:-5000}"]