#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

echo "🚀 Starting ApexKit Static Build (x86_64-unknown-linux-musl)..."

# ------------------------------------------------------------------
# 1. Prerequisite Checks
# ------------------------------------------------------------------

# Ensure Clang is installed
if ! command -v clang &> /dev/null; then
    echo "❌ Error: 'clang' is not installed."
    echo "   Please run: sudo apt-get install clang"
    exit 1
fi

# Ensure LLD (LLVM Linker) is installed - CRITICAL for this fix
if ! command -v lld &> /dev/null; then
    echo "❌ Error: 'lld' is not installed."
    echo "   Please run: sudo apt-get install lld"
    exit 1
fi

# Ensure musl-tools (headers)
if ! dpkg -s musl-tools >/dev/null 2>&1; then
    echo "⚠️  Warning: 'musl-tools' might not be installed."
    echo "   run: sudo apt-get install musl-tools"
fi

# Ensure Rust target exists
if ! rustup target list --installed | grep -q "x86_64-unknown-linux-musl"; then
    echo "⬇️  Adding Rust target..."
    rustup target add x86_64-unknown-linux-musl
fi

# ------------------------------------------------------------------
# 2. Configure Environment (The Fix)
# ------------------------------------------------------------------
echo "🔧 Configuring Compiler Environment..."

# 1. Force the C Compiler to map fcntl64 -> fcntl directly in the command.
#    This prevents the build script from ignoring a separate CFLAGS variable.
export CC_x86_64_unknown_linux_musl="clang -target x86_64-unknown-linux-musl -Dfcntl64=fcntl"

# 2. Configure C++ Compiler
export CXX_x86_64_unknown_linux_musl="clang++ -target x86_64-unknown-linux-musl -Dfcntl64=fcntl"

# 3. Use LLD Linker
export RUSTFLAGS="-C linker=clang -C link-arg=-target -C link-arg=x86_64-unknown-linux-musl -C link-arg=-fuse-ld=lld"

# ------------------------------------------------------------------
# 3. Clean & Build
# ------------------------------------------------------------------
echo "🧹 Cleaning previous database build artifacts..."
# We specifically clean libsql-ffi to force recompilation of sqlite3.c with the new flags
cargo clean -p libsql-ffi 2>/dev/null || true
# Also clean the final binary target to ensure linking happens again
cargo clean -p apexkit-api --target x86_64-unknown-linux-musl 2>/dev/null || true

echo "🔨 Building Release Binary..."
cargo build --release --target x86_64-unknown-linux-musl

# ------------------------------------------------------------------
# 4. Verify
# ------------------------------------------------------------------
BINARY_PATH="target/x86_64-unknown-linux-musl/release/apexkit-api"

if [ -f "$BINARY_PATH" ]; then
    echo "✅ Build Successful!"
    
    # Strip symbols to reduce size
    echo "✂️  Stripping binary..."
    strip "$BINARY_PATH"

    echo "📊 Binary Info:"
    ls -lh "$BINARY_PATH"
    file "$BINARY_PATH"
else
    echo "❌ Error: Binary not found."
    exit 1
fi