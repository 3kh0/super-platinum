#!/usr/bin/env python3
"""Compare capture responsiveness using an isolated native fixture (never builds).

Example:
  scripts/check-capture-responsiveness.py --binary tmp/dev-capture-baseline --label baseline
  scripts/check-capture-responsiveness.py --binary target/debug/super-platinum \
      --label patched --expect-responsive --timeout-case

Requires macOS Accessibility permission for System Events and screen capture.
The receipt measures AX responsiveness, not screenshot visual correctness; inspect
its PNG separately. Only the launched fixture and its exact capture child are killed.
"""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
TMP = Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"


def request(endpoint, command, timeout=20, **fields):
    started = time.monotonic()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(timeout)
        stream.connect(str(endpoint))
        stream.sendall((json.dumps({"id": 1, "cmd": command, **fields}) + "\n").encode())
        with stream.makefile("rb") as source:
            line = source.readline(1024 * 1024)
    return {"response": json.loads(line), "started": started,
            "completed": time.monotonic()}


def ax_resize(pid, width, height):
    script = f'''tell application "System Events"
        tell (first application process whose unix id is {pid})
            set frontmost to true
            set size of window 1 to {{{width}, {height}}}
            get size of window 1
        end tell
    end tell'''
    result = subprocess.run(["osascript", "-e", script], capture_output=True,
                            text=True, timeout=15, check=True)
    size = [int(part.strip()) for part in result.stdout.strip().split(",")]
    if size != [width, height]:
        raise RuntimeError(f"AX resize did not apply: expected {[width, height]}, got {size}")
    return size


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--label", required=True)
    parser.add_argument("--expect-responsive", action="store_true",
                        help="require AX resize <1s and before the 2s screenshot completes")
    parser.add_argument("--timeout-case", action="store_true",
                        help="patched binary only: require a 30s capture delay to time out near 10s")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this harness requires macOS")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", args.label):
        parser.error("--label must contain only letters, numbers, underscores, and hyphens")
    binary = args.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("--binary must be an existing executable; this script does not build")
    captures = ROOT / "tmp/agent-ui"
    captures.mkdir(parents=True, exist_ok=True)
    prefix = captures / f"capture-responsiveness-{args.label}"
    receipt_path = prefix.with_suffix(".json")
    endpoint = TMP / f"capture-responsive-{os.getpid()}.sock"
    previous = MARKER.read_bytes() if MARKER.exists() else None
    receipt = {"label": args.label, "binary": str(binary), "fixture": "activity-navigation",
               "ok": False, "expect_responsive": args.expect_responsive}
    process = None
    children = set()
    # One worker sends the agent command; resize uses the independent AX interface.
    executor = concurrent.futures.ThreadPoolExecutor(max_workers=1)
    try:
        with tempfile.TemporaryDirectory(prefix="sp-capture-delay-") as temporary:
            temporary = Path(temporary)
            config = temporary / "config.json"
            wrapper = temporary / "screencapture"
            wrapper.write_text(f"#!{sys.executable}\n" + '''import json, os, sys, time
from pathlib import Path
config = json.loads(Path(os.environ["SP_CAPTURE_DELAY_CONFIG"]).read_text())
marker = Path(config["marker"])
temporary = marker.with_suffix(".partial")
temporary.write_text(json.dumps({"pid": os.getpid(), "parent_pid": os.getppid(),
                                 "ready": time.monotonic()}))
os.replace(temporary, marker)
time.sleep(config["delay"])
os.execv("/usr/sbin/screencapture", ["/usr/sbin/screencapture", *sys.argv[1:]])
''')
            wrapper.chmod(0o755)
            # Do not inherit live-session flags, fixture checkers, or alternate sockets.
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith("SUPER_PLATINUM_")}
            env.update(SUPER_PLATINUM_FIXTURE="activity-navigation",
                       SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(endpoint),
                       SP_CAPTURE_DELAY_CONFIG=str(config),
                       PATH=str(temporary) + os.pathsep + os.environ.get("PATH", ""))
            with prefix.with_suffix(".log").open("w") as log:
                process = subprocess.Popen([str(binary)], cwd=ROOT, env=env,
                                           stdout=log, stderr=log)
                receipt["fixture_pid"] = process.pid
                deadline = time.monotonic() + 30
                while not endpoint.exists():
                    if process.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("isolated fixture failed to start")
                    time.sleep(.05)
                if request(endpoint, "ping", timeout=5)["response"].get("ok") is not True:
                    raise RuntimeError("fixture agent ping failed")
                ax_resize(process.pid, 1100, 740)
                time.sleep(.25)

                def capture_case(name, delay, resize):
                    ready = temporary / f"{name}-ready.json"
                    png = Path(str(prefix) + f"-{name}.png")
                    png.unlink(missing_ok=True)
                    config.write_text(json.dumps({"delay": delay, "marker": str(ready)}))
                    future = executor.submit(request, endpoint, "screenshot", 20, path=str(png))
                    deadline = time.monotonic() + 5
                    while not ready.exists():
                        if future.done():
                            raise RuntimeError(f"capture finished before wrapper started: {future.result()}")
                        if time.monotonic() >= deadline:
                            raise RuntimeError("PATH capture wrapper was not invoked")
                        time.sleep(.01)
                    child = json.loads(ready.read_text())
                    if child["parent_pid"] != process.pid:
                        raise RuntimeError("capture wrapper is not a direct child of the fixture")
                    children.add(child["pid"])
                    result = {"delay_seconds": delay, "wrapper_pid": child["pid"]}
                    # Store even a failed case so the root can diagnose the boundary.
                    receipt[name] = result
                    if resize:
                        start = time.monotonic()
                        try:
                            result["window_size"] = ax_resize(process.pid, 1000, 700)
                        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
                            # A blocked AX request is baseline evidence; still
                            # await capture so cleanup/latency remain measurable.
                            result["resize_error"] = str(getattr(error, "stderr", None) or error)
                        finished = time.monotonic()
                        result["resize_seconds"] = finished - start
                    reply = future.result(timeout=20)
                    result["capture_seconds"] = reply["completed"] - reply["started"]
                    result["response"] = reply["response"]
                    if resize:
                        result["resize_before_capture_response"] = finished < reply["completed"]
                        result["responsive"] = ("resize_error" not in result
                                                and result["resize_seconds"] < 1
                                                and result["resize_before_capture_response"])
                        if reply["response"].get("ok") is not True or not png.is_file():
                            raise RuntimeError("delayed screenshot failed")
                        result["png"] = str(png)
                    deadline = time.monotonic() + 2
                    while alive(child["pid"]) and time.monotonic() < deadline:
                        time.sleep(.02)
                    result["wrapper_pid_gone"] = not alive(child["pid"])
                    if not result["wrapper_pid_gone"]:
                        raise RuntimeError("capture wrapper process was not reaped")
                    return result

                delayed = capture_case("delayed", 2, True)
                if args.expect_responsive and not delayed["responsive"]:
                    raise RuntimeError("AX resize blocked during capture")
                if args.timeout_case:
                    timed = capture_case("timeout", 30, False)
                    response = timed["response"]
                    if (response.get("ok") is not False
                            or "timed out" not in str(response.get("error", "")).lower()
                            or not 8 <= timed["capture_seconds"] <= 15):
                        raise RuntimeError("capture did not report its bounded 10s timeout")
                receipt["ok"] = True
    except BaseException as error:
        receipt["error"] = str(error)
        raise
    finally:
        # Kill only child PIDs observed from our wrapper, while still parented by
        # this exact fixture. Never pkill by name or touch a pre-existing client.
        for pid in children:
            if alive(pid) and process is not None:
                parent = subprocess.run(["ps", "-p", str(pid), "-o", "ppid="],
                                        capture_output=True, text=True, timeout=2)
                if parent.stdout.strip() == str(process.pid):
                    os.kill(pid, signal.SIGKILL)
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        executor.shutdown(wait=True, cancel_futures=True)
        if MARKER.exists() and MARKER.read_text().strip() == str(endpoint):
            if previous is None:
                MARKER.unlink()
            else:
                MARKER.write_bytes(previous)
        endpoint.unlink(missing_ok=True)
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps({"receipt": str(receipt_path), **receipt}, indent=2), flush=True)


if __name__ == "__main__":
    main()
