#!/usr/bin/env bash
# Credential-free paired benchmark of the shipped ranker and former full sort.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_TEST_OPT_LEVEL=3
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
cargo test --locked --manifest-path "$ROOT/crates/super-platinum-core/Cargo.toml" \
  large_workspace_benchmark -- --ignored --nocapture
