#!/usr/bin/env bash
# Validate synthetic IME key ordering through the real desktop WebView.
# The probe refuses a connected state and only mutates an offline thread fixture.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo build --locked
python3 - "$ROOT" "${SUPER_PLATINUM_BINARY:-$ROOT/target/debug/super-platinum}" <<'PY'
import os
import pathlib
import subprocess
import sys
import time

root, binary = sys.argv[1:]
log = pathlib.Path(root) / 'tmp' / 'ime-native-check.log'
log.parent.mkdir(exist_ok=True)
env = dict(os.environ, SUPER_PLATINUM_FIXTURE='threads', SUPER_PLATINUM_IME_CHECK='1')
env.pop('SUPER_PLATINUM_AGENT', None)
env.pop('SUPER_PLATINUM_AGENT_SOCK', None)
with log.open('w') as output:
    process = subprocess.Popen([binary], cwd=root, env=env, stdout=output, stderr=output)
    try:
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            lines = log.read_text().splitlines()
            results = [line for line in lines if 'IME native check' in line]
            if results:
                print(results[-1])
                sys.exit(0 if 'IME native check passed:' in results[-1] else 1)
            if process.poll() is not None:
                raise SystemExit(f'Fixture exited before verification; see {log}')
            time.sleep(0.1)
        raise SystemExit(f'Native IME check timed out; see {log}')
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
PY
