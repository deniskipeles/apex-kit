FROM debian:trixie-slim

# Disable interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# 1. Install dependencies
# msmtp-mta is installed to automatically configure the standard sendmail symlink.
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl jq ca-certificates msmtp msmtp-mta tar && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 2. Download & Extract Logic (Public Repository)
# Fetches the latest 'linux-gnu.tar.gz' archive directly from the public GitHub releases.
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

# 4. Run with dynamic port
#    - Koyeb provides the $PORT variable automatically.
#    - Defaults to 5000 if not provided.
CMD ["sh", "-c", "apexkit --port ${PORT:-5000}"]