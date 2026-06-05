FROM alpine:3.19

# 1. Install dependencies including rsync and bash for the entrypoint script
RUN apk add --no-cache curl jq msmtp ca-certificates tar rsync bash && \
    ln -sf /usr/bin/msmtp /usr/sbin/sendmail

# 2. Create a non-root user (UID 1000) required by Hugging Face Spaces
RUN adduser -D -u 1000 user

WORKDIR /app

# 3. Download & Extract ApexKit Binary
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-musl.tar.gz")) | .browser_download_url') && \
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