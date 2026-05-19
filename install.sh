#!/usr/bin/env bash
set -euo pipefail

APP_NAME="rustclaw"
BIN_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.rustclaw"
CONFIG_DIR="${DATA_DIR}"

cargo build --release

# Create directories
mkdir -p "${BIN_DIR}" "${DATA_DIR}/migrations" "${CONFIG_DIR}"

# 1. Copy binary (always overwrite)
if [ ! -f "target/release/${APP_NAME}" ]; then
    echo "Error: binary not found at target/release/${APP_NAME}" >&2
    echo "Run 'cargo build --release' first." >&2
    exit 1
fi
cp "target/release/${APP_NAME}" "${BIN_DIR}/${APP_NAME}"
chmod +x "${BIN_DIR}/${APP_NAME}"

# 2. Copy migrations (always overwrite)
if [ -d "migrations" ]; then
    cp -r "migrations" "${DATA_DIR}/"
fi

# 3. Copy config.toml only if it doesn't exist
if [ ! -f "${CONFIG_DIR}/config.toml" ] && [ -f "config.toml" ]; then
    cp "config.toml" "${CONFIG_DIR}/config.toml"
fi

# 4. Copy .env from .env.example only if it doesn't exist
if [ ! -f "${CONFIG_DIR}/.env" ] && [ -f ".env.example" ]; then
    cp ".env.example" "${CONFIG_DIR}/.env"
fi
