#!/bin/bash

# 1. Pull latest DB from S3 and Restore it cleanly
echo "🔄 [Stage 1] Attempting to restore storage from S3..."
python backup.py --restore

# 2. Start the internal background backup listener
echo "🚀 [Stage 2] Starting local Backup API (Flask) in the background..."
python backup.py --serve &

# 3. Start the main App
# Koyeb and Render inject the $PORT env variable. If empty, default to 7860.
BIND_PORT=${PORT:-7860}
echo "⚡ [Stage 3] Starting ApexKit on port $BIND_PORT..."
exec ./apexkit --port $BIND_PORT