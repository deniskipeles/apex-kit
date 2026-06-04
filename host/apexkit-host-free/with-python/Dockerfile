FROM alpine:3.19

# 1. Install dependencies
RUN apk add --no-cache curl jq msmtp ca-certificates tar python3 py3-pip bash

# 2. Create non-root user (Required for Hugging Face, safe for Render/Koyeb)
RUN adduser -D -u 1000 user
USER user
ENV HOME=/home/user
WORKDIR $HOME/app

# 3. Set up Python environment
RUN python3 -m venv $HOME/venv
ENV PATH="$HOME/venv/bin:$PATH"
RUN pip install --no-cache-dir flask boto3

# 4. Download latest ApexKit release from GitHub
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-musl.tar.gz")) | .browser_download_url') && \
    curl -L -o apexkit.tar.gz "$LATEST_URL" && \
    tar -xzf apexkit.tar.gz && \
    chmod +x apexkit && \
    rm apexkit.tar.gz

# 5. Create storage directories
RUN mkdir -p storage/system storage/tenants storage/sandboxes storage/backups

# 6. Copy host-free wrapper scripts
COPY --chown=user:user backup.py .
COPY --chown=user:user start.sh .
RUN chmod +x start.sh

# 7. Start
CMD ["./start.sh"]