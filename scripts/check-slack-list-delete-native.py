#!/usr/bin/env python3
"""Check isolated native List deletion on Windows/macOS; never builds."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from native_check import CheckSpec, validate_receipt as validate_native_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

SPEC = CheckSpec(
    fixture="slack-list",
    flag="SUPER_PLATINUM_SLACK_LIST_DELETE_CHECK",
    prefix="slack-list-delete-check: ",
    required=(
        "cancelNoSend", "duplicateBlocked", "exactReadbackOnly", "uncertainNoReplay",
        "reopenLocked", "differentDetailPreserved", "noOverflow", "noTransport",
        "undoRedoSameIds", "childlessImmediate", "transitionDuplicatesBlocked",
        "restoredBaselineExact", "restoredChildrenRendered",
    ),
    prepared_requests=2,
)
STAGES = ("confirm", "pending", "uncertain", "restored")


def validate_receipt(receipt, fixture, captures):
    validate_native_receipt(SPEC, fixture, receipt)
    if receipt.get("deletionStateRequests") != 2:
        raise RuntimeError(f"{fixture}-delete: incorrect deletion-state request count: {receipt}")
    expected = [captures / f"{fixture}-delete-{stage}.png" for stage in STAGES]
    if receipt.get("captures") != [str(path) for path in expected] or not all(path.is_file() for path in expected):
        raise RuntimeError(f"{fixture}-delete: missing native deletion stage captures")


def run_fixture(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    capture_name = f"{fixture}-delete"
    evidence = {"fixture": fixture, "check": "delete", "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    for stage in STAGES:
        (captures / f"{capture_name}-{stage}.png").unlink(missing_ok=True)
    try:
        with NativeFixture(fixture, captures / f"{capture_name}.log", binary, {
            SPEC.flag: "1",
            "SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR": str(captures),
        }) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + 90
                last_focus = 0.0
                while True:
                    native.require_running()
                    receipts = native.receipts(SPEC.prefix)
                    if receipts:
                        for receipt in receipts:
                            evidence["native"] = receipt
                            validate_receipt(receipt, fixture, captures)
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{capture_name}: native assertions timed out")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                screenshot = captures / f"{capture_name}.png"
                native.capture(screenshot)
                evidence.update(ok=True, screenshot=str(screenshot))
            except BaseException:
                try:
                    screenshot = captures / f"{capture_name}-failure.png"
                    native.capture(screenshot)
                    evidence["failure_screenshot"] = str(screenshot)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
    except BaseException as error:
        evidence.update(ok=False, error=str(error))
        raise
    finally:
        path = captures / f"{capture_name}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append", choices=("slack-list", "slack-list-narrow"),
                        help="run only this fixture (repeatable); default runs both")
    args = parser.parse_args()
    for fixture in args.fixture or ("slack-list", "slack-list-narrow"):
        run_fixture(fixture, args.binary, args.captures.resolve())


if __name__ == "__main__":
    main()
