#!/usr/bin/env bash
set -e

echo "=== [1/2] Running cargo check ==="
cargo check --all-targets --message-format=short --quiet

echo "=== [2/2] Running cargo clippy ==="
cargo clippy --all-targets --message-format=short --quiet -- \
    -D warnings \
    -D clippy::unwrap_used \
    -D clippy::expect_used
