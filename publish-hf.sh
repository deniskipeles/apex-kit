#!/usr/bin/env bash
set -euo pipefail

# ------------------------------------------------------------------------------
# Configuration
# ------------------------------------------------------------------------------
# Replace with your Hugging Face username and repository name
HF_REPO="${HF_REPO:-kipeles/apexkit-releases}"
# Repo type: "dataset" (recommended for binaries), "model", or "space"
REPO_TYPE="${REPO_TYPE:-dataset}"

TAG="${TAG:-v0.1.0-beta.2}"
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

# ------------------------------------------------------------------------------
# Stage Root Files (README, LICENSE) and Tagged Binaries
# ------------------------------------------------------------------------------
STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "${STAGING_DIR}"' EXIT

echo "📁 Staging files for atomic repository commit..."

# 1. Copy README.md & LICENSE to the repo root
if [ -f "README.md" ]; then
  cp "README.md" "${STAGING_DIR}/"
  echo "  📄 Included /README.md"
else
  echo "⚠️  README.md not found in current directory"
fi

if [ -f "LICENSE" ]; then
  cp "LICENSE" "${STAGING_DIR}/"
  echo "  ⚖️  Included /LICENSE"
else
  echo "⚠️  LICENSE not found in current directory"
fi

# 2. Copy Logo SVG for repo branding
mkdir -p "${STAGING_DIR}/static/images"
LOGO_FOUND=false

for logo_candidate in "apex-kit/static/images/apexkit-logo.svg" "static/images/apexkit-logo.svg"; do
  if [ -f "${logo_candidate}" ]; then
    cp "${logo_candidate}" "${STAGING_DIR}/static/images/apexkit-logo.svg"
    echo "  🎨 Included /static/images/apexkit-logo.svg (from ${logo_candidate})"
    LOGO_FOUND=true
    break
  fi
done

if [ "${LOGO_FOUND}" = false ]; then
  echo "⚠️  apexkit-logo.svg not found"
fi

# 3. Copy binaries into the release tag folder (${TAG}/...)
mkdir -p "${STAGING_DIR}/${TAG}"
cp -r "${DIST_DIR}/"* "${STAGING_DIR}/${TAG}/"

# Also include copies of LICENSE and README inside the release folder
[ -f "LICENSE" ] && cp "LICENSE" "${STAGING_DIR}/${TAG}/"
[ -f "README.md" ] && cp "README.md" "${STAGING_DIR}/${TAG}/"

echo "  📦 Included ${TAG}/* artifacts"

# ------------------------------------------------------------------------------
# Upload Staged Tree in a Single Atomic Commit
# ------------------------------------------------------------------------------
python3 - <<EOF
import os
import sys
from huggingface_hub import HfApi

repo_id = "${HF_REPO}"
repo_type = "${REPO_TYPE}"
tag = "${TAG}"
staging_dir = "${STAGING_DIR}"
token = os.environ["HF_TOKEN"]

api = HfApi(token=token)

print(f"⏳ Uploading README, LICENSE, and {tag}/* to {repo_id}...")

try:
    api.upload_folder(
        folder_path=staging_dir,
        path_in_repo="",  # Upload to repository root
        repo_id=repo_id,
        repo_type=repo_type,
        commit_message=f"Release {tag} with root LICENSE and README.md",
    )
    print("✅ Atomic upload complete!")
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
echo ""
echo "Root Repository Links:"
echo "  📄 https://huggingface.co/${REPO_TYPE}s/${HF_REPO}/blob/main/README.md"
echo "  ⚖️  https://huggingface.co/${REPO_TYPE}s/${HF_REPO}/blob/main/LICENSE"
echo "  🎨 https://huggingface.co/${REPO_TYPE}s/${HF_REPO}/resolve/main/static/images/apexkit-logo.svg"
echo "======================================================================"