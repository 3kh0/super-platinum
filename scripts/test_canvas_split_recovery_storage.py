"""Actual SQLite checks for exact pair custody and a reversible fixture failure."""
import base64
from contextlib import closing
import copy
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from canvas_split_recovery_storage import ACCOUNT, SplitReviewStorage


class StorageTests(unittest.TestCase):
    MODE = "middle"
    TYPING = "none"
    CADENCE = "straight"
    PRE_ENTER = "none"
    MIDDLE_PARENT = "none"
    SECOND_MIDDLE = "none"
    VERSION = 3
    ACTION = {"Split": {"parent_draft": "Keep this", "child_draft": "Second paragraph"}}

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.store = SplitReviewStorage(Path(self.temp.name), split_mode=self.MODE, typing=self.TYPING, sequence_cadence=self.CADENCE, pre_enter=self.PRE_ENTER, middle_parent=self.MIDDLE_PARENT, second_middle=self.SECOND_MIDDLE)
        self.checkpoint = {"version": self.VERSION, "action": copy.deepcopy(self.ACTION)}
        self.operation = "00000000-0000-4000-8000-000000000123"
        with closing(sqlite3.connect(self.store.path)) as db:
            db.executescript("PRAGMA user_version=5; CREATE TABLE identity(singleton INTEGER,account TEXT);"
                             "CREATE TABLE receipts(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT);"
                             "CREATE TABLE local_reviews(workspace TEXT,user TEXT,file TEXT,operation TEXT,body TEXT,"
                             "observed TEXT,reviewed_at INTEGER,decision TEXT);")
            db.execute("INSERT INTO identity VALUES(1,?)", (ACCOUNT,))
            db.execute("INSERT INTO receipts VALUES(?,?,?,?,?)", ("T1", "U1", "Ffixture", self.operation, json.dumps(self.checkpoint)))
            db.commit()
        self.stage = {"stage": "seeded", "journal": {"active": {"operation": self.operation, "checkpoint": self.checkpoint},
                      "archivedExact": False, "archiveCount": 0}}
        self.store.observe(self.stage)
        b64 = lambda value: base64.b64encode(bytes(value)).decode()
        self.raw = [16, 232, 7]  # field2 revision1000, not a product checkpoint oracle
        self.observed = {"thread_sequence": 1000, "document_sequence": 1000,
                         "thread_raw": self.raw, "document_raw": self.raw, "sections": [[8, 1], [8, 2]]}
        if self.VERSION == 1:
            self.observed.pop("thread_raw")
            self.observed.pop("document_raw")
        self.archived = {"stage": "removed", "journal": {"active": None, "archivedExact": True, "archiveCount": 1},
                         "canonical": {"threadRaw": b64(self.raw), "documentRaw": b64(self.raw),
                                       "sections": [{"raw": b64([8, 1])}, {"raw": b64([8, 2])}]}}

    def archive(self):
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("INSERT INTO local_reviews SELECT *,?,1,'local_unlock_outcome_unknown' FROM receipts",
                       (json.dumps(self.observed),))
            db.execute("DELETE FROM receipts")
            db.commit()

    def test_exact_body_and_native_pair_observation(self):
        bad = copy.deepcopy(self.stage)
        bad["journal"]["active"]["checkpoint"]["action"][next(iter(self.ACTION))]["child_draft"] = "lost"
        with self.assertRaises(RuntimeError):
            self.store.observe(bad)
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE receipts SET body=?", (json.dumps(self.checkpoint, indent=2),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.observe(self.stage)

    def test_archive_requires_complete_bound_source_and_stable_reopen(self):
        self.archive()
        self.store.observe(self.archived)
        self.store.observe(dict(self.archived, stage="archive-restored",
                                journal={"active": None, "archivedExact": None, "archiveCount": 1}))
        for changed in ({**self.observed, "sections": [[8, 1]]}, {**self.observed, "thread_sequence": 1001}):
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET observed=?", (json.dumps(changed),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.observe(self.archived)

    def test_fixture_archive_failure_preserves_pair_then_releases(self):
        self.store.inject_failure(True)
        with self.assertRaises(sqlite3.IntegrityError):
            self.archive()
        self.store.observe(dict(self.stage, stage="storage-failed"))
        self.store.inject_failure(False)
        self.archive()
        self.store.observe(self.archived)


