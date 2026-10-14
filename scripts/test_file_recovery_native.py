"""Pure receipt and read-only SQLite checks; never launches the application."""
import copy
from contextlib import closing
from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

spec = spec_from_file_location("file_recovery_native", Path(__file__).with_name("check-file-recovery-native.py"))
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


def seed():
    rows = {}
    for index, (file, operation) in enumerate(runner.EXPECTED.items(), 1):
        operation_id = f"00000000-0000-4000-8000-{index:012d}"
        body = {"operation_id": operation_id,
                "scope": {"team_id": "T1", "user_id": "U0", "file_id": file},
                "operation": operation, "status": "Uncertain", "created_at": 100,
                "updated_at": 100, "retry_at": 0}
        rows[file] = {"team": "T1", "user": "U0", "file": file,
                      "operation_id": operation_id, "terminal": 0, "updated_at": 100,
                      "retry_at": 0, "body": json.dumps(body, ensure_ascii=False)}
    return rows


def change_body(rows, file, key, value):
    body = json.loads(rows[file]["body"])
    body[key] = value
    rows[file]["body"] = json.dumps(body, ensure_ascii=False)


class FileRecoveryReceiptTests(unittest.TestCase):
    def test_restart_and_exact_retirement(self):
        original = seed()
        runner.validate_saved(original, "seed")
        runner.validate_saved(copy.deepcopy(original), "restart", original)
        retired = copy.deepcopy(original)
        file = "F_RECOVERY_MISSING"
        change_body(retired, file, "status", "Retired")
        change_body(retired, file, "updated_at", 120)
        retired[file].update(terminal=1, updated_at=120)
        runner.validate_saved(retired, "retire", original)
        for field, value in [("operation_id", "00000000-0000-4000-8000-000000000099"),
                             ("created_at", 99), ("retry_at", 1), ("updated_at", 99),
                             ("scope", {"team_id": "T1", "user_id": "U0", "file_id": "FACTIONS"}),
                             ("operation", {"Delete": {"original_title": "Different"}})]:
            with self.subTest(field=field):
                bad = copy.deepcopy(retired)
                change_body(bad, file, field, value)
                with self.assertRaises((RuntimeError, ValueError)):
                    runner.validate_saved(bad, "retire", original)

    def test_restart_rejects_identity_operation_and_raw_rename_changes(self):
        original = seed()
        for file in original:
            for field, value in [("status", "Confirmed"), ("updated_at", 101),
                                 ("created_at", False), ("operation_id", "not-a-uuid"),
                                 ("operation", {"Rename": {"original_title": "Release notes",
                                                          "requested_title": "Recovered name"}})]:
                with self.subTest(file=file, field=field):
                    bad = copy.deepcopy(original)
                    change_body(bad, file, field, value)
                    with self.assertRaises((RuntimeError, ValueError)):
                        runner.validate_saved(bad, "restart", original)
        bad = copy.deepcopy(original)
        bad["FACTIONS"]["body"] += " "
        with self.assertRaises(RuntimeError):
            runner.validate_saved(bad, "restart", original)
        bad = copy.deepcopy(original)
        bad["FACTIONS"]["team"] = "T_OTHER"
        with self.assertRaises(RuntimeError):
            runner.validate_saved(bad, "seed")

    def test_native_receipt_requires_one_success_for_exact_phase(self):
        runner.validate_native([{"ok": True, "mode": "seed"}], "seed")
        for receipts in ([], [{"ok": False, "mode": "seed"}], [{"ok": 1, "mode": "seed"}],
                         [{"ok": True, "mode": "restart"}], [{"ok": True, "mode": "seed"}] * 2):
            with self.subTest(receipts=receipts), self.assertRaises(RuntimeError):
                runner.validate_native(receipts, "seed")

    def test_sql_is_read_only_and_account_indexes_are_checked(self):
        with tempfile.TemporaryDirectory(prefix="file-recovery-validator-") as folder:
            database = Path(folder) / "receipts.sqlite"
            with self.assertRaises(sqlite3.OperationalError):
                runner.read_receipts(database)
            self.assertFalse(database.exists())
            with closing(sqlite3.connect(database)) as connection, connection:
                connection.executescript("PRAGMA user_version=1; CREATE TABLE identity(singleton,account); "
                                         "CREATE TABLE receipts(team,user,file,operation_id,terminal,updated_at,retry_at,body);")
                connection.execute("INSERT INTO identity VALUES(1,?)", (runner.ACCOUNT,))
                for row in seed().values():
                    connection.execute("INSERT INTO receipts VALUES(?,?,?,?,?,?,?,?)", tuple(row[key] for key in runner.COLUMNS))
            before = database.read_bytes()
            self.assertEqual(runner.read_receipts(database), seed())
            self.assertEqual(database.read_bytes(), before)
            with closing(sqlite3.connect(database)) as connection, connection:
                connection.execute("UPDATE identity SET account='wrong'")
            with self.assertRaises(RuntimeError):
                runner.read_receipts(database)
            with closing(sqlite3.connect(database)) as connection, connection:
                connection.execute("UPDATE identity SET account=?", (runner.ACCOUNT,))
                connection.execute("INSERT INTO receipts SELECT * FROM receipts WHERE file='FACTIONS'")
            with self.assertRaises(RuntimeError):
                runner.read_receipts(database)


if __name__ == "__main__":
    unittest.main()
