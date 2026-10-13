#!/usr/bin/env bash
# Exercise the native file action workflows without a Slack session or OS keyboard events.
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
for fixture in ['file-rename', 'file-rename-narrow', 'file-rename-channel-narrow']:
    log = pathlib.Path(root) / 'tmp' / f'{fixture}-native-check.log'
    log.parent.mkdir(exist_ok=True)
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture, SUPER_PLATINUM_FILE_RENAME_CHECK='1')
    env.pop('SUPER_PLATINUM_AGENT', None)
    env.pop('SUPER_PLATINUM_AGENT_SOCK', None)
    with log.open('w') as output:
        process = subprocess.Popen([binary], cwd=root, env=env, stdout=output, stderr=output)
        try:
            if sys.platform == 'darwin':
                for _ in range(50):
                    activated = subprocess.run(['osascript', '-l', 'JavaScript', '-e', f'ObjC.import("AppKit"); var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); if(app.finishedLaunching) app.activateWithOptions(2); app.finishedLaunching && app.active'], capture_output=True, text=True)
                    if activated.returncode == 0 and activated.stdout.strip() == 'true':
                        break
                    time.sleep(.1)
            deadline = time.monotonic() + 35
            last_activation = 0
            while time.monotonic() < deadline:
                if sys.platform == 'darwin' and time.monotonic() - last_activation > 1:
                    subprocess.run(['osascript', '-l', 'JavaScript', '-e', f'ObjC.import("AppKit"); $.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}).activateWithOptions(2)'], stdout=subprocess.DEVNULL, check=True)
                    last_activation = time.monotonic()
                results = [line for line in log.read_text().splitlines() if 'file rename check' in line]
                if results:
                    print(f'{fixture}: {results[-1]}')
                    if 'file rename check passed:' not in results[-1]:
                        raise SystemExit(1)
                    break
                if process.poll() is not None:
                    raise SystemExit(f'Fixture exited before verification; see {log}')
                time.sleep(0.1)
            else:
                raise SystemExit(f'File rename check timed out; see {log}')
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
PY
