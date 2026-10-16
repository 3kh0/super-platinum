#!/usr/bin/env python3
"""Four-process paired Canvas workflow; synthetic input/ACKs and real SQLite, no server/crash proof."""
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

ACCOUNT = "00000000-0000-4000-8000-000000009997"
FIXTURES = ("canvas-replacement", "canvas-replacement-narrow")
STAGES = {"seed": ("initial", "confirmation", "reserved", "saving", "accepted-locked", "replaced", "edit-draft", "edited", "uncertain-before-exit"),
          "restart": ("restored-locked",),
          "review": ("review-unchecked", "review-controls-unchecked", "review-confirmation", "review-removed", "review-refreshed"),
          "reviewed": ("review-decision-retained",)}
COUNTS = {"seed": {"dispatches": 2, "reservations": 2, "retirements": 1, "textAttempts": 1},
          **{mode: {"dispatches": 0, "reservations": 0, "retirements": 0, "textAttempts": 0} for mode in ("restart", "review", "reviewed")}}
FLAGS = {"seed": ("cancelSafe", "persistedBeforeDispatch", "sealedReadback", "retiredBeforeUnlock", "replacementFocused", "subsequentEdit", "duplicateSafe", "unknownRetained", "stableActions"),
         "restart": ("inertLock", "noReplay", "refreshPreserved", "reopenPreserved"),
         "review": ("manualAcknowledgement", "cancelSafe", "archiveBeforeUnlock", "refreshRequired", "canonicalUnchanged", "outcomeUnknown", "noReplay"),
         "reviewed": ("archiveRetained", "noActiveLock", "noReplay")}
STAGE_COUNTS = dict(zip(STAGES["seed"], ((0,0,0,0),(0,0,0,0),(0,1,0,0),(1,1,0,0),(1,1,0,0),(1,1,1,0),(1,1,1,0),(1,1,1,1),(2,2,1,1))))
STAGE_COUNTS.update({name:(0,0,0,0) for mode in ("restart","review","reviewed") for name in STAGES[mode]})
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
        validate_body(json.loads(row["body"]))
    return result


def fields(data):
    if not isinstance(data, (list, bytes)) or not data or len(data)>8*1024*1024 or any(type(b) is not int or not 0<=b<=255 for b in data):
        raise RuntimeError("Invalid fixture protobuf bytes")
    data=bytes(data); at=0; result=[]
    def varint():
        nonlocal at
        value=0
        for shift in range(0,70,7):
            if at>=len(data): raise RuntimeError("Truncated varint")
            b=data[at]; at+=1; value|=(b&127)<<shift
            if not b&128:
                if value>2**64-1: raise RuntimeError("Varint overflow")
                return value
        raise RuntimeError("Varint overflow")
    while at<len(data):
        key=varint(); tag=key>>3; wire=key&7
        if not 0<tag<2**29 or len(result)>=65536: raise RuntimeError("Invalid field")
        if wire==0: value=varint()
        elif wire in (1,2,5):
            length=varint() if wire==2 else (8 if wire==1 else 4)
            if length>len(data)-at: raise RuntimeError("Truncated field")
            value=data[at:at+length]; at+=length
        else: raise RuntimeError("Unsupported fixture wire")
        result.append((tag,wire,value))
    return result


def one(values,tag,wire):
    matches=[value for t,w,value in values if t==tag and w==wire]
    if len(matches)!=1 or sum(t==tag for t,_,_ in values)!=1: raise RuntimeError("Missing/duplicate/wrong-wire field")
    return matches[0]


def validate_body(body):
    if set(body)!={"version","scope","thread","document","target","session","sequence","action","draft","sections","request"}:
        raise RuntimeError("Unexpected checkpoint fields")
    if (type(body["version"]) is not int or body["version"]!=2 or type(body["sequence"]) is not int or body["sequence"]!=1
        or body["scope"]!={"workspace":"T1","user":"U1","file":"Ffixture"}
        or body["thread"]!="thread" or body["document"]!="document"
        or not isinstance(body["session"],str) or len(body["session"])!=12 or any(c not in "0123456789abcdef" for c in body["session"])
        or not isinstance(body["sections"],list) or len(body["sections"])!=2
        or not isinstance(body["action"],dict) or set(body["action"])!={"Replace"}):
        raise RuntimeError("Invalid paired checkpoint")
    action=body["action"]["Replace"]
    if not isinstance(action,dict) or set(action)!={"replacement","position"} or not isinstance(action["replacement"],str) or not action["replacement"].startswith("temp:C:xyz") or not isinstance(action["position"],str) or not action["position"].endswith(":temp"):
        raise RuntimeError("Invalid pair action")
    target=body["target"]
    if not isinstance(target,str) or target==action["replacement"]: raise RuntimeError("Invalid distinct pair identities")
    inventory=[fields(raw) for raw in body["sections"]]
    if sorted(one(row,1,2).decode() for row in inventory)!=sorted(["title",target]): raise RuntimeError("Incomplete original inventory")
    for row in inventory:
        if one(row,6,2)!=b"thread" or one(row,7,2)!=b"document" or one(row,2,0)<=0: raise RuntimeError("Invalid original identity")
    title=next(raw for raw in body["sections"] if one(fields(raw),1,2)==b"title")
    if bytes([0xc8,0x3e,0x81,0]) not in bytes(title): raise RuntimeError("Unknown title bytes lost")
    data=fields(body["request"])
    if [tag for tag,_,_ in data]!=[1,1,14]: raise RuntimeError("Not one complete pair payload")
    updates=[fields(value) for tag,wire,value in data if tag==1 and wire==2]
    if len(updates)!=2: raise RuntimeError("Wrong pair members")
    for row,identity,deleted in zip(updates,(target,action["replacement"]),(1,0)):
        if one(row,1,2).decode()!=identity or one(row,3,0)!=deleted or one(row,19,0)!=1 or one(row,20,0)!=1 or one(row,33,2)!=(body["session"]+"-0").encode(): raise RuntimeError("Pair identity/local sequence/trace mismatch")
        history=fields(one(row,38,2))
        if not any(tag==6 for tag,_,_ in history): raise RuntimeError("Missing source history heads")
    if one(updates[1],8,2).decode()!=action["position"] or one(fields(one(updates[1],38,2)),2,0)!=0:
        raise RuntimeError("Replacement position/base changed")


