#!/usr/bin/env python3
"""Measure native Windows dev-fixture host/WebView2 CPU and memory; never builds.

Example: python -B scripts/measure-idle-windows-native.py --label baseline --seconds 15

One disconnected visible fixture, one fixed quiet window after paint/settling.
The runner sends no agent commands, focuses no window, and captures no screenshot
during that window. All captures happen after the second counter snapshot.

Scope: observed owned process tree, not all Edge processes. Working-set sums
include shared pages repeatedly; they are NOT unique RSS. PrivateUsage measures
private committed bytes, not private resident memory. Per-process snapshots are
non-atomic. Retained handles preserve CPU counters of observed exited processes;
processes born AND exited between snapshots, or behind an unobserved exited
parent, can be missed. This is fixture baseline evidence, not live Slack startup,
an improvement claim, hidden/minimized behavior, GPU memory, or whole-system cost.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from windows_process_metrics import OwnedProcessTree, filetime_now, summarize


def quiet_wait(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        time.sleep(min(1, max(0, deadline - time.monotonic())))


def startup_snapshot(fixture):
    deadline = time.monotonic() + 30
    while True:
        snapshot = fixture.control("state")["data"]
        startup = snapshot.get("performance", {}).get("startup", {})
        if isinstance(startup.get("first_paint_ms"), (int, float)):
            return {"startup": startup, "screen": snapshot.get("screen"),
                    "active_channel": snapshot.get("active_channel"),
                    "observation_monotonic": time.monotonic()}
        if time.monotonic() >= deadline:
            raise RuntimeError("fixture did not report first_paint_ms within 30 seconds")
        time.sleep(.1)


def measure(args):
    binary = args.binary.resolve()
    with binary.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    name = f"idle-windows-{args.label}"
    evidence = {
        "ok": False, "label": args.label, "fixture": args.fixture,
        "binary": str(binary), "binary_sha256": digest,
        "platform": platform.platform(), "python_pointer_bits": 64 if sys.maxsize > 2**32 else 32,
        "logical_processors": os.cpu_count(),
        "settle_seconds": args.settle_seconds, "requested_quiet_seconds": args.seconds,
        "scope": "disconnected visible dev fixture; host plus observed owned descendants",
        "limitations": [
            "Working-set sum includes shared pages repeatedly; it is not unique RSS.",
            "Private commit bytes are committed private memory, not private resident memory.",
            "Boundary snapshots are non-atomic; per-process observation times are retained.",
            "Processes born and exited between snapshots or behind unobserved exited parents can be missed.",
            "Observed exited processes retain final CPU counters; exited memory is omitted.",
            "Foreground is requested before sampling; user focus changes or occlusion are not prevented.",
            "No live Slack traffic, hardware power control, GPU memory, or performance improvement is established.",
        ],
    }
    try:
        with NativeFixture(args.fixture, directory / f"{name}.log", binary,
                           {"SUPER_PLATINUM_STARTUP_TRACE": "1"}) as fixture:
            evidence["fixture_pid"] = fixture.process.pid
            evidence["paint_snapshot"] = startup_snapshot(fixture)
            quiet_wait(args.settle_seconds)
            fixture.require_running()
            with OwnedProcessTree(fixture.process.pid) as tree:
                before = tree.snapshot()
                evidence["before"] = before
                if not any(p["group"] == "webview2_descendants" and not p.get("exit_filetime")
                           for p in before["processes"]):
                    raise RuntimeError("No owned WebView2 descendant identified; refusing host-only measurement")
                evidence["window_start_filetime"] = filetime_now()
                evidence["window_started_monotonic"] = time.monotonic()
                print(json.dumps({"phase": "quiet_window", "label": args.label,
                                  "seconds": args.seconds, "owned_processes": len(before["processes"])}), flush=True)
                quiet_wait(args.seconds)
                evidence["window_finished_monotonic"] = time.monotonic()
                evidence["quiet_elapsed_seconds"] = (evidence["window_finished_monotonic"]
                                                       - evidence["window_started_monotonic"])
                after = tree.snapshot()
                evidence["after"] = after
                evidence["measurement"] = summarize(before, after, evidence["window_start_filetime"])
            fixture.require_running()
            # All process-counter reads are complete before these commands can
            # wake the UI or create its PowerShell screenshot child process.
            evidence["after_paint_snapshot"] = startup_snapshot(fixture)
            screenshot = directory / f"{name}.png"
            fixture.capture(screenshot)
            evidence["screenshot_after_sample"] = str(screenshot)
            evidence["ok"] = evidence["measurement"]["observed_coverage_complete"]
            if not evidence["ok"]:
                raise RuntimeError("Observed process counters were incomplete; inspect errors/coverage in receipt")
    except BaseException as error:
        evidence.update(ok=False, error=str(error))
        raise
    finally:
        destination = directory / f"{name}.json"
        destination.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(destination), "ok": evidence["ok"],
                          "measurement": evidence.get("measurement"), "error": evidence.get("error")}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--output", type=Path, default=ROOT / "tmp/agent-ui")
    parser.add_argument("--label", required=True)
    parser.add_argument("--fixture", choices=("channel", "channel-window-10000"), default="channel")
    parser.add_argument("--seconds", type=float, default=15)
    parser.add_argument("--settle-seconds", type=float, default=5)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("requires native Windows; no Unix/macOS counters are substituted")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", args.label):
        parser.error("--label must use letters/numbers/underscores/hyphens")
    if not math.isfinite(args.seconds) or not 1 <= args.seconds <= 300:
        parser.error("--seconds must be finite and between 1 and 300")
    if not math.isfinite(args.settle_seconds) or not 0 <= args.settle_seconds <= 120:
        parser.error("--settle-seconds must be finite and between 0 and 120")
    if not args.binary.is_file():
        parser.error("--binary must be an existing dev executable; this script never builds")
    measure(args)


if __name__ == "__main__":
    main()
