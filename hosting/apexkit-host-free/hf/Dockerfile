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

# 2. Delete the pre-created 'ubuntu' user (UID 1000) and create your custom 'user' (UID 1000)
RUN userdel -r ubuntu && useradd -m -u 1000 user

WORKDIR /app

# 3. Download & Extract ApexKit Binary
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-gnu.tar.gz")) | .browser_download_url') && \
    echo "URL: $LATEST_URL" && \
    curl -L -o apexkit.tar.gz "$LATEST_URL" && \
    tar -xzf apexkit.tar.gz && \
    mv apexkit /usr/local/bin/apexkit && \
    chmod +x /usr/local/bin/apexkit && \
    rm apexkit.tar.gz

# 4. Setup directories
# /app/storage is the fast ephemeral NVMe
# /data/storage is the mounted persistent storage provided by HF
RUN mkdir -p /app/storage /data/storage && \
    chown -R user:user /app /data

# 5. Copy and prepare the entrypoint script
COPY --chown=user:user entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# 6. Switch to the non-root user
USER user

# Hugging Face exposes port 7860 by default
ENV PORT=7860

# 7. Start via the sync manager script
ENTRYPOINT ["/app/entrypoint.sh"]