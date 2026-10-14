#!/usr/bin/env python3
"""Disconnected file recovery in three isolated processes at each window size."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import time
import uuid

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("file-recovery", "file-recovery-narrow")
ACCOUNT = "00000000-0000-4000-8000-000000009999"
PREFIX = "file recovery check passed: "
EXPECTED = {
    "FACTIONS": {"Rename": {"original_title": "Release notes \u00b7 \u65e5\u672c\u8a9e",
                            "requested_title": "Recovered name \u00b7 \u65e5\u672c\u8a9e"}},
    "F_RECOVERY_MISSING": {"Delete": {"original_title": "Missing recovery file"}},
}
COLUMNS = ("team", "user", "file", "operation_id", "terminal", "updated_at", "retry_at", "body")


def read_receipts(database):
    # mode=ro refuses missing storage; closing also releases Windows file handles.
    with closing(sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True)) as connection:
        if connection.execute("PRAGMA user_version").fetchone() != (1,):
            raise RuntimeError("Unexpected file journal schema")
        if connection.execute("SELECT singleton,account FROM identity").fetchall() != [(1, ACCOUNT)]:
            raise RuntimeError("File fixture account identity changed")
        rows = [dict(zip(COLUMNS, values)) for values in connection.execute(
            "SELECT team,user,file,operation_id,terminal,updated_at,retry_at,body FROM receipts ORDER BY file"
        )]
    if len(rows) != 2 or {row["file"] for row in rows} != set(EXPECTED):
        raise RuntimeError("Expected exactly the original rename and missing-file deletion")
    return {row["file"]: row for row in rows}


def validate_saved(rows, mode, initial=None):
    if mode not in ("seed", "restart", "retire") or set(rows) != set(EXPECTED):
        raise RuntimeError("Unexpected recovery phase or file inventory")
    ids = set()
    for file, stored in rows.items():
        row = json.loads(stored["body"])
        if set(row) != {"operation_id", "scope", "operation", "status", "created_at", "updated_at", "retry_at"}:
            raise RuntimeError("Unexpected file receipt fields")
        expected_status = "Retired" if mode == "retire" and file == "F_RECOVERY_MISSING" else "Uncertain"
        if (row["scope"] != {"team_id": "T1", "user_id": "U0", "file_id": file}
                or row["operation"] != EXPECTED[file] or row["status"] != expected_status):
            raise RuntimeError("File scope, exact Unicode operation or uncertainty changed")
        operation_id = row["operation_id"]
        if not isinstance(operation_id, str) or str(uuid.UUID(operation_id)) != operation_id or operation_id in ids:
            raise RuntimeError("Invalid or duplicate file operation ID")
        ids.add(operation_id)
        if any(type(row[key]) is not int or row[key] < 0 for key in ("created_at", "updated_at", "retry_at")):
            raise RuntimeError("Invalid receipt timestamps")
        if row["updated_at"] < row["created_at"] or row["retry_at"] != 0:
            raise RuntimeError("Offline check changed chronology or cooldown")
        expected_index = {
            "team": "T1", "user": "U0", "file": file, "operation_id": operation_id,
            "terminal": int(expected_status == "Retired"),
            "updated_at": row["updated_at"], "retry_at": row["retry_at"], "body": stored["body"],
        }
        if stored != expected_index:
            raise RuntimeError("SQL indexes disagree with the exact receipt")
        if initial is None:
            if mode != "seed" or row["created_at"] != row["updated_at"]:
                raise RuntimeError("Unexpected initial receipt chronology")
        elif file == "FACTIONS" or mode != "retire":
            if stored != initial[file]:
                raise RuntimeError("Restart or offline check changed an original receipt")
        else:
            before = json.loads(initial[file]["body"])
            allowed = dict(before, status="Retired", updated_at=row["updated_at"])
            allowed_index = dict(initial[file], terminal=1, updated_at=row["updated_at"], body=stored["body"])
            if row != allowed or stored != allowed_index or row["updated_at"] < before["updated_at"]:
                raise RuntimeError("Retirement changed more than the missing deletion's status and update time")


def validate_native(receipts, mode):
    if (len(receipts) != 1 or not isinstance(receipts[0], dict)
            or receipts[0].get("ok") is not True
            or receipts[0] != {"ok": True, "mode": mode}):
        raise RuntimeError(f"Invalid file recovery native receipt: {receipts}")


def run_phase(fixture, mode, database, binary, captures):
    label = f"{fixture}-{mode}"
    evidence = {"phase": mode, "ok": False}
    native = NativeFixture(fixture, captures / f"{label}.log", binary, {
        "SUPER_PLATINUM_FILE_RECOVERY_CHECK": mode,
        "SUPER_PLATINUM_FILE_RECOVERY_DB": str(database),
    })
    # NativeFixture strips inherited checks and isolates discovery and WebView data.
    try:
        with native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + 90
                last_focus = 0
                while True:
                    native.require_running()
                    result = native.log.read_text(encoding="utf-8", errors="replace")
                    if "file recovery check failed" in result:
                        raise RuntimeError(f"{label}: native assertion failed; see {native.log}")
                    receipts = native.receipts(PREFIX)
                    if receipts:
                        validate_native(receipts, mode)
                        evidence["native"] = receipts[0]
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{label}: no native receipt; see {native.log}")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                time.sleep(.25)
                screenshot = captures / f"{label}.png"
                native.capture(screenshot)
                evidence["screenshot"] = str(screenshot)
            except BaseException:
                try:
                    state = native.control("state", timeout=3)
                    (captures / f"{label}-failed-state.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
                except (OSError, RuntimeError, ValueError):
                    pass
                try:
                    failure = captures / f"{label}-failure.png"
                    native.capture(failure)
                    evidence["failureScreenshot"] = str(failure)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned file fixture did not exit before the next phase")
        evidence["ok"] = True
        return evidence
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        if native.process is not None:
            evidence["pid"] = native.process.pid
            evidence["processExited"] = native.process.poll() is not None
            evidence["exitCode"] = native.process.poll()
        (captures / f"{label}.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


def run(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    storage = Path(tempfile.mkdtemp(prefix=f"{fixture}-", dir=captures)).resolve()
    database = storage / "file-recovery.sqlite"
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "isolatedDatabase": str(database), "ok": False, "phases": []}
    try:
        if database.exists():
            raise RuntimeError("Seed database must be absent")
        initial = None
        for mode in ("seed", "restart", "retire"):
            if mode != "seed":
                validate_saved(read_receipts(database), "restart", initial)
            evidence["phases"].append(run_phase(fixture, mode, database, binary, captures))
            rows = read_receipts(database)
            validate_saved(rows, mode, initial)
            if initial is None:
                initial = rows
            evidence["phases"][-1]["savedReceipts"] = rows
        evidence.update(ok=True, threeProcessRecovery=True, renameReceiptUnchanged=True,
                        exactDeletionRetirement=True, originalOperationsUnchanged=True,
                        fixtureConnectionGuard="recovery_check::run rejects session or Transport before journal setup")
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
