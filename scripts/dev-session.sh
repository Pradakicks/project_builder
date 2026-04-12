#!/bin/zsh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SESSION_ROOT="$ROOT_DIR/.debug-sessions"
TIMESTAMP="$(date -u +"%Y%m%dT%H%M%SZ")"
UUID_SEGMENT="$(uuidgen | tr '[:upper:]' '[:lower:]' | cut -d- -f1)"
SESSION_ID="${TIMESTAMP}-${UUID_SEGMENT}"
SESSION_DIR="$SESSION_ROOT/$SESSION_ID"
LOG_PATH="$SESSION_DIR/desktop.log"
SESSION_JSON="$SESSION_DIR/session.json"
STARTED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

mkdir -p "$SESSION_DIR"
ln -sfn "$SESSION_DIR" "$SESSION_ROOT/current"

cat > "$SESSION_JSON" <<EOF
{
  "sessionId": "$SESSION_ID",
  "sessionDir": "$SESSION_DIR",
  "startedAt": "$STARTED_AT",
  "logPath": "$LOG_PATH",
  "mode": "${1:-tauri-dev}"
}
EOF

export PATH="$HOME/.cargo/bin:$PATH"
export PROJECT_BUILDER_DEBUG_SESSION_ID="$SESSION_ID"
export PROJECT_BUILDER_DEBUG_SESSION_DIR="$SESSION_DIR"
export PROJECT_BUILDER_DEBUG_SESSION_STARTED_AT="$STARTED_AT"
export PROJECT_BUILDER_DEBUG_LOG_PATH="$LOG_PATH"
export VITE_DEBUG_SESSION_ID="$SESSION_ID"
export VITE_DEBUG_SESSION_DIR="$SESSION_DIR"
export VITE_DEBUG_LOG_PATH="$LOG_PATH"

cd "$ROOT_DIR"

echo "Debug session: $SESSION_ID"
echo "Session dir: $SESSION_DIR"
echo "Log file:    $LOG_PATH"
echo ""

if [[ "${1:-}" == "--host-container" ]]; then
  shift
  ./scripts/tauri-host-dev.sh "$@" 2>&1 | tee -a "$LOG_PATH"
else
  npm run tauri dev -- "$@" 2>&1 | tee -a "$LOG_PATH"
fi
