FROM alpine:3.19

# 1. Install dependencies
# We install libc6-compat just in case our binary has any dynamic linking needs,
# and install curl, jq, tar, and msmtp (the Alpine alternative to sendmail)
RUN apk add --no-cache curl jq msmtp ca-certificates tar gcompat libstdc++ && \
    ln -sf /usr/bin/msmtp /usr/sbin/sendmail

WORKDIR /app

# 2. Download & Extract Logic (Public Repository)
# Fetches the latest 'linux-gnu.tar.gz' archive directly from your public GitHub releases.
# linux-musl was depreciated 
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