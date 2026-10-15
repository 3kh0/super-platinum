#!/usr/bin/env python3
"""Controlled two-process Canvas recovery; disconnected synthetic ACKs, real SQLite."""
import argparse
from contextlib import closing
import hashlib
import json
import math
from pathlib import Path
import sqlite3
import tempfile
import time
import uuid

from canvas_native import CanvasCheck, audited_fixture, capture_stage, closed_receipts, validate
from native_fixture import DEFAULT_BINARY, NativeFixture
from native_check import expected_viewport

ACCOUNT = "00000000-0000-4000-8000-000000009998"
FIXTURES = ("canvas-structure", "canvas-structure-narrow")
STAGES = {"seed": ("initial", "reserved", "accepted-locked", "retired", "reserved-before-exit"),
          "restart": ("restored-locked",),
          "review": ("review-unchecked", "review-controls-unchecked", "review-confirmation", "review-removed", "review-refreshed"),
          "reviewed": ("review-decision-retained",)}
COUNTS = {"seed": {"dispatches": 2, "reservations": 3, "retirements": 2},
          "restart": {"dispatches": 0, "reservations": 0, "retirements": 0},
          "review": {"dispatches": 0, "reservations": 0, "retirements": 0},
          "reviewed": {"dispatches": 0, "reservations": 0, "retirements": 0}}
FLAGS = {"seed": ("persistedBeforeDispatch", "sealedReadback", "retiredBeforeUnlock", "originalRestored", "pendingNotDispatched"),
         "restart": ("inertLock", "noReplay", "refreshPreserved", "reopenPreserved"),
         "review": ("manualAcknowledgement", "cancelSafe", "archiveBeforeUnlock", "refreshRequired", "canonicalUnchanged", "outcomeUnknown", "noReplay"),
         "reviewed": ("archiveRetained", "noActiveLock", "canonicalUnchanged", "noReplay")}
STAGE_COUNTS = {"initial": (0, 0, 0), "reserved": (0, 1, 0), "accepted-locked": (1, 1, 0),
                "retired": (1, 1, 1), "reserved-before-exit": (2, 3, 2), "restored-locked": (0, 0, 0),
                **{name: (0, 0, 0) for mode in ("review", "reviewed") for name in STAGES[mode]}}
COLUMNS = ("workspace", "user", "file", "operation", "body")


def rows(database):
    with closing(sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True)) as db:
        if db.execute("PRAGMA user_version").fetchone() != (2,):
            raise RuntimeError("Canvas recovery schema changed")
        if db.execute("SELECT singleton,account FROM identity").fetchall() != [(1, ACCOUNT)]:
            raise RuntimeError("Canvas recovery account changed")
        result = [dict(zip(COLUMNS, row)) for row in db.execute(
            "SELECT workspace,user,file,operation,body FROM receipts ORDER BY workspace,user,file")]
    for row in result:
        if (row["workspace"], row["user"], row["file"]) != ("T1", "U1", "Ffixture"):
            raise RuntimeError("Unexpected Canvas owner")
        if str(uuid.UUID(row["operation"])) != row["operation"]:
            raise RuntimeError("Invalid operation identity")
        body = json.loads(row["body"])
        if set(body) != {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}:
            raise RuntimeError("Unexpected checkpoint fields")
        if (body["version"] != 1 or body["sequence"] != 1
                or body["scope"] != {"workspace": "T1", "user": "U1", "file": "Ffixture"}
                or len(body["sections"]) != 3 or not body["request"]
                or len(body["session"]) != 12 or any(c not in "0123456789abcdef" for c in body["session"])):
            raise RuntimeError("Invalid durable checkpoint")
        if not any(bytes([0xc8, 0x3e, 0x81, 0x00]) in bytes(raw) for raw in body["sections"]):
            raise RuntimeError("Unknown canonical field bytes were lost")
    return result


def validate_stage(database, name, original=None):
    if name in (*STAGES["review"], *STAGES["reviewed"]):
        return validate_local_review(database, name, original)
    saved = rows(database)
    count = 0 if name in ("initial", "retired") else 1
    if len(saved) != count:
        raise RuntimeError(f"{name}: expected {count} durable rows")
    if saved:
        body = json.loads(saved[0]["body"])
        expected = "Retain after restart" if name in ("reserved-before-exit", "restored-locked") else "Durable paragraph"
        if body["draft"] != expected or set(body["action"]) != {"Insert"}:
            raise RuntimeError("Exact retained intent changed")
    if original is not None and saved != original:
        raise RuntimeError("Restart changed exact SQLite row/body bytes")
    return saved


