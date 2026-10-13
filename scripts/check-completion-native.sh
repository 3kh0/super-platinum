#!/usr/bin/env bash
# Validate mention selection and wire identities through the real desktop WebView.
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
import tempfile
import re

root, binary = sys.argv[1:]
log = pathlib.Path(root) / 'tmp' / 'completion-native-check.log'
log.parent.mkdir(exist_ok=True)
env = dict(os.environ, SUPER_PLATINUM_FIXTURE='threads', SUPER_PLATINUM_COMPLETION_CHECK='1')
socket_dir = tempfile.TemporaryDirectory(prefix='sp-completion-')
env['SUPER_PLATINUM_AGENT'] = '1'
env['SUPER_PLATINUM_AGENT_SOCK'] = str(pathlib.Path(socket_dir.name) / 'agent.sock')

with log.open('w') as output:
    process = subprocess.Popen([binary], cwd=root, env=env, stdout=output, stderr=output)
    try:
        keys = set()
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            lines = log.read_text().splitlines()
            for line in lines:
                key = re.search(r'completion native key: (\d+) (activate|enter|delete|undo|redo)$', line)
                if not key or key[1] in keys:
                    continue
                keys.add(key[1])
                action = {'activate':'', 'enter':'key code 36', 'delete':'key code 51',
                          'undo':'keystroke "z" using command down',
                          'redo':'keystroke "z" using {command down, shift down}'}[key[2]]
                # Address only this disconnected fixture, never the live client.
                script = f'tell application "System Events"\n tell (first process whose unix id is {process.pid})\n set frontmost to true\n delay 0.1\n if not frontmost then error "Fixture lost focus"\n {action}\n end tell\nend tell'
                subprocess.run(['osascript', '-e', script], check=True, timeout=5)
            results = [line for line in lines if 'completion native check' in line or 'completion native capture' in line]
            if results:
                print(results[-1])
                time.sleep(0.15)
                subprocess.run([str(pathlib.Path(root) / 'scripts/agentctl.sh'), 'screenshot', str(pathlib.Path(root) / 'tmp/completion-native.png')], env=env, check=True, stdout=subprocess.DEVNULL)
                sys.exit(0 if 'passed:' in results[-1] else 1)
            if process.poll() is not None:
                raise SystemExit(f'Fixture exited before verification; see {log}')
            time.sleep(0.1)
        raise SystemExit(f'Native completion check timed out; see {log}')
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
PY
