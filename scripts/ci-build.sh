#!/usr/bin/env bash
# CI build script for Cloudflare Workers Builds.
# Safe to re-run: skips steps whose outputs are already cached in ~/.cargo and target/.
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal
fi

# shellcheck disable=SC1091
. "$HOME/.cargo/env"

rustup target add wasm32-unknown-unknown

if ! command -v worker-build >/dev/null 2>&1; then
  cargo install -q worker-build --locked
fi

worker-build --release
