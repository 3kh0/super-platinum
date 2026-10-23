#!/usr/bin/env python3
"""Offline split-pair review across owned native processes and one exact journal."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import time

from canvas_native import CanvasCheck, audited_fixture, capture_stage, closed_receipts, validate
from canvas_split_recovery_oracle import STAGES, validate as validate_pair
from canvas_split_recovery_storage import SplitReviewStorage, require
from native_fixture import DEFAULT_BINARY, NativeFixture

PREFIX = "canvas-split-recovery"
FLAG = "SUPER_PLATINUM_CANVAS_SPLIT_RECOVERY_CHECK"
FIXTURES = ("canvas-local-review", "canvas-local-review-narrow")


def phase(fixture, mode, binary, captures, storage, corpus="ascii", split_mode="middle", typing="none", reuse_base="positive", reuse_content="empty", parent_mode="bootstrap", empty_history="bootstrap", empty_layout="no-following", repeat_history="bootstrap", repeat_layout="no-following", sequence_count=2, sequence_history="bootstrap", sequence_layout="no-following", sequence_cadence="straight"):
    if split_mode not in ("sequence", "repeat", "empty", "start", "middle", "end", "reuse"):
        raise RuntimeError("Unknown Canvas recovery split mode")
    if split_mode == "sequence":
        from canvas_sequence_recovery_oracle import validate as validate_sequence
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_sequence(
            phase, stages, terminal, checkpoint, append_count=sequence_count, history=sequence_history,
            layout=sequence_layout, cadence=sequence_cadence, corpus=corpus)
    elif split_mode == "repeat" and typing != "none":
        from canvas_repeat_typing_recovery_oracle import validate as validate_repeat_typing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_repeat_typing(
            phase, stages, terminal, checkpoint, typing=typing, repeat_history=repeat_history,
            repeat_layout=repeat_layout, corpus=corpus)
    elif split_mode == "repeat":
        from canvas_repeat_enter_recovery_oracle import validate as validate_repeat
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_repeat(
            phase, stages, terminal, checkpoint, repeat_history=repeat_history, repeat_layout=repeat_layout, corpus=corpus)
    elif split_mode == "empty" and typing != "none":
        from canvas_empty_typing_recovery_oracle import validate as validate_empty_typing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_empty_typing(
            phase, stages, terminal, checkpoint, typing=typing, empty_history=empty_history,
            empty_layout=empty_layout, corpus=corpus)
    elif split_mode == "empty":
        from canvas_empty_enter_recovery_oracle import validate as validate_empty
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_empty(
            phase, stages, terminal, checkpoint, empty_history=empty_history, empty_layout=empty_layout)
    elif split_mode == "start" and typing != "none":
        from canvas_start_typing_recovery_oracle import validate as validate_start_typing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_start_typing(
            phase, stages, terminal, checkpoint, corpus=corpus, typing=typing, parent_mode=parent_mode)
    elif split_mode == "start":
        from canvas_start_recovery_oracle import validate as validate_start
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_start(
            phase, stages, terminal, checkpoint, corpus=corpus, parent_mode=parent_mode)
    elif reuse_content != "empty":
        from canvas_end_clearing_recovery_oracle import validate as validate_clearing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_clearing(
            phase, stages, terminal, checkpoint, corpus=corpus, typing=typing,
            reuse_base=reuse_base, reuse_content=reuse_content)
    elif typing != "none" and split_mode == "middle":
        from canvas_middle_typing_recovery_oracle import validate as validate_middle_typing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_middle_typing(
            phase, stages, terminal, checkpoint, corpus=corpus, typing=typing, parent_mode=parent_mode)
    elif typing != "none":
        from canvas_end_typing_recovery_oracle import validate as validate_typing
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_typing(
            phase, stages, terminal, checkpoint, corpus=corpus, mode=split_mode, typing=typing, reuse_base=reuse_base)
    elif split_mode == "reuse":
        from canvas_end_reuse_recovery_oracle import validate as validate_reuse
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_reuse(
            phase, stages, terminal, checkpoint, corpus=corpus, reuse_base=reuse_base)
    elif split_mode == "end":
        from canvas_end_recovery_oracle import validate as validate_operation
    else:
        validate_operation = lambda phase, stages, terminal, checkpoint, corpus: validate_pair(
            phase, stages, terminal, checkpoint, corpus=corpus, parent_mode=parent_mode)
    label = f"{fixture}-{mode}"
    path = captures / f"{label}.json"
    evidence = {"fixture": fixture, "mode": mode, "splitMode": split_mode, "corpus": corpus,
                "typing": typing, "reuseBase": reuse_base, "reuseContent": reuse_content,
                "parentMode": parent_mode, "emptyHistory": empty_history,
                "emptyLayout": empty_layout, "repeatHistory": repeat_history,
                "repeatLayout": repeat_layout, "sequenceCount": sequence_count, "sequenceHistory": sequence_history,
                "sequenceLayout": sequence_layout, "sequenceCadence": sequence_cadence, "ok": False, "stages": []}
    spec = CanvasCheck(PREFIX, FLAG, ("noReplay", "outcomeUnknown", "canonicalUnchanged"),
                       dict(dispatches=0, reservations=int(mode == "seed"), retirements=0), STAGES[mode])
    native = None
    packets = []
    try:
        with tempfile.TemporaryDirectory(prefix="sp-split-review-ack-") as directory:
            acknowledgments = Path(directory).resolve()
            native = NativeFixture(fixture, captures / f"{label}.log", binary, {
                FLAG: mode, "SUPER_PLATINUM_CANVAS_RECOVERY_DB": str(storage.path),
                "SUPER_PLATINUM_CANVAS_SPLIT_CORPUS": corpus,
                "SUPER_PLATINUM_CANVAS_SPLIT_MODE": split_mode,
                "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": typing,
                "SUPER_PLATINUM_CANVAS_SPLIT_PARENT": parent_mode,
                **({"SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT": str(sequence_count),
                    "SUPER_PLATINUM_CANVAS_SEQUENCE_HISTORY": sequence_history,
                    "SUPER_PLATINUM_CANVAS_SEQUENCE_LAYOUT": sequence_layout,
                    "SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE": sequence_cadence} if split_mode == "sequence" else {}),
                **({"SUPER_PLATINUM_CANVAS_REPEAT_HISTORY": repeat_history,
                    "SUPER_PLATINUM_CANVAS_REPEAT_LAYOUT": repeat_layout} if split_mode == "repeat" else {}),
                **({"SUPER_PLATINUM_CANVAS_EMPTY_HISTORY": empty_history,
                    "SUPER_PLATINUM_CANVAS_EMPTY_LAYOUT": empty_layout} if split_mode == "empty" else {}),
                "SUPER_PLATINUM_CANVAS_REUSE_BASE": reuse_base,
                "SUPER_PLATINUM_CANVAS_REUSE_CONTENT": reuse_content,
                "SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR": str(acknowledgments)})
            with audited_fixture(native, evidence) as audit:
                deadline = time.monotonic() + 180
                sampled = 0.
                while True:
                    native.require_running()
                    if time.monotonic() - sampled >= .2:
                        audit.checkpoint("running")
                        sampled = time.monotonic()
                    for stage in native.receipts(PREFIX + "-stage: ")[len(packets):]:
                        ordinal = len(packets)
                        require(ordinal < len(STAGES[mode]) and stage.get("stage") == STAGES[mode][ordinal]
                                and stage.get("mode") == mode, "Unexpected split recovery stage")
                        packets.append(stage)
                        database = storage.observe(stage)
                        mutation = None
                        if mode == "storage-failure" and stage["stage"] in ("review-confirmation", "storage-failed"):
                            mutation = storage.inject_failure(stage["stage"] == "review-confirmation")
                        captured = capture_stage(native, spec, label, stage, ordinal, captures, acknowledgments, audit=audit)
                        evidence["stages"].append(dict(native=captured, database=database, failureControl=mutation))
                    receipts = native.receipts(PREFIX + "-check: ")
                    if receipts:
                        evidence["terminalReceipts"] = receipts
                        require(len(receipts) == 1, "Expected exactly one split recovery terminal")
                        terminal = receipts[0]
                        evidence["native"] = terminal
                        require(terminal.get("ok") is True, "Native split recovery failed: " + str(terminal.get("error")))
                        require(len(packets) == len(STAGES[mode]) and terminal.get("mode") == mode,
                                "Split recovery ended before all captures")
                        validate(spec, fixture, terminal)
                        evidence["database"] = storage.observe(packets[-1])
                        screenshot = captures / f"{label}.png"
                        audit.capture(screenshot, "final")
                        evidence["screenshot"] = str(screenshot)
                        break
                    require(time.monotonic() < deadline, "Split recovery native check timed out")
                    time.sleep(.05)
            require(closed_receipts(native.log, PREFIX + "-stage: ") == packets
                    and closed_receipts(native.log, PREFIX + "-check: ") == [evidence["native"]],
                    "Split recovery emitted late or changed receipts")
            evidence["closedLogRevalidated"] = True
            evidence["child"] = validate_operation(mode, packets, evidence["native"], json.loads(storage.original["body"]), corpus=corpus)
            require(storage.snapshot() == evidence["database"], "Process exit changed journal/archive")
            require(not evidence["database"]["failureTrigger"], "Failure trigger remained after phase")
            evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        if native is not None and native.log.exists():
            try:
                evidence["closedStages"] = closed_receipts(native.log, PREFIX + "-stage: ")
                evidence["closedFinals"] = closed_receipts(native.log, PREFIX + "-check: ")
            except Exception as error:
                evidence.update(ok=False, closedLogError=str(error))
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    require(evidence["ok"], "Split recovery phase final audit failed")
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", choices=FIXTURES, default=FIXTURES[0])
    parser.add_argument("--scenario", choices=("review", "storage-failure"), default="review")
    parser.add_argument("--corpus", choices=("ascii", "unicode"), default="ascii")
    parser.add_argument("--mode", choices=("sequence", "repeat", "empty", "start", "middle", "end", "reuse"), default="middle")
    parser.add_argument("--repeat-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--repeat-layout", choices=("no-following", "text-following", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--empty-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--empty-layout", choices=("no-following", "text-following-ascii", "text-following-unicode", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--typing", choices=("none", "correction", "erase", "restore"), default="none")
    parser.add_argument("--parent-mode", choices=("bootstrap", "retained"), default="bootstrap")
    parser.add_argument("--reuse-base", choices=("positive", "zero"), default="positive")
    parser.add_argument("--reuse-content", choices=("empty", "spaces", "nbsp", "br", "mixed", "mixed-br", "separators", "feff-leading", "feff-interior", "feff-post-br"), default="empty")
    parser.add_argument("--sequence-count", type=int, choices=range(2, 17), default=2)
    parser.add_argument("--sequence-history", choices=("bootstrap", "retained-positive", "retained-zero"), default="bootstrap")
    parser.add_argument("--sequence-layout", choices=("no-following", "text-following", "empty-final-following", "empty-nonfinal-following"), default="no-following")
    parser.add_argument("--sequence-cadence", choices=("straight", "correction-before-enter", "erase-before-enter", "revert-before-enter", "trailing-empty"), default="straight")
    args = parser.parse_args()
    if args.mode != "sequence" and (args.sequence_count != 2 or args.sequence_history != "bootstrap"
            or args.sequence_layout != "no-following" or args.sequence_cadence != "straight"):
        parser.error("Sequence selectors require sequence mode")
    if args.mode == "sequence" and args.typing != "none":
        parser.error("Sequence input uses sequence cadence, not legacy typing selectors")
    if (args.mode == "sequence" and args.sequence_cadence not in ("straight", "trailing-empty")
            and args.sequence_count not in (2, 3, 8, 16)):
        parser.error("Sequence correction cadence requires a pinned 2, 3, 8 or 16 append cohort")
    if args.mode != "repeat" and (args.repeat_history != "bootstrap" or args.repeat_layout != "no-following"):
        parser.error("Repeat source selectors require repeat mode")
    if args.mode == "empty" and args.typing == "none" and args.corpus != "ascii":
        parser.error("Empty source uses ascii corpus; Unicode neighbor is selected by empty layout")
    if args.mode != "empty" and (args.empty_history != "bootstrap" or args.empty_layout != "no-following"):
        parser.error("Empty source selectors require empty mode")
    if args.typing == "restore" and args.mode not in ("repeat", "empty", "start", "middle"):
        parser.error("Restoring original text requires repeat, empty, start or middle mode")
    if args.parent_mode != "bootstrap" and args.mode not in ("start", "middle"):
        parser.error("Retained parent history requires start or middle mode")
    if args.reuse_base == "zero" and args.mode != "reuse":
        parser.error("Zero-base history requires reuse mode")
    if args.reuse_content != "empty" and args.mode != "reuse":
        parser.error("Clearing existing content requires reuse mode")
    binary, captures = Path(args.binary).resolve(), args.captures.resolve()
    captures.mkdir(parents=True, exist_ok=False)
    storage = SplitReviewStorage(captures, args.mode, args.typing, args.reuse_content)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    result = dict(ok=False, fixture=args.fixture, scenario=args.scenario, corpus=args.corpus,
                  splitMode=args.mode, typing=args.typing, reuseBase=args.reuse_base, parentMode=args.parent_mode,
                  reuseContent=args.reuse_content, emptyHistory=args.empty_history,
                  emptyLayout=args.empty_layout, repeatHistory=args.repeat_history,
                  repeatLayout=args.repeat_layout, sequenceCount=args.sequence_count, sequenceHistory=args.sequence_history,
                  sequenceLayout=args.sequence_layout, sequenceCadence=args.sequence_cadence, binary=str(binary),
                  binarySha256=digest, database=str(storage.path), phases=[])
    identities = set()
    try:
        for mode in ("seed", "restart", args.scenario, "reviewed"):
            value = phase(args.fixture, mode, binary, captures, storage, args.corpus, args.mode, args.typing, args.reuse_base, args.reuse_content, args.parent_mode, args.empty_history, args.empty_layout, args.repeat_history, args.repeat_layout, args.sequence_count, args.sequence_history, args.sequence_layout, args.sequence_cadence)
            identity = (value["pid"], value.get("processCreationFiletime"))
            require(identity not in identities, "Recovery did not restart the owned native process")
            identities.add(identity)
            result["phases"].append(dict(mode=mode, receipt=str(captures / f"{args.fixture}-{mode}.json"),
                                         pid=identity[0], processCreationFiletime=identity[1], ok=True))
        result["binarySha256After"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        require(result["binarySha256After"] == digest, "Binary changed during recovery acceptance")
        result["original"] = storage.original
        result["archive"] = storage.archive
        result["ok"] = True
    except BaseException as error:
        result["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        (captures / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
