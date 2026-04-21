# Project Builder Dashboard — Dev Commands
# All targets ensure Cargo is in PATH automatically.

SHELL := /bin/zsh
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: dev build check verify verify-feature clean setup icons container-up container-shell container-frontend check-container host-tauri-dev dev-session dev-session-host dev-session-tail

# ── Primary commands ──────────────────────────────────────

## Start dev server (frontend + Tauri backend with hot-reload)
dev:
	npm run tauri dev

## Start a captured desktop debug session (logs + scenario artifacts)
dev-session:
	./scripts/dev-session.sh

## Start a captured desktop debug session against the container frontend
dev-session-host:
	./scripts/dev-session.sh --host-container

## Tail the most recent captured desktop debug session log
dev-session-tail:
	./scripts/dev-session-tail.sh

## Build production app bundle
build:
	npm run tauri build

## Type-check everything without building
check:
	npx tsc --noEmit
	cd src-tauri && cargo check

## Run the full local verification gate
verify:
	npm run test
	npx tsc --noEmit
	cd src-tauri && cargo test
	npm run test:e2e

## Run the agent verification loop for a specific feature brief
verify-feature:
	@if [[ -z "$(FEATURE)" ]]; then \
		echo "ERROR: set FEATURE, for example: make verify-feature FEATURE=forced-fail-repair"; \
		exit 1; \
	fi
	FEATURE="$(FEATURE)" npm run verify:agent

## Clean all build artifacts
clean:
	rm -rf dist
	cd src-tauri && cargo clean

# ── Setup ─────────────────────────────────────────────────

## First-time setup: install deps, verify toolchain
setup:
	@echo "Checking prerequisites..."
	@command -v rustc >/dev/null 2>&1 || (echo "Installing Rust..." && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path)
	@command -v node >/dev/null 2>&1 || (echo "ERROR: Node.js is required" && exit 1)
	npm install
	cd src-tauri && cargo fetch
	@echo ""
	@echo "✔ Setup complete. Run 'make dev' to start."

# ── Frontend only ─────────────────────────────────────────

## Run just the Vite frontend (no Tauri backend)
dev-frontend:
	npm run dev

## Build just the frontend
build-frontend:
	npm run build

# ── Rust only ─────────────────────────────────────────────

## Cargo check (Rust only)
check-rust:
	cd src-tauri && cargo check

## Cargo build (Rust only)
build-rust:
	cd src-tauri && cargo build

## Run clippy lints
lint-rust:
	cd src-tauri && cargo clippy -- -W clippy::all

# ── Container workflow ───────────────────────────────────

## Build and start the dev container in the background
container-up:
	docker compose up -d --build

## Open a shell in the running dev container
container-shell:
	docker compose exec -u vscode dev zsh

## Run the Vite frontend inside the dev container
container-frontend:
	docker compose exec -u vscode dev zsh -lc "npm install && npm run dev -- --host 0.0.0.0"

## Run repo checks inside the dev container
check-container:
	docker compose run --rm -u vscode dev zsh -lc "npm install && make check"

## Run the native Tauri shell on the host against the container frontend
host-tauri-dev:
	./scripts/tauri-host-dev.sh

# ── Help ──────────────────────────────────────────────────

## Show available commands
help:
	@echo "Project Builder Dashboard"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@grep -E '^## ' Makefile | sed 's/^## /  /'
	@echo ""
	@echo "Quick start:"
	@echo "  make setup   # first time"
	@echo "  make dev     # start developing"
