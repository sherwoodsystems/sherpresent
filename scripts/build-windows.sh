#!/usr/bin/env bash
set -euo pipefail

# build-windows.sh
# Builds Windows (NSIS + MSI) artifacts via SSH to the Dockurr Windows VM.
# Usage: bash scripts/build-windows.sh [--no-stop]
#   --no-stop    Keep the VM running after build (default: stop after build)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

AUTO_STOP=true
for arg in "$@"; do
    case "$arg" in
        --no-stop) AUTO_STOP=false ;;
    esac
done

SSH_HOST="localhost"
SSH_PORT="2222"
SSH_USER="${WINDOWS_SSH_USER:-Docker}"
REMOTE_DIR="C:/sherpresent"
SSH_OPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"

# Local paths for artifacts
LOCAL_BUNDLE_DIR="$PROJECT_ROOT/apps/desktop/src-tauri/target/release/bundle"

ssh_cmd() {
    ssh $SSH_OPTS -p "$SSH_PORT" "$SSH_USER@$SSH_HOST" "$@"
}

scp_cmd() {
    scp $SSH_OPTS -P "$SSH_PORT" "$@"
}

echo "=== Sherpresent Windows Build ==="

# 0. Ensure VM is running
echo "[0/4] Starting Windows VM..."
docker compose -f "$PROJECT_ROOT/docker-compose.windows.yml" up -d 2>&1 | grep -v "^$"

# 1. Wait for SSH connectivity (VM may need time to boot)
echo "[1/4] Waiting for SSH..."
for i in $(seq 1 30); do
    if ssh_cmd "echo ok" > /dev/null 2>&1; then
        echo "  SSH connected."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: SSH not available after 60s. Is the VM set up?"
        echo "  Check: http://127.0.0.1:8006/ or docker logs sherpresent-windows-build"
        exit 1
    fi
    sleep 2
done

# 2. Sync project files (tar over SSH since Windows doesn't have rsync)
echo "[2/4] Syncing project files to Windows VM..."
ssh_cmd "powershell -Command \"if (Test-Path $REMOTE_DIR) { Remove-Item -Recurse -Force $REMOTE_DIR }; New-Item -ItemType Directory -Path $REMOTE_DIR -Force | Out-Null\""
tar cf - -C "$PROJECT_ROOT" \
    --exclude='node_modules' \
    --exclude='.git' \
    --exclude='target' \
    --exclude='.svelte-kit' \
    --exclude='build' \
    --exclude='ref' \
    --exclude='apps/companion-module-sherpresent' \
    --exclude='apps/bridge' \
    --exclude='archive' \
    . | ssh_cmd "powershell -Command \"cd $REMOTE_DIR; tar xf -\""
echo "  Files synced."

# 3. Build on Windows
echo "[3/4] Building on Windows VM (this may take a while on first run)..."
scp_cmd "$SCRIPT_DIR/windows-build-remote.ps1" "$SSH_USER@$SSH_HOST:$REMOTE_DIR/build.ps1"
ssh_cmd "powershell -ExecutionPolicy Bypass -File $REMOTE_DIR/build.ps1" 2>&1 | while IFS= read -r line; do
    echo "  [win] $line"
done

BUILD_EXIT=${PIPESTATUS[0]}
if [ "$BUILD_EXIT" -ne 0 ]; then
    echo "ERROR: Windows build failed with exit code $BUILD_EXIT"
    exit 1
fi

# 4. Copy artifacts back
echo "[4/4] Copying build artifacts..."
mkdir -p "$LOCAL_BUNDLE_DIR/nsis" "$LOCAL_BUNDLE_DIR/msi"

# NSIS artifacts
scp_cmd "$SSH_USER@$SSH_HOST:$REMOTE_DIR/apps/desktop/src-tauri/target/release/bundle/nsis/*" \
    "$LOCAL_BUNDLE_DIR/nsis/" 2>/dev/null && echo "  NSIS artifacts copied." || echo "  No NSIS artifacts found."

# MSI artifacts
scp_cmd "$SSH_USER@$SSH_HOST:$REMOTE_DIR/apps/desktop/src-tauri/target/release/bundle/msi/*" \
    "$LOCAL_BUNDLE_DIR/msi/" 2>/dev/null && echo "  MSI artifacts copied." || echo "  No MSI artifacts found."

echo ""
echo "=== Windows Build Complete ==="
echo "Artifacts in: $LOCAL_BUNDLE_DIR"
ls -lh "$LOCAL_BUNDLE_DIR/nsis/" 2>/dev/null || true
ls -lh "$LOCAL_BUNDLE_DIR/msi/" 2>/dev/null || true

# Auto-stop the VM after successful build
if [ "$AUTO_STOP" = true ]; then
    echo ""
    echo "Stopping Windows VM..."
    docker compose -f "$PROJECT_ROOT/docker-compose.windows.yml" stop
    echo "  VM stopped."
fi
