"""Portable canvas fixture runner; intermediate states wait for captured pixels."""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import sys
import tempfile
import time

from native_check import expected_viewport, field
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


@dataclass(frozen=True)
class CanvasCheck:
    fixture: str
    flag: str
    required: tuple[str, ...]
    counts: dict[str, int]
    stages: tuple[str, ...]


def validate(spec, fixture, receipt):
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    if field(receipt, "layout.viewport") != expected_viewport(fixture):
        raise RuntimeError(f"{fixture}: incorrect native viewport")
    required = (*spec.required, "noSession", "noTransport", "layout.ok")
    missing = [key for key in required if field(receipt, key) is not True]
    if missing or any(receipt.get(key) != count for key, count in spec.counts.items()):
        raise RuntimeError(f"{fixture}: incomplete native receipt ({missing}): {receipt}")


def capture_stage(native, spec, fixture, stage, ordinal, captures, acknowledgments):
    if (stage.get("ok") is not True or type(stage.get("ordinal")) is not int
            or stage.get("ordinal") != ordinal
            or ordinal >= len(spec.stages) or stage.get("stage") != spec.stages[ordinal]
            or stage.get("noSession") is not True or stage.get("noTransport") is not True):
        raise RuntimeError(f"Unexpected canvas stage: {stage}")
    screenshot = captures / f"{fixture}-{spec.stages[ordinal]}.png"
    native.capture(screenshot)
    # The app remains in this stage until fresh PNG pixels have been saved.
    # Exclusive creation also rejects duplicated acknowledgments.
    pending = acknowledgments / f"{ordinal}.pending"
    with pending.open("x", encoding="utf-8", newline="\n") as output:
        output.write("captured\n")
    # Publish a complete file atomically and exclusively, so the native poll
    # cannot observe an empty/partial acknowledgement on Windows.
    os.link(pending, acknowledgments / f"{ordinal}.ack")
    pending.unlink()
    return {**stage, "screenshot": str(screenshot)}


def run(spec, fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "ok": False, "stages": []}
    try:
        with tempfile.TemporaryDirectory(prefix="sp-canvas-ack-") as directory:
            acknowledgments = Path(directory)
            checks = {spec.flag: "1", "SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR": directory}
            with NativeFixture(fixture, captures / f"{fixture}.log", binary, checks) as native:
                evidence["pid"] = native.process.pid
                try:
                    deadline = time.monotonic() + 180
                    last_focus = 0
                    seen = 0
                    while True:
                        native.require_running()
                        stages = native.receipts(f"{spec.fixture}-stage: ")
                        for stage in stages[seen:]:
                            evidence["stages"].append(capture_stage(
                                native, spec, fixture, stage, seen, captures, acknowledgments))
                            seen += 1
                        receipts = native.receipts(f"{spec.fixture}-check: ")
                        if receipts:
                            if len(receipts) != 1:
                                raise RuntimeError("Expected exactly one final canvas receipt")
                            evidence["native"] = receipts[0]
                            validate(spec, fixture, receipts[0])
                            if seen != len(spec.stages):
                                raise RuntimeError("Canvas check ended before all intermediate captures")
                            break
                        if time.monotonic() >= deadline:
                            raise TimeoutError(f"{fixture}: no final canvas receipt; see {native.log}")
                        if time.monotonic() - last_focus >= 1:
                            native.focus()
                            last_focus = time.monotonic()
                        time.sleep(.05)
                    screenshot = captures / f"{fixture}.png"
                    native.capture(screenshot)
                    evidence["screenshot"] = str(screenshot)
                except BaseException:
                    try:
                        native.capture(captures / f"{fixture}-failure.png")
                    except (OSError, RuntimeError, ValueError):
                        pass
                    raise
            evidence["processExited"] = native.process.poll() is not None
            if not evidence["processExited"]:
                raise RuntimeError("Owned canvas fixture did not exit")
            evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


def main(spec, description):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    fixtures = (spec.fixture, spec.fixture + "-narrow")
    parser.add_argument("--fixture", action="append", choices=fixtures)
    args = parser.parse_args()
    for fixture in args.fixture or fixtures:
        run(spec, fixture, args.binary.resolve(), args.captures.resolve())
