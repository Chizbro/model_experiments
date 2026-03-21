#!/usr/bin/env bash
# Mirrors `.github/workflows/ci.yml` for local verification.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets
cargo test --all-targets

pushd web >/dev/null
npm ci
npm run lint
npm run typecheck
npm run build
npm run test
popd >/dev/null

echo "ci-local: OK"
