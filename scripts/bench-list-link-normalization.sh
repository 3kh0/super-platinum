#!/usr/bin/env bash
# Uses the production desktop-profile core artifact; no Cargo or session access.
set -euo pipefail
cd "$(dirname "$0")/.."
link_core_rlib=$(python3 - <<'PY'
from pathlib import Path
paths = list(Path('target/debug/deps').glob('libsuper_platinum_core-*.rlib'))
if not paths:
    raise SystemExit('Build the locked desktop app first.')
print(max(paths, key=lambda path: path.stat().st_mtime))
PY
)
mkdir -p tmp
rustc --edition 2024 -C opt-level=1 -L dependency=target/debug/deps \
  --extern "super_platinum_core=$link_core_rlib" \
  scripts/bench-list-link-normalization.rs -o tmp/bench-list-link-normalization
tmp/bench-list-link-normalization
