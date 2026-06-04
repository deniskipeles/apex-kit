#!/bin/bash

# ==============================================================================
# APEXKIT BASH DEPLOYMENT WRAPPER
# Automatically restores DB from S3 on boot, and backs it up on shutdown.
# ==============================================================================

BIND_PORT=${PORT:-5000}
BUCKET=${S3_BUCKET_NAME}
ENDPOINT=${S3_ENDPOINT_URL}
PREFIX="apexkit_backup"
MAX_BACKUPS_TO_KEEP=${MAX_BACKUPS:-5}

echo "☁️ [Init] Configuring S3 Client..."
if [ -n "$ENDPOINT" ]; then
    mc alias set s3cloud "$ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY" --api S3v4 > /dev/null 2>&1
fi

# ==============================================================================
# RESTORE LOGIC (Runs on Boot)
# ==============================================================================
echo "🔄 [Stage 1] Checking S3 for existing backups..."

if [ -n "$ENDPOINT" ]; then
    # Grab the single newest file
    LATEST_BACKUP=$(mc ls s3cloud/$BUCKET/ | grep "$PREFIX" | sort -r | head -n 1 | awk '{print $NF}')

    if [ -n "$LATEST_BACKUP" ]; then
        echo "📥 Found latest backup: $LATEST_BACKUP. Downloading..."
        mc cp "s3cloud/$BUCKET/$LATEST_BACKUP" restore_candidate.tar.gz
        
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
# SHUTDOWN LOGIC (Triggered by SIGTERM from Docker/Render/Koyeb)
# ==============================================================================
cleanup() {
    echo "🛑 [Shutdown] Received SIGTERM! Stopping ApexKit gracefully..."
    kill -TERM "$APP_PID" 2>/dev/null
    wait "$APP_PID"

    if [ -n "$ENDPOINT" ]; then
        echo "📦 [Backup] Zipping database via CLI..."
        TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
        ARCHIVE_NAME="${PREFIX}_${TIMESTAMP}.tar.gz"
        
        # Safely package the active database
        ./apexkit backup --root="*" --tenants="*" --out "$ARCHIVE_NAME"

        echo "☁️ [Backup] Uploading $ARCHIVE_NAME to S3..."
        mc cp "$ARCHIVE_NAME" "s3cloud/$BUCKET/$ARCHIVE_NAME"
        
        # --- PRUNE OLD BACKUPS ---
        echo "🧹 [Backup] Pruning old backups (Keeping $MAX_BACKUPS_TO_KEEP)..."
        # 1. List all matching files. 2. Sort Newest->Oldest. 3. Skip the top N. 4. Delete the rest.
        mc ls "s3cloud/$BUCKET/" | grep "$PREFIX" | sort -r | awk '{print $NF}' | tail -n +$((MAX_BACKUPS_TO_KEEP + 1)) | while read OLD_BACKUP; do
            if [ -n "$OLD_BACKUP" ]; then
                echo "  🗑️ Deleting $OLD_BACKUP..."
                mc rm "s3cloud/$BUCKET/$OLD_BACKUP"
            fi
        done

        echo "✅ [Backup] Upload & Prune complete. Safely exiting."
    else
        echo "⚠️ [Backup] No S3 config. Exiting without backing up."
    fi
    exit 0
}

trap cleanup SIGTERM SIGINT

# ==============================================================================
# APP LAUNCH
# ==============================================================================
echo "⚡ [Stage 2] Starting ApexKit on port $BIND_PORT..."
./apexkit --port $BIND_PORT &
APP_PID=$!

wait "$APP_PID"