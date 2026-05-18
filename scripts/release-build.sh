#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is required but was not found on PATH" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required but was not found on PATH" >&2
  exit 1
fi

echo "==> Building frontend bundle"
cd "${ROOT_DIR}/web"
npm run build

echo "==> Building Rust release binaries"
cd "${ROOT_DIR}"
cargo build --release --bin server --bin seed

echo "==> Release build complete"
echo "Frontend bundle: ${ROOT_DIR}/web/dist"
echo "Server binary:   ${ROOT_DIR}/target/release/server"
echo "Seed binary:     ${ROOT_DIR}/target/release/seed"