def validate_local_review(database, name, original):
    active = rows(database)
    with closing(sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True)) as db:
        archive = [dict(zip((*COLUMNS, "observed", "reviewed_at", "decision"), row)) for row in db.execute(
            "SELECT workspace,user,file,operation,body,observed,reviewed_at,decision FROM local_reviews")]
    if name in ("review-unchecked", "review-controls-unchecked", "review-confirmation"):
        if active != original or archive:
            raise RuntimeError("Review/cancel changed the exact retained record")
        return active
    if active or original is None or len(original) != 1 or len(archive) != 1:
        raise RuntimeError("Local unlock did not archive one exact active row")
    row = archive[0]
    if {key: row[key] for key in COLUMNS} != original[0] or row["decision"] != "local_unlock_outcome_unknown":
        raise RuntimeError("Local review rewrote the retained body or claimed success")
    if type(row["reviewed_at"]) is not int or row["reviewed_at"] <= 0:
        raise RuntimeError("Missing local decision timestamp")
    observed = json.loads(row["observed"])
    if (set(observed) != {"thread_sequence", "document_sequence", "sections"}
            or any(type(observed[k]) is not int or observed[k] != 1000 for k in ("thread_sequence", "document_sequence"))
            or json.dumps(observed["sections"]) != json.dumps(json.loads(original[0]["body"])["sections"])):
        raise RuntimeError("Local review changed the fixture canonical inventory")
    return archive


def validate_counts(stage):
    expected = STAGE_COUNTS.get(stage.get("stage"))
    keys = ("dispatches", "reservations", "retirements")
    if expected is None or any(type(stage.get(k)) is not int for k in keys) or tuple(stage.get(k) for k in keys) != expected:
        raise RuntimeError("Recovery stage transition counts changed")


def validate_review_geometry(stage, fixture):
    if stage.get("stage") not in ("review-controls-unchecked", "review-confirmation"):
        return
    proof = stage.get("reviewGeometry")
    expected = stage["stage"] == "review-confirmation"
    if not isinstance(proof, dict) or proof.get("ok") is not True:
        raise RuntimeError("Missing visible review controls proof")
    for key, value in {"checked": expected, "submitDisabled": not expected,
                       "cancelDisabled": False, "checkboxDisabled": False}.items():
        if proof.get(key) is not value:
            raise RuntimeError("Review control state changed")
    viewport = proof.get("viewport")
    if not isinstance(viewport, list) or len(viewport) != 2 or any(type(v) not in (int, float) or not math.isfinite(v) or v <= 0 for v in viewport):
        raise RuntimeError("Invalid review viewport")
    if viewport != expected_viewport(fixture):
        raise RuntimeError("Review geometry viewport changed")
    for name in ("checkbox", "label", "cancel", "remove"):
        item = proof.get(name)
        if not isinstance(item, dict) or item.get("visible") is not True or item.get("hit") is not True or item.get("hitTested") is not (name != "label"):
            raise RuntimeError("Review control is obscured or not hit tested")
        rect, clip, point = (item.get(k) for k in ("rect", "clip", "point"))
        for values, length in ((rect, 4), (clip, 4), (point, 2)):
            if not isinstance(values, list) or len(values) != length or any(type(v) not in (int, float) or not math.isfinite(v) for v in values):
                raise RuntimeError("Malformed review geometry")
        if not (0 <= clip[0] <= rect[0] < rect[2] <= clip[2] <= viewport[0]
                and 0 <= clip[1] <= rect[1] < rect[3] <= clip[3] <= viewport[1]
                and point == [(rect[0]+rect[2])/2, (rect[1]+rect[3])/2]):
            raise RuntimeError("Review control outside clipping bounds")


def validate_final(spec, fixture, value):
    if any(type(value.get(k)) is not int for k in ("dispatches", "reservations", "retirements")):
        raise RuntimeError("Recovery final transition counts must be integers")
    validate(spec, fixture, value)


def validate_closed(native, stages, final):
    if closed_receipts(native.log, "canvas-structure-recovery-stage: ") != stages or closed_receipts(native.log, "canvas-structure-recovery-check: ") != [final]:
        raise RuntimeError("Recovery emitted late or changed receipts")


def record_terminal(evidence, receipts, mode, seen):
    evidence["nativeReceipts"] = receipts
    if len(receipts) != 1:
        raise RuntimeError("Expected one recovery terminal receipt")
    evidence["native"] = receipts[0]
    if receipts[0].get("ok") is not True:
        raise RuntimeError("Native recovery check failed: " + str(receipts[0].get("error", "missing error")))
    if receipts[0].get("mode") != mode or seen != len(STAGES[mode]):
        raise RuntimeError("Unexpected recovery completion")


def finalize_phase(native, evidence, path, stage_packets):
    was_success = evidence.get("ok") is True
    final_error = None
    try:
        evidence["closedStages"] = closed_receipts(native.log, "canvas-structure-recovery-stage: ")
        evidence["closedFinals"] = closed_receipts(native.log, "canvas-structure-recovery-check: ")
        if was_success:
            if evidence["closedStages"] != stage_packets or evidence["closedFinals"] != [evidence["native"]]:
                raise RuntimeError("Recovery emitted late or changed receipts")
            evidence["closedLogRevalidated"] = True
    except (OSError, RuntimeError, ValueError) as error:
        final_error = error
        evidence["closedLogError"] = str(error)
        evidence["ok"] = False
        if was_success:
            evidence["error"] = "Recovery closed-log validation failed: " + str(error)
    path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    if was_success and final_error is not None:
        raise RuntimeError(evidence["error"]) from final_error


