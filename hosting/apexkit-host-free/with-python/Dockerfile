FROM ubuntu:24.04

# Disable interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# 1. Install dependencies
# We include python3-venv on Ubuntu to support virtual environments natively,
# and msmtp-mta to auto-create the standard sendmail symlink.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    jq \
    msmtp \
    msmtp-mta \
    ca-certificates \
    tar \
    python3 \
    python3-pip \
    python3-venv \
    bash \
    && rm -rf /var/lib/apt/lists/*

# 2. Create non-root user (Required for Hugging Face, safe for Render/Koyeb)
# Ubuntu 24.04 pre-creates a user named 'ubuntu' (UID 1000). We delete it first.
RUN userdel -r ubuntu && useradd -m -u 1000 user
USER user
ENV HOME=/home/user
WORKDIR $HOME/app

# 3. Set up Python environment
RUN python3 -m venv $HOME/venv
ENV PATH="$HOME/venv/bin:$PATH"
RUN pip install --no-cache-dir flask boto3

# 4. Download latest ApexKit release from GitHub (using the native glibc binary)
RUN echo "Downloading latest public release archive..." && \
    LATEST_URL=$(curl -s https://api.github.com/repos/deniskipeles/apexkit/releases/latest | \
    jq -r '.assets[] | select(.name | contains("linux-gnu.tar.gz")) | .browser_download_url') && \
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