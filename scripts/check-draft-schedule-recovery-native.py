#!/usr/bin/env python3
"""Isolated known-ID Schedule recovery native acceptance.

Preparation under tmp cannot launch; promote into scripts after integration.
Synthetic HTTP/DOM input only. No production Session/Transport or clipboard.
"""
import argparse
import copy
from contextlib import ExitStack
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import sys
import time
from urllib.parse import urlsplit

# Preparation is isolated under tmp/managed-schedule-recovery-native-prep. No active runner edits.
SCRIPTS = next((parent / "scripts" for parent in Path(__file__).resolve().parents
                if (parent / "Cargo.toml").is_file() and (parent / "scripts/native_fixture.py").is_file()), None)
if SCRIPTS is None:
    raise RuntimeError("Cannot locate repository-owned native helpers")
sys.path.insert(0, str(SCRIPTS))
from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import expected_viewport

UTIL = runpy.run_path(str(SCRIPTS / "check-slack-list-board-add-native.py"))
PREF = runpy.run_path(str(SCRIPTS / "check-read-preferences-native.py"))
sha = UTIL["binary_sha"]
png = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
strict_equal = PREF["strict_equal"]
number = PREF["number"]
rectangle = PREF["rectangle"]
validate_query = PREF["validate_query"]
FIXTURE = "draft-schedule-recovery"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("positive-channel", "positive-thread", "absent-channel", "changed-payload-channel",
             "cooldown-channel", "typing-read-thread", "typing-commit-thread", "owner-rotation-channel",
             "writer-replacement-channel", "queue-reset-channel", "queue-newer-channel", "storage-retry-channel",
             "pending-check-channel", "unmount-thread", "successor-channel", "reenroll-channel")
RESTART_SCENARIOS = ("cooldown-channel", "pending-check-channel")
RACE_SCENARIOS = ("owner-rotation-channel", "writer-replacement-channel", "queue-reset-channel", "queue-newer-channel",
                  "successor-channel", "reenroll-channel", "unmount-thread")
UNCONFIRMED_SCENARIOS = ("absent-channel", "changed-payload-channel", "cooldown-channel", "pending-check-channel")
FLAG = "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_CHECK"
PREFIX, WAITING, STAGE = "draft-schedule-recovery-check: ", "draft-schedule-recovery-waiting: ", "draft-schedule-recovery-stage: "
UPDATE = runpy.run_path(str(SCRIPTS / "check-draft-update-recovery-native.py"))
require, same, uuid4 = (UPDATE[name] for name in ("require", "same", "uuid4"))
native = UPDATE["native"]
validate_request_url = UPDATE["validate_request_url"]
# Load the sibling pure contract from the staged or promoted location.
CONTRACT = runpy.run_path(str(Path(__file__).with_name("draft_schedule_recovery_receipts.py")))
globals().update({name: value for name, value in CONTRACT.items() if not name.startswith("__")})


def validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory, receipt, edges, dimensions):
    validate(fixture, scenario, phase, nonce, pid, database, receipt)
    names = stages(scenario, phase)
    same(edges, [{"stage": s, "edge": e} for s in names for e in ("before", "after")], "Missing or extra ordered capture edges")
    require(set(receipt.get("captures", {})) == set(names), "Wrong capture set")
    sizes = {}
    for stage in names:
        path = directory / f"{fixture}-{scenario}-{phase}-{stage}.png"
        same(receipt["captures"][stage], str(path), "Unowned capture path")
        sizes[stage] = png(path, dimensions)
    return sizes


def ready_tuple(nonce, pid, scenario, phase):
    if scenario not in SCENARIOS or phase not in phases(scenario):
        raise RuntimeError("Unknown schedule-recovery scenario/phase")
    if (type(pid) is not int or pid <= 0 or not isinstance(nonce, str) or len(nonce) != 32
            or any(c not in "0123456789abcdef" for c in nonce)):
        raise RuntimeError("Invalid readiness owner")
    return {"version": 1, "nonce": nonce, "pid": pid, "scenario": scenario, "phase": phase}


def validate_ready(actual, expected):
    if not strict_equal(actual, expected):
        raise RuntimeError("Wrong exact readiness tuple")


def publish_ready(database, expected):
    path = database.parent / f"ready-{expected['phase']}.json"
    temporary = path.with_suffix(".staged")
    with temporary.open("x", encoding="utf-8") as output:
        json.dump(expected, output)
        output.flush()
        os.fsync(output.fileno())
    try:
        os.link(temporary, path)
    finally:
        temporary.unlink()


