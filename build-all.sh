#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# High-Speed Multi-Target Parallel Build Pipeline for ApexKit
# ==============================================================================

DIST_DIR="$(pwd)/dist"
ADMIN_DIR="$(pwd)/admin-ui"
STATIC_DIR="$(pwd)/static/dashboard"
SDK_DIR="$(pwd)/.cache/sdks"
MACOS_SDK_URL="https://github.com/joseluisq/macosx-sdks/releases/download/12.3/MacOSX12.3.sdk.tar.xz"
MACOS_SDK_PATH="${SDK_DIR}/MacOSX12.3.sdk"

# Maximum concurrent target builds (Default: 2 to prevent RAM exhaustion; adjust as needed)
MAX_CONCURRENT_BUILDS="${MAX_JOBS:-2}"

# Filter flags (allows: ./build-all.sh --only-perf, ./build-all.sh --only-mac, etc.)
FILTER_TARGET="${1:-all}"

echo "======================================================================"
echo "⚡ Starting ApexKit High-Speed Parallel Build Pipeline"
echo "⚙️  Concurrency Limit: ${MAX_CONCURRENT_BUILDS} targets in parallel"
echo "======================================================================"

# ------------------------------------------------------------------------------
# 1. Pipeline: Start Frontend Build & SDK Download Concurrently
# ------------------------------------------------------------------------------
echo "⏳ [Pipeline] Spawning background setup tasks..."

mkdir -p "${DIST_DIR}" "${STATIC_DIR}" "${SDK_DIR}"

