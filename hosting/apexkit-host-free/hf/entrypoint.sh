#!/bin/bash

APP_STORAGE="/app/storage"
# This is the storage you mount just rename the path from `/data` -> `/data/storage` in the hf space ui when mounting
PERSISTENT_STORAGE="/data/storage"

echo "[Init] Setting up Hugging Face Storage Sync..."

# 1. Restore from persistent storage on boot
if [ -d "$PERSISTENT_STORAGE" ] && [ "$(ls -A $PERSISTENT_STORAGE 2>/dev/null)" ]; then
    echo "[Sync] Restoring data from persistent storage ($PERSISTENT_STORAGE) to NVMe ($APP_STORAGE)..."
    mkdir -p "$APP_STORAGE"
    # rsync is used because it is much faster and safer than cp
    rsync -a "$PERSISTENT_STORAGE/" "$APP_STORAGE/"
    echo "[Sync] Restore complete."
else
    echo "[Sync] No existing persistent data found. Starting fresh."
    mkdir -p "$APP_STORAGE"
    mkdir -p "$PERSISTENT_STORAGE"
fi

# Inject Replica ID if provided via Env Vars (Survives HF Space Restarts)
if [ -n "$APEX_REPLICA_ID" ]; then
    mkdir -p "$APP_STORAGE/system"
    echo "$APEX_REPLICA_ID" > "$APP_STORAGE/system/.replica_id"
    echo "[Init] Injected static Replica ID: $APEX_REPLICA_ID"
fi

# 2. Background Sync Loop (Every 5 minutes = 300 seconds)
sync_data() {
    while true; do
        sleep 300
        echo "[Sync] Backing up NVMe data to persistent storage..."
        # --delete ensures files deleted in the app are also removed from backup
        rsync -a --delete "$APP_STORAGE/" "$PERSISTENT_STORAGE/"
        echo "[Sync] Backup complete."
    done
}

# Start sync loop in the background
sync_data &
SYNC_PID=$!

# 3. Graceful Shutdown Handler (Catches Space restarts/pauses)
cleanup() {
    echo "[Shutdown] Caught stop signal. Stopping application..."
    kill -TERM "$APP_PID" 2>/dev/null
    wait "$APP_PID"
    
    echo "[Shutdown] Performing final data sync to persistent storage..."
    rsync -a --delete "$APP_STORAGE/" "$PERSISTENT_STORAGE/"
    echo "[Shutdown] Final sync complete. Safely exiting."
    
    kill -9 "$SYNC_PID" 2>/dev/null
    exit 0
}

# Trap termination signals to trigger the cleanup function
trap cleanup SIGINT SIGTERM

# 4. Start the application in the background so bash can listen for signals
echo "[App] Starting ApexKit on port ${PORT:-7860}..."
apexkit --port ${PORT:-7860} &
APP_PID=$!

# Wait for the application to exit
wait "$APP_PID"

# If the app crashes or exits naturally, do a final sync before the container dies
echo "[App] Application exited. Syncing data before container stops..."
rsync -a --delete "$APP_STORAGE/" "$PERSISTENT_STORAGE/"
kill -9 "$SYNC_PID" 2>/dev/null