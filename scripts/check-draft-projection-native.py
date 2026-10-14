#!/usr/bin/env python3
"""Isolated native draft projection and exact second-process durable readback; never builds."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import secrets
import runpy
import sys
import struct
import tempfile
import time
from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

SCENARIOS = ("apply", "ahead", "input-before", "input-after", "input-between", "selection", "composition-input-first", "composition-end-first", "remount", "navigation")
ALTERNATE = frozenset(("ahead", "input-before", "input-after", "input-between", "composition-input-first", "composition-end-first"))
AFTER = "Observed \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
NEWER = "Newer local \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
SPEC = CheckSpec(fixture="draft-native-projection", flag="SUPER_PLATINUM_DRAFT_PROJECTION_CHECK", prefix="draft-projection-check: ", prepared_requests=0,
    required=("noTransport", "noSession", "canonicalMessagesUnchanged", "syntheticInput", "syntheticIME", "noProductionRoute"))
FIXTURES = (SPEC.fixture, SPEC.fixture + "-narrow")


def validate(fixture, scenario, phase, nonce, pid, database, receipt):
    validate_receipt(SPEC, fixture, receipt)
    for key, expected in {"scenario":scenario, "phase":phase, "nonce":nonce, "pid":pid, "isolatedDatabase":str(database)}.items():
        if receipt.get(key) != expected:
            raise RuntimeError(f"Wrong {key}: {receipt.get(key)!r}")
    if scenario not in SCENARIOS or phase not in ("exercise", "restart"):
        raise RuntimeError("Unknown case")
    durable = receipt.get("durable", {})
    results = receipt.get("results", {})
    if set(durable) != {"channel", "thread"} or set(results) != {"channel", "thread"}:
        raise RuntimeError("Missing exact channel/thread evidence")
    for target in ("channel", "thread"):
        row, result = durable[target], results[target]
        expected_key = {"team":"T1", "channel":"C2", "thread":"1719800400.000000" if target == "thread" else None}
        if row.get("key") != expected_key or result.get("ok") is not True:
            raise RuntimeError("Wrong destination or failed target")
        canonical = row.get("canonical", {})
        if (canonical.get("composer", {}).get("text") != AFTER
                or canonical["composer"].get("reply_broadcast") is not (target == "thread")
                or canonical.get("attachments") != []):
            raise RuntimeError("Canonical observed draft not retained")
        alternates = row.get("alternates", [])
        if len(alternates) != (1 if scenario in ALTERNATE else 0):
            raise RuntimeError("Wrong alternate inventory")
        if alternates:
            alternate = alternates[0]
            blocked=row.get("blocked")
            if not isinstance(blocked,dict) or not blocked.get("token") or blocked.get("local")!=alternate.get("local"):
                raise RuntimeError("Exact blocked local version missing")
            if (not isinstance(alternate.get("writer"), str) or not alternate["writer"]
                    or type(alternate.get("serial")) is not int or alternate["serial"] <= 0
                    or alternate.get("local", {}).get("composer", {}).get("text") != NEWER
                    or alternate["local"]["composer"].get("reply_broadcast") is not (target == "thread")):
                raise RuntimeError("Wrong exact preserved alternate")
        if not alternates and row.get("blocked") is not None:
            raise RuntimeError("Unexpected blocked local version")
        if phase == "exercise":
            if result.get("barrierRetained") is not (scenario != "apply"):
                raise RuntimeError("Wrong barrier outcome")
            if scenario in ALTERNATE and (type(result.get("serial")) is not int or result["serial"] != alternates[0]["serial"]):
                raise RuntimeError("Durable alternate does not identify the final editor serial")
            if scenario == "ahead":
                preserved = result.get("preservation", {})
                if any(preserved.get(k) != alternates[0].get(k) for k in ("writer", "serial", "local")) or preserved.get("storageRevision") != row.get("storageRevision"):
                    raise RuntimeError("Preservation receipt does not identify durable tuple")
        native = result.get("native", {})
        expected_text = AFTER if phase == "restart" or scenario == "apply" else NEWER if scenario in ALTERNATE else "Original \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
        if native.get("text") != expected_text or native.get("display") != expected_text:
            raise RuntimeError("Native plain Unicode wire/display projection mismatch")
        if native.get("codePoints") != [ord(c) for c in expected_text] or native.get("utf16Length") != len(expected_text.encode("utf-16-le"))//2:
            raise RuntimeError("Exact Unicode/UTF16 receipt missing")
        if not {0x2014,0x65e5,0x672c,0x8a9e,0x1f680,0x301}.issubset(set(native["codePoints"])):
            raise RuntimeError("Unicode coverage was replaced or normalized")
    if phase == "exercise" and scenario in ALTERNATE - {"ahead"} and receipt.get("routedInputs", 0) < 2:
        raise RuntimeError("Synchronous input routing not exercised")
    return durable


def sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def valid_png(path):
    with path.open("rb") as source:
        header=source.read(24)
    if len(header)!=24 or header[:8]!=b"\x89PNG\r\n\x1a\n" or header[12:16]!=b"IHDR" or min(struct.unpack(">II",header[16:24]))==0:
        raise RuntimeError("Invalid native stage PNG header")


def ready_tuple(nonce, pid, scenario, phase):
    return {"version":1,"nonce":nonce,"pid":pid,"scenario":scenario,"phase":phase}


def validate_ready(actual, expected):
    if actual != expected or type(actual.get("version")) is not int or type(actual.get("pid")) is not int:
        raise RuntimeError("Wrong runner readiness owner or phase")


def release_ready(database, expected):
    path=database.parent/f"ready-{expected['phase']}.json"
    staged=path.with_suffix(".staged")
    with staged.open("x",encoding="utf-8") as output:
        output.write(json.dumps(expected));output.flush();os.fsync(output.fileno())
    try:
        # Atomic publish without replacing any prior marker. Both fixed paths
        # are inside this run's freshly created private storage directory.
        os.link(staged,path)
    finally:
        staged.unlink()


def normalize(native, evidence, stack):
    if sys.platform != "win32":
        evidence["normalization"]={"enabled":False,"reason":"Windows-only work-area/pointer helpers"}
        return None
    from windows_process_metrics import WindowsAPI
    from visual_placement import fit_work_area
    from visual_pointer import FixturePointer
    visual=runpy.run_path(str(Path(__file__).with_name("check-visual-native.py")))
    api=WindowsAPI();handle=api.open(native.process.pid)
    stack.callback(api.kernel.CloseHandle,handle)
    creation=api.times(handle)["creation_filetime"]
    evidence["processCreationFiletime"]=creation
    def verify():
        native.require_running()
        current=api.times(handle)
        if current["creation_filetime"]!=creation or current["exit_filetime"]!=0:
            raise RuntimeError("Retained fixture process identity changed")
    observe=lambda:visual["window_observation"](native.process.pid)
    evidence["placement"]={}
    fit_work_area(observe,verify,evidence["placement"],position_policy="top-left")
    pointer=FixturePointer(observe,verify)
    stack.callback(pointer.close)  # ExitStack closes before NativeFixture terminates.
    evidence["pointer"]=pointer.data
    pointer.park()
    pointer.settle(native,visual["fingerprint"])
    pointer.check("before_ready",require_clear=True)
    return pointer


def run_phase(fixture, scenario, phase, nonce, database, binary, captures, binary_sha):
    label = f"{fixture}-{scenario}-{phase}"
    env = {SPEC.flag:"1", "SUPER_PLATINUM_DRAFT_PROJECTION_DB":str(database),
        "SUPER_PLATINUM_DRAFT_PROJECTION_NONCE":nonce, "SUPER_PLATINUM_DRAFT_PROJECTION_SCENARIO":scenario,
        "SUPER_PLATINUM_DRAFT_PROJECTION_PHASE":phase, "SUPER_PLATINUM_DRAFT_PROJECTION_CAPTURE_DIR":str(captures)}
    if sha(binary) != binary_sha:
        raise RuntimeError("Binary changed before launch")
    evidence = {"phase":phase, "ok":False}
    with NativeFixture(fixture, captures / f"{label}.log", binary, env) as native, ExitStack() as stack:
        evidence["pid"] = native.process.pid
        try:
            expected=ready_tuple(nonce,native.process.pid,scenario,phase)
            waiting_deadline=time.monotonic()+25
            while not (waiting:=native.receipts("draft-projection-waiting: ")):
                native.require_running()
                if native.receipts(SPEC.prefix) or time.monotonic()>=waiting_deadline:
                    raise RuntimeError("Fixture did not reach bounded runner readiness")
                time.sleep(.05)
            if len(waiting)!=1:
                raise RuntimeError("Duplicate readiness request")
            validate_ready(waiting[0],expected)
            pointer=normalize(native,evidence,stack)
            release_ready(database,expected)
            evidence["readiness"]=expected
            deadline, last_observation, stage_count = time.monotonic()+110, 0., 0
            while True:
                native.require_running()
                stages_seen=native.receipts("draft-projection-stage: ")
                if pointer:
                    for stage in stages_seen[stage_count:]:
                        pointer.check(f"{stage.get('stage')}:{stage.get('edge')}",require_clear=True)
                    if time.monotonic()-last_observation>=.2:
                        pointer.check("running",require_clear=True)
                        last_observation=time.monotonic()
                stage_count=len(stages_seen)
                receipts = native.receipts(SPEC.prefix)
                if receipts:
                    if len(receipts) != 1:
                        raise RuntimeError("Duplicate terminal receipt")
                    receipt=receipts[0]
                    evidence["native"]=receipt
                    validate(fixture, scenario, phase, nonce, native.process.pid, database, receipt)
                    stages=("channel", "thread") if phase == "restart" else ("channel-before", "channel-result", "thread-before", "thread-result")
                    if set(receipt.get("captures", {})) != set(stages):
                        raise RuntimeError("Missing native stage capture")
                    for stage in stages:
                        path=captures/f"{label}-{stage}.png"
                        if receipt["captures"][stage] != str(path) or not path.is_file():
                            raise RuntimeError(f"Missing owned stage PNG: {stage}")
                        valid_png(path)
                    if stages_seen != [{"stage":stage,"edge":edge} for stage in stages for edge in ("before","after")]:
                        raise RuntimeError("Missing ordered capture stage checkpoints")
                    if pointer:
                        pointer.check("terminal",require_clear=True)
                    evidence["ok"]=True
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError("Native projection receipt timed out")
                time.sleep(.05)
        except BaseException as error:
            evidence["error"]=str(error)
            try:
                native.capture(captures/f"{label}-failure.png")
            except (OSError, RuntimeError, ValueError):
                pass
            (captures/f"{label}.json").write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf-8")
            raise
    evidence["processExited"]=native.process.poll() is not None
    if not evidence["processExited"] or sha(binary) != binary_sha:
        raise RuntimeError("Owned process exit or immutable binary proof failed")
    (captures/f"{label}.json").write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf-8")
    return evidence


def run(fixture, scenario, binary, captures):
    captures.mkdir(parents=True,exist_ok=True)
    captures=captures/f"{fixture}-{scenario}"
    captures.mkdir()  # Refuse an existing case directory; stale evidence never counts.
    storage=Path(tempfile.mkdtemp(prefix="draft-native-projection-",dir=captures)).resolve()
    nonce=secrets.token_hex(16)
    (storage/"owner.json").write_text(json.dumps({"version":1,"nonce":nonce}),encoding="utf-8")
    database=storage/"draft-projection.sqlite"
    binary_sha=sha(binary)
    evidence={"fixture":fixture,"scenario":scenario,"platform":sys.platform,"binary":str(binary),"binarySha256":binary_sha,"isolatedDatabase":str(database),"nonce":nonce,"ok":False,"phases":[]}
    try:
        for phase in ("exercise","restart"):
            evidence["phases"].append(run_phase(fixture,scenario,phase,nonce,database,binary,captures,binary_sha))
        first,second=(p["native"]["durable"] for p in evidence["phases"])
        if first != second:
            raise RuntimeError("Second process changed canonical/alternate tuple")
        evidence.update(ok=True,twoProcessRestart=True,exactDurableReadback=True)
    except BaseException as error:
        evidence["error"]=str(error)
        raise
    finally:
        path=captures/f"{fixture}-{scenario}.json"
        path.write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf-8")
        print(json.dumps({"receipt":str(path),**evidence}),flush=True)

if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary",type=Path,default=Path(os.environ.get("SUPER_PLATINUM_BINARY",DEFAULT_BINARY)))
    parser.add_argument("--captures",type=Path,default=ROOT/"tmp/agent-ui")
    parser.add_argument("--fixture",action="append",choices=FIXTURES)
    parser.add_argument("--scenario",action="append",choices=SCENARIOS)
    args=parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or SCENARIOS:
            run(fixture,scenario,args.binary.resolve(),args.captures.resolve())
