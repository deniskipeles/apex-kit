#!/bin/bash

# Configuration
BIND_PORT=${PORT:-5000}
BUCKET=${S3_BUCKET_NAME}
REGION=${S3_REGION:-auto}
ACCESS_KEY=${S3_ACCESS_KEY}
SECRET_KEY=${S3_SECRET_KEY}
ENDPOINT=${S3_ENDPOINT_URL}
PREFIX="apexkit_backup"

# ==============================================================================
# S3 AWS Signature V4 cURL Helper
# ==============================================================================
s3_curl() {
    local method=$1
    local path=$2
    local file=$3
    
    local host=$(echo $ENDPOINT | sed -e 's|^[^/]*//||' -e 's|/.*$||')
    local url="${ENDPOINT}/${BUCKET}/${path}"
    
    local date=$(date -u +"%Y%m%dT%H%M%SZ")
    local date_short=$(date -u +"%Y%m%d")
    
    local empty_hash="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    local payload_hash=$empty_hash
    if [ "$method" == "PUT" ] && [ -f "$file" ]; then
        payload_hash=$(sha256sum "$file" | awk '{print $1}')
    fi

    local canonical_req="${method}\n/${BUCKET}/${path}\n\nhost:${host}\nx-amz-content-sha256:${payload_hash}\nx-amz-date:${date}\n\nhost;x-amz-content-sha256;x-amz-date\n${payload_hash}"
    local canonical_hash=$(echo -en "$canonical_req" | sha256sum | awk '{print $1}')
    local string_to_sign="AWS4-HMAC-SHA256\n${date}\n${date_short}/${REGION}/s3/aws4_request\n${canonical_hash}"

    local kSecret="AWS4${SECRET_KEY}"
    local kDate=$(echo -en "${date_short}" | openssl dgst -sha256 -mac HMAC -macopt hexkey:$(echo -n "$kSecret" | xxd -p -c 256) | awk '{print $2}')
    local kRegion=$(echo -en "${REGION}" | openssl dgst -sha256 -mac HMAC -macopt hexkey:${kDate} | awk '{print $2}')
    local kService=$(echo -en "s3" | openssl dgst -sha256 -mac HMAC -macopt hexkey:${kRegion} | awk '{print $2}')
    local kSigning=$(echo -en "aws4_request" | openssl dgst -sha256 -mac HMAC -macopt hexkey:${kService} | awk '{print $2}')
    local signature=$(echo -en "$string_to_sign" | openssl dgst -sha256 -mac HMAC -macopt hexkey:${kSigning} | awk '{print $2}')

    local auth_header="AWS4-HMAC-SHA256 Credential=${ACCESS_KEY}/${date_short}/${REGION}/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=${signature}"

    if [ "$method" == "PUT" ]; then
        curl -s -X PUT -T "$file" \
            -H "Host: ${host}" \
            -H "x-amz-date: ${date}" \
            -H "x-amz-content-sha256: ${payload_hash}" \
            -H "Authorization: ${auth_header}" \
            "$url"
    else
        curl -s -f -X "$method" \
            -H "Host: ${host}" \
            -H "x-amz-date: ${date}" \
            -H "x-amz-content-sha256: ${payload_hash}" \
            -H "Authorization: ${auth_header}" \
            -o "$file" \
            "$url"
    fi
}

# ==============================================================================
# RESTORE LOGIC (Runs on Boot)
# ==============================================================================
echo "🔄 [Stage 1] Checking S3 for existing backups..."

if [ -n "$ENDPOINT" ]; then
    # Download XML list of objects from S3
    s3_curl GET "?prefix=${PREFIX}" "/tmp/s3_list.xml"
    
    # Extract the newest backup filename using grep (parsing XML via Regex)
    LATEST_BACKUP=$(cat /tmp/s3_list.xml | grep -o "<Key>[^<]*" | cut -d'>' -f2 | sort -r | head -n 1)

    if [ -n "$LATEST_BACKUP" ]; then
        echo "📥 Found latest backup: $LATEST_BACKUP. Downloading..."
        s3_curl GET "$LATEST_BACKUP" "restore_candidate.tar.gz"
        
        if [ -f "restore_candidate.tar.gz" ]; then
            echo "📦 Restoring database safely via CLI..."
            ./apexkit restore restore_candidate.tar.gz --yes
            rm restore_candidate.tar.gz
            echo "✅ Restoration complete."
        else
            echo "⚠️ Download failed. Starting fresh."
        fi
    else
        echo "⚠️ No backups found. Starting fresh."
    fi
else
    echo "⚠️ S3 credentials not provided. Skipping restore."
fi

# ==============================================================================
# SHUTDOWN LOGIC (Triggered by SIGTERM)
# ==============================================================================
cleanup() {
    echo "🛑 [Shutdown] Received SIGTERM! Stopping ApexKit gracefully..."
    
    # Send SIGTERM to the Rust binary and wait for it to flush WAL and close connections
    kill -TERM "$APP_PID" 2>/dev/null
    wait "$APP_PID"

    if [ -n "$ENDPOINT" ]; then
        echo "📦 [Backup] Zipping database via CLI..."
        TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
        ARCHIVE_NAME="${PREFIX}_${TIMESTAMP}.tar.gz"
        
        # We backup all root databases and files safely
        ./apexkit backup --root="*" --out "$ARCHIVE_NAME"

        echo "☁️ [Backup] Uploading $ARCHIVE_NAME to S3..."
        s3_curl PUT "$ARCHIVE_NAME" "$ARCHIVE_NAME"
        
        echo "✅ [Backup] Upload complete. Safely exiting."
    else
        echo "⚠️ [Backup] No S3 config. Exiting without backing up."
    fi
    exit 0
}

# Trap the SIGTERM signal (sent by Render/Koyeb on redeploy or sleep)
trap cleanup SIGTERM SIGINT

# ==============================================================================
# APP LAUNCH
# ==============================================================================
echo "⚡ [Stage 2] Starting ApexKit on port $BIND_PORT..."
# We run it in the background so bash can wait and listen for the trap
./apexkit --port $BIND_PORT &
APP_PID=$!

# Wait for the application to exit
wait "$APP_PID"