def phase(fixture, mode, database, binary, captures, original=None):
    label = f"{fixture}-recovery-{mode}"
    evidence = {"mode": mode, "ok": False, "stages": []}
    spec = CanvasCheck("canvas-structure-recovery", "SUPER_PLATINUM_CANVAS_STRUCTURE_RECOVERY_CHECK",
                       FLAGS[mode], COUNTS[mode], STAGES[mode])
    with tempfile.TemporaryDirectory(prefix="canvas-recovery-ack-") as temp:
        acknowledgments = Path(temp).resolve()
        native = NativeFixture(fixture, captures / f"{label}.log", binary, {
            spec.flag: mode, "SUPER_PLATINUM_CANVAS_RECOVERY_DB": str(database),
            "SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR": str(acknowledgments)})
        stage_packets = []
        try:
            with audited_fixture(native, evidence) as audit:
                evidence["pid"] = native.process.pid
                deadline = time.monotonic() + 180
                seen = 0
                while True:
                    native.require_running()
                    for stage in native.receipts("canvas-structure-recovery-stage: ")[seen:]:
                        if seen >= len(STAGES[mode]) or stage.get("stage") != STAGES[mode][seen]:
                            raise RuntimeError("Unexpected recovery stage")
                        validate_counts(stage)
                        validate_review_geometry(stage, fixture)
                        stage_packets.append(stage)
                        saved = validate_stage(database, stage["stage"], original)
                        captured = capture_stage(native, spec, label, stage, seen, captures, acknowledgments, audit=audit)
                        evidence["stages"].append({**captured, "database": saved})
                        seen += 1
                    receipts = native.receipts("canvas-structure-recovery-check: ")
                    if receipts:
                        record_terminal(evidence, receipts, mode, seen)
                        validate_final(spec, fixture, receipts[0])
                        evidence["database"] = validate_stage(database, STAGES[mode][-1], original)
                        screenshot = captures / f"{label}.png"
                        audit.capture(screenshot, "final")
                        evidence["screenshot"] = str(screenshot)
                        break
                    if time.monotonic() > deadline:
                        raise TimeoutError("Canvas recovery fixture timed out")
                    time.sleep(.05)
            if native.process.poll() is None:
                raise RuntimeError("Owned process did not exit")
            validate_closed(native, stage_packets, evidence["native"])
            evidence["processExited"] = True
            evidence["exitCode"] = native.process.poll()
            evidence["ok"] = True
            return evidence
        except BaseException as error:
            evidence["error"] = str(error)
            raise
        finally:
            # Preserve terminal protocol evidence after the owned process closes,
            # including failures before mode/final-count fields were available.
            finalize_phase(native, evidence, captures / f"{label}.json", stage_packets)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--local-review", action="store_true", help="Add explicit local-only review UI verification in a third and fourth isolated process")
    args = parser.parse_args()
    captures = args.captures.resolve()
    captures.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    result = {"ok": False, "binary": str(binary), "sha256": digest, "controlledRestart": True, "crashProof": False, "fixtures": []}
    try:
        for fixture in args.fixture or FIXTURES:
            database = captures / f"{fixture}-recovery.sqlite"
            if database.exists():
                raise RuntimeError("Refusing existing recovery database")
            item = {"fixture": fixture,
                    "seedReceipt": str(captures / f"{fixture}-recovery-seed.json"),
                    "restartReceipt": str(captures / f"{fixture}-recovery-restart.json")}
            result["fixtures"].append(item)
            seed = phase(fixture, "seed", database, binary, captures)
            item["seed"] = seed
            restart = phase(fixture, "restart", database, binary, captures, seed["database"])
            item["restart"] = restart
            if seed["pid"] == restart["pid"]:
                raise RuntimeError("Restart reused the first process")
            if rows(database) != seed["database"]:
                raise RuntimeError("Final durable receipt changed")
            if args.local_review:
                review_fixture = "canvas-local-review" + ("-narrow" if fixture.endswith("-narrow") else "")
                item["reviewReceipt"] = str(captures / f"{review_fixture}-recovery-review.json")
                review = phase(review_fixture, "review", database, binary, captures, seed["database"])
                if review["pid"] in (seed["pid"], restart["pid"]):
                    raise RuntimeError("Local review reused a previous process")
                item["review"] = review
                item["reviewedReceipt"] = str(captures / f"{review_fixture}-recovery-reviewed.json")
                reviewed = phase(review_fixture, "reviewed", database, binary, captures, seed["database"])
                if reviewed["pid"] in (seed["pid"], restart["pid"], review["pid"]):
                    raise RuntimeError("Archived decision check reused a previous process")
                if reviewed["database"] != review["database"]:
                    raise RuntimeError("Process reopen changed the exact archived decision")
                item["reviewed"] = reviewed
        if hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
            raise RuntimeError("Native binary changed during verification")
        result["ok"] = True
    except BaseException as error:
        result["error"] = str(error)
        raise
    finally:
        (captures / "canvas-structure-recovery.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
