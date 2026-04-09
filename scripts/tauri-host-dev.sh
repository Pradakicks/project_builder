#!/bin/zsh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEV_URL="${TAURI_CONTAINER_DEV_URL:-http://127.0.0.1:5174}"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for the host/container split workflow." >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required to verify the forwarded frontend dev server." >&2
  exit 1
fi

if [[ -z "$(docker compose ps -q dev 2>/dev/null)" ]]; then
  cat >&2 <<EOF
The dev container is not running.

Start it first:
  make container-up
EOF
  exit 1
fi

if ! curl -sf "$DEV_URL" >/dev/null; then
  cat >&2 <<EOF
The containerized frontend dev server is not reachable at $DEV_URL.

In another terminal, start it inside the container:
  make container-frontend

Then rerun:
  make host-tauri-dev
EOF
  exit 1
fi

if [[ ! -x "$ROOT_DIR/node_modules/.bin/tauri" ]] && ! command -v tauri >/dev/null 2>&1; then
  cat >&2 <<EOF
The host-side Tauri CLI is not available.

The containerized frontend is running, but launching the native desktop shell
still requires host access to the Tauri CLI and project JS dependencies.

Options:
  1. Run 'npm install' on the host for this repo, then rerun:
     make host-tauri-dev
  2. Install the Tauri CLI globally on the host.

This is the current host/container boundary for native Tauri development.
EOF
  exit 1
fi

TAURI_BIN="$ROOT_DIR/node_modules/.bin/tauri"
if [[ ! -x "$TAURI_BIN" ]]; then
  TAURI_BIN="$(command -v tauri)"
fi

TAURI_CONFIG_OVERRIDE='{"build":{"beforeDevCommand":"true","devUrl":"'"$DEV_URL"'"}}'

exec "$TAURI_BIN" dev \
  --config "$TAURI_CONFIG_OVERRIDE" \
  "$@"
