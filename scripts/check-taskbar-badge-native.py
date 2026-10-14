#!/usr/bin/env python3
"""Exercise an isolated Windows dev taskbar badge and capture actual taskbar pixels.

No Slack traffic, Explorer restart, focus-setting change, or taskbar preference
write. A successful HRESULT is separate from pixels: inspect every stage PNG.
Auto-hide/small taskbar icons or shell grouping can suppress visible overlays.
"""
import argparse
import ctypes
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import warnings

from native_fixture import NativeFixture, ROOT
from windows_capture import capture_rect, contains_rect, monitor_rect, taskbar_rects

STAGES = ["clear", "number", "syntheticRecreation", "unchanged", "dot", "zero",
          "newNumber", "accountChange", "retired", "beforeDrop", "dropClear"]
LABELS = [None, "42", "42", "42", "•", None, "7", "7", None, "•", None]
WRITES = [1, 1, 0, 0, 1, 1, 1, 2, 1, 1, 1]
PROJECTION_STAGES = ["activityExcludesReaction", "activityZero", "eligibleLater",
                     "hiddenLater", "positiveBeforeRetirement", "accountRetirement"]
PROJECTION_LABELS = ["6", None, "4", None, "4", None]


def edge_point(rect, monitor):
    """Hover only the visible screen-edge intersection of the observed taskbar."""
    left, top, width, height = rect
    x, y, w, h = monitor
    if width >= height:
        px = max(x, min(left + width // 2, x + w - 1))
        py = y if top < y else y + h - 1
    else:
        px = x if left < x else x + w - 1
        py = max(y, min(top + height // 2, y + h - 1))
    return px, py


class TaskbarHover:
    """Optional runner-only hover; never change auto-hide or other OS settings."""
    def __init__(self, enabled):
        self.enabled = enabled
        self.original = self.placed = self.prior_dpi = None
        self.user_moved = self.restored = False
        self.user = None

    def __enter__(self):
        if self.enabled:
            self.user = ctypes.WinDLL("user32", use_last_error=True)
            self.user.GetCursorPos.argtypes = [ctypes.POINTER(W.POINT)]
            self.user.GetCursorPos.restype = W.BOOL
            self.user.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
            self.user.SetCursorPos.restype = W.BOOL
            self.user.SetThreadDpiAwarenessContext.argtypes = [W.HANDLE]
            self.user.SetThreadDpiAwarenessContext.restype = W.HANDLE
            self.prior_dpi = self.user.SetThreadDpiAwarenessContext(W.HANDLE(-4))
            if not self.prior_dpi:
                raise ctypes.WinError(ctypes.get_last_error())
        return self

    def position(self):
        point = W.POINT()
        if not self.user.GetCursorPos(ctypes.byref(point)):
            raise ctypes.WinError(ctypes.get_last_error())
        return point.x, point.y

    def observe_cursor(self):
        if self.placed is not None and self.position() != self.placed:
            self.user_moved = True

    def visible_rects(self, hwnd):
        monitor = monitor_rect(hwnd)
        rectangles = taskbar_rects(hwnd)
        self.observe_cursor()
        hidden = [rect for rect in rectangles if not contains_rect(monitor, rect)]
        if not hidden:
            return rectangles
        if not self.enabled:
            raise RuntimeError(f"Taskbar is partly offscreen ({hidden}, monitor {monitor}); "
                               "visual evidence is insufficient. Use --reveal-auto-hidden for "
                               "a temporary screen-edge hover without changing OS settings.")
        if self.user_moved:
            raise RuntimeError("Cursor moved after runner hover; leaving it untouched; taskbar evidence insufficient")
        point = edge_point(hidden[0], monitor)
        if self.original is None:
            self.original = self.position()
        if not self.user.SetCursorPos(*point):
            raise ctypes.WinError(ctypes.get_last_error())
        self.placed = point
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            self.observe_cursor()
            if self.user_moved:
                raise RuntimeError("Cursor moved during taskbar reveal; leaving it untouched")
            rectangles = taskbar_rects(hwnd)
            if all(contains_rect(monitor, rect) for rect in rectangles):
                return rectangles
            time.sleep(.05)
        raise RuntimeError("Taskbar did not become fully onscreen within 5 seconds; visual evidence insufficient")

    def close(self):
        if self.prior_dpi is None:
            return
        try:
            self.observe_cursor()
            if self.placed is not None and not self.user_moved:
                if not self.user.SetCursorPos(*self.original):
                    raise ctypes.WinError(ctypes.get_last_error())
                self.restored = True
        finally:
            self.user.SetThreadDpiAwarenessContext(self.prior_dpi)
            self.prior_dpi = None

    def receipt(self):
        return {"requested": self.enabled, "hovered": self.placed is not None,
                "original": self.original, "placed": self.placed,
                "userMoved": self.user_moved, "cursorRestored": self.restored,
                "osSettingsChanged": False}

    def __exit__(self, exc_type, exc, traceback):
        try:
            self.close()
        except OSError as error:
            warnings.warn(f"Could not restore runner cursor: {error}")


def validate(receipt):
    for key in ("ok", "noTransport", "noPreferenceWrites", "isolatedAppId",
                "nativeHresultSuccess", "unchangedNoWrite", "ownerClear",
                "dropClear", "syntheticTaskbarRecovery"):
        if receipt.get(key) is not True:
            raise RuntimeError(f"Incomplete taskbar receipt ({key}): {receipt}")
    if (receipt.get("writes") != 10 or receipt.get("actualExplorerRestart") is not False
            or receipt.get("visualTaskbarPixelsProven") is not False):
        raise RuntimeError(f"Overstated or incomplete taskbar result: {receipt}")
    stages = receipt.get("stages", [])
    if [stage.get("stage") for stage in stages] != STAGES:
        raise RuntimeError(f"Missing taskbar stages: {receipt}")
    for stage, label, writes in zip(stages, LABELS, WRITES):
        native = stage.get("native", {})
        if (native.get("label") != label or native.get("desired_label") != label
                or native.get("last_error") is not None or stage.get("writes") != writes
                or native.get("attempts") != native.get("successes")):
            raise RuntimeError(f"Unexpected native badge state: {stage}")


def validate_projection(receipt):
    for key in ("ok", "noTransport", "noPreferenceWrites", "isolatedAppId",
                "productionHook", "singleProductionSink", "nativeHresultSuccess",
                "filteredActivity", "authoritativeZero", "laterEligibility", "ownerClear"):
        if receipt.get(key) is not True:
            raise RuntimeError(f"Incomplete production taskbar receipt ({key}): {receipt}")
    if (receipt.get("mode") != "projection" or receipt.get("actualExplorerRestart") is not False
            or receipt.get("visualTaskbarPixelsProven") is not False):
        raise RuntimeError(f"Overstated or incomplete production taskbar result: {receipt}")
    stages = receipt.get("stages", [])
    if [stage.get("stage") for stage in stages] != PROJECTION_STAGES:
        raise RuntimeError(f"Missing production taskbar stages: {receipt}")
    previous = 0
    for ordinal, (stage, label) in enumerate(zip(stages, PROJECTION_LABELS)):
        native = stage.get("native", {})
        successes, writes = native.get("successes"), stage.get("writes")
        if (type(successes) is not int or type(writes) is not int
                or successes <= previous or not 0 < writes <= successes - previous
                or native.get("attempts") != successes or native.get("last_error") is not None
                or "label" not in native or "desired_label" not in native or "cachedLabel" not in stage
                or native.get("label") != label or native.get("desired_label") != label
                or stage.get("cachedLabel") != label or stage.get("ordinal") != ordinal):
            raise RuntimeError(f"Unexpected production taskbar state: {stage}")
        previous = successes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="already-built native dev executable")
    parser.add_argument("--projection", action="store_true",
                        help="exercise the actual production hook from cached Activity/Later/account state")
    parser.add_argument("--reveal-auto-hidden", action="store_true",
                        help="temporarily hover the observed taskbar edge; restore cursor unless it moved")
    parser.add_argument("--output", type=Path, default=Path(os.environ.get(
        "SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows taskbar badge check requires Windows")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    basename = "taskbar-badge-projection" if args.projection else "taskbar-badge"
    expected_stages = PROJECTION_STAGES if args.projection else STAGES
    log = output / f"{basename}.log"
    captures = []
    with tempfile.TemporaryDirectory(prefix="sp-taskbar-ack-") as directory:
        checks = {"SUPER_PLATINUM_TASKBAR_BADGE_CHECK": "projection" if args.projection else "1",
                  "SUPER_PLATINUM_TASKBAR_BADGE_ACK_DIR": directory}
        with NativeFixture("channel", log, binary=args.binary, checks=checks) as fixture, TaskbarHover(args.reveal_auto_hidden) as hover:
            deadline = time.monotonic() + 180
            seen = 0
            while True:
                fixture.require_running()
                receipts = fixture.receipts("taskbar-badge-check: ")
                if receipts:
                    receipt = receipts[-1]
                    (validate_projection if args.projection else validate)(receipt)
                    if seen != len(expected_stages):
                        raise RuntimeError("Native check completed without all taskbar captures")
                    receipt["captures"] = captures
                    receipt["captureScope"] = "actual physical taskbar rectangles on fixture monitor"
                    receipt["requiresVisualInspection"] = True
                    hover.close()
                    receipt["cursorReveal"] = hover.receipt()
                    (output / f"{basename}.json").write_text(
                        json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
                    print(json.dumps(receipt, ensure_ascii=False), flush=True)
                    return
                stages = fixture.receipts("taskbar-badge-stage: ")
                for stage in stages[seen:]:
                    ordinal = stage.get("ordinal")
                    if (ordinal != seen or seen >= len(expected_stages)
                            or stage.get("stage") != expected_stages[seen]):
                        raise RuntimeError(f"Unexpected taskbar capture checkpoint: {stage}")
                    paths = []
                    for number, rect in enumerate(hover.visible_rects(stage["hwnd"])):
                        path = output / f"{basename}-{stage['stage']}-{number}.png"
                        path.unlink(missing_ok=True)
                        metadata = capture_rect(path, *rect)
                        paths.append(metadata)
                    captures.append({"stage": stage["stage"], "images": paths})
                    # Only acknowledge after fresh screen pixels reached the file.
                    (Path(directory) / f"{ordinal}.ack").write_text("captured\n", encoding="utf-8")
                    seen += 1
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Taskbar badge check timed out; see {log}")
                time.sleep(.05)


if __name__ == "__main__":
    main()
