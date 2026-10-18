#!/usr/bin/env python3
"""Measure native Windows dev-fixture host/WebView2 CPU and memory; never builds.

Example: python -B scripts/measure-idle-windows-native.py --label baseline --seconds 15

One disconnected visible fixture, one fixed quiet window after paint/settling.
The runner sends no agent commands, focuses no window, and captures no screenshot
during that window. All captures happen after the second counter snapshot.
Optional pointer/work-area normalization happens before settling. During the
quiet window its 250ms Win32 observations are read-only; they send no explicit window
messages. Use matching flags in paired runs. Observed input/geometry changes
make a sample inconclusive; the runner never re-parks against user movement.

Scope: observed owned process tree, not all Edge processes. Working-set sums
include shared pages repeatedly; they are NOT unique RSS. PrivateUsage measures
private committed bytes, not private resident memory. Per-process snapshots are
non-atomic. Retained handles preserve CPU counters of observed exited processes;
processes born AND exited between snapshots, or behind an unobserved exited
parent, can be missed. This is fixture baseline evidence, not live Slack startup,
an improvement claim, hidden/minimized behavior, GPU memory, or whole-system cost.
"""
import argparse
from contextlib import ExitStack
import hashlib
import importlib
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
from windows_client_occlusion import sample_client_ownership


def quiet_wait(seconds, observe=None):
    deadline = time.monotonic() + seconds
    if observe:
        observe()
    while time.monotonic() < deadline:
        time.sleep(min(.25 if observe else 1, max(0, deadline - time.monotonic())))
        if observe:
            observe()


