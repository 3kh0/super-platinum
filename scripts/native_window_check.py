"""Isolated runner for existing channel/thread window receipts; no app changes."""
import argparse
import json
import os
from pathlib import Path
import platform
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


def arguments(description, fixtures):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("fixtures", nargs="*", choices=fixtures)
    parser.add_argument("--fixture", action="append", choices=fixtures)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--label", default=os.environ.get("SUPER_PLATINUM_CAPTURE_LABEL", ""))
    args = parser.parse_args()
    if args.label and not args.label.replace("-", "").replace("_", "").isalnum():
        parser.error("Capture label must be alphanumeric with optional dashes/underscores")
    args.fixtures = args.fixture or args.fixtures or fixtures
    return args


def run(fixture, args, flag, prefix, validate):
    captures = args.captures.resolve()
    captures.mkdir(parents=True, exist_ok=True)
    name = fixture + (f"-{args.label}" if args.label else "")
    log = captures / f"{name}.log"
    evidence = {"fixture": fixture, "platform": sys.platform, "platformRelease": platform.release(),
                "binary": str(args.binary.resolve()), "ok": False, "log": str(log),
                "input": "synthetic DOM fixture events", "isolatedEndpoint": True}
    try:
        with NativeFixture(fixture, log, args.binary, {flag: "1"}) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + 60
                last_focus = 0
                while True:
                    native.require_running()
                    # Some preflight failures intentionally have no JSON body.
                    output = log.read_text(encoding="utf-8", errors="replace")
                    if f"{prefix} failed" in output:
                        failures = []
                        for line in output.splitlines():
                            marker = f"{prefix} failed: "
                            if marker in line:
                                payload = line.split(marker, 1)[1]
                                try:
                                    failures.append(json.loads(payload))
                                except json.JSONDecodeError:
                                    failures.append({"error": payload})
                        evidence["nativeFailures"] = failures
                        raise RuntimeError(f"{fixture}: native check failed; see {log}")
                    receipts = native.receipts(f"{prefix} passed: ")
                    if receipts:
                        if len(receipts) != 1:
                            raise RuntimeError(f"{fixture}: expected one terminal native receipt")
                        evidence["native"] = receipts[0]
                        validate(fixture, receipts[0])
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{fixture}: native check timed out; see {log}")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                time.sleep(.2)
                snapshot = native.control("state")
                state_path = captures / f"{name}-state.json"
                state_path.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
                screenshot = captures / f"{name}.png"
                native.capture(screenshot)
                evidence.update(ok=True, state=str(state_path), screenshot=str(screenshot))
            except BaseException:
                for kind in ("state", "screenshot"):
                    try:
                        if kind == "state":
                            snapshot = native.control("state")
                            (captures / f"{name}-failure-state.json").write_text(
                                json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
                        else:
                            path = captures / f"{name}-failure.png"
                            native.capture(path)
                            evidence["failureScreenshot"] = str(path)
                    except (OSError, RuntimeError, ValueError):
                        pass
                raise
        evidence["ownedProcessExited"] = native.process.poll() is not None
    except BaseException as error:
        evidence.update(ok=False, error=str(error))
        raise
    finally:
        path = captures / f"{name}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)
