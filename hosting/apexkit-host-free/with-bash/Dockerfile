FROM alpine:3.19

# 1. Install pure-bash dependencies (curl, openssl for HMAC signing, tar)
RUN apk add --no-cache curl openssl tar bash jq ca-certificates minio-client

# 2. Add non-root user for security (Koyeb/Render friendly)
RUN adduser -D -u 1000 user
USER user
ENV HOME=/home/user
WORKDIR $HOME/app

# 3. Download latest ApexKit release from GitHub
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-musl.tar.gz")) | .browser_download_url') && \
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