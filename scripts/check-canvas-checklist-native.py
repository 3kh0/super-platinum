#!/usr/bin/env python3
"""Drive and capture isolated native canvas checklist fixtures after their native receipt."""
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get("SUPER_PLATINUM_BINARY", ROOT / "target/debug/super-platinum"))
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
CAPTURES = ROOT / "tmp/agent-ui"
CAPTURES.mkdir(parents=True, exist_ok=True)

for fixture in ("canvas-checklist", "canvas-checklist-narrow"):
    capture_name = fixture
    socket = TMP / f"canvas-checklist-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(socket),
               SUPER_PLATINUM_CANVAS_CHECKLIST_CHECK="1")
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
                receipts = [line.split("canvas-checklist-check: ", 1)[1]
                            for line in log.read_text().splitlines(keepends=True)
                            if line.endswith("\n") and "canvas-checklist-check: " in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    if receipt.get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: native assertion failed: {receipt}")
                    required = ("duplicateBlocked", "canonicalReadback", "restored")
                    expected_width = 668 if fixture.endswith("-narrow") else 1280
                    if (receipt.get("attempts") != 2
                            or any(receipt.get(key) is not True for key in required)
                            or receipt.get("layout", {}).get("ok") is not True
                            or receipt.get("layout", {}).get("viewport", [None])[0] != expected_width
                            or receipt.get("layout", {}).get("unsupportedInert") is not True):
                        raise RuntimeError(f"{capture_name}: incomplete native receipt: {receipt}")
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: native assertions timed out")
                time.sleep(.1)
            control("screenshot", CAPTURES / f"{capture_name}.png")
            print(f"{capture_name}: native canvas checklist interactions passed; screenshot captured", flush=True)
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
