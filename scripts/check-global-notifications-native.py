#!/usr/bin/env python3
"""Verify synthetic global-notification workflows in isolated native fixtures; never builds."""
import argparse
import json
import math
import os
from pathlib import Path
import sys
import time

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("global-notifications", "global-notifications-narrow")
PREFIX = "global-notifications-check: "
FLAG = "SUPER_PLATINUM_GLOBAL_NOTIFICATIONS_CHECK"
REQUIRED = (
    "noTransport", "openNoWrite", "canonicalInit", "localOnlyOverrides",
    "linkedIndependentMobile", "explicitKeywords", "dirtyNavigation",
    "invalidNoWrite", "nativeCaret", "completionFocus", "duplicateBlocked",
    "unknownReadOnly", "exactWire", "preservedOverrides",
)
REACHED = (
    "global-desktop-mode", "global-desktop-push", "global-mobile-push",
    "global-mobile-overrides", "global-thread-replies", "global-keywords-input",
    "global-keywords-save",
)


def rect(value):
    return (isinstance(value, list) and len(value) == 4
            and all(type(n) in (int, float) and math.isfinite(n) for n in value)
            and value[0] < value[2] and value[1] < value[3])


def validate_receipt(fixture, receipt, platform=sys.platform):
    if fixture not in FIXTURES or not isinstance(receipt, dict):
        raise RuntimeError("Unknown fixture or malformed receipt")
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    missing = [key for key in REQUIRED if receipt.get(key) is not True]
    if missing:
        raise RuntimeError(f"{fixture}: missing behavior assertions: {missing}")
    for key, expected in (("writes", 9), ("reads", 12)):
        if type(receipt.get(key)) is not int or receipt[key] != expected:
            raise RuntimeError(f"{fixture}: incorrect synthetic {key} count")
    reached = receipt.get("reached")
    if (not isinstance(reached, list) or len(reached) != len(REACHED)
            or any(not isinstance(item, dict) or item.get("ok") is not True for item in reached)
            or [item.get("id") for item in reached] != list(REACHED)):
        raise RuntimeError(f"{fixture}: expected seven distinct reached controls in order")
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
        raise ValueError("Only named global-notification fixtures are allowed")
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False,
                "operationKind": "synthetic preference requests; no server or OS settings writes"}
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
