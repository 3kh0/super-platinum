"""Strict normal/narrow receipt runner shared by disconnected native UI checks."""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


@dataclass(frozen=True)
class CheckSpec:
    fixture: str
    flag: str
    prefix: str
    required: tuple[str, ...]
    prepared_requests: int
    viewport_path: str = "viewport"
    timeout: int = 90


def field(receipt, path):
    value = receipt
    for part in path.split("."):
        if not isinstance(value, dict) or part not in value:
            return None
        value = value[part]
    return value


def expected_viewport(fixture, platform=sys.platform):
    width, height = (668, 400) if fixture.endswith("-narrow") else (1280, 800)
    # Verified pinned Tao/Wry Windows menu consumes 20 logical pixels of the
    # requested client area. These are actual WebView dimensions, not PNG pixels.
    return [width, height - 20 if platform == "win32" else height]


def validate_receipt(spec, fixture, receipt, platform=sys.platform):
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    viewport = field(receipt, spec.viewport_path)
    if viewport != expected_viewport(fixture, platform):
        raise RuntimeError(f"{fixture}: incorrect native viewport: {viewport}")
    missing = [key for key in spec.required if field(receipt, key) is not True]
    if missing or receipt.get("preparedRequests") != spec.prepared_requests:
        raise RuntimeError(f"{fixture}: incomplete native receipt ({missing}): {receipt}")


def run_fixture(spec, fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary,
                           {spec.flag: "1"}) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + spec.timeout
                last_focus = 0.0
                while True:
                    native.require_running()
                    receipts = native.receipts(spec.prefix)
                    if receipts:
                        # Any assertion failure invalidates the run, even if a
                        # later success line appears in the same process log.
                        for receipt in receipts:
                            evidence["native"] = receipt
                            validate_receipt(spec, fixture, receipt)
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{fixture}: native assertion receipt timed out")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                screenshot = captures / f"{fixture}.png"
                native.capture(screenshot)
                evidence.update(ok=True, screenshot=str(screenshot))
            except BaseException:
                try:
                    screenshot = captures / f"{fixture}-failure.png"
                    native.capture(screenshot)
                    evidence["failure_screenshot"] = str(screenshot)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
    except BaseException as error:
        evidence["ok"] = False
        evidence["error"] = str(error)
        raise
    finally:
        receipt_path = captures / f"{fixture}.json"
        receipt_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(receipt_path), **evidence}), flush=True)


def main(spec, description):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    fixtures = (spec.fixture, f"{spec.fixture}-narrow")
    parser.add_argument("--fixture", action="append", choices=fixtures,
                        help="run only this fixture (repeatable); default runs both")
    args = parser.parse_args()
    for fixture in args.fixture or fixtures:
        run_fixture(spec, fixture, args.binary, args.captures.resolve())
