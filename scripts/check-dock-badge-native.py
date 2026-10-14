#!/usr/bin/env python3
"""Isolated bare-dev AppKit badge label readback; no Slack preference writes."""
import argparse
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--projection", action="store_true", help="exercise the production cached projection hook")
    args = parser.parse_args()
    if sys.platform != "darwin":
        raise SystemExit("Dock badge check requires macOS")
    CAPTURES.mkdir(parents=True, exist_ok=True)
    socket = TMP / f"dock-badge-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE="channel", SUPER_PLATINUM_AGENT="1",
               SUPER_PLATINUM_AGENT_SOCK=str(socket), SUPER_PLATINUM_DOCK_BADGE_CHECK="projection" if args.projection else "1")
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / ("dock-badge-projection.log" if args.projection else "dock-badge.log")

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
            deadline = time.monotonic() + 40
            while True:
                receipts = [line.split("dock-badge-check: ", 1)[1]
                            for line in log.read_text().splitlines(keepends=True)
                            if line.endswith("\n") and "dock-badge-check: " in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    (validate_projection if args.projection else validate)(receipt)
                    print(json.dumps(receipt), flush=True)
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Dock badge check timed out; see " + str(log))
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
    for key in ("ok", "noTransport", "noPreferenceWrites", "actualAppKitReadback",
                "unchangedNoWrite", "ownerClear", "dropClear"):
        if receipt.get(key) is not True:
            raise RuntimeError(f"Incomplete badge receipt: {receipt}")
    stages = receipt.get("stages", [])
    names = ["clear", "number", "unchanged", "dot", "zero", "newNumber",
             "accountChange", "retired", "beforeDrop"]
    expected_labels = [None, "42", "42", "•", None, "7", "7", None, "•"]
    expected_writes = [1, 1, 0, 1, 1, 1, 2, 1, 1]
    if [s.get("stage") for s in stages] != names:
        raise RuntimeError(f"Missing badge transitions: {receipt}")
    for stage, label, writes in zip(stages, expected_labels, expected_writes):
        if (stage.get("label") or None) != label or stage.get("writes") != writes:
            raise RuntimeError(f"Incorrect native badge readback: {receipt}")
    if receipt.get("writes") != 10 or receipt.get("visualDockPixelsProven") is not False:
        raise RuntimeError(f"Unexpected write count or overstated visual proof: {receipt}")


def validate_projection(receipt):
    for key in ("ok", "noTransport", "noPreferenceWrites", "productionHook",
                "actualAppKitReadback", "filteredActivity", "authoritativeZero",
                "laterEligibility", "ownerClear"):
        if receipt.get(key) is not True:
            raise RuntimeError(f"Incomplete production badge receipt: {receipt}")
    names = ["activityExcludesReaction", "activityZero", "eligibleLater", "hiddenLater",
             "positiveBeforeRetirement", "accountRetirement"]
    labels = ["6", None, "4", None, "4", None]
    stages = receipt.get("stages", [])
    if (receipt.get("mode") != "projection" or receipt.get("visualDockPixelsProven") is not False
            or [s.get("stage") for s in stages] != names
            or [(s.get("label") or None) for s in stages] != labels):
        raise RuntimeError(f"Incorrect production hook transitions: {receipt}")


if __name__ == "__main__":
    main()
