#!/usr/bin/env python3
"""Measure isolated native 10,001-row channel scrolls; no Slack session or writes."""
import argparse
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/Super Platinum.app/Contents/MacOS/super-platinum"
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
CAPTURES = ROOT / "tmp/agent-ui"
CAPTURES.mkdir(parents=True, exist_ok=True)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=pathlib.Path, default=BINARY)
parser.add_argument("--label", default="baseline")
parser.add_argument("--runs", type=int, default=3)
parser.add_argument("--run-start", type=int, default=1)
args = parser.parse_args()
if not args.label.replace("-", "").replace("_", "").isalnum() or not 1 <= args.runs <= 20:
    parser.error("Use a simple label and between 1 and 20 runs")
BINARY = args.binary.resolve()
for run in range(args.run_start, args.run_start + args.runs):
    fixture = "channel-window-10000"
    capture_name = f"channel-scroll-{args.label}-{run}"
    socket = TMP / f"channel-scroll-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(socket),
               SUPER_PLATINUM_PERF_SCROLL="1", SUPER_PLATINUM_STARTUP_TRACE="1")
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
            # Auto-scroll performs one top/bottom sweep. Wait for the final
            # observer batch; this is sampling time, not an interaction latency.
            time.sleep(4)
            snapshot = json.loads(control("state").stdout)
            data = snapshot.get("data", snapshot)
            metrics = data["performance"]
            if not metrics["scroll_frame_ms"]:
                raise RuntimeError("No visible native scroll frames recorded")
            artifact = {"fixture": fixture, "run": run, "label": args.label,
                        "scope": "disconnected native channel auto-scroll; no thread workload",
                        "performance": metrics}
            (CAPTURES / f"{capture_name}.json").write_text(json.dumps(artifact, indent=2))
            control("screenshot", CAPTURES / f"{capture_name}.png")
            print(json.dumps({"label": args.label, "run": run, "summaries": metrics["summaries"]}), flush=True)
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
