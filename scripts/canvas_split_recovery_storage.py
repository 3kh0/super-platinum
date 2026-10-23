"""Observe a fixture-owned split journal and its atomic local-review archive."""
import base64
from contextlib import closing
import json
from pathlib import Path
import sqlite3
import uuid
from canvas_heading import field, wire

ACCOUNT = "00000000-0000-4000-8000-000000009994"
COLUMNS = ("workspace", "user", "file", "operation", "body")
ARCHIVE_COLUMNS = (*COLUMNS, "observed", "reviewed_at", "decision")
ARCHIVED = {"removed", "refreshed", "archive-restored"}
TRIGGER = "fixture_split_review_rejection"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def bytes_vector(value):
    require(isinstance(value, list) and all(type(v) is int and 0 <= v <= 255 for v in value),
            "Malformed archived byte vector")
    return bytes(value)


class SplitReviewStorage:
    def __init__(self, captures, split_mode="middle", typing="none", reuse_content="empty"):
        require(split_mode in ("sequence", "repeat", "empty", "start", "middle", "end", "reuse") and typing in ("none", "correction", "erase", "restore")
                and (split_mode != "sequence" or typing == "none")
                and (typing != "restore" or split_mode in ("repeat", "empty", "start", "middle"))
                and reuse_content in ("empty", "spaces", "nbsp", "br", "mixed", "mixed-br", "separators", "feff-leading", "feff-interior", "feff-post-br")
                and (reuse_content == "empty" or split_mode == "reuse"), "Unknown Canvas recovery mode")
        self.version, self.action = ((14, "EndSequence") if split_mode == "sequence" else
            (13, "EndRepeatTyping") if split_mode == "repeat" and typing != "none" else
            (12, "EndRepeat") if split_mode == "repeat" else
            (11, "EmptyTyping") if split_mode == "empty" and typing != "none" else
            (10, "EmptyEnter") if split_mode == "empty" else
            (9, "StartTyping") if split_mode == "start" and typing != "none" else
            (1, "Insert") if split_mode == "start" else
            (7, "EndClearing") if reuse_content != "empty" else
            (8, "SplitTyping") if split_mode == "middle" and typing != "none" else
            (6, "EndTyping") if typing != "none" else
            {"middle": (3, "Split"), "end": (4, "End"), "reuse": (5, "EndReuse")}[split_mode])
        self.captures = Path(captures).resolve()
        self.path = self.captures / "split-recovery.sqlite"
        require(not self.path.exists(), "Refusing an existing recovery database")
        self.original = None
        self.archive = None

    def snapshot(self):
        require(self.path.is_file() and self.path.resolve().parent == self.captures,
                "Missing owned fixture journal")
        with closing(sqlite3.connect(self.path.as_uri() + "?mode=ro", uri=True, timeout=2)) as db:
            require(db.execute("PRAGMA user_version").fetchone() == (5,), "Journal schema changed")
            require(db.execute("SELECT singleton,account FROM identity").fetchall() == [(1, ACCOUNT)],
                    "Journal account changed")
            active = [dict(zip(COLUMNS, row)) for row in db.execute(
                "SELECT workspace,user,file,operation,body FROM receipts")]
            archive = [dict(zip(ARCHIVE_COLUMNS, row)) for row in db.execute(
                "SELECT workspace,user,file,operation,body,observed,reviewed_at,decision FROM local_reviews")]
            triggers = db.execute("SELECT name FROM sqlite_master WHERE type='trigger'").fetchall()
        require(all(row == (TRIGGER,) for row in triggers), "Unexpected fixture journal trigger")
        return {"active": active, "archive": archive, "failureTrigger": bool(triggers)}

    def observe(self, stage):
        observed = self.snapshot()
        active, archive = observed["active"], observed["archive"]
        name = stage["stage"]
        if self.original is None:
            require(name == "seeded" and len(active) == 1 and not archive, "Missing original split receipt")
            row = active[0]
            require(tuple(row[k] for k in COLUMNS[:3]) == ("T1", "U1", "Ffixture"), "Wrong receipt owner")
            require(str(uuid.UUID(row["operation"])) == row["operation"], "Invalid operation identity")
            checkpoint = json.loads(row["body"])
            require(checkpoint.get("version") == self.version and list(checkpoint.get("action", {})) == [self.action],
                    "Seed is not an inert split pair")
            self.original = row.copy()
        retained = json.loads(self.original["body"])
        native = stage.get("journal", {})
        if name not in ARCHIVED:
            require(active == [self.original] and archive == [], "Review changed the exact active pair")
            require(native.get("active") == {"operation": self.original["operation"], "checkpoint": retained}
                    and native.get("archivedExact") is False and type(native.get("archiveCount")) is int
                    and native["archiveCount"] == 0, "Native journal observation differs from SQLite")
        else:
            require(active == [] and len(archive) == 1, "Local unlock did not archive exactly one whole pair")
            row = archive[0]
            require({k: row[k] for k in COLUMNS} == self.original, "Archived pair body changed")
            require(row["decision"] == "local_unlock_outcome_unknown" and type(row["reviewed_at"]) is int
                    and row["reviewed_at"] > 0, "Archive incorrectly claims a remote result")
            data = json.loads(row["observed"])
            expected_fields = {"thread_sequence", "document_sequence", "sections"}
            if self.version != 1:
                expected_fields |= {"thread_raw", "document_raw"}
            require(set(data) == expected_fields,
                    "Incomplete split canonical observation")
            canonical = stage["canonical"]
            raw_thread = base64.b64decode(canonical["threadRaw"], validate=True)
            raw_document = base64.b64decode(canonical["documentRaw"], validate=True)
            if self.version != 1:
                require(bytes_vector(data["thread_raw"]) == raw_thread
                        and bytes_vector(data["document_raw"]) == raw_document,
                        "Archived source differs from the bound canonical read")
            require(all(type(data[key]) is int and data[key] > 0
                        and data[key] == field(wire(raw), 2, 0)
                        for key, raw in (("thread_sequence", raw_thread), ("document_sequence", raw_document))),
                    "Archived source revisions differ")
            expected = sorted(base64.b64decode(s["raw"], validate=True) for s in canonical["sections"])
            require(sorted(bytes_vector(s) for s in data["sections"]) == expected,
                    "Archive omitted or changed a canonical section")
            # A reopened process has no in-memory original receipt to compare;
            # exact archive custody is proven above from the actual SQLite row.
            require(native.get("active") is None
                    and native.get("archivedExact") is (None if name == "archive-restored" else True)
                    and type(native.get("archiveCount")) is int and native["archiveCount"] == 1,
                    "Native archive observation differs from SQLite")
            if self.archive is None:
                self.archive = row.copy()
            require(row == self.archive, "Reopen changed the exact archived decision")
        return observed

    def inject_failure(self, enabled):
        state = self.snapshot()
        require(state["active"] == [self.original] and not state["archive"],
                "Failure control requires the exact unresolved fixture pair")
        require(state["failureTrigger"] is (not enabled), "Failure trigger state changed")
        with closing(sqlite3.connect(self.path)) as db:
            if enabled:
                db.execute("CREATE TRIGGER fixture_split_review_rejection BEFORE INSERT ON local_reviews "
                           "BEGIN SELECT RAISE(FAIL, 'fixture split archive failure'); END")
            else:
                db.execute("DROP TRIGGER fixture_split_review_rejection")
            db.commit()
        result = self.snapshot()
        require(result["active"] == state["active"] and result["archive"] == state["archive"]
                and result["failureTrigger"] is enabled, "Failure control changed fixture data")
        return result
