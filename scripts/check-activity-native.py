#!/usr/bin/env python3
"""Check Activity navigation in isolated Windows/macOS dev fixtures; never builds.

Captures are native PNGs, which must also be inspected for visual correctness.
The process uses a private temporary agent marker and never touches a live client.
"""
import argparse
import os
from pathlib import Path
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


def check_fixture(fixture, binary, captures):
    capture_name = fixture
    log = captures / f"{capture_name}.log"
    with NativeFixture(fixture, log, binary,
                       {"SUPER_PLATINUM_ACTIVITY_CHECK": "1"}) as native:
        try:
            deadline = time.monotonic() + 90
            last_activation = 0.0
            while True:
                if time.monotonic() - last_activation > 1:
                    native.focus()
                    last_activation = time.monotonic()
                receipts = native.receipts("activity-navigation-check: ")
                if receipts:
                    receipt = receipts[-1]
                    if receipt.get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: native assertion failed: {receipt}")
                    required = ("bundleOpensLatest", "channelClosesThread", "exactReplyVisible",
                                "activitySurfacePreserved", "noTransport",
                                "canonicalBadgeUpdates", "zeroClearsUnreadRows",
                                "cursorBoundedReads", "cursorPageReconciliation",
                                "filteredCategories", "dmHighlightCount", "dmExcludedChannels", "optOutConservation",
                                "guardedCategoryFlags", "unknownMapNoFallback")
                    viewport = receipt.get("viewport", [])
                    expected_viewport = [668, 400] if fixture.endswith("-narrow") else [1280, 800]
                    # The pinned desktop runtime's Windows menu occupies 20
                    # logical pixels inside Tao's requested client area. Keep
                    # the actual smaller WebView2 viewport explicit in evidence.
                    if sys.platform == "win32":
                        expected_viewport[1] -= 20
                    if viewport != expected_viewport:
                        raise RuntimeError(f"{capture_name}: incorrect native viewport: {viewport}")
                    if any(receipt.get(key) is not True for key in required):
                        raise RuntimeError(f"{capture_name}: incomplete native receipt: {receipt}")
                    if receipt.get("bundleGeometry", {}).get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: bundle did not reach visible latest reply")
                    break
                native.require_running()
                if time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: native assertions timed out")
                time.sleep(.1)
            # The control-plane command must preserve the same old-reply target
            # as the DOM click path, rather than reopening at latest.
            for index, at_bottom in ((1, True), (0, False)):
                native.control("activity-select", index=index)
                deadline = time.monotonic() + 8
                while True:
                    state = native.control("state")["data"]
                    target = None if at_bottom else "1719800001.000001"
                    if (state.get("main_view") == "activity"
                            and state.get("active_thread") == "1719800000.000100"
                            and state.get("thread_timeline", {}).get("at_bottom") is at_bottom
                            # Successful DOM scrolling consumes the target.
                            and state.get("thread_timeline", {}).get("target") in (None, target)):
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError(f"{capture_name}: agent command lost exact reply")
                    time.sleep(.1)
                stage = "bundle" if at_bottom else "reply"
                deadline = time.monotonic() + 10
                while True:
                    receipts = native.receipts("activity-agent-check: ")
                    if any(r.get("ok") is not True for r in receipts):
                        raise RuntimeError(f"{capture_name}: agent geometry failed: {receipts}")
                    proof = next((r for r in receipts if r.get("stage") == stage), None)
                    if proof is not None:
                        geometry = proof.get("geometry", {})
                        if geometry.get("ok") is not True:
                            raise RuntimeError(f"{capture_name}: missing agent {stage} geometry")
                        if not at_bottom and (geometry.get("replyId") != target
                                              or proof.get("noTransport") is not True):
                            raise RuntimeError(f"{capture_name}: agent displayed the wrong reply")
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError(f"{capture_name}: no agent {stage} geometry receipt")
                    time.sleep(.1)
            native.capture(captures / f"{capture_name}.png")
            print(f"{capture_name}: native Activity navigation interactions passed; screenshot captured", flush=True)
        except BaseException:
            try:
                native.capture(captures / f"{capture_name}-failure.png")
            except (OSError, RuntimeError, ValueError):
                pass  # Preserve the original startup/assertion failure.
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append",
                        choices=("activity-navigation", "activity-navigation-narrow"),
                        help="run only this fixture (repeatable); default runs both")
    args = parser.parse_args()
    captures = args.captures.resolve()
    for fixture in args.fixture or ("activity-navigation", "activity-navigation-narrow"):
        check_fixture(fixture, args.binary, captures)


if __name__ == "__main__":
    main()
