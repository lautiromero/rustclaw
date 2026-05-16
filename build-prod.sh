#!/usr/bin/env bash
set -euo pipefail

APP_NAME="rustclaw"
BIN_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/$APP_NAME"

mkdir -p "$BIN_DIR" "$CONFIG_DIR"

cargo build --release

cp "target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
cp ".env" "$CONFIG_DIR/.env"

chmod +x "$BIN_DIR/$APP_NAME"

printf 'Built and installed: %s\n' "$BIN_DIR/$APP_NAME"
printf 'Config installed: %s\n' "$CONFIG_DIR/.env"
