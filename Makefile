# Project Builder Dashboard — Dev Commands
# All targets ensure Cargo is in PATH automatically.

SHELL := /bin/zsh
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: dev build check clean setup icons

# ── Primary commands ──────────────────────────────────────

## Start dev server (frontend + Tauri backend with hot-reload)
dev:
	npm run tauri dev

## Build production app bundle
build:
	npm run tauri build

## Type-check everything without building
check:
	npx tsc --noEmit
	cd src-tauri && cargo check

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
