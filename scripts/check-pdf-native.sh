#!/usr/bin/env bash
# Exercise the native PDF workflows without a Slack session or OS keyboard events.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo build --locked
python3 - "$ROOT" "${SUPER_PLATINUM_BINARY:-$ROOT/target/debug/super-platinum}" "$@" <<'PY'
import os
import pathlib
import subprocess
import sys
import time

root, binary = sys.argv[1:3]
for fixture in sys.argv[3:] or ['pdf-preview', 'pdf-preview-narrow', 'pdf-preview-error', 'pdf-preview-search-long']:
    log = pathlib.Path(root) / 'tmp' / f'{fixture}-native-check.log'
    log.parent.mkdir(exist_ok=True)
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture, SUPER_PLATINUM_PDF_CHECK='1')
    env.pop('SUPER_PLATINUM_AGENT', None)
    env.pop('SUPER_PLATINUM_AGENT_SOCK', None)
    with log.open('w') as output:
        process = subprocess.Popen([binary], cwd=root, env=env, stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 35
            while time.monotonic() < deadline:
                results = [line for line in log.read_text().splitlines() if 'PDF check' in line]
                if results:
                    print(f'{fixture}: {results[-1]}')
                    if 'PDF check passed' not in results[-1]:
                        raise SystemExit(1)
                    break
                if process.poll() is not None:
                    raise SystemExit(f'Fixture exited before verification; see {log}')
                time.sleep(0.1)
            else:
                raise SystemExit(f'PDF check timed out; see {log}')
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
PY
