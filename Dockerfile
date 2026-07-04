FROM ubuntu:24.04

# Disable interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# 1. Install dependencies (using msmtp-mta to auto-create the sendmail symlink)
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    jq \
    msmtp \
    msmtp-mta \
    ca-certificates \
    tar \
    rsync \
    bash \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 2. Download & Extract Logic (Public Repository)
# Fetches the latest 'linux-gnu.tar.gz' archive directly from your public GitHub releases.
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-gnu.tar.gz")) | .browser_download_url') && \
    echo "URL: $LATEST_URL" && \
    curl -L -o apexkit.tar.gz "$LATEST_URL" && \
    echo "Extracting binary..." && \
    tar -xzf apexkit.tar.gz && \
    mv apexkit /usr/local/bin/apexkit && \
    chmod +x /usr/local/bin/apexkit && \
    rm apexkit.tar.gz

# 3. Prepare storage
RUN mkdir -p storage/system storage/tenants storage/sandboxes storage/backups

# 4. Run with dynamic port and Replica ID injection
#    - Cloud providers (like Koyeb/Render) provide the $PORT variable automatically.
#    - If $APEX_REPLICA_ID is set, it forces the app to use that ID to survive disk wipes.
CMD ["sh", "-c", "if [ -n \"$APEX_REPLICA_ID\" ]; then mkdir -p storage/system && echo \"$APEX_REPLICA_ID\" > storage/system/.replica_id && echo \"[Init] Injected static Replica ID: $APEX_REPLICA_ID\"; fi; exec apexkit --port ${PORT:-5000}"]