class NormalizedObservation:
    """Optional passive guards; quiet polling never explicitly sends window messages."""
    def __init__(self, fixture, tree, evidence):
        self.fixture, self.tree = fixture, tree
        self.data = evidence
        self.pointer = self.window = None
        self.hit_probe = sample_client_ownership
        # Reuse the static runner's existing read-only physical geometry probe.
        self.visual = importlib.import_module("check-visual-native")
        self.observe = lambda: self.visual.window_observation(fixture.process.pid)
        root = tree.owned[fixture.process.pid]
        self.data["process_identity"] = {"pid": root["pid"],
                                        "creation_filetime": root["creation_filetime"]}
        self.data["observations"] = []
        self.data["quiet_observer"] = "Passive Win32 cursor, process identity, foreground/window/taskbar geometry and nine client HWND ownership hits"
        self.data["quiet_poll_interval_seconds"] = .25
        self.data["continuous_input_monitoring"] = False

    def verify_identity(self):
        self.fixture.require_running()
        root = self.tree.owned[self.fixture.process.pid]
        current = self.tree.api.times(root["handle"])
        if current["creation_filetime"] != root["creation_filetime"] or current["exit_filetime"]:
            raise RuntimeError("Retained owned process identity is no longer live")

    def prepare(self, args, stack):
        from visual_pointer import FixturePointer
        self.window_identity = FixturePointer.identity
        if args.fit_work_area:
            from visual_placement import fit_work_area
            fit_work_area(self.observe, self.verify_identity, self.data["placement"])
        if args.normalize_pointer:
            self.pointer = FixturePointer(self.observe, self.verify_identity)
            self.data["pointer"] = self.pointer.data
            def restore_pointer(_kind, primary, _traceback):
                try:
                    self.pointer.close()
                except BaseException as error:
                    if primary is None:
                        raise
                    primary.add_note(f"Pointer restoration also failed: {type(error).__name__}: {error}")
            stack.push(restore_pointer)
            self.pointer.park()
            self.pointer.settle(self.fixture, self.visual.fingerprint)
        self.verify_identity()
        self.window = self.observe()
        self.data["window_before_settle"] = self.window
        self.check("prepared")

    def check(self, stage):
        # Do NOT call FixturePointer.check here: its caption hit-test sends
        # WM_NCHITTEST. No explicit message is sent here; WindowFromPoint internal work is not excluded.
        record = {"stage": stage, "monotonic": time.monotonic()}
        self.data["observations"].append(record)
        try:
            if self.pointer:
                position = self.pointer.io.position()
                record["position"] = position
                self.pointer.data["movement_observed"] |= position != self.pointer.placed
                if self.pointer.data["movement_observed"]:
                    raise RuntimeError("Pointer movement observed; idle sample is inconclusive")
            self.verify_identity()
            window = self.observe()
            record["window"] = window
            if self.window_identity(window) != self.window_identity(self.window):
                raise RuntimeError("Owned foreground/window geometry changed; idle sample is inconclusive")
            if window.get("taskbar_client_overlap") is not False:
                raise RuntimeError("Taskbar overlaps owned client; idle sample is inconclusive")
            record["client_ownership"] = self.hit_probe(window)
            if record["client_ownership"].get("ok") is not True:
                raise RuntimeError("Client ownership hit was foreign or unknown; idle sample is inconclusive")
        except BaseException as error:
            record["error"] = str(error)
            self.data["inconclusive"] = True
            raise


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
        "normalization": {"pointer": {"enabled": args.normalize_pointer},
                          "placement": {"enabled": args.fit_work_area}},
        "scope": "disconnected visible dev fixture; host plus observed owned descendants",
        "limitations": [
            "Working-set sum includes shared pages repeatedly; it is not unique RSS.",
            "Private commit bytes are committed private memory, not private resident memory.",
            "Boundary snapshots are non-atomic; per-process observation times are retained.",
            "Processes born and exited between snapshots or behind unobserved exited parents can be missed.",
            "Observed exited processes retain final CPU counters; exited memory is omitted.",
            "Foreground is requested before sampling; user focus changes or occlusion are not prevented.",
            "Optional normalization observes pointer/window state every 250ms; movement away and back between polls can escape detection.",
            "Optional passive Win32 polling adds runner-side observer work; compare only matching normalization settings.",
            "WindowFromPoint may internally perform hit testing; zero implicit message work is not asserted. Compare matching observer-enabled runs.",
            "Client HWND ownership uses nine physical points per poll; overlays between points/times or skipped by WindowFromPoint may escape detection. PNG inspection remains required.",
            "No live Slack traffic, hardware power control, GPU memory, or performance improvement is established.",
        ],
    }
    fixture = None
    primary = None

    def failed(stage, error):
        nonlocal primary
        evidence["ok"] = False
        if primary is None:
            primary = error
            evidence["error"] = str(error)
        else:
            primary.add_note(f"{stage} also failed: {type(error).__name__}: {error}")
        evidence.setdefault("finalization_errors", []).append(
            {"stage": stage, "error": str(error)})

    try:
        fixture = NativeFixture(args.fixture, directory / f"{name}.log", binary,
                                {"SUPER_PLATINUM_STARTUP_TRACE": "1"})
        with fixture:
            evidence["fixture_pid"] = fixture.process.pid
            evidence["paint_snapshot"] = startup_snapshot(fixture)
            with OwnedProcessTree(fixture.process.pid) as tree, ExitStack() as stack:
                normalization = None
                if args.normalize_pointer or args.fit_work_area:
                    normalization = NormalizedObservation(fixture, tree, evidence["normalization"])
                    normalization.prepare(args, stack)
                quiet_wait(args.settle_seconds,
                           (lambda: normalization.check("settle")) if normalization else None)
                fixture.require_running()
                before = tree.snapshot()
                evidence["before"] = before
                if not any(p["group"] == "webview2_descendants" and not p.get("exit_filetime")
                           for p in before["processes"]):
                    raise RuntimeError("No owned WebView2 descendant identified; refusing host-only measurement")
                evidence["window_start_filetime"] = filetime_now()
                evidence["window_started_monotonic"] = time.monotonic()
                print(json.dumps({"phase": "quiet_window", "label": args.label,
                                  "seconds": args.seconds, "owned_processes": len(before["processes"])}), flush=True)
                quiet_wait(args.seconds,
                           (lambda: normalization.check("quiet")) if normalization else None)
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
                if normalization:
                    normalization.check("before_capture")
                screenshot = directory / f"{name}.png"
                fixture.capture(screenshot)
                evidence["screenshot_after_sample"] = str(screenshot)
                if normalization:
                    normalization.check("after_capture")
                if not evidence["measurement"]["observed_coverage_complete"]:
                    raise RuntimeError("Observed process counters were incomplete; inspect errors/coverage in receipt")
                # ExitStack restores the pointer before closing retained process
                # handles or terminating the fixture, including on failure.
            if args.normalize_pointer:
                pointer = evidence["normalization"]["pointer"]
                if pointer["movement_observed"] or not pointer["restore_observed"]:
                    evidence["normalization"]["inconclusive"] = True
                    raise RuntimeError("Pointer movement or incomplete restoration; idle sample is inconclusive")
        # NativeFixture may retain a nonfatal cleanup error. Sampling success
        # is not a completed run until the owned process and cleanup close.
        evidence["cleanup_errors"] = list(fixture.cleanup_errors)
        evidence["process_exited"] = (fixture.process is not None
                                      and fixture.process.poll() is not None)
        if not evidence["process_exited"] or evidence["cleanup_errors"]:
            raise RuntimeError("Idle fixture process or cleanup did not close cleanly")
        evidence["ok"] = True
    except BaseException as error:
        primary = error
        evidence.update(ok=False, error=str(error))
    finally:
        # Preserve closure observations even when startup or measurement failed.
        try:
            evidence["cleanup_errors"] = list(fixture.cleanup_errors) if fixture else []
        except BaseException as error:
            failed("cleanup observation", error)
        try:
            evidence["process_exited"] = (fixture is not None and fixture.process is not None
                                          and fixture.process.poll() is not None)
        except BaseException as error:
            evidence["process_exited"] = False
            evidence["closure_observation_error"] = str(error)
            failed("process closure observation", error)
        if primary is None and (not evidence["process_exited"] or evidence.get("cleanup_errors")):
            failed("fixture closure", RuntimeError("Idle fixture process or cleanup did not close cleanly"))
        destination = directory / f"{name}.json"
        try:
            destination.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        except BaseException as error:
            failed("receipt write", error)
        try:
            print(json.dumps({"receipt": str(destination), "ok": evidence["ok"],
                              "measurement": evidence.get("measurement"), "error": evidence.get("error")}), flush=True)
        except BaseException as error:
            failed("summary output", error)
    if primary is not None:
        raise primary.with_traceback(primary.__traceback__)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--output", type=Path, default=ROOT / "tmp/agent-ui")
    parser.add_argument("--label", required=True)
    parser.add_argument("--fixture", choices=("channel", "channel-window-10000", "completion-directory-2000", "completion-directory-10000", "completion-directory-64000"), default="channel")
    parser.add_argument("--seconds", type=float, default=15)
    parser.add_argument("--settle-seconds", type=float, default=5)
    parser.add_argument("--normalize-pointer", action="store_true",
                        help="Park on owned title bar before settling; passively observe and safely restore after capture")
    parser.add_argument("--fit-work-area", action="store_true",
                        help="Move owned fixture into taskbar-aware monitor area before settling; never resize")
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