class StartStorageTests(StorageTests):
    MODE = "start"
    VERSION = 1
    ACTION = {"Insert": {"anchor": "paragraph", "before": True, "position": "aab"}}

    def test_v1_archive_rejects_invented_raw_fields_and_wrong_revision(self):
        self.archive()
        self.store.observe(self.archived)
        for changed in ({**self.observed, "thread_raw": self.raw},
                        {**self.observed, "document_sequence": 1001}):
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET observed=?", (json.dumps(changed),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.observe(self.archived)


class StartTypingStorageTests(StorageTests):
    MODE = "start"
    TYPING = "restore"
    VERSION = 9
    ACTION = {"StartTyping": {"child": "temp:blank", "position": "aab",
                              "original_updates": ["xOriginal", "Original"]}}

    def test_v9_archive_requires_raw_observation_even_with_unchanged_final_text(self):
        self.archive()
        self.store.observe(self.archived)
        wrong = dict(self.observed)
        wrong.pop("thread_raw")
        wrong.pop("document_raw")
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE local_reviews SET observed=?", (json.dumps(wrong),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.observe(self.archived)


class EmptyEnterStorageTests(StorageTests):
    MODE = "empty"
    VERSION = 10
    ACTION = {"EmptyEnter": {"child": "existing-empty", "position": "aac", "reused": True}}

    def test_v10_unchanged_bodies_still_require_full_source_archive(self):
        self.archive()
        self.store.observe(self.archived)
        for removed in ("thread_raw", "document_raw"):
            wrong = dict(self.observed)
            wrong.pop(removed)
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET observed=?", (json.dumps(wrong),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.observe(self.archived)


class EmptyTypingStorageTests(EmptyEnterStorageTests):
    TYPING = "restore"
    VERSION = 11
    ACTION = {"EmptyTyping": {"child": "existing-empty", "position": "aac", "reused": True,
                               "child_updates": ["New", "New draftx", "New", ""]}}


class RepeatEnterStorageTests(StorageTests):
    MODE = "repeat"
    VERSION = 12
    ACTION = {"EndRepeat": {"child": "existing-empty", "position": "aac", "reused": True,
                            "second_child": "temp:second", "second_position": "aad"}}

    def setUp(self):
        super().setUp()
        self.observed["sections"].append([8, 3])
        self.archived["canonical"]["sections"].append({"raw": base64.b64encode(bytes([8, 3])).decode()})

    def test_v12_complete_raw_source_required_for_whole_operation_archive(self):
        self.archive()
        self.store.observe(self.archived)
        wrong = dict(self.observed)
        wrong.pop("document_raw")
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE local_reviews SET observed=?", (json.dumps(wrong),))
            db.commit()
        with self.assertRaises(RuntimeError):
            self.store.observe(self.archived)


class RepeatTypingStorageTests(RepeatEnterStorageTests):
    TYPING = "restore"
    VERSION = 13
    ACTION = {"EndRepeatTyping": {"child": "existing-empty", "position": "aac", "reused": True,
        "second_child": "temp:third", "second_position": "aad",
        "second_child_updates": ["New", "New draftx", "New", ""]}}


class SequenceStorageTests(RepeatEnterStorageTests):
    MODE = "sequence"
    VERSION = 14
    ACTION = {"EndSequence": {"members": [
        {"id": "existing-empty", "position": "aac", "reused": True,
         "updates": ["New", "New draftx", "New", ""]},
        {"id": "temp:second", "position": "aad", "reused": False,
         "updates": ["Next paragraph"]},
        {"id": "temp:third", "position": "aae", "reused": False, "updates": []}]}}

    def setUp(self):
        super().setUp()
        self.observed["sections"].append([8, 4])
        self.archived["canonical"]["sections"].append({"raw": base64.b64encode(bytes([8, 4])).decode()})

    def test_archive_preserves_all_members_and_empty_return_history(self):
        self.archive()
        self.store.observe(self.archived)
        original = json.loads(self.store.original["body"])
        for mutation in ("history", "order", "missing"):
            changed = json.loads(json.dumps(original))
            members = changed["action"][next(iter(self.ACTION))]["members"]
            if mutation == "history":
                members[0]["updates"] = ["New", ""]
            elif mutation == "order":
                members.reverse()
            else:
                members.pop()
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET body=?", (json.dumps(changed),))
                db.commit()
            with self.assertRaises(RuntimeError):
                self.store.observe(self.archived)


