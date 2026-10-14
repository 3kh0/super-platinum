#!/usr/bin/env python3
"""Exercise synthetic COM callbacks through the real Windows routing hook.

Uses only a disconnected fixture and synthetic in-memory session. This is not
an actual OS toast click or cold process launch check. Never builds.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path,
                        default=ROOT / "tmp/agent-ui/windows-notification-route")
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("This check requires native Windows")
    path = Path(__file__).with_name("check-notifications-windows-native.py")
    spec = importlib.util.spec_from_file_location("toast_check", path)
    toast = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(toast)
    before = toast.registration_snapshot()
    output = args.captures.resolve()
    output.mkdir(parents=True, exist_ok=True)
    evidence = {"ok": False, "actual_os_click": False, "cold_process_launch": False,
                "binary": str(args.binary.resolve()),
                "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    try:
        with NativeFixture("channel", output / "notification-route.log", args.binary,
                           {"SUPER_PLATINUM_NOTIFICATION_ROUTE_CHECK": "1"}) as fixture:
            deadline = time.monotonic() + 45
            while True:
                fixture.require_running()
                receipts = fixture.receipts("notification-route-check: ")
                if receipts:
                    evidence["native"] = receipts[-1]
                    required = ("ok", "syntheticComCallback", "coldQueuedLatest",
                                "exactThreadDestination", "wrongAccountPreserved",
                                "previewForegroundOnly", "noTransport", "noCache", "replyVisible")
                    if any(receipts[-1].get(key) is not True for key in required):
                        raise RuntimeError(f"Route check failed: {receipts[-1]}")
                    if receipts[-1].get("actualOsClick") is not False:
                        raise RuntimeError("Synthetic callback cannot prove an OS click")
                    screenshot = output / "notification-route.png"
                    fixture.capture(screenshot)
                    evidence.update(ok=True, screenshot=str(screenshot))
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("Notification route check timed out")
                time.sleep(.1)
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        evidence["registration_restored"] = toast.registration_snapshot() == before
        if not evidence["registration_restored"]:
            evidence["ok"] = False
        (output / "notification-route.json").write_text(
            json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(evidence), flush=True)
    if not evidence["ok"]:
        raise RuntimeError("Route check changed application registration")


if __name__ == "__main__":
    main()
