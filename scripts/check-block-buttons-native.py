#!/usr/bin/env python3
"""Read-only assertions/captures for disconnected signed Block Kit fixtures."""
import json
import os
import pathlib
import subprocess
import time

root = pathlib.Path(__file__).resolve().parents[1]
os.chdir(root)
binary = root / 'target/debug/super-platinum'
tmp = pathlib.Path(os.environ.get('TMPDIR', '/tmp'))
marker = tmp / 'super-platinum-agent.sock.path'
for fixture in ('button-metadata', 'button-metadata-narrow'):
    socket = tmp / f'block-buttons-{os.getpid()}.sock'
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_AGENT='1', SUPER_PLATINUM_AGENT_SOCK=str(socket),
               SUPER_PLATINUM_BUTTON_METADATA_CHECK='1')
    previous = marker.read_text() if marker.exists() else None
    log = root / 'tmp/agent-ui' / f'{fixture}.log'
    def control(*args, check=True):
        return subprocess.run(['scripts/agentctl.sh', *map(str, args)], env=env,
                              capture_output=True, text=True, timeout=8, check=check)
    with log.open('w') as output:
        process = subprocess.Popen([str(binary)], env=env, stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 30
            while not socket.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f'{fixture} failed to start: {log.read_text()}')
                time.sleep(.1)
            control('ping')
            subprocess.run(['osascript', '-l', 'JavaScript', '-e',
                'ObjC.import("AppKit"); '
                f'var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); '
                'if(app.finishedLaunching) app.activateWithOptions(2);'],
                capture_output=True, check=True, timeout=5)
            deadline = time.monotonic() + 45
            while True:
                receipts = [line.split('button-metadata-check: ', 1)[1]
                            for line in log.read_text().splitlines()
                            if 'button-metadata-check: ' in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    if receipt.get('ok') is not True:
                        raise RuntimeError(f'{fixture}: native assertion failed: {receipt}')
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f'{fixture}: native assertions timed out')
                time.sleep(.1)
            state = control('state')
            (root / 'tmp/agent-ui' / f'{fixture}-state.json').write_text(state.stdout)
            control('screenshot', root / 'tmp/agent-ui' / f'{fixture}.png')
            print(f'{fixture}: DOM checks passed; state and screenshot captured', flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if marker.exists() and pathlib.Path(marker.read_text().strip()) == socket:
                marker.unlink()
                if previous:
                    check_env = dict(os.environ, SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    live = subprocess.run(['scripts/agentctl.sh', 'ping'], env=check_env,
                        capture_output=True, timeout=3)
                    if live.returncode == 0:
                        marker.write_text(previous)
            socket.unlink(missing_ok=True)