class ParentSequenceStorageTests(SequenceStorageTests):
    CADENCE = "parent-correction"
    VERSION = 15
    ACTION = {"EndSequenceParent": {
        "parent_updates": ["Keep this draftx", "Keep this draft", "Keep this revised"],
        "members": SequenceStorageTests.ACTION["EndSequence"]["members"]}}

    def test_parent_history_and_v15_cannot_change_during_archive(self):
        self.archive();self.store.observe(self.archived)
        for mutation in ("parent-reports", "v14"):
            changed=json.loads(self.store.original['body'])
            if mutation=="parent-reports":changed['action']['EndSequenceParent']['parent_updates']=[]
            else:
                changed['version']=14
                changed['action']={'EndSequence':{'members':changed['action']['EndSequenceParent']['members']}}
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET body=?",(json.dumps(changed),));db.commit()
            with self.assertRaises(RuntimeError):self.store.observe(self.archived)


class ParentEraseSequenceStorageTests(ParentSequenceStorageTests):
    CADENCE = "parent-erase"
    ACTION = {"EndSequenceParent": {"parent_updates": [""],
        "members": SequenceStorageTests.ACTION["EndSequence"]["members"]}}


class ParentRevertSequenceStorageTests(ParentSequenceStorageTests):
    CADENCE = "parent-revert"
    ACTION = {"EndSequenceParent": {"parent_updates": ["Keep this draftx", "Keep this"],
        "members": SequenceStorageTests.ACTION["EndSequence"]["members"]}}


class PreenterCorrectionStorageTests(ParentSequenceStorageTests):
    CADENCE = "preenter-correction"


class PreenterEraseStorageTests(ParentEraseSequenceStorageTests):
    CADENCE = "preenter-erase"


class PreenterRevertStorageTests(ParentRevertSequenceStorageTests):
    CADENCE = "preenter-revert"


class PreMiddleStorageTests(StorageTests):
    PRE_ENTER = "revert"
    VERSION = 16
    ACTION = {"SplitPreEnter": {"enter_draft":"Original tail", "parent_draft":"Original", "child_draft":" tail",
                                "pre_enter_updates":["Original changed tail","Original tail"], "child_updates":[]}}

    def test_v16_pre_enter_history_cannot_disappear_after_unlock(self):
        self.archive();self.store.observe(self.archived)
        for key in ('enter_draft','pre_enter_updates'):
            body=json.loads(self.store.original['body']);body['action']['SplitPreEnter'][key]='wrong' if key=='enter_draft' else []
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET body=?",(json.dumps(body),));db.commit()
            with self.assertRaises(RuntimeError):self.store.observe(self.archived)


class MiddleParentStorageTests(StorageTests):
    MIDDLE_PARENT = "revert"
    VERSION = 17
    ACTION = {"SplitParent": {"enter_draft":"Original tail", "parent_draft":"Original", "child_draft":" tail",
                             "pre_enter_updates":[], "child_updates":[], "parent_updates":["Original draftx","Original"]}}

    def test_v17_parent_history_cannot_disappear_after_unlock(self):
        self.archive();self.store.observe(self.archived)
        body=json.loads(self.store.original['body']);body['action']['SplitParent']['parent_updates']=[]
        with closing(sqlite3.connect(self.store.path)) as db:
            db.execute("UPDATE local_reviews SET body=?",(json.dumps(body),));db.commit()
        with self.assertRaises(RuntimeError):self.store.observe(self.archived)


class SecondMiddleStorageTests(StorageTests):
    SECOND_MIDDLE = "parent"
    VERSION = 18
    ACTION = {"SplitSequence": {"child":"first","second_child":"second","second_target":"Original",
        "second_enter_draft":"Keep this revised","final_updates":[[""],["Second draftx","Second"],["this revised"]]}}

    def test_v18_three_identities_and_report_history_remain_exact_after_archive(self):
        self.archive();self.store.observe(self.archived)
        for key in ('child','second_child','second_target','second_enter_draft','final_updates'):
            body=json.loads(self.store.original['body']);body['action']['SplitSequence'][key]=None
            with closing(sqlite3.connect(self.store.path)) as db:
                db.execute("UPDATE local_reviews SET body=?",(json.dumps(body),));db.commit()
            with self.assertRaises(RuntimeError):self.store.observe(self.archived)


if __name__ == "__main__":
    unittest.main()