def validate_stage(database,name,original=None,observed=None):
    if name not in STAGE_COUNTS: raise RuntimeError("Unknown pair stage")
    if name in (*STAGES["review"],*STAGES["reviewed"]):
        return validate_local_review(database,name,original,observed)
    saved=rows(database)
    expected_count=1 if name in ("reserved","saving","accepted-locked","uncertain-before-exit","restored-locked") else 0
    if len(saved)!=expected_count: raise RuntimeError("Wrong pair durable row count")
    if saved:
        body=json.loads(saved[0]["body"])
        expected="Replacement <b>edited</b>" if name in ("uncertain-before-exit","restored-locked") else "First <b>paragraph</b>"
        if body["draft"]!=expected: raise RuntimeError("Wrong retained pair text")
    if original is not None and saved!=original: raise RuntimeError("Restart changed exact pair body")
    return saved


def validate_local_review(database,name,original,observed):
    active=rows(database)
    with closing(sqlite3.connect(database.resolve().as_uri()+"?mode=ro",uri=True)) as db:
        archive=[dict(zip((*COLUMNS,"observed","reviewed_at","decision"),row)) for row in db.execute("SELECT workspace,user,file,operation,body,observed,reviewed_at,decision FROM local_reviews")]
    if name in ("review-unchecked","review-controls-unchecked","review-confirmation"):
        if active!=original or archive: raise RuntimeError("Review/Cancel changed exact pair")
        return active
    if active or original is None or len(original)!=1 or len(archive)!=1: raise RuntimeError("Missing exact pair archive")
    row=archive[0]
    if {k:row[k] for k in COLUMNS}!=original[0] or row["decision"]!="local_unlock_outcome_unknown" or type(row["reviewed_at"]) is not int or row["reviewed_at"]<=0:
        raise RuntimeError("Pair archive changed body or outcome")
    saved=json.loads(row["observed"])
    if set(saved)!={"thread_sequence","document_sequence","sections"} or any(type(saved[k]) is not int or saved[k]!=1000 for k in ("thread_sequence","document_sequence")) or observed is None or saved["sections"]!=observed:
        raise RuntimeError("Review did not preserve the complete fresh fixture inventory")
    return archive


def validate_counts(stage):
    expected = STAGE_COUNTS.get(stage.get("stage"))
    keys = ("dispatches", "reservations", "retirements", "textAttempts")
    if expected is None or any(type(stage.get(k)) is not int for k in keys) or tuple(stage.get(k) for k in keys) != expected:
        raise RuntimeError("Recovery stage transition counts changed")


def validate_review_geometry(stage, fixture):
    if stage.get("stage")=="confirmation":
        proof=stage.get("reviewGeometry")
        if not isinstance(proof,dict) or proof.get("ok") is not True or proof.get("viewport")!=expected_viewport(fixture):
            raise RuntimeError("Missing complete pair confirmation geometry")
        width,height=proof["viewport"]
        for key in ("hint","cancel","save"):
            item=proof.get(key)
            if not isinstance(item,dict) or item.get("ok") is not True or item.get("hit") is not True: raise RuntimeError("Pair copy/control obstructed")
            rect,clip=item.get("rect"),item.get("clip")
            if not isinstance(rect,list) or len(rect)!=4 or not isinstance(clip,dict) or set(clip)!={"left","top","right","bottom"}: raise RuntimeError("Malformed pair geometry")
            if any(type(v) not in (int,float) or not math.isfinite(v) for v in [*rect,*clip.values()]): raise RuntimeError("Nonfinite pair geometry")
            x,y,w,h=rect
            if not(w>0 and h>0 and 0<=clip["left"]<clip["right"]<=width and 0<=clip["top"]<clip["bottom"]<=height and x>=clip["left"]-1 and y>=clip["top"]-1 and x+w<=clip["right"]+1 and y+h<=clip["bottom"]+1): raise RuntimeError("Pair confirmation clipped")
        return
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
    if any(type(value.get(k)) is not int for k in ("dispatches", "reservations", "retirements", "textAttempts")):
        raise RuntimeError("Recovery final transition counts must be integers")
    validate(spec, fixture, value)
    if "stableActions" in spec.required:
        validate_action_geometry(value.get("actionGeometry"))


