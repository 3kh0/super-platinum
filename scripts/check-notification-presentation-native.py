#!/usr/bin/env python3
"""Verify isolated notification presentation with real local audio previews; never builds."""
import argparse
import json
import math
import os
from pathlib import Path
import sys
import time

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("notification-presentation", "notification-presentation-narrow")
PREFIX = "notification-presentation-check: "
FLAG = "SUPER_PLATINUM_NOTIFICATION_PRESENTATION_CHECK"
REQUIRED = (
    "noTransport", "openNoWrite", "canonicalInit", "conditionalControls",
    "previewNoWrite", "selectionPreview", "duplicateBlocked", "unknownReadOnlyCheck",
    "exactWire", "unknownPreserved", "completionFocus", "tabNavigation",
)
CONTROLS = (
    "presentation-desktop-sound", "presentation-include-preview",
    "presentation-mute-sounds", "presentation-priority-sound",
    "presentation-priority-sound-preview",
)
MAC_CONTROLS = ("presentation-dock-bounce", "presentation-dock-duration")


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def rect(value):
    return (isinstance(value, list) and len(value) == 4
            and all(number(n) for n in value)
            and value[0] < value[2] and value[1] < value[3])


def validate_receipt(fixture, receipt, platform=sys.platform):
    if fixture not in FIXTURES or not isinstance(receipt, dict):
        raise RuntimeError("Unknown fixture or malformed receipt")
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    missing = [key for key in REQUIRED if receipt.get(key) is not True]
    if missing:
        raise RuntimeError(f"{fixture}: missing behavior assertions: {missing}")
    macos = platform == "darwin"
    if receipt.get("dockBounce") is not macos:
        raise RuntimeError(f"{fixture}: wrong platform Dock contract")
    counts = (10, 16) if macos else (6, 11)
    for key, expected in zip(("writes", "reads"), counts):
        if type(receipt.get(key)) is not int or receipt[key] != expected:
            raise RuntimeError(f"{fixture}: incorrect synthetic {key} count")
    controls = list(CONTROLS + (MAC_CONTROLS if macos else ()))
    reached = receipt.get("reached")
    if (not isinstance(reached, list) or len(reached) != len(controls)
            or any(not isinstance(item, dict) or item.get("ok") is not True for item in reached)
            or [item.get("id") for item in reached] != controls):
        raise RuntimeError(f"{fixture}: expected distinct reached controls in order")
    preview = receipt.get("preview")
    playback = preview.get("preview") if isinstance(preview, dict) else None
    if (not isinstance(preview, dict) or preview.get("ok") is not True
            or not isinstance(playback, dict) or playback.get("state") != "ended"
            or playback.get("name") != "been_tree.mp3"
            or any(not number(playback.get(key)) or playback[key] <= 0
                   for key in ("duration", "currentTime"))):
        raise RuntimeError(f"{fixture}: bundled local audio preview did not complete")
    viewport = expected_viewport(fixture, platform)
    for key in ("layout", "finalLayout"):
        layout = receipt.get(key)
        if (not isinstance(layout, dict) or layout.get("ok") is not True
                or layout.get("viewport") != viewport):
            raise RuntimeError(f"{fixture}: incorrect {key} viewport or assertion")
        dialog, scroll = layout.get("dialog"), layout.get("scroll")
        if (not rect(dialog) or not rect(scroll)
                or not (0 <= dialog[0] < dialog[2] <= viewport[0]
                        and 0 <= dialog[1] < dialog[3] <= viewport[1])
                or not (dialog[0] <= scroll[0] < scroll[2] <= dialog[2]
                        and dialog[1] <= scroll[1] < scroll[3] <= dialog[3])
                or scroll[3] - scroll[1] < 64):
            raise RuntimeError(f"{fixture}: invalid {key} dialog/scroll bounds")


def validate_receipts(fixture, receipts, platform=sys.platform):
    if not isinstance(receipts, list) or len(receipts) != 1:
        raise RuntimeError(f"{fixture}: expected exactly one final receipt")
    validate_receipt(fixture, receipts[0], platform)
    return receipts[0]


def run(fixture, binary, captures):
    if fixture not in FIXTURES:
        raise ValueError("Only named notification-presentation fixtures are allowed")
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False,
                "operationKind": "synthetic preference requests; no server or OS settings writes",
                "localAudioPreviewEnabled": True,
                "localAudioEffect": "Plays bundled notification previews through the local audio element; not an OS notification"}
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary, {FLAG: "1"}) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline, last_focus = time.monotonic() + 90, 0.0
                while True:
                    native.require_running()
                    receipts = native.receipts(PREFIX)
                    if receipts:
                        evidence["nativeReceipts"] = receipts
                        evidence["native"] = validate_receipts(fixture, receipts)
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{fixture}: native assertions timed out")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                screenshot = captures / f"{fixture}.png"
                native.capture(screenshot)
                # Reject a second/failing receipt arriving during capture.
                evidence["nativeReceipts"] = native.receipts(PREFIX)
                evidence["native"] = validate_receipts(fixture, evidence["nativeReceipts"])
                evidence["screenshot"] = str(screenshot)
            except BaseException:
                try:
                    path = captures / f"{fixture}-failure.png"
                    native.capture(path)
                    evidence["failure_screenshot"] = str(path)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned notification fixture did not exit")
        evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=str(error))
        raise
    finally:
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary, args.captures.resolve())