# Background Task A: Build Admin UI
(
  if [ -d "${ADMIN_DIR}" ]; then
    echo "  📦 [Task A] Building Admin UI in background..."
    cd "${ADMIN_DIR}"
    npm install --silent
    npm run build --silent
    cp -r dist/* "${STATIC_DIR}/"
    echo "  ✅ [Task A] Admin UI ready."
  fi
) &
PID_ADMIN=$!

# Background Task B: Download macOS SDK stubs
(
  if [ ! -d "${MACOS_SDK_PATH}" ]; then
    echo "  📥 [Task B] Downloading macOS SDK stubs in background..."
    curl -sSL "${MACOS_SDK_URL}" | tar -xJ -C "${SDK_DIR}"
    echo "  ✅ [Task B] macOS SDK ready."
  fi
) &
PID_SDK=$!

# ------------------------------------------------------------------------------
# 2. Setup Tools, Compilers, and sccache in Main Thread
# ------------------------------------------------------------------------------
echo "🔧 Setting up rustup targets and toolchains..."

# Install pinned ziglang and cargo-zigbuild if missing
if ! command -v cargo-zigbuild &> /dev/null; then
  echo "📥 Installing cargo-zigbuild & ziglang..."
  pip3 install --quiet --no-cache-dir "cargo-zigbuild==0.23.1" "ziglang==0.13.0"
fi

# Enable compiler caching if sccache is available or install via cargo
if command -v sccache &> /dev/null; then
  export RUSTC_WRAPPER="$(command -v sccache)"
  export SCCACHE_DIR="$(pwd)/.cache/sccache"
  mkdir -p "${SCCACHE_DIR}"
  echo "⚡ sccache detected and activated for fast caching."
fi

# Set Clang/LLVM library path for bindgen
if [ -z "${LIBCLANG_PATH:-}" ]; then
  for p in /usr/lib/llvm-*/lib /usr/lib/x86_64-linux-gnu; do
    if [ -f "$p/libclang.so" ] || [ -f "$p/libclang.so.1" ]; then
      export LIBCLANG_PATH="$p"
      break
    fi
  done
fi

# Install Rust targets (non-blocking if already installed)
rustup target add \
  x86_64-unknown-linux-gnu \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl \
  x86_64-pc-windows-gnu \
  aarch64-apple-darwin \
  x86_64-apple-darwin > /dev/null 2>&1

# Wait for background setup jobs before compiling Rust crates
wait "${PID_ADMIN}"
wait "${PID_SDK}"
export SDKROOT="${MACOS_SDK_PATH}"

# Check for pigz (parallel gzip) for instant compression
COMPRESS_CMD="tar -czf"
if command -v pigz &> /dev/null; then
  COMPRESS_CMD="tar -I pigz -cf"
  echo "⚡ pigz detected: Multi-core archive compression enabled."
fi

# ------------------------------------------------------------------------------
# 3. Parallel Build Engine (Avoids target/ Lock via Scoped Target Dirs)
# ------------------------------------------------------------------------------
build_worker() {
  local tool="$1"
  local target="$2"
  local profile="$3"
  local tag="$4"
  local flags="$5"
  local is_windows="$6"

  local ext=""
  [ "${is_windows}" == "true" ] && ext=".exe"

  # Assign isolated target dir to allow parallel execution without Cargo locks
  local scoped_target_dir="target/build_${target}_${tag}"
  mkdir -p "${scoped_target_dir}"

  echo "  🔨 [Start] ${target} (${tag})..."

  if [ "${tool}" == "cargo" ]; then
    CARGO_TARGET_DIR="${scoped_target_dir}" cargo build \
      --profile "${profile}" \
      --target "${target}" \
      -p apexkit-cli \
      ${flags}
  else
    CARGO_TARGET_DIR="${scoped_target_dir}" cargo zigbuild \
      --profile "${profile}" \
      --target "${target}" \
      -p apexkit-cli \
      ${flags}
  fi

  local src_bin="${scoped_target_dir}/${target}/${profile}/apexkit-cli${ext}"
  local staged_bin="apexkit${ext}"
  local staged_target_dir="${DIST_DIR}/tmp_${target}_${tag}"
  mkdir -p "${staged_target_dir}"

  cp "${src_bin}" "${staged_target_dir}/${staged_bin}"

  if [ "${is_windows}" == "true" ]; then
    local archive="${DIST_DIR}/apexkit-${target}-${tag}.zip"
    (
      cd "${staged_target_dir}"
      zip -q "${archive}" "${staged_bin}"
    )
  else
    local archive="${DIST_DIR}/apexkit-${target}-${tag}.tar.gz"
    ${COMPRESS_CMD} "${archive}" -C "${staged_target_dir}" "${staged_bin}"
  fi

  rm -rf "${staged_target_dir}"
  echo "  ✅ [Finished] ${target} (${tag})"
}

# Semaphore Queue Implementation for Parallel Execution
JOB_PIDS=()

run_job() {
  # Wait if we reached the maximum concurrent builds
  while [ "${#JOB_PIDS[@]}" -ge "${MAX_CONCURRENT_BUILDS}" ]; do
    local new_pids=()
    for pid in "${JOB_PIDS[@]}"; do
      if kill -0 "$pid" 2>/dev/null; then
        new_pids+=("$pid")
      fi
    done
    JOB_PIDS=("${new_pids[@]}")
    sleep 0.5
  done

  # Run the build in background
  build_worker "$@" &
  JOB_PIDS+=($!)
}

# ------------------------------------------------------------------------------
# 4. Queue Target Builds
# ------------------------------------------------------------------------------
echo ""
echo "🚀 Queueing compilation jobs..."

# 1. Linux glibc (ONNX)
if [[ "${FILTER_TARGET}" == "all" || "${FILTER_TARGET}" == *"linux"* || "${FILTER_TARGET}" == *"gnu"* ]]; then
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "cargo" "x86_64-unknown-linux-gnu" "release" "perf" "" "false"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "cargo" "x86_64-unknown-linux-gnu" "release-small" "small" "" "false"
fi

# 2. Linux musl (Candle Static)
if [[ "${FILTER_TARGET}" == "all" || "${FILTER_TARGET}" == *"musl"* ]]; then
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "zigbuild" "x86_64-unknown-linux-musl" "release" "perf" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "zigbuild" "x86_64-unknown-linux-musl" "release-small" "small" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "zigbuild" "aarch64-unknown-linux-musl" "release" "perf" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "zigbuild" "aarch64-unknown-linux-musl" "release-small" "small" "--no-default-features" "false"
fi

# 3. Windows (Candle)
if [[ "${FILTER_TARGET}" == "all" || "${FILTER_TARGET}" == *"win"* ]]; then
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "zigbuild" "x86_64-pc-windows-gnu" "release" "perf" "--no-default-features" "true"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "zigbuild" "x86_64-pc-windows-gnu" "release-small" "small" "--no-default-features" "true"
fi

# 4. macOS Apple Silicon & Intel (Candle)
if [[ "${FILTER_TARGET}" == "all" || "${FILTER_TARGET}" == *"mac"* || "${FILTER_TARGET}" == *"darwin"* ]]; then
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "zigbuild" "aarch64-apple-darwin" "release" "perf" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "zigbuild" "aarch64-apple-darwin" "release-small" "small" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"small"* ]] && run_job "zigbuild" "x86_64-apple-darwin" "release" "perf" "--no-default-features" "false"
  [[ "${FILTER_TARGET}" != *"perf"* ]]  && run_job "zigbuild" "x86_64-apple-darwin" "release-small" "small" "--no-default-features" "false"
fi

# Wait for all background compilation tasks to complete
echo "⏳ Waiting for parallel compilation jobs to finish..."
wait

# ------------------------------------------------------------------------------
# 5. Generate Checksums
# ------------------------------------------------------------------------------
echo ""
echo "🔒 Generating SHA-256 Checksums..."
(
  cd "${DIST_DIR}"
  sha256sum * > checksums.txt
)

echo ""
echo "======================================================================"
echo "🎉 Build Complete!"
echo "📂 Artifacts available in: ${DIST_DIR}/"
echo "======================================================================"
ls -lh "${DIST_DIR}"
