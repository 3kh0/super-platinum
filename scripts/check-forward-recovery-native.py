#!/usr/bin/env python3
"""Disconnected seed/restart/retire checks using an isolated journal at each size."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("forward-recovery", "forward-recovery-narrow")
ACCOUNT = "00000000-0000-4000-8000-000000000071"
PREFIX = "forward recovery check passed: "


def read_receipt(database):
    with closing(sqlite3.connect(database.resolve().as_uri() + "?mode=ro", uri=True)) as connection:
        owner = connection.execute("SELECT account FROM identity WHERE singleton=1").fetchone()
        if owner != (ACCOUNT,):
            raise RuntimeError("Forward fixture account identity changed")
        rows = [json.loads(row[0]) for row in connection.execute("SELECT body FROM receipts")]
    if len(rows) != 1:
        raise RuntimeError("Recovery must retain exactly one original request")
    return rows[0]


def validate_saved(row, mode, initial=None):
    if "Uncertain" not in row["delivery"]["status"]:
        raise RuntimeError("Offline recovery/retirement cannot claim delivery")
    if row["retired"] != (mode == "retire"):
        raise RuntimeError("Unexpected retirement")
    if row["scope"]["destination"] != "C_FORWARD_RECOVERY_MISSING":
        raise RuntimeError("Original forwarding destination changed")
    if "Recovered note · 日本語 🚀" not in json.dumps(row["delivery"]["request"]["blocks"], ensure_ascii=False):
        raise RuntimeError("Original Unicode note missing")
    if initial is not None:
        if row["attempt_id"] != initial["attempt_id"]:
            raise RuntimeError("Offline actions started another attempt")
        if row["delivery"] != initial["delivery"] or row["scope"] != initial["scope"]:
            raise RuntimeError("Restart changed the exact request, note, source or destination")


def run_phase(fixture, mode, database, binary, captures):
    label = f"{fixture}-{mode}"
    evidence = {"phase": mode, "ok": False}
    # NativeFixture strips inherited SUPER_PLATINUM_* flags, including the
    # ordinary forward check. Only this disconnected recovery check is enabled.
    checks = {"SUPER_PLATINUM_FORWARD_RECOVERY_CHECK": mode,
              "SUPER_PLATINUM_FORWARD_RECOVERY_DB": str(database)}
    try:
        with NativeFixture(fixture, captures / f"{label}.log", binary, checks) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline = time.monotonic() + 90
                last_focus = 0
                while True:
                    native.require_running()
                    result = native.log.read_text(encoding="utf-8", errors="replace")
                    if "forward recovery check failed" in result:
                        raise RuntimeError(f"{label}: native assertion failed; see {native.log}")
                    receipts = native.receipts(PREFIX)
                    if receipts:
                        if len(receipts) != 1 or receipts[0].get("ok") is not True or receipts[0].get("mode") != mode:
                            raise RuntimeError(f"{label}: invalid native receipt: {receipts}")
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
                    native.capture(captures / f"{label}-failure.png")
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned forward fixture did not exit before the next phase")
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
    database = storage / "forward-recovery.sqlite"
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "isolatedDatabase": str(database), "ok": False, "phases": []}
    try:
        if database.exists():
            raise RuntimeError("Seed database must be absent")
        initial = None
        for mode in ("seed", "restart", "retire"):
            if mode != "seed":
                validate_saved(read_receipt(database), "restart", initial)
            evidence["phases"].append(run_phase(fixture, mode, database, binary, captures))
            row = read_receipt(database)
            validate_saved(row, mode, initial)
            if initial is None:
                initial = row
        evidence.update(ok=True, threeProcessRecovery=True, originalRequestUnchanged=True,
                        noAutomaticReplay=True, trackingExplicitlyRetired=True)
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
