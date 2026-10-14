#!/usr/bin/env python3
"""Capture disconnected signed bounded-channel fixture without OS input."""
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get("SUPER_PLATINUM_BINARY", ROOT / "target/debug/super-platinum"))
CAPTURES = ROOT / "tmp/agent-ui"
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
FIXTURES = sys.argv[1:] or ["channel-window-10000"]
LABEL = os.environ.get("SUPER_PLATINUM_CAPTURE_LABEL", "")
if LABEL and not LABEL.replace("-", "").replace("_", "").isalnum():
    raise ValueError("Capture label must be alphanumeric with optional dashes/underscores")
CAPTURES.mkdir(parents=True, exist_ok=True)


def control(env, *args, check=True):
    return subprocess.run(
        [str(ROOT / "scripts/agentctl.sh"), *map(str, args)],
        cwd=ROOT, env=env, capture_output=True, text=True, timeout=8, check=check,
    )


def activate(pid):
    subprocess.run(
        ["osascript", "-l", "JavaScript", "-e",
         'ObjC.import("AppKit"); '
         f'var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({pid}); '
         'if(app.finishedLaunching) app.activateWithOptions(2);'],
        check=True, capture_output=True, timeout=5,
    )


for fixture in FIXTURES:
    if fixture not in {"channel-window-10000"}:
        raise ValueError(f"Not a disconnected bounded-channel fixture: {fixture}")
    capture_name = fixture + (f"-{LABEL}" if LABEL else "")
    sock = TMP / f"draftcheck-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_CHANNEL_WINDOW_CHECK="1", SUPER_PLATINUM_AGENT="1",
               SUPER_PLATINUM_AGENT_SOCK=str(sock))
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / f"{capture_name}.log"
    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env,
                                   stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 60
            activated = 0
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"Fixture exited: {log.read_text()}")
                if sock.exists() and time.monotonic() - activated > 1:
                    activate(process.pid)
                    activated = time.monotonic()
                result = log.read_text()
                if "channel window check failed" in result:
                    raise RuntimeError(result)
                if "channel window check passed" in result:
                    break
                time.sleep(0.1)
            else:
                raise TimeoutError(f"Native verification timed out: {log.read_text()}")
            time.sleep(0.2)
            snapshot = json.loads(control(env, "state").stdout)
            (CAPTURES / f"{capture_name}-state.json").write_text(json.dumps(snapshot, indent=2))
            control(env, "screenshot", CAPTURES / f"{capture_name}.png")
            print(f"{fixture}: passed; screenshot captured", flush=True)
        except BaseException:
            state = control(env, "state", check=False)
            (CAPTURES / f"{capture_name}-failure-state.json").write_text(state.stdout)
            control(env, "screenshot", CAPTURES / f"{capture_name}-failure.png", check=False)
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if MARKER.exists() and pathlib.Path(MARKER.read_text().strip()) == sock:
                MARKER.unlink()
                if previous:
                    prior_env = dict(os.environ, SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    if control(prior_env, "ping", check=False).returncode == 0:
                        MARKER.write_text(previous)
            sock.unlink(missing_ok=True)
