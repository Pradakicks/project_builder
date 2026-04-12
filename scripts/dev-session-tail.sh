#!/bin/zsh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOG_PATH="$ROOT_DIR/.debug-sessions/current/desktop.log"
LINES="${1:-120}"

if [[ ! -f "$LOG_PATH" ]]; then
  echo "No current debug session log found at $LOG_PATH" >&2
  exit 1
fi

tail -n "$LINES" "$LOG_PATH"
