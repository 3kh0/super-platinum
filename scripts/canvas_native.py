"""Portable canvas fixture runner; intermediate states wait for captured pixels."""
import argparse
from contextlib import nullcontext
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
    synthetic_context: bool = False
    prefix: str | None = None


SYNTHETIC_FIXTURES = {
    "canvas-root-attribution": "SUPER_PLATINUM_CANVAS_ROOT_ATTRIBUTION_CHECK",
    "canvas-editor-recovery": "SUPER_PLATINUM_CANVAS_EDITOR_RECOVERY_CHECK",
}


def context_valid(spec, fixture, receipt):
    if not spec.synthetic_context:
        return receipt.get("noSession") is True and receipt.get("noTransport") is True
    if (spec.fixture not in SYNTHETIC_FIXTURES
            or SYNTHETIC_FIXTURES[spec.fixture] != spec.flag
            or fixture not in (spec.fixture, spec.fixture + "-narrow")):
        return False
    # The native fixture proves exact synthetic credentials and a disconnected
    # transport with no HTTP client. Never label this mode session/transport-free.
    return (receipt.get("noSession") is False and receipt.get("noTransport") is False
            and receipt.get("isolation") == {
                "syntheticSession": True, "noHttp": True,
                "fixture": spec.fixture,
                "account": "147b5480-9f1a-4443-89da-836472cbb209",
                "team": "T1", "self": "UATTRSELF",
            }
            and receipt["isolation"]["syntheticSession"] is True
            and receipt["isolation"]["noHttp"] is True)


def private_checks(spec, directory):
    if spec.synthetic_context and (spec.fixture not in SYNTHETIC_FIXTURES
            or SYNTHETIC_FIXTURES[spec.fixture] != spec.flag):
        raise RuntimeError("Synthetic Canvas context requires an exact owned fixture and flag")
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
    if not context_valid(spec, fixture, receipt):
        raise RuntimeError(f"{fixture}: invalid Canvas context isolation")
    required = (*spec.required, "layout.ok")
    missing = [key for key in required if field(receipt, key) is not True]
    if missing or any(type(receipt.get(key)) is not int or receipt.get(key) != count
                      for key, count in spec.counts.items()):
        raise RuntimeError(f"{fixture}: incomplete native receipt ({missing}): {receipt}")


def capture_stage(native, spec, fixture, stage, ordinal, captures, acknowledgments, audit=None):
    if (stage.get("ok") is not True or type(stage.get("ordinal")) is not int
            or stage.get("ordinal") != ordinal
            or ordinal >= len(spec.stages) or stage.get("stage") != spec.stages[ordinal]
            or not context_valid(spec, fixture, stage)):
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
    if (closed_receipts(log, (spec.prefix or spec.fixture) + "-stage: ") != expected
            or closed_receipts(log, (spec.prefix or spec.fixture) + "-check: ") != [terminal]):
        raise RuntimeError("Closed Canvas log changed, duplicated, or omitted receipts")
    if len(expected) != len(spec.stages):
        raise RuntimeError("Closed Canvas stage count differs")
    for ordinal, stage in enumerate(expected):
        if (stage.get("ordinal") != ordinal or type(stage.get("ordinal")) is not int
                or stage.get("stage") != spec.stages[ordinal]
                or stage.get("ok") is not True or not context_valid(spec, fixture, stage)):
            raise RuntimeError("Closed Canvas stage order/isolation differs")
    validate(spec, fixture, terminal)


def run(spec, fixture, binary, captures, storage_factory=None, input_factory=None):
    captures.mkdir(parents=True, exist_ok=True)
    # Preserve older evidence; a new run needs new names/directory.
    names = [f"{fixture}.log", f"{fixture}.json", f"{fixture}.png",
             f"{fixture}-failure.png", *[f"{fixture}-{s}.png" for s in spec.stages]]
    if any((captures / name).exists() for name in names):
        raise RuntimeError("Refusing to overwrite existing Canvas evidence")
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "ok": False, "stages": []}
    native = None
    input_control = None
    try:
        with tempfile.TemporaryDirectory(prefix="sp-canvas-ack-") as directory:
            acknowledgments = Path(directory)
            checks = private_checks(spec, acknowledgments)
            readiness = canvas_readiness.prepare(spec, fixture, acknowledgments, checks)
            storage = (storage_factory.prepare(acknowledgments, fixture, checks)
                       if storage_factory is not None else None)
            input_control = (input_factory.prepare(acknowledgments, fixture, checks)
                             if input_factory is not None else None)
            if input_control is not None:
                evidence["nativeInput"] = input_control.evidence
            if storage is not None:
                evidence["storageControl"] = storage.evidence
            native = NativeFixture(fixture, captures / f"{fixture}.log", binary, checks)
            with audited_fixture(native, evidence) as audit, storage if storage is not None else nullcontext():
                if storage is not None:
                    storage.bind(native)
                if input_control is not None:
                    input_control.bind(native)
                if readiness is not None:
                    evidence["readiness"] = canvas_readiness.release(native, spec, acknowledgments, readiness)
                deadline = time.monotonic() + 180
                seen = 0
                sampled = 0.
                while True:
                    native.require_running()
                    if storage is not None:
                        storage.service(native)
                    if input_control is not None:
                        input_control.service(native)
                    if time.monotonic() - sampled >= .2:
                        audit.checkpoint("running")
                        sampled = time.monotonic()
                    stages = native.receipts(f"{spec.prefix or spec.fixture}-stage: ")
                    for stage in stages[seen:]:
                        if storage is not None and hasattr(storage, "before_capture"):
                            storage.before_capture(native, stage)
                        evidence["stages"].append(capture_stage(
                            native, spec, fixture, stage, seen, captures, acknowledgments, audit=audit))
                        seen += 1
                    receipts = native.receipts(f"{spec.prefix or spec.fixture}-check: ")
                    if receipts:
                        evidence["terminalReceipts"] = receipts
                        if len(receipts) != 1:
                            raise RuntimeError("Expected exactly one final canvas receipt")
                        evidence["native"] = receipts[0]
                        validate(spec, fixture, receipts[0])
                        if seen != len(spec.stages):
                            raise RuntimeError("Canvas check ended before all intermediate captures")
                        if storage is not None:
                            storage.finish(native)
                        if input_control is not None:
                            input_control.finish(native)
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{fixture}: no final canvas receipt; see {native.log}")
                    time.sleep(.05)
                screenshot = captures / f"{fixture}.png"
                audit.capture(screenshot, "final")
                evidence["screenshot"] = str(screenshot)
            validate_closed(spec, fixture, native.log, evidence["stages"], evidence["native"])
            if storage is not None:
                storage.validate_closed(native)
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
        if input_control is not None:
            try:
                input_control.closed(native)
            except Exception as error:
                evidence.update(ok=False, inputCleanupError=f"{type(error).__name__}: {error}")
        # Retain the actual failed terminal/error and late records on failure too.
        if native is not None and native.log.exists():
            try:
                evidence["closedStageReceipts"] = closed_receipts(native.log, (spec.prefix or spec.fixture) + "-stage: ")
                evidence["closedTerminalReceipts"] = closed_receipts(native.log, (spec.prefix or spec.fixture) + "-check: ")
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
