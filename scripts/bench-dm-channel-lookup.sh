#!/usr/bin/env bash
# Synthetic CPU-only paired probe; no Cargo, app launch, or session access.
set -euo pipefail
cd "$(dirname "$0")/.."
core_rlib=$(python3 - <<'PY'
from pathlib import Path
paths = list(Path('target/debug/deps').glob('libsuper_platinum_core-*.rlib'))
if not paths:
    raise SystemExit('Run a normal locked desktop build first; no core artifact found.')
print(max(paths, key=lambda path: path.stat().st_mtime))
PY
)
mkdir -p tmp
rustc --edition 2024 -C opt-level=1 -L dependency=target/debug/deps \
  --extern "super_platinum_core=$core_rlib" \
  scripts/bench-dm-channel-lookup.rs -o tmp/bench-dm-channel-lookup
tmp/bench-dm-channel-lookup
