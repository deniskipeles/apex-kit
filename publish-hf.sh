#!/usr/bin/env bash
set -euo pipefail

# ------------------------------------------------------------------------------
# Configuration
# ------------------------------------------------------------------------------
# Replace with your Hugging Face username and repository name
HF_REPO="${HF_REPO:-kipeles/apexkit-releases}"
# Repo type: "dataset" (recommended for binaries), "model", or "space"
REPO_TYPE="${REPO_TYPE:-dataset}"

TAG="${TAG:-v0.1.0-beta.1}"
DIST_DIR="$(pwd)/dist"

if [ ! -d "${DIST_DIR}" ]; then
  echo "❌ Error: Directory '${DIST_DIR}' not found. Run ./build-all.sh first."
  exit 1
fi

# Ensure HF_TOKEN is set
if [ -z "${HF_TOKEN:-}" ]; then
  echo "❌ Error: HF_TOKEN environment variable is not set."
  echo "👉 Run: export HF_TOKEN=\"hf_your_write_token_here\""
  exit 1
fi

echo "======================================================================"
echo "🚀 Publishing ApexKit ${TAG} to Hugging Face Hub"
echo "📂 Target: ${HF_REPO} (${REPO_TYPE}) -> ${TAG}/*"
echo "======================================================================"

# Ensure huggingface_hub is installed
if ! python3 -c "import huggingface_hub" &> /dev/null; then
  echo "📦 Installing huggingface_hub..."
  pip3 install --quiet --no-cache-dir "huggingface_hub>=0.20.0"
fi

# Upload the entire dist folder to the tag subfolder in the repository
python3 - <<EOF
import os
import sys
from huggingface_hub import HfApi

repo_id = "${HF_REPO}"
repo_type = "${REPO_TYPE}"
tag = "${TAG}"
dist_dir = "${DIST_DIR}"
token = os.environ["HF_TOKEN"]

api = HfApi(token=token)

print(f"⏳ Uploading files from {dist_dir} to {repo_id}/{tag}...")

try:
    api.upload_folder(
        folder_path=dist_dir,
        path_in_repo=tag,
        repo_id=repo_id,
        repo_type=repo_type,
        commit_message=f"Release {tag} binaries",
    )
    print("✅ Upload complete!")
except Exception as e:
    print(f"❌ Upload failed: {e}", file=sys.stderr)
    sys.exit(1)
EOF

echo ""
echo "======================================================================"
echo "🎉 Release Published Successfully!"
echo ""
echo "Direct Download CDN Links:"
for file in "${DIST_DIR}"/*; do
  fname="$(basename "$file")"
  echo "  📥 https://huggingface.co/${REPO_TYPE}s/${HF_REPO}/resolve/main/${TAG}/${fname}"
done
echo "======================================================================"