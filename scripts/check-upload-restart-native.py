#!/usr/bin/env python3
"""Two isolated native processes prove durable upload restoration without resend."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import time

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("upload-restart", "upload-restart-narrow")
PREFIX = "upload-restart-check: "
ACCOUNT = "00000000-0000-4000-8000-000000000071"


def rows(database):
    # Read-only URI refuses a missing DB instead of silently creating it.
    with closing(sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True)) as db:
        records = db.execute(
            "SELECT sequence,client_id,format_version,draft,file_ids "
            "FROM upload_receipts ORDER BY sequence"
        ).fetchall()
    for record in records:
        draft = json.loads(record[3])
        if draft.get("account_id") != ACCOUNT or draft.get("client_msg_id") != record[1]:
            raise RuntimeError("Upload receipt account/client identity changed")
    return records


def validate(fixture, phase, receipt):
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    if receipt.get("viewport") != expected_viewport(fixture) or receipt.get("phase") != phase:
        raise RuntimeError(f"{fixture}: incorrect native viewport/phase: {receipt}")
    if phase == "seed":
        if receipt.get("persisted") != 2 or receipt.get("noTransport") is not True:
            raise RuntimeError(f"{fixture}: incomplete seed proof: {receipt}")
    else:
        required = ("noTransport", "noResend", "inertPaths", "exactRetirement", "initialVisible", "visible")
        if (receipt.get("requests") != 1 or receipt.get("restored") != 2
                or receipt.get("remaining") != 1
                or any(receipt.get(key) is not True for key in required)):
            raise RuntimeError(f"{fixture}: incomplete restoration proof: {receipt}")


def run_phase(fixture, phase, database, binary, captures):
    label = f"{fixture}-{phase}"
    evidence = {"phase": phase, "ok": False}
    checks = {"SUPER_PLATINUM_UPLOAD_RESTART_CHECK": "1",
              "SUPER_PLATINUM_UPLOAD_RESTART_DB": str(database),
              "SUPER_PLATINUM_UPLOAD_RESTART_PHASE": phase}
    try:
        with NativeFixture(fixture, captures / f"{label}.log", binary, checks) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + 90
                last_focus = 0
                while True:
                    native.require_running()
                    receipts = native.receipts(PREFIX)
                    if receipts:
                        if len(receipts) != 1:
                            raise RuntimeError("Expected exactly one upload restart receipt")
                        evidence["native"] = receipts[0]
                        validate(fixture, phase, receipts[0])
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{label}: no native receipt; see {native.log}")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                screenshot = captures / f"{label}.png"
                native.capture(screenshot)
                evidence["screenshot"] = str(screenshot)
            except BaseException:
                try:
                    native.capture(captures / f"{label}-failure.png")
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned upload fixture did not exit before the next phase")
        evidence["ok"] = True
        return evidence
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        (captures / f"{label}.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


def run(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    storage = Path(tempfile.mkdtemp(prefix=f"{fixture}-", dir=captures)).resolve()
    database = storage / "uploads.sqlite"
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "isolatedDatabase": str(database), "ok": False, "phases": []}
    try:
        if database.exists():
            raise RuntimeError("Seed database must be absent")
        evidence["phases"].append(run_phase(fixture, "seed", database, binary, captures))
        initial = rows(database)
        if len(initial) != 2 or {row[1] for row in initial} != {"upload-check-one", "upload-check-two"}:
            raise RuntimeError("First process did not leave exactly two expected durable receipts")
        evidence["phases"].append(run_phase(fixture, "restore", database, binary, captures))
        remaining = rows(database)
        expected = [row for row in initial if row[1] == "upload-check-two"]
        if remaining != expected:
            raise RuntimeError("Restore did not retire only the confirmed receipt and preserve the other exactly")
        evidence.update(ok=True, twoProcessRestart=True, exactRetirement=True,
                        unconfirmedReceiptUnchanged=True)
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
