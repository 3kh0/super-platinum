"""Actual SQLite tests, exclusively under newly owned temporary directories."""
import copy
from contextlib import closing
import json
from pathlib import Path
import re
import sqlite3
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import uuid

import canvas_reply_storage_lock as S


class StorageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="sp-canvas-ack-")
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name).resolve()
        self.checks = {
            "SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR": str(self.directory),
            "SUPER_PLATINUM_CANVAS_REPLY_DB": str(self.directory / "replies.sqlite"),
            "SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK": "1",
        }
        self.control = S.StorageControl.prepare(self.directory, S.FIXTURES[0], self.checks)
        self.addCleanup(self.control.close)
        self.requests = []
        self.native = SimpleNamespace(fixture=S.FIXTURES[0], process=SimpleNamespace(pid=123),
                                      receipts=lambda prefix: copy.deepcopy(self.requests))
        self.owner = self.control.bind(self.native)
        self.draft = dict(id=str(uuid.uuid4()), revision=1, capture=str(uuid.uuid4()),
                          text="Original 中🦊", blocks=[{"type":"rich_text","opaque":[0,False,None]}])
        # Use the actual v5 schema declaration, not an alternate test database shape.
        source = (Path(__file__).resolve().parents[1] /
                  "crates/super-platinum-core/src/canvas/edit/structure/recovery/journal.rs").read_text()
        ddl = re.search(r'tx\.execute_batch\("(CREATE TABLE IF NOT EXISTS identity.*?PRAGMA user_version=5;)"\)', source, re.S)
        self.assertIsNotNone(ddl)
        self.db = self.directory / "replies.sqlite"
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript(ddl[1])
            db.execute("INSERT INTO identity VALUES(1,?)", (S.ACCOUNT,))
            db.execute("UPDATE reply_clock SET value=1")
            db.execute("INSERT INTO reply_captures VALUES(?)", (self.draft["capture"],))
            db.execute("INSERT INTO reply_drafts VALUES(?,?,?,?,?,?)",
                       ("T1","U0","Ffixture",S.ROOT_TS,self.draft["id"],json.dumps(self.draft)))
            db.commit()

    def command(self, action):
        self.requests.append(dict(self.owner, sequence=len(self.requests)+1, action=action))
        self.control.service(self.native)

    def mutate(self, sql, params=()):
        with closing(sqlite3.connect(self.db)) as db:
            db.execute(sql, params)
            db.commit()

    def refuse_hold(self):
        with self.assertRaises((RuntimeError, ValueError)):
            self.command("hold")
        self.assertIsNone(self.control.connection)

    def test_real_lock_query_only_unchanged_release_and_closed_evidence(self):
        with self.control:
            self.command("hold")
            with closing(sqlite3.connect(self.db, timeout=.01)) as other:
                with self.assertRaisesRegex(sqlite3.OperationalError, "locked"):
                    other.execute("SELECT * FROM reply_drafts").fetchall()
            with self.assertRaisesRegex(sqlite3.OperationalError, "readonly"):
                self.control.connection.execute("DELETE FROM reply_drafts")
            self.command("release")
            self.control.finish(self.native)
        evidence = self.control.validate_closed(self.native)
        hold, release = evidence["acknowledgments"]
        self.assertEqual(hold["journal"], dict(drafts=[self.draft],attempt=None,history=[]))
        self.assertEqual(hold["contentSha256"], release["contentSha256"])
        self.assertTrue(release["unchanged"])
        for ack in (hold, release):
            self.assertEqual(json.loads((self.directory/f"storage-{ack['sequence']}.json").read_text()), ack)
        self.assertFalse(evidence["mutations"])

    def test_failure_cleanup_preserves_original_exception_and_unlocks(self):
        with self.assertRaisesRegex(ValueError, "original"):
            with self.control:
                self.command("hold")
                raise ValueError("original")
        with closing(sqlite3.connect(self.db, timeout=.01)) as db:
            self.assertEqual(db.execute("SELECT body FROM reply_drafts").fetchone()[0], json.dumps(self.draft))

    def test_success_context_requires_complete_commands(self):
        with self.assertRaisesRegex(RuntimeError, "finish"):
            with self.control:
                pass

    def test_foreign_account(self):
        self.mutate("UPDATE identity SET account=?", (str(uuid.uuid4()),))
        self.refuse_hold()

    def test_foreign_scope_and_root(self):
        for field in ("workspace","user","file","root"):
            with self.subTest(field=field):
                original = {"workspace":"T1","user":"U0","file":"Ffixture","root":S.ROOT_TS}[field]
                self.mutate(f"UPDATE reply_drafts SET {field}='foreign'")
                self.requests.clear()
                self.refuse_hold()
                self.mutate(f"UPDATE reply_drafts SET {field}=?", (original,))

    def test_extra_mutation_attempt_refused(self):
        self.mutate("INSERT INTO reply_attempts VALUES('T1','U0','Ffixture','unexpected','{}')")
        self.refuse_hold()

    def test_unknown_table_or_trigger_refused(self):
        self.mutate("CREATE TABLE surprise(x)")
        self.refuse_hold()
        self.mutate("DROP TABLE surprise")
        self.mutate("CREATE TRIGGER surprising AFTER UPDATE ON reply_clock BEGIN SELECT 1; END")
        self.requests.clear()
        self.refuse_hold()

    def test_oversized_body_refused_before_decode(self):
        self.mutate("UPDATE reply_drafts SET body=?", (" "*(S.MAX_BODY+1),))
        self.refuse_hold()

    def test_duplicate_json_and_revision_bool_refused(self):
        for body in ('{"id":"a","id":"b"}', json.dumps(dict(self.draft, revision=True))):
            self.mutate("UPDATE reply_drafts SET body=?", (body,))
            self.requests.clear()
            self.refuse_hold()

    def test_unknown_capture_refused(self):
        self.mutate("DELETE FROM reply_captures")
        self.refuse_hold()

    def test_foreign_version_refused(self):
        self.mutate("PRAGMA user_version=4")
        self.refuse_hold()

    def test_duplicate_or_foreign_protocol_refused(self):
        self.requests.append(dict(self.owner,sequence=1,action="hold",extra=True))
        with self.assertRaises(RuntimeError): self.control.service(self.native)
        self.requests[:] = [dict(self.owner,sequence=True,action="hold")]
        with self.assertRaises(RuntimeError): self.control.service(self.native)
        self.requests[:] = []
        self.command("hold")
        self.requests[0]["nonce"] = "foreign"
        with self.assertRaises(RuntimeError): self.control.service(self.native)

    def test_third_command_and_closed_history_refused(self):
        self.command("hold"); self.command("release"); self.control.finish(self.native)
        self.requests.append(dict(self.owner,sequence=3,action="hold"))
        with self.assertRaises(RuntimeError): self.control.validate_closed(self.native)

    def test_closed_boolean_alias_refused(self):
        self.command("hold"); self.command("release"); self.control.finish(self.native)
        self.requests[0]["version"] = True
        with self.assertRaises(RuntimeError): self.control.validate_closed(self.native)

    def test_replaced_database_refused(self):
        self.command("hold"); self.command("release")
        self.db.rename(self.directory/"old.sqlite")
        self.db.write_bytes((self.directory/"old.sqlite").read_bytes())
        with self.assertRaises(RuntimeError): self.control.finish(self.native)

    def test_exact_four_aliases_prepare_only_with_owned_directory(self):
        for fixture in S.FIXTURES:
            with tempfile.TemporaryDirectory(prefix="sp-canvas-ack-") as name:
                directory=Path(name).resolve()
                checks=dict(self.checks)
                checks.pop(S.NONCE)
                checks["SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR"]=str(directory)
                checks["SUPER_PLATINUM_CANVAS_REPLY_DB"]=str(directory/"replies.sqlite")
                control=S.StorageControl.prepare(directory,fixture,checks)
                self.assertEqual(control.owner["fixture"],fixture)
                control.close()

    def test_owner_replacement_refused(self):
        self.control.owner_path.write_text("{}")
        self.refuse_hold()

    def test_preexisting_ack_does_not_overwrite_and_cleanup_unlocks(self):
        marker=self.directory/"storage-1.json";marker.write_text("original")
        with self.assertRaises(RuntimeError):
            with self.control:
                self.command("hold")
        self.assertEqual(marker.read_text(),"original")
        self.assertIsNone(self.control.connection)

    def test_database_hardlink_refused(self):
        import os
        os.link(self.db,self.directory/"duplicate.sqlite")
        self.refuse_hold()

    def test_deadline_and_wrong_pid_refused(self):
        self.command("hold")
        with patch.object(S.time,"monotonic",return_value=self.control.hold_started+91):
            with self.assertRaises(RuntimeError): self.control.service(self.native)
        self.native.process.pid=999
        with self.assertRaises(RuntimeError): self.control.service(self.native)

    def test_prepare_requires_exact_fresh_path_and_environment(self):
        for fixture,checks in [("canvas-comments",self.checks),
                               (S.FIXTURES[0],dict(self.checks,SUPER_PLATINUM_CANVAS_REPLY_DB="foreign"))]:
            with self.assertRaises(RuntimeError): S.StorageControl.prepare(self.directory,fixture,checks)
        with self.assertRaises(RuntimeError): S.StorageControl.prepare(self.directory,S.FIXTURES[0],self.checks)


if __name__ == "__main__":
    unittest.main()
