#!/usr/bin/env python3
"""Capture disconnected signed rejected-send recovery fixtures without OS input."""
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/Super Platinum.app/Contents/MacOS/super-platinum"
CAPTURES = ROOT / "tmp/agent-ui"
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
FIXTURES = sys.argv[1:] or ["rejected-send", "rejected-send-narrow"]
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
    if fixture not in {"rejected-send", "rejected-send-narrow"}:
        raise ValueError(f"Not a disconnected rejected-send recovery fixture: {fixture}")
    sock = TMP / f"draftcheck-{os.getpid()}.sock"
    storage = CAPTURES / f"{fixture}-settlement-{os.getpid()}"
    storage.mkdir()
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_DRAFT_SETTLEMENT_DB=str(storage / "draft-settlement.sqlite"),
               SUPER_PLATINUM_REJECTED_SEND_CHECK="1", SUPER_PLATINUM_AGENT="1",
               SUPER_PLATINUM_AGENT_SOCK=str(sock))
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / f"{fixture}.log"
    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env,
                                   stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 60
            activated = 0
            review_captured = False
            restored_captured = False
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"Fixture exited: {log.read_text()}")
                if sock.exists() and time.monotonic() - activated > 1:
                    activate(process.pid)
                    activated = time.monotonic()
                result = log.read_text()
                if "rejected send before ready" in result and not review_captured:
                    control(env, "screenshot", CAPTURES / f"{fixture}-review.png")
                    review_captured = True
                if "rejected send restored ready" in result and not restored_captured:
                    control(env, "screenshot", CAPTURES / f"{fixture}-restored.png")
                    restored_captured = True
                if "rejected send check failed" in result:
                    raise RuntimeError(result)
                if "rejected send check passed" in result:
                    break
                time.sleep(0.1)
            else:
                raise TimeoutError(f"Native verification timed out: {log.read_text()}")
            if not review_captured or not restored_captured:
                raise RuntimeError("Review interaction marker was not captured")
            time.sleep(0.2)
            control(env, "screenshot", CAPTURES / f"{fixture}.png")
            print(f"{fixture}: passed; screenshot captured", flush=True)
        except BaseException:
            state = control(env, "state", check=False)
            (CAPTURES / f"{fixture}-failure-state.json").write_text(state.stdout)
            control(env, "screenshot", CAPTURES / f"{fixture}-failure.png", check=False)
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