def run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest):
    require_contract()
    label = f"{fixture}-{scenario}-{phase}"
    env = {FLAG: "1", "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_DB": str(database),
           "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_NONCE": nonce, "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_SCENARIO": scenario,
           "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_PHASE": phase, "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_CAPTURE_DIR": str(directory)}
    if sha(binary) != digest:
        raise RuntimeError("Binary changed before owned launch")
    evidence = {"ok": False, "phase": phase, "binarySha256Before": digest}
    native = NativeFixture(fixture, directory / f"{label}.log", binary, env)
    process_api = handle = None
    try:
        with native:
            pid = native.process.pid
            evidence["pid"] = pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(pid)
                evidence["processCreationFiletime"] = process_api.times(handle)["creation_filetime"]
            with ExitStack() as stack:
                def capture_failure():
                    if evidence.get("nativeValidated"):
                        return
                    try:
                        path = directory / f"{label}-failure.png"
                        native.capture(path)
                        evidence["failureScreenshot"] = str(path)
                    except (OSError, RuntimeError, ValueError):
                        pass
                stack.callback(capture_failure)
                expected = ready_tuple(nonce, pid, scenario, phase)
                deadline = time.monotonic() + 25
                while not (waiting := native.receipts(WAITING)):
                    native.require_running()
                    if native.receipts(PREFIX) or time.monotonic() >= deadline:
                        raise RuntimeError("Bounded readiness not reached")
                    time.sleep(.05)
                if len(waiting) != 1:
                    raise RuntimeError("Duplicate readiness")
                validate_ready(waiting[0], expected)
                pointer = normalize(native, evidence, stack, process_api, handle)
                publish_ready(database, expected)
                evidence["readiness"] = expected
                deadline, sampled, edge_count = time.monotonic() + 160, 0., 0
                while True:
                    native.require_running()
                    edges = native.receipts(STAGE)
                    if pointer:
                        for edge in edges[edge_count:]:
                            pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                        if time.monotonic() - sampled >= .2:
                            pointer.check("running", require_clear=True)
                            sampled = time.monotonic()
                    edge_count = len(edges)
                    if receipts := native.receipts(PREFIX):
                        if len(receipts) != 1:
                            raise RuntimeError("Duplicate terminal receipt")
                        evidence["native"] = receipts[0]
                        evidence["pngDimensions"] = validate_artifacts(fixture, scenario, phase, nonce, pid, database,
                            directory, receipts[0], edges, evidence.get("expectedPngDimensions"))
                        evidence["nativeValidated"] = True
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Native schedule-recovery receipt timed out")
                    time.sleep(.05)
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned process did not exit")
        if sys.platform == "win32":
            validate_pointer_cleanup(evidence.get("pointer", {}))
        waiting, terminals = closed_receipts(native.log, WAITING), closed_receipts(native.log, PREFIX)
        if len(waiting) != 1 or not strict_equal(terminals, [evidence["native"]]):
            raise RuntimeError("Late extra readiness or terminal receipt")
        validate_ready(waiting[0], expected)
        validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory,
            terminals[0], closed_receipts(native.log, STAGE), evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        if handle:
            evidence["exitIdentity"] = process_api.times(handle)
            validate_exit(evidence["processCreationFiletime"], evidence["exitIdentity"])
        evidence["binarySha256After"] = sha(binary)
        if evidence["binarySha256After"] != digest:
            raise RuntimeError("Binary changed while owned process was running")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        evidence["processExited"] = native.process is not None and native.process.poll() is not None
        if handle:
            evidence["cleanupProcessIdentity"] = process_api.times(handle)
            process_api.kernel.CloseHandle(handle)
        (directory / f"{label}.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return evidence


def run(fixture, scenario, binary, captures):
    require_contract()
    stages(scenario, "exercise")
    directory = captures / f"{fixture}-{scenario}"
    directory.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    storage = directory / f"draft-schedule-recovery-{nonce}"
    storage.mkdir()
    with (storage / "owner.json").open("x", encoding="utf-8") as marker:
        json.dump({"version": 1, "nonce": nonce}, marker)
    database = storage / "draft-schedule-recovery.sqlite"
    digest = sha(binary)
    evidence = {"ok": False, "fixture": fixture, "scenario": scenario, "nonce": nonce,
                "platform": sys.platform, "binary": str(binary), "binarySha256": digest,
                "isolatedDatabase": str(database), "phases": [],
                "limitations": ["Synthetic DOM input; no trusted keyboard, OS IME, or live Slack.",
                    "Pointer/geometry checks are sampled; actual PNG inspection is still required.",
                    "PNG chunk/CRC/scanline validation does not decode pixel colors.",
                    "Exact frozen known-ID Schedule readback only; no Schedule resend or live synchronization proof.",
                    "Pending response is held through controlled owned process termination; this is not a crash-durability claim."]}
    try:
        for phase in phases(scenario):
            evidence["phases"].append(run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest))
        if scenario in RESTART_SCENARIOS:
            first, second = (p["native"]["durable"] for p in evidence["phases"])
            validate_restart_pair(scenario, first, second)
            evidence.update(twoProcessRestart=True, exactDurableReadback=True)
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        path = directory / f"{fixture}-{scenario}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-draft-schedule-recovery")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or SCENARIOS:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
