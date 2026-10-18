#!/usr/bin/env python3
"""Measure isolated native 10,001-row channel scrolls; no Slack session or writes.

Use distinct labels and --binary paths for paired comparisons. These samples
measure one synthetic channel workload, not live startup or whole-app speed.
"""
import argparse
import ctypes
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import platform
import sys
import time

from native_fixture import DEFAULT_BINARY, NativeFixture, ROOT


class ScrollCursor:
    """Park outside the fixture client before launch; never click or refocus.

    The primary top edge avoids the normal decorated fixture's client. Verify
    the actual owned foreground window after launch instead of assuming bounds.
    User movement observed at any checkpoint prevents restoration. This cannot
    detect a move away and back between checkpoints; it is not an input monitor.
    """

    def __init__(self):
        self.user = self.dpi = self.original = self.placed = None
        self.fixture_hwnd = None
        self.data = {"enabled": sys.platform == "win32", "restored": False,
                     "movement_observed": False, "checkpoints": [],
                     "continuous_input_monitoring": False,
                     "foreground_restoration": "unchanged: runner has no foreground restore"}

    def position(self):
        point = W.POINT()
        if not self.user.GetCursorPos(ctypes.byref(point)):
            raise ctypes.WinError(ctypes.get_last_error())
        return point.x, point.y

    def __enter__(self):
        if not self.data["enabled"]:
            return self
        from windows_capture import _libraries, primary_rect, taskbar_rects, contains_rect
        self.user, _ = _libraries()
        # Win32 contracts (physical screen coordinates under per-monitor DPI):
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setcursorpos
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclientrect
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-clienttoscreen
        for name, args, result in [
            ("GetCursorPos", [ctypes.POINTER(W.POINT)], W.BOOL),
            ("SetCursorPos", [ctypes.c_int, ctypes.c_int], W.BOOL),
            ("GetForegroundWindow", [], W.HWND),
            ("GetWindowThreadProcessId", [W.HWND, ctypes.POINTER(W.DWORD)], W.DWORD),
            ("GetClientRect", [W.HWND, ctypes.POINTER(W.RECT)], W.BOOL),
            ("ClientToScreen", [W.HWND, ctypes.POINTER(W.POINT)], W.BOOL),
        ]:
            function = getattr(self.user, name)
            function.argtypes, function.restype = args, result
        self.dpi = self.user.SetThreadDpiAwarenessContext(W.HANDLE(-4))
        if not self.dpi:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            bounds = primary_rect()
            left, top, width, height = bounds
            if width < 100 or height < 100:
                raise RuntimeError("Primary display too small for cursor normalization")
            point = (left + width // 2, top + 1)
            if any(contains_rect(rect, (*point, 1, 1)) for rect in taskbar_rects()):
                raise RuntimeError("Parking point intersects taskbar; no cursor move or settings change")
            self.original = self.position()
            self.data.update(original=self.original, primary_rect=bounds,
                             foreground_before=int(self.user.GetForegroundWindow() or 0))
            if not self.user.SetCursorPos(*point):
                raise ctypes.WinError(ctypes.get_last_error())
            self.placed = self.position()
            self.data["placed"] = self.placed
            if self.placed != point:
                raise RuntimeError("Cursor placement was clipped; no normalized sample")
            return self
        except BaseException as error:
            self._close_preserving(error)
            raise

    def check(self, pid, stage):
        if not self.data["enabled"]:
            return
        point = self.position()
        moved = point != self.placed
        self.data["movement_observed"] |= moved
        hwnd = self.user.GetForegroundWindow()
        owner = W.DWORD()
        self.user.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        checkpoint = {"stage": stage, "position": point, "foreground_hwnd": int(hwnd or 0),
                      "foreground_pid": owner.value,
                      "foreground_owned": owner.value == pid}
        self.data["checkpoints"].append(checkpoint)
        if moved or owner.value != pid:
            raise RuntimeError("Cursor moved or fixture lost foreground; normalized sample rejected")
        if self.fixture_hwnd is not None and hwnd != self.fixture_hwnd:
            raise RuntimeError("Fixture foreground HWND changed; normalized sample rejected")
        self.fixture_hwnd = hwnd
        rect, origin = W.RECT(), W.POINT()
        if (not self.user.GetClientRect(hwnd, ctypes.byref(rect))
                or not self.user.ClientToScreen(hwnd, ctypes.byref(origin))):
            raise ctypes.WinError(ctypes.get_last_error())
        bounds = (origin.x, origin.y, rect.right, rect.bottom)
        x, y = point
        outside = not (origin.x <= x < origin.x + rect.right
                       and origin.y <= y < origin.y + rect.bottom)
        checkpoint.update(client_rect=bounds, outside_client=outside)
        if not outside:
            raise RuntimeError("Parked cursor is inside fixture client; normalized sample rejected")

    def close(self):
        if not self.dpi:
            return
        try:
            if self.placed is not None:
                self.data["movement_observed"] |= self.position() != self.placed
                if not self.data["movement_observed"]:
                    if not self.user.SetCursorPos(*self.original):
                        raise ctypes.WinError(ctypes.get_last_error())
                    self.data["restored"] = self.position() == self.original
        finally:
            self.user.SetThreadDpiAwarenessContext(self.dpi)
            self.dpi = None

    def _close_preserving(self, primary):
        try:
            self.close()
        except BaseException as error:
            if primary is None:
                raise
            primary.add_note(f"Cursor cleanup also failed: {type(error).__name__}: {error}")

    def __exit__(self, _kind, error, _traceback):
        self._close_preserving(error)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui")
    parser.add_argument("--label", default="baseline")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--run-start", type=int, default=1)
    args = parser.parse_args()
    if (not args.label.replace("-", "").replace("_", "").isalnum()
            or not 1 <= args.runs <= 20 or args.run_start < 1):
        parser.error("Use a simple label, positive run start, and between 1 and 20 runs")
    binary = args.binary.resolve()
    with binary.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    captures = args.captures.resolve()
    captures.mkdir(parents=True, exist_ok=True)
    for run in range(args.run_start, args.run_start + args.runs):
        name = f"channel-scroll-{args.label}-{run}"
        cursor = ScrollCursor()
        artifact = {"fixture": "channel-window-10000", "run": run, "label": args.label,
                    "binary": str(binary), "binary_sha256": digest,
                    "platform": platform.platform(), "ok": False,
                    "scope": "disconnected native channel auto-scroll; no thread workload",
                    "cursor": cursor.data}
        fixture = None
        primary = None
        try:
            with cursor, (fixture := NativeFixture(
                    "channel-window-10000", captures / f"{name}.log", binary,
                    {"SUPER_PLATINUM_PERF_SCROLL": "1",
                     "SUPER_PLATINUM_STARTUP_TRACE": "1"})):
                try:
                    cursor.check(fixture.process.pid, "after_startup")
                    # Auto-scroll performs one top/bottom sweep. Wait for the final
                    # observer batch; this is sampling time, not interaction latency.
                    time.sleep(4)
                    cursor.check(fixture.process.pid, "after_sample")
                    snapshot = fixture.control("state")
                    data = snapshot.get("data", snapshot)
                    metrics = data["performance"]
                    if not metrics["scroll_frame_ms"]:
                        raise RuntimeError("No visible native scroll frames recorded")
                    artifact["performance"] = metrics
                    artifact["post_sample_state"] = {key: data.get(key) for key in
                                                      ("timeline", "profile_hover", "screen")}
                    if data.get("profile_hover") is not None:
                        raise RuntimeError("Profile hover is open; normalized sample rejected")
                    fixture.capture(captures / f"{name}.png")
                    cursor.check(fixture.process.pid, "after_capture")
                    # Diagnostic captures do not replace the original PNG or alter
                    # the primary four-second metrics snapshot. No further scroll,
                    # pointer movement, or focus request is sent to repair a pane.
                    immediate = time.monotonic()
                    artifact["delayed_diagnostics"] = []
                    for delay in (1, 3):
                        time.sleep(max(0, immediate + delay - time.monotonic()))
                        cursor.check(fixture.process.pid, f"before_delayed_{delay}s")
                        later = fixture.control("state")
                        later = later.get("data", later)
                        path = captures / f"{name}-delayed-{delay}s.png"
                        fixture.capture(path)
                        cursor.check(fixture.process.pid, f"after_delayed_{delay}s")
                        artifact["delayed_diagnostics"].append({
                            "requested_delay_seconds": delay,
                            "capture_completed_seconds_after_immediate": time.monotonic() - immediate,
                            "path": str(path),
                            "state": {key: later.get(key) for key in
                                      ("timeline", "profile_hover", "screen")},
                            "scroll_samples": len(later["performance"]["scroll_frame_ms"]),
                        })
                except BaseException as sample_error:
                    try:
                        fixture.capture(captures / f"{name}-failure.png")
                    except BaseException as capture_error:
                        # Capture is diagnostic; the sample failure stays primary.
                        sample_error.add_note(
                            f"Failure screenshot also failed: {type(capture_error).__name__}: {capture_error}")
                    raise
        except BaseException as error:
            primary = error
            artifact.update(ok=False, error=f"{type(error).__name__}: {error}")
        finally:
            # A completed sample is not a successful run until both contexts
            # exit. NativeFixture can retain nonfatal temporary cleanup errors.
            artifact["cleanup_errors"] = list(fixture.cleanup_errors) if fixture else []
            try:
                artifact["process_exited"] = (fixture is not None and fixture.process is not None
                                              and fixture.process.poll() is not None)
            except BaseException as error:
                artifact["process_exited"] = False
                artifact["closure_observation_error"] = f"{type(error).__name__}: {error}"
            # Preserve a primary startup/sample/exit exception and its receipt.
            # This block never raises a secondary observation failure over it.
            if "error" not in artifact:
                artifact["ok"] = artifact["process_exited"] and not artifact["cleanup_errors"]
                if not artifact["ok"]:
                    artifact["error"] = "Scroll fixture process or cleanup did not close cleanly"
                    primary = RuntimeError(artifact["error"])
            try:
                (captures / f"{name}.json").write_text(
                    json.dumps(artifact, indent=2) + chr(10), encoding="utf-8")
            except BaseException as error:
                if primary is None:
                    raise
                primary.add_note(f"Result could not be written: {type(error).__name__}: {error}")
        if primary is not None:
            raise primary.with_traceback(primary.__traceback__)
        print(json.dumps({"label": args.label, "run": run,
                          "summaries": metrics["summaries"]}), flush=True)


if __name__ == "__main__":
    main()
