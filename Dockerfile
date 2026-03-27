FROM alpine:3.19

# 1. Install dependencies
# We install libc6-compat just in case our binary has any dynamic linking needs,
# and install curl, jq, and msmtp (the Alpine alternative to sendmail)
RUN apk add --no-cache curl jq msmtp ca-certificates && \
    ln -sf /usr/bin/msmtp /usr/sbin/sendmail

WORKDIR /app

# 2. Download Logic (Public Repository)
# Fetches the latest 'linux-musl' binary directly from your public GitHub releases.
RUN echo "Downloading latest public release..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-musl")) | .browser_download_url') && \
    curl -L -o /usr/local/bin/apexkit "$LATEST_URL" && \
    chmod +x /usr/local/bin/apexkit

# 3. Prepare storage
RUN mkdir -p storage/system storage/tenants storage/sandboxes storage/backups

# 4. Run with dynamic port
#    - Koyeb provides the $PORT variable automatically.
#    - Defaults to 5000 if not provided.
CMD ["sh", "-c", "apexkit --port ${PORT:-5000}"]