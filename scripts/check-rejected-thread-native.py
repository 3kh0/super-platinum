#!/usr/bin/env python3
"""Restore a rejected thread, exit, and verify a second process loads its private DB."""
import argparse
import json
import os
from pathlib import Path
import tempfile
import time

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

SPEC = CheckSpec(
    fixture="rejected-thread-return", flag="SUPER_PLATINUM_REJECTED_THREAD_CHECK",
    prefix="rejected-thread-check: ", prepared_requests=0,
    required=("exactThreadComposer", "channelDraftPreserved", "inputEventApplied",
              "failedThreadRowRemoved", "noTransport", "noSession", "editable", "focused",
              "broadcastChecked", "eachVisible", "hitTested", "resumeHitBeforeClick", "durable.ok",
              "durable.durableThreadRestored", "durable.uncertainPreserved"),
)
KEY = {"team": "T1", "channel": "C2", "thread": "1719800400.000000"}


def run_phase(fixture, phase, database, binary, captures):
    label = f"{fixture}-{phase}"
    checks = {SPEC.flag: "1", "SUPER_PLATINUM_REJECTED_THREAD_PHASE": phase,
              "SUPER_PLATINUM_DRAFT_SETTLEMENT_DB": str(database)}
    with NativeFixture(fixture, captures / f"{label}.log", binary, checks) as native:
        evidence = {"phase": phase, "pid": native.process.pid, "stages": []}
        try:
            deadline = time.monotonic() + 65
            seen = set()
            last_focus = 0
            while True:
                native.require_running()
                for stage in native.receipts("rejected-thread-stage: "):
                    name = stage.get("stage")
                    if stage.get("ok") is not True or stage.get("phase") != phase or name not in ("before", "restored"):
                        raise RuntimeError(f"Invalid capture stage: {stage}")
                    if name not in seen:
                        native.capture(captures / f"{label}-{name}.png")
                        seen.add(name)
                        evidence["stages"].append(stage)
                receipts = native.receipts(SPEC.prefix)
                if receipts:
                    if len(receipts) != 1:
                        raise RuntimeError("Expected one native thread check receipt")
                    receipt = receipts[0]
                    evidence["native"] = receipt
                    validate_receipt(SPEC, fixture, receipt)
                    if receipt.get("phase") != phase or receipt.get("pid") != native.process.pid or receipt.get("threadKey") != KEY:
                        raise RuntimeError("Wrong phase, process, or thread scope")
                    if phase == "restore" and [sample.get("label") for sample in receipt.get("noticeClearance", [])] != ["restored", "expanded-thread", "restored-height"]:
                        raise RuntimeError("Missing notice/composer clearance proof across thread resize")
                    if phase == "restore":
                        heights = [sample.get("threadField", {}).get("height") for sample in receipt["noticeClearance"]]
                        if (any(not isinstance(height, (int, float)) for height in heights)
                                or heights[1] <= heights[0] + 1 or abs(heights[2] - heights[0]) >= 1):
                            raise RuntimeError("Thread textarea resize was not observed in actual geometry")
                    if seen != ({"before", "restored"} if phase == "restore" else {"restored"}):
                        raise RuntimeError("Missing native restoration capture")
                    time.sleep(.25)
                    native.capture(captures / f"{label}-composer.png")
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"No thread receipt; see {native.log}")
                if time.monotonic() - last_focus >= 1:
                    native.focus()
                    last_focus = time.monotonic()
                time.sleep(.05)
        except BaseException:
            try:
                native.capture(captures / f"{label}-failure.png")
            except (OSError, RuntimeError, ValueError):
                pass
            raise
    # NativeFixture has waited for this exact process to exit before restart.
    evidence["processExited"] = native.process.poll() is not None
    return evidence


def run(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "ok": False, "phases": []}
    # Keep the DB as test evidence in a new directory; it contains synthetic
    # text only. Never open or remove an existing user database.
    storage = Path(tempfile.mkdtemp(prefix=f"{fixture}-", dir=captures)).resolve()
    database = storage / "draft-settlement.sqlite"
    evidence["isolatedDatabase"] = str(database)
    try:
        evidence["phases"].append(run_phase(fixture, "restore", database, binary, captures))
        if not evidence["phases"][0]["processExited"] or not database.is_file():
            raise RuntimeError("First process did not exit with a persisted fixture database")
        evidence["phases"].append(run_phase(fixture, "restart", database, binary, captures))
        evidence["ok"] = all(phase["processExited"] for phase in evidence["phases"])
        evidence["twoProcessRestart"] = evidence["ok"]
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui")
    fixtures = (SPEC.fixture, SPEC.fixture + "-narrow")
    parser.add_argument("--fixture", action="append", choices=fixtures)
    args = parser.parse_args()
    for fixture in args.fixture or fixtures:
        run(fixture, args.binary.resolve(), args.captures.resolve())
