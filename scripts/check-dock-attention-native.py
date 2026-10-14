#!/usr/bin/env python3
"""Isolated bare-dev AppKit request/cancel check; never bypasses actual Focus."""
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get("SUPER_PLATINUM_BINARY", ROOT / "target/debug/super-platinum"))
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
CAPTURES = ROOT / "tmp/agent-ui"


def main():
    if sys.platform != "darwin":
        raise SystemExit("Dock attention invocation check requires macOS")
    CAPTURES.mkdir(parents=True, exist_ok=True)
    socket = TMP / f"dock-attention-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE="channel", SUPER_PLATINUM_AGENT="1",
               SUPER_PLATINUM_AGENT_SOCK=str(socket), SUPER_PLATINUM_DOCK_ATTENTION_CHECK="1")
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / "dock-attention.log"

    def control(*args, check=True, tool_env=env):
        return subprocess.run([str(ROOT / "scripts/agentctl.sh"), *map(str, args)],
                              cwd=ROOT, env=tool_env, capture_output=True, text=True,
                              timeout=8, check=check)

    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env, stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 30
            while not socket.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Dock fixture failed to start")
                time.sleep(.1)
            control("ping")
            control("focus-window")
            # Bare dev processes need not have an NSRunningApplication entry.
            # System Events identifies the already-running owned process by PID.
            subprocess.run(["osascript", "-e",
                            f'tell application "System Events" to set visible of (first application process whose unix id is {process.pid}) to false'],
                           capture_output=True, text=True, check=True, timeout=5)
            deadline = time.monotonic() + 40
            while True:
                receipts = [line.split("dock-attention-check: ", 1)[1]
                            for line in log.read_text().splitlines(keepends=True)
                            if line.endswith("\n") and "dock-attention-check: " in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    validate(receipt)
                    print(json.dumps(receipt), flush=True)
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Dock invocation check timed out; see " + str(log))
                time.sleep(.1)
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
                        pass
            socket.unlink(missing_ok=True)


def validate(receipt):
    for key in ("ok", "noTransport", "noPreferenceWrites", "focusRespected", "immediateCancellation"):
        if receipt.get(key) is not True:
            raise RuntimeError(f"Incomplete attention receipt: {receipt}")
    modes = receipt.get("modes", [])
    if [m.get("mode") for m in modes] != ["once", "untilActive"]:
        raise RuntimeError(f"Missing attention modes: {receipt}")
    for mode in modes:
        available = mode.get("focus") == "available"
        if mode.get("focus") not in ("available", "quiet", "unknown"):
            raise RuntimeError(f"Invalid Focus status: {receipt}")
        if (mode.get("accepted") is not available or mode.get("cancelled") is not available
                or mode.get("suppressed") is not (not available)):
            raise RuntimeError(f"Focus suppression or exact cancellation failed: {receipt}")
    if (receipt.get("appKitInvocationProven") is not all(m["accepted"] for m in modes)
            or receipt.get("visualBounceProven") is not False or not receipt.get("proofGap")):
        raise RuntimeError(f"Overstated attention proof: {receipt}")


if __name__ == "__main__":
    main()
