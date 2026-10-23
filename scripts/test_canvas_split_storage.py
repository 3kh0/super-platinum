from contextlib import closing
import base64
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from canvas_split_storage import SplitStorage


class SplitJournalObservation(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {})
        with closing(sqlite3.connect(self.store.path)) as db:
            db.executescript("CREATE TABLE identity(singleton INTEGER,account TEXT); CREATE TABLE receipts(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT);")
            db.execute("INSERT INTO identity VALUES(1,?)", ("00000000-0000-4000-8000-000000009996",))
            db.commit()
        self.stage = dict(stage="saving", ordinal=2, fixture={"writes":[{"documentData":base64.b64encode(b"pair").decode()}]})

    def insert(self, request=b"pair"):
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("INSERT INTO receipts VALUES('T1','U1','Ffixture','operation',?)",
                       (json.dumps(dict(version=3, action={"Split":{}}, request=list(request))),))
            db.commit()

    def test_actual_pair_is_required_at_held_saving_stage(self):
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        self.insert()
        with self.store:
            self.store.before_capture(None, self.stage)
        self.assertEqual(len(self.store.stages[0]["rows"]), 1)

    def test_wrong_pair_or_undischarged_success_is_refused(self):
        self.insert(b"different pair")
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, dict(self.stage, stage="confirmed"))

    def test_untouched_cancel_and_uncertain_pair_observed(self):
        self.store.before_capture(None, dict(self.stage, stage="cancelled"))
        self.insert()
        self.store.before_capture(None, dict(self.stage, stage="uncertain"))
        self.store.finish(None)
        self.assertEqual(self.store.evidence["finalRows"], self.store.stages[-1]["rows"])

    def test_typed_middle_custody_rejects_v3_and_preserves_ordered_v8_reports(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "middle",
            "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": "restore"})
        self.insert()
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        body = dict(version=8, action={"SplitTyping": {
            "child_draft": "Second paragraph",
            "child_updates": ["Second paragraphx", "Second paragraph"]}}, request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        self.assertEqual(self.store.stages[-1]["rows"][0]["body"], body)
        body["action"]["SplitTyping"]["child_updates"].pop(0)
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.finish(None)


    def test_empty_enter_requires_two_row_action_and_exact_custody(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "empty"})
        self.insert()
        for version, action in ((1, "Insert"), (4, "End"), (5, "EndReuse")):
            body = dict(version=version, action={action: {}}, request=list(b"pair"))
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.before_capture(None, self.stage)
        body = dict(version=10, action={"EmptyEnter": {
            "child": "existing-empty", "position": "aac", "reused": True}}, request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        body["action"]["EmptyEnter"]["reused"] = False
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.finish(None)

    def test_empty_enter_does_not_accept_unreferenced_typing(self):
        for typing in ("rich", "newline", "repeated-enter"):
            with self.assertRaises(RuntimeError):
                SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
                    "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "empty",
                    "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": typing})

    def test_typed_empty_revert_requires_v11_and_complete_ordered_reports(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "empty",
            "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": "restore"})
        self.insert()
        for version, action in ((10, "EmptyEnter"), (6, "EndTyping")):
            body = dict(version=version, action={action: {}}, request=list(b"pair"))
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.before_capture(None, self.stage)
        body = dict(version=11, draft="", action={"EmptyTyping": {
            "child": "existing-empty", "position": "aac", "reused": True,
            "child_updates": ["New", "New draftx", "New", ""]}}, request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        body["action"]["EmptyTyping"]["child_updates"].pop(1)
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.finish(None)

    def test_repeated_enter_requires_three_row_action_and_preserves_second_identity(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "repeat"})
        self.insert()
        body = dict(version=4, draft="Keep this", action={"End": {
            "child": "temp:first", "position": "aab"}}, request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        body.update(version=12, action={"EndRepeat": {
            "child": "temp:first", "position": "aab", "reused": False,
            "second_child": "temp:second", "second_position": "aac"}})
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        body["action"]["EndRepeat"]["second_child"] = "temp:first"
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.finish(None)

    def test_typed_repeat_keeps_ordered_third_history_and_refuses_v12(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "repeat",
            "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": "restore"})
        self.insert()
        shape = dict(child="existing-empty", position="aac", reused=True,
                     second_child="temp:third", second_position="aad")
        body = dict(version=12, draft="Keep this", action={"EndRepeat": shape}, request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        body.update(version=13, action={"EndRepeatTyping": {
            **shape, "second_child_updates": ["New", "New draftx", "New", ""]}})
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        for field, replacement in (("second_child_updates", ["New", ""]),
                                   ("second_child", "existing-empty")):
            wrong = json.loads(json.dumps(body))
            wrong["action"]["EndRepeatTyping"][field] = replacement
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE receipts SET body=?", (json.dumps(wrong),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.finish(None)

    def test_sequence_keeps_all_ordered_members_and_reports(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "sequence"})
        self.insert()
        body = dict(version=13, draft="Keep this", action={"EndRepeatTyping": {}},
                    request=list(b"pair"))
        def write(value):
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE receipts SET body=?", (json.dumps(value),))
                db.commit()
        write(body)
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        members = [dict(id="existing-empty", position="aac", reused=True,
                        updates=["New", "New draftx", "New", ""]),
                   dict(id="temp:second", position="aad", reused=False,
                        updates=["Next paragraph"]),
                   dict(id="temp:third", position="aae", reused=False, updates=[])]
        body.update(version=14, action={"EndSequence": {"members": members}})
        write(body)
        self.store.before_capture(None, self.stage)
        for mutation in ("order", "history", "identity", "omission"):
            wrong = json.loads(json.dumps(body))
            values = wrong["action"]["EndSequence"]["members"]
            if mutation == "order":
                values.reverse()
            elif mutation == "history":
                values[0]["updates"] = ["New", ""]
            elif mutation == "identity":
                values[2]["id"] = values[1]["id"]
            else:
                values.pop()
            write(wrong)
            with self.assertRaises(RuntimeError):
                self.store.finish(None)

    def test_typed_start_cannot_collapse_to_v1_after_exact_revert(self):
        self.store = SplitStorage.prepare(Path(self.temporary.name), "canvas-structure", {
            "SUPER_PLATINUM_CANVAS_SPLIT_MODE": "start",
            "SUPER_PLATINUM_CANVAS_SPLIT_TYPING": "restore"})
        body = dict(version=1, action={"Insert": {"anchor": "paragraph", "before": True}},
                    request=list(b"pair"))
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("INSERT INTO receipts VALUES('T1','U1','Ffixture','operation',?)", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.before_capture(None, self.stage)
        body.update(version=9, action={"StartTyping": {"child": "temp:blank", "position": "aab",
                                                      "original_updates": ["xOriginal", "Original"]}})
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        self.store.before_capture(None, self.stage)
        self.assertEqual(self.store.stages[-1]["rows"][0]["body"], body)
        body["action"]["StartTyping"]["original_updates"].pop(0)
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(body),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.finish(None)


if __name__ == "__main__":
    unittest.main()
