"""Portable canvas fixture runner; intermediate states wait for captured pixels."""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import sys
import tempfile
import time

from canvas_evidence import audited_fixture, closed_receipts, png_signature
import canvas_readiness
from native_check import expected_viewport, field
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


@dataclass(frozen=True)
class CanvasCheck:
    fixture: str
    flag: str
    required: tuple[str, ...]
    counts: dict[str, int]
    stages: tuple[str, ...]
    private_files: tuple[tuple[str, str], ...] = ()
    readiness: bool = False


def private_checks(spec, directory):
    checks = {spec.flag: "1", "SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR": str(directory)}
    for key, name in spec.private_files:
        if (not key.startswith("SUPER_PLATINUM_") or key in checks
                or not name or name in (".", "..") or "/" in name or "\\" in name
                or ":" in name or (directory / name).exists()):
            raise RuntimeError("Invalid private fixture file")
        checks[key] = str(directory / name)
    return checks


def validate(spec, fixture, receipt):
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    if field(receipt, "layout.viewport") != expected_viewport(fixture):
        raise RuntimeError(f"{fixture}: incorrect native viewport")
    required = (*spec.required, "noSession", "noTransport", "layout.ok")
    missing = [key for key in required if field(receipt, key) is not True]
    if missing or any(type(receipt.get(key)) is not int or receipt.get(key) != count
                      for key, count in spec.counts.items()):
        raise RuntimeError(f"{fixture}: incomplete native receipt ({missing}): {receipt}")


def capture_stage(native, spec, fixture, stage, ordinal, captures, acknowledgments, audit=None):
    if (stage.get("ok") is not True or type(stage.get("ordinal")) is not int
            or stage.get("ordinal") != ordinal
            or ordinal >= len(spec.stages) or stage.get("stage") != spec.stages[ordinal]
            or stage.get("noSession") is not True or stage.get("noTransport") is not True):
        raise RuntimeError(f"Unexpected canvas stage: {stage}")
    screenshot = captures / f"{fixture}-{spec.stages[ordinal]}.png"
    if audit is None:
        native.capture(screenshot)
    else:
        audit.capture(screenshot, spec.stages[ordinal])
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


def validate_closed(spec, fixture, log, stages, terminal):
    expected = [{k: v for k, v in stage.items() if k != "screenshot"} for stage in stages]
    if (closed_receipts(log, spec.fixture + "-stage: ") != expected
            or closed_receipts(log, spec.fixture + "-check: ") != [terminal]):
        raise RuntimeError("Closed Canvas log changed, duplicated, or omitted receipts")
    if len(expected) != len(spec.stages):
        raise RuntimeError("Closed Canvas stage count differs")
    for ordinal, stage in enumerate(expected):
        if (stage.get("ordinal") != ordinal or type(stage.get("ordinal")) is not int
                or stage.get("stage") != spec.stages[ordinal]
                or any(stage.get(k) is not True for k in ("ok", "noSession", "noTransport"))):
            raise RuntimeError("Closed Canvas stage order/isolation differs")
    validate(spec, fixture, terminal)


def run(spec, fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    # Preserve older evidence; a new run needs new names/directory.
    names = [f"{fixture}.log", f"{fixture}.json", f"{fixture}.png",
             f"{fixture}-failure.png", *[f"{fixture}-{s}.png" for s in spec.stages]]
    if any((captures / name).exists() for name in names):
        raise RuntimeError("Refusing to overwrite existing Canvas evidence")
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "ok": False, "stages": []}
    native = None
    try:
        with tempfile.TemporaryDirectory(prefix="sp-canvas-ack-") as directory:
            acknowledgments = Path(directory)
            checks = private_checks(spec, acknowledgments)
            readiness = canvas_readiness.prepare(spec, fixture, acknowledgments, checks)
            native = NativeFixture(fixture, captures / f"{fixture}.log", binary, checks)
            with audited_fixture(native, evidence) as audit:
                if readiness is not None:
                    evidence["readiness"] = canvas_readiness.release(native, spec, acknowledgments, readiness)
                deadline = time.monotonic() + 180
                seen = 0
                sampled = 0.
                while True:
                    native.require_running()
                    if time.monotonic() - sampled >= .2:
                        audit.checkpoint("running")
                        sampled = time.monotonic()
                    stages = native.receipts(f"{spec.fixture}-stage: ")
                    for stage in stages[seen:]:
                        evidence["stages"].append(capture_stage(
                            native, spec, fixture, stage, seen, captures, acknowledgments, audit=audit))
                        seen += 1
                    receipts = native.receipts(f"{spec.fixture}-check: ")
                    if receipts:
                        evidence["terminalReceipts"] = receipts
                        if len(receipts) != 1:
                            raise RuntimeError("Expected exactly one final canvas receipt")
                        evidence["native"] = receipts[0]
                        validate(spec, fixture, receipts[0])
                        if seen != len(spec.stages):
                            raise RuntimeError("Canvas check ended before all intermediate captures")
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{fixture}: no final canvas receipt; see {native.log}")
                    time.sleep(.05)
                screenshot = captures / f"{fixture}.png"
                audit.capture(screenshot, "final")
                evidence["screenshot"] = str(screenshot)
            validate_closed(spec, fixture, native.log, evidence["stages"], evidence["native"])
            if readiness is not None:
                canvas_readiness.validate_closed(native.log, spec, evidence["readiness"])
            evidence["closedLogRevalidated"] = True
            for capture in evidence["captures"]:
                png_signature(Path(capture["path"]), evidence.get("expectedPngDimensions"))
            evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        # Retain the actual failed terminal/error and late records on failure too.
        if native is not None and native.log.exists():
            try:
                evidence["closedStageReceipts"] = closed_receipts(native.log, spec.fixture + "-stage: ")
                evidence["closedTerminalReceipts"] = closed_receipts(native.log, spec.fixture + "-check: ")
            except Exception as error:
                evidence.update(ok=False, closedLogError=f"{type(error).__name__}: {error}")
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    if not evidence["ok"]:
        raise RuntimeError("Final Canvas closed-log audit failed")


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
