"""Read-only observations of the actual split journal at held native stages."""
from contextlib import closing
import base64
import hashlib
import json
from pathlib import Path
import sqlite3


class SplitStorage:
    def __enter__(self):
        return self

    def __exit__(self, kind, error, traceback):
        if error is not None and self.path.is_file() and hasattr(self, "native"):
            try:
                destination = self.native.log.parent / "split-journal-failure.sqlite"
                if destination.exists():
                    raise RuntimeError("Refusing to overwrite failed journal evidence")
                with closing(sqlite3.connect(self.path.as_uri() + "?mode=ro", uri=True)) as source:
                    with closing(sqlite3.connect(destination)) as target:
                        source.backup(target)
                self.evidence["failureDatabase"] = str(destination)
            except Exception as failure:
                self.evidence["failureDatabaseError"] = str(failure)
        return False

    @classmethod
    def prepare(cls, directory, fixture, checks):
        result = cls()
        mode = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MODE", "middle")
        typing = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_TYPING", "none")
        content = checks.get("SUPER_PLATINUM_CANVAS_REUSE_CONTENT", "empty")
        if (mode not in ("sequence", "repeat", "empty", "start", "middle", "end", "reuse") or typing not in ("none", "correction", "erase", "restore")
                or (mode == "sequence" and typing != "none")
                or (typing == "restore" and mode not in ("repeat", "empty", "start", "middle"))
                or content not in ("empty", "spaces", "nbsp", "br", "mixed", "mixed-br", "separators", "feff-leading", "feff-interior", "feff-post-br")
                or (content != "empty" and mode != "reuse")):
            raise RuntimeError("Unknown Canvas journal mode")
        pre_enter = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER", "none")
        if pre_enter not in ("none", "correction", "revert", "retype") or (pre_enter != "none" and mode != "middle"):
            raise RuntimeError("Unknown pre-middle journal selector")
        middle_parent = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT", "none")
        middle_cadence = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE", "parent-first")
        if (middle_parent not in ("none", "correction", "erase", "revert") or middle_cadence not in ("parent-first", "child-first", "interleaved")
                or (middle_parent != "none" and mode != "middle") or (middle_parent == "none" and middle_cadence != "parent-first")):
            raise RuntimeError("Unknown middle parent journal selector")
        from canvas_second_middle_wire import selectors
        second_middle=checks.get("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE", "none")
        second_middle_cadence=checks.get("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE", "straight")
        selectors(mode,typing,pre_enter,middle_parent,middle_cadence,second_middle,second_middle_cadence)
        parent_sequence = mode == "sequence" and checks.get("SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE") in (
            "parent-correction", "parent-erase", "parent-revert", "preenter-correction", "preenter-erase", "preenter-revert")
        result.version, result.action = ((18, "SplitSequence") if second_middle != "none" else (17, "SplitParent") if middle_parent != "none" else (16, "SplitPreEnter") if pre_enter != "none" else
            (15, "EndSequenceParent") if parent_sequence else
            (14, "EndSequence") if mode == "sequence" else
            (13, "EndRepeatTyping") if mode == "repeat" and typing != "none" else
            (12, "EndRepeat") if mode == "repeat" else
            (11, "EmptyTyping") if mode == "empty" and typing != "none" else
            (10, "EmptyEnter") if mode == "empty" else
            (9, "StartTyping") if mode == "start" and typing != "none" else
            (1, "Insert") if mode == "start" else
            (7, "EndClearing") if content != "empty" else
            (8, "SplitTyping") if mode == "middle" and typing != "none" else
            (6, "EndTyping") if typing != "none" else
            {"middle": (3, "Split"), "end": (4, "End"), "reuse": (5, "EndReuse")}[mode])
        result.path = Path(directory) / "split-journal.sqlite"
        result.stages = []
        result.evidence = {"kind": "actual isolated SQLite journal", "stages": result.stages}
        checks["SUPER_PLATINUM_CANVAS_RECOVERY_DB"] = str(result.path)
        return result

    def bind(self, native):
        self.native = native

    def service(self, native):
        pass

    def read(self):
        with closing(sqlite3.connect(self.path.as_uri() + "?mode=ro", uri=True, timeout=2)) as db:
            identity = db.execute("SELECT account FROM identity WHERE singleton=1").fetchall()
            if identity != [("00000000-0000-4000-8000-000000009996",)]:
                raise RuntimeError("Split journal account differs")
            return [dict(workspace=w, user=u, file=f, operation=o, body=json.loads(b),
                         bodySha256=hashlib.sha256(b.encode()).hexdigest())
                    for w,u,f,o,b in db.execute("SELECT workspace,user,file,operation,body FROM receipts")]

    def before_capture(self, native, stage):
        rows = self.read()
        expected = int(stage["stage"] in ("saving", "uncertain"))
        if len(rows) != expected:
            raise RuntimeError("Actual split journal stage count differs")
        if rows:
            row, = rows
            body = row["body"]
            wire = base64.b64decode(stage["fixture"]["writes"][0]["documentData"], validate=True)
            if (body.get("version") != self.version or list(body.get("action", {})) != [self.action]
                    or bytes(body.get("request", [])) != wire
                    or (row["workspace"], row["user"], row["file"]) != ("T1", "U1", "Ffixture")):
                raise RuntimeError("Actual journal did not retain the exact split pair")
        self.stages.append({"stage": stage["stage"], "ordinal": stage["ordinal"], "rows": rows})

    def finish(self, native):
        self.evidence["finalRows"] = self.read()
        if self.evidence["finalRows"] != self.stages[-1]["rows"]:
            raise RuntimeError("Split journal changed after final held stage")

    def validate_closed(self, native):
        rows = self.read()
        if rows != self.evidence["finalRows"]:
            raise RuntimeError("Closed split journal changed")
        # Preserve the observed fixture database before temporary directory cleanup.
        destination = native.log.parent / "split-journal.sqlite"
        if destination.exists():
            raise RuntimeError("Refusing to overwrite journal evidence")
        destination.write_bytes(self.path.read_bytes())
        self.evidence.update(closedRows=rows, database=str(destination),
                             databaseSha256=hashlib.sha256(destination.read_bytes()).hexdigest())
