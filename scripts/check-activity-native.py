#!/usr/bin/env python3
"""Drive and capture isolated signed activity-navigation fixtures without OS input."""
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get("SUPER_PLATINUM_BINARY", ROOT / "target/debug/Super Platinum.app/Contents/MacOS/super-platinum"))
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
CAPTURES = ROOT / "tmp/agent-ui"
CAPTURES.mkdir(parents=True, exist_ok=True)

for fixture in ("activity-navigation", "activity-navigation-narrow"):
    capture_name = fixture
    socket = TMP / f"activity-navigation-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(socket),
               SUPER_PLATINUM_ACTIVITY_CHECK="1")
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / f"{capture_name}.log"

    def control(*args, check=True, tool_env=env):
        return subprocess.run([str(ROOT / "scripts/agentctl.sh"), *map(str, args)],
                              cwd=ROOT, env=tool_env, capture_output=True,
                              text=True, timeout=8, check=check)

    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env,
                                   stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 30
            while not socket.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: failed to start")
                time.sleep(.1)
            control("ping")
            subprocess.run(["osascript", "-l", "JavaScript", "-e",
                            'ObjC.import("AppKit"); '
                            f'var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); '
                            'if(app.finishedLaunching) app.activateWithOptions(2);'],
                           capture_output=True, check=True, timeout=5)
            deadline = time.monotonic() + 90
            last_activation = 0.0
            while True:
                if time.monotonic() - last_activation > 1:
                    subprocess.run(["osascript", "-l", "JavaScript", "-e",
                                    'ObjC.import("AppKit"); '
                                    f'var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); '
                                    'if(app.finishedLaunching) app.activateWithOptions(2);'],
                                   capture_output=True, check=True, timeout=5)
                    last_activation = time.monotonic()
                receipts = [line.split("activity-navigation-check: ", 1)[1]
                            for line in log.read_text().splitlines(keepends=True)
                            if line.endswith("\n") and "activity-navigation-check: " in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    if receipt.get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: native assertion failed: {receipt}")
                    required = ("bundleOpensLatest", "channelClosesThread", "exactReplyVisible",
                                "activitySurfacePreserved", "noTransport")
                    viewport = receipt.get("viewport", [])
                    expected_viewport = [668, 400] if fixture.endswith("-narrow") else [1280, 800]
                    if viewport != expected_viewport:
                        raise RuntimeError(f"{capture_name}: incorrect native viewport: {viewport}")
                    if any(receipt.get(key) is not True for key in required):
                        raise RuntimeError(f"{capture_name}: incomplete native receipt: {receipt}")
                    if receipt.get("bundleGeometry", {}).get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: bundle did not reach visible latest reply")
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: native assertions timed out")
                time.sleep(.1)
            # The control-plane command must preserve the same old-reply target
            # as the DOM click path, rather than reopening at latest.
            for index, at_bottom in ((1, True), (0, False)):
                control("activity-select", index)
                deadline = time.monotonic() + 8
                while True:
                    state = json.loads(control("state").stdout)["data"]
                    target = None if at_bottom else "1719800001.000001"
                    if (state.get("main_view") == "activity"
                            and state.get("active_thread") == "1719800000.000100"
                            and state.get("thread_timeline", {}).get("at_bottom") is at_bottom
                            # Successful DOM scrolling consumes the target.
                            and state.get("thread_timeline", {}).get("target") in (None, target)):
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError(f"{capture_name}: agent command lost exact reply")
                    time.sleep(.1)
                stage = "bundle" if at_bottom else "reply"
                deadline = time.monotonic() + 10
                while True:
                    receipts = [json.loads(line.split("activity-agent-check: ", 1)[1])
                                for line in log.read_text().splitlines(keepends=True)
                                if line.endswith("\n") and "activity-agent-check: " in line]
                    if any(r.get("ok") is not True for r in receipts):
                        raise RuntimeError(f"{capture_name}: agent geometry failed: {receipts}")
                    proof = next((r for r in receipts if r.get("stage") == stage), None)
                    if proof is not None:
                        geometry = proof.get("geometry", {})
                        if geometry.get("ok") is not True:
                            raise RuntimeError(f"{capture_name}: missing agent {stage} geometry")
                        if not at_bottom and (geometry.get("replyId") != target
                                              or proof.get("noTransport") is not True):
                            raise RuntimeError(f"{capture_name}: agent displayed the wrong reply")
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError(f"{capture_name}: no agent {stage} geometry receipt")
                    time.sleep(.1)
            control("screenshot", CAPTURES / f"{capture_name}.png")
            print(f"{capture_name}: native Activity navigation interactions passed; screenshot captured", flush=True)
        except BaseException:
            try:
                control("screenshot", CAPTURES / f"{capture_name}-failure.png", check=False)
            except (OSError, subprocess.SubprocessError):
                pass  # Preserve the original startup/assertion failure.
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if MARKER.exists() and pathlib.Path(MARKER.read_text().strip()) == socket:
                MARKER.unlink()
                if previous:
                    prior = dict(os.environ, SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    try:
                        if control("ping", check=False, tool_env=prior).returncode == 0:
                            MARKER.write_text(previous)
                    except (OSError, subprocess.SubprocessError):
                        pass  # A stale prior marker must not mask this run's result.
            socket.unlink(missing_ok=True)
