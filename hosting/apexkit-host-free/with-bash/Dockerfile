FROM ubuntu:24.04

# Disable interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# 1. Install pure-bash and other utilities
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    openssl \
    tar \
    bash \
    jq \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install the official MinIO Client (mc) and symlink it to "minio-client" for script compatibility
RUN curl -L -o /usr/local/bin/mc https://dl.min.io/client/mc/release/linux-amd64/mc && \
    chmod +x /usr/local/bin/mc && \
    ln -sf /usr/local/bin/mc /usr/local/bin/minio-client

# 2. Add non-root user for security (Koyeb/Render friendly)
# Ubuntu 24.04 pre-creates a user named 'ubuntu' (UID 1000). We delete it first.
RUN userdel -r ubuntu && useradd -m -u 1000 user
USER user
ENV HOME=/home/user
WORKDIR $HOME/app

# 3. Download latest ApexKit release from GitHub
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-gnu.tar.gz")) | .browser_download_url') && \
    curl -L -o apexkit.tar.gz "$LATEST_URL" && \
    tar -xzf apexkit.tar.gz && \
    chmod +x apexkit && \
    rm apexkit.tar.gz

# 4. Create storage directories
RUN mkdir -p storage/system storage/tenants storage/sandboxes storage/backups

# 5. Copy and prepare the startup script
COPY --chown=user:user start.sh .
RUN chmod +x start.sh

# 6. Start the app via the wrapper script
CMD ["./start.sh"]