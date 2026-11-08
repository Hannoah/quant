#!/usr/bin/env bash
# Install `quant` from this repo root (Claude-Code style: one command, then `quant` works anywhere).
# Usage: ./install.sh
set -euo pipefail

fail() { echo "error: $1" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || fail "cargo not found. Install Rust 1.85+ from https://rustup.rs first."

cd "$(dirname "$0")"

echo "Installing quant (release)..."
cargo install --locked --path . --force

ver="$(quant --version 2>/dev/null || true)"
[ -n "$ver" ] || fail "installed but 'quant' is not on PATH. Add '\$HOME/.cargo/bin' to PATH, then reopen the terminal."

echo "Done: $ver"
echo "Try: quant show --input data/sample.csv --limit 3"