def validate_action_geometry(records):
    labels=["Delete paragraph","Saving\u2026","Saving\u2026","Checking\u2026","Delete paragraph","Unconfirmed"]
    if not isinstance(records,list) or len(records)!=len(labels):
        raise RuntimeError("Missing action-width state observations")
    for row,label in zip(records,labels):
        if not isinstance(row,dict) or row.get("ok") is not True or row.get("label")!=label or any(not isinstance(row.get(k),str) or not row[k].isdigit() for k in ("generation","visit")):
            raise RuntimeError("Wrong action-width state or owner")
        width=row.get("groupWidth")
        if type(width) not in (int,float) or not math.isfinite(width) or width<=0: raise RuntimeError("Invalid action group width")
        for key in ("cancel","save"):
            r=row.get(key)
            if not isinstance(r,list) or len(r)!=4 or any(type(v) not in (int,float) or not math.isfinite(v) for v in r) or r[2]<=0 or r[3]<=0 or r[0]<-.5 or r[0]+r[2]>width+.5:
                raise RuntimeError("Invalid action bounds")
    if records[0]["generation"]==records[4]["generation"]: raise RuntimeError("Distinct confirmations reused an editor")
    for index,base in ((1,0),(2,0),(3,0),(5,4)):
        row,old=records[index],records[base]
        if row["generation"]!=old["generation"] or row["visit"]!=old["visit"] or abs(row["groupWidth"]-old["groupWidth"])>.5 or any(abs(v-o)>.5 for key in ("cancel","save") for v,o in zip(row[key],old[key])):
            raise RuntimeError("Canvas status moved or resized its controls")


def validate_closed(native, stages, final):
    if closed_receipts(native.log, "canvas-replacement-stage: ") != stages or closed_receipts(native.log, "canvas-replacement-check: ") != [final]:
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
        evidence["closedStages"] = closed_receipts(native.log, "canvas-replacement-stage: ")
        evidence["closedFinals"] = closed_receipts(native.log, "canvas-replacement-check: ")
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


def phase(fixture, mode, database, binary, captures, original=None, observed=None):
    label = f"{fixture}-recovery-{mode}"
    evidence = {"mode": mode, "ok": False, "stages": []}
    spec = CanvasCheck("canvas-replacement", "SUPER_PLATINUM_CANVAS_REPLACEMENT_CHECK",
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
                    for stage in native.receipts("canvas-replacement-stage: ")[seen:]:
                        if seen >= len(STAGES[mode]) or stage.get("stage") != STAGES[mode][seen]:
                            raise RuntimeError("Unexpected recovery stage")
                        validate_counts(stage)
                        validate_review_geometry(stage, fixture)
                        stage_packets.append(stage)
                        saved = validate_stage(database, stage["stage"], original, observed)
                        if stage["stage"] == "reserved":
                            evidence["reviewInventory"] = sorted(json.loads(saved[0]["body"])["sections"], key=lambda raw: one(fields(raw),1,2))
                        captured = capture_stage(native, spec, label, stage, seen, captures, acknowledgments, audit=audit)
                        evidence["stages"].append({**captured, "database": saved})
                        seen += 1
                    receipts = native.receipts("canvas-replacement-check: ")
                    if receipts:
                        record_terminal(evidence, receipts, mode, seen)
                        validate_final(spec, fixture, receipts[0])
                        evidence["database"] = validate_stage(database, STAGES[mode][-1], original, observed)
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
            restart = phase(fixture, "restart", database, binary, captures, seed["database"], seed["reviewInventory"])
            item["restart"] = restart
            if seed["pid"] == restart["pid"]:
                raise RuntimeError("Restart reused the first process")
            if rows(database) != seed["database"]:
                raise RuntimeError("Final durable receipt changed")
            review_fixture = fixture
            item["reviewReceipt"] = str(captures / f"{review_fixture}-recovery-review.json")
            review = phase(review_fixture, "review", database, binary, captures, seed["database"], seed["reviewInventory"])
            if review["pid"] in (seed["pid"], restart["pid"]):
                raise RuntimeError("Local review reused a previous process")
            item["review"] = review
            item["reviewedReceipt"] = str(captures / f"{review_fixture}-recovery-reviewed.json")
            reviewed = phase(review_fixture, "reviewed", database, binary, captures, seed["database"], seed["reviewInventory"])
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
        (captures / "canvas-replacement.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
