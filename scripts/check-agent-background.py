#!/usr/bin/env python3
"""Measure agent ping latency in an isolated hidden native fixture (macOS)."""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/debug/super-platinum")
parser.add_argument("--label", required=True)
parser.add_argument("--seconds", type=float, default=180)
parser.add_argument("--idle-seconds", type=float, default=0)
args = parser.parse_args()
if not args.label.replace("-", "").isalnum() or args.seconds < 1 or args.idle_seconds < 0:
    parser.error("label must be alphanumeric/hyphen and seconds must be positive")
output = ROOT / "tmp" / ("agent-background-" + args.label)
sock = pathlib.Path(os.environ.get("TMPDIR", "/tmp")) / f"sp-background-{os.getpid()}.sock"
marker = pathlib.Path(os.environ.get("TMPDIR", "/tmp")) / "super-platinum-agent.sock.path"
previous = marker.read_text() if marker.exists() else None
env = {k: v for k, v in os.environ.items() if not k.startswith("SUPER_PLATINUM_")}
env.update(SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(sock), SUPER_PLATINUM_FIXTURE="channel")

def ping(path=sock, timeout=3):
    start = time.monotonic()
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(timeout)
        client.connect(str(path))
        client.sendall(b'{"id":1,"cmd":"ping"}\n')
        data = b""
        while b"\n" not in data:
            chunk = client.recv(4096)
            if not chunk:
                raise RuntimeError("connection closed")
            data += chunk
        reply = json.loads(data.split(b"\n", 1)[0])
        if not reply.get("ok") or reply.get("data", {}).get("pong") is not True:
            raise RuntimeError("unexpected ping reply")
    return round((time.monotonic() - start) * 1000, 2)

records = []
with output.with_suffix(".log").open("w") as log:
    process = subprocess.Popen([str(args.binary.resolve())], cwd=ROOT, env=env, stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 30
        while not sock.exists():
            if process.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("fixture failed to start")
            time.sleep(.1)
        ping()
        time.sleep(2)
        subprocess.run(["osascript", "-e", f'tell application "System Events" to set visible of (first application process whose unix id is {process.pid}) to false'], check=True, capture_output=True, timeout=5)
        started = time.monotonic()
        header = {"pid": process.pid, "binary": str(args.binary.resolve()), "duration": args.seconds,
                  "quiet_idle_seconds": args.idle_seconds,
                  "hidden_at": time.strftime("%Y-%m-%d %H:%M:%S%z"), "fixture": "channel"}
        output.with_suffix(".json").write_text(json.dumps({**header, "samples": records}, indent=2))
        print(json.dumps(header), flush=True)
        # Frequent pings themselves wake the app. A quiet interval is required
        # to exercise the App Nap case instead of merely sustained traffic.
        quiet_until = started + args.idle_seconds
        while time.monotonic() < quiet_until:
            if process.poll() is not None:
                raise RuntimeError("fixture exited during quiet interval")
            time.sleep(min(10, max(0, quiet_until - time.monotonic())))
        started = time.monotonic()
        while time.monotonic() - started < args.seconds:
            if process.poll() is not None:
                raise RuntimeError("fixture exited during measurement")
            sample = {"elapsed": round(time.monotonic() - started, 2)}
            try:
                sample.update(ok=True, latency_ms=ping())
            except (OSError, RuntimeError, ValueError) as error:
                sample.update(ok=False, error=type(error).__name__)
            records.append(sample)
            output.with_suffix(".json").write_text(json.dumps({**header, "samples": records}, indent=2))
            time.sleep(min(10, max(0, args.seconds - (time.monotonic() - started))))
        print(json.dumps({"samples": len(records), "failures": sum(not r["ok"] for r in records),
                          "max_ms": max((r.get("latency_ms", 0) for r in records), default=0)}), flush=True)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        sock.unlink(missing_ok=True)
        if marker.exists() and pathlib.Path(marker.read_text().strip()) == sock:
            marker.unlink()
            if previous:
                try:
                    ping(pathlib.Path(previous.strip()))
                    marker.write_text(previous)
                except (OSError, RuntimeError, ValueError):
                    pass
