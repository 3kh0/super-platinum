"""Two-command, query-only SQLite lock for the named private Canvas fixtures."""
import hashlib
import json
import os
from pathlib import Path
import secrets
import sqlite3
import time
import uuid

from slack_list_comments_storage_lock import identity, require, strict_json

PREFIX = "canvas-canonical-native-storage-control: "
NONCE = "SUPER_PLATINUM_CANVAS_STORAGE_NONCE"
ACCOUNT = "147b5480-9f1a-4443-89da-836472cbb209"
SCOPE = dict(workspace="T1", user="U0", file="Ffixture")
ROOT_TS = "1719800000.000100"
FIXTURES = tuple("canvas-canonical-native-" + kind + suffix
                 for kind in ("canvas", "list") for suffix in ("", "-narrow"))
MAX_BODY = 192 * 1024
# Actual recovery/journal.rs v5 tables. Any mutation/history is outside this case.
COLUMNS = {
    "identity": "singleton account",
    "receipts": "workspace user file operation body",
    "local_reviews": "workspace user file operation body observed reviewed_at decision",
    "comment_receipts": "workspace user file operation body",
    "comment_reviews": "workspace user file operation body observed decision",
    "reply_drafts": "workspace user file root id body",
    "reply_clock": "singleton value",
    "reply_captures": "id",
    "reply_attempts": "workspace user file id body",
    "reply_late_results": "id observed",
    "reply_history": "workspace user file id body observed status",
    "annotation_attempts": "workspace user file id body",
    "annotation_history": "workspace user file id body status observed",
}


def canonical_uuid(value):
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except ValueError:
        return False


class StorageControl:
    @classmethod
    def prepare(cls, directory, fixture, checks):
        directory = Path(directory)
        require(fixture in FIXTURES, "Unsupported Canvas storage fixture")
        require(directory.is_absolute() and directory.name.startswith("sp-canvas-ack-")
                and len(directory.name) > len("sp-canvas-ack-"), "Foreign private directory")
        for parent in reversed((directory, *directory.parents)):
            identity(parent, directory=True)
        require(directory.resolve(strict=True) == directory, "Redirected private directory")
        require(checks.get("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR") == str(directory)
                and checks.get("SUPER_PLATINUM_CANVAS_REPLY_DB") == str(directory / "replies.sqlite")
                and checks.get("SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK") == "1"
                and NONCE not in checks, "Foreign private environment")
        require(not any(directory.iterdir()), "Storage preparation requires fresh directory")
        value = cls()
        value.directory = directory
        value.path = directory / "replies.sqlite"
        value.owner_path = directory / "owner.json"
        value.owner = dict(version=1, nonce=secrets.token_hex(16), fixture=fixture)
        value.owner_bytes = (json.dumps(value.owner) + "\n").encode()
        with value.owner_path.open("xb") as stream:
            stream.write(value.owner_bytes)
            stream.flush()
            os.fsync(stream.fileno())
        value.directory_identity = identity(directory, directory=True)
        value.owner_identity = identity(value.owner_path)
        value.db_identity = None
        value.connection = None
        value.expected = None
        value.requests = []
        value.before = None
        value.hold_started = None
        value.closed = False
        value.evidence = dict(protocol=1, owner=value.owner.copy(), requests=[],
                              acknowledgments=[], held=False, finished=False,
                              mutations=False, cleanupErrors=[])
        checks[NONCE] = value.owner["nonce"]
        return value

    def bind(self, native):
        require(self.expected is None and not self.closed
                and native.fixture == self.owner["fixture"]
                and type(native.process.pid) is int and native.process.pid > 0,
                "Invalid original process binding")
        self.expected = dict(self.owner, pid=native.process.pid)
        self.evidence["readiness"] = self.expected.copy()
        return self.expected.copy()

    def _paths(self):
        for parent in reversed((self.directory, *self.directory.parents)):
            identity(parent, directory=True)
        require(identity(self.directory, directory=True) == self.directory_identity
                and identity(self.owner_path) == self.owner_identity
                and self.owner_path.stat().st_size <= 4096
                and self.owner_path.read_bytes() == self.owner_bytes,
                "Original storage owner replaced")
        found = identity(self.path)
        require(self.path.stat().st_size <= 32 * 1024 * 1024, "Oversized private database")
        if self.db_identity is None:
            self.db_identity = found
        require(found == self.db_identity, "Original database replaced")
        for suffix in ("-journal", "-wal", "-shm"):
            path = Path(str(self.path) + suffix)
            if path.exists() or path.is_symlink():
                identity(path)

    def _snapshot(self):
        conn = self.connection
        require(conn.execute("PRAGMA user_version").fetchone() == (5,), "Foreign journal version")
        require(conn.execute("PRAGMA journal_mode").fetchone() == ("delete",), "Foreign journal mode")
        objects = conn.execute("SELECT type,name FROM sqlite_master WHERE type IN ('table','view','trigger') AND name NOT LIKE 'sqlite_%' LIMIT 32").fetchall()
        require(set(objects) == {("table", name) for name in COLUMNS}, "Foreign journal tables or executable schema")
        for table, columns in COLUMNS.items():
            require([r[1] for r in conn.execute(f'PRAGMA table_info("{table}")')] == columns.split(),
                    "Foreign journal columns")
        require(conn.execute("SELECT singleton,account FROM identity LIMIT 2").fetchall() == [(1, ACCOUNT)],
                "Foreign journal account")
        clock = conn.execute("SELECT singleton,value FROM reply_clock LIMIT 2").fetchall()
        require(len(clock) == 1 and clock[0][0] == 1 and type(clock[0][1]) is int
                and 0 < clock[0][1] <= 2**63-1, "Invalid reply clock")
        for table in COLUMNS.keys() - {"identity", "reply_clock", "reply_drafts", "reply_captures"}:
            require(conn.execute(f'SELECT 1 FROM "{table}" LIMIT 1').fetchone() is None,
                    "Unexpected mutation attempt/history")
        rows = conn.execute("SELECT workspace,user,file,root,id,CASE WHEN length(CAST(body AS BLOB))<=? THEN body ELSE NULL END FROM reply_drafts ORDER BY id LIMIT 2", (MAX_BODY,)).fetchall()
        require(len(rows) == 1 and rows[0][:4] == ("T1", "U0", "Ffixture", ROOT_TS),
                "Foreign or multiple reply scope")
        row = rows[0]
        require(isinstance(row[5], str), "Oversized reply draft")
        draft = strict_json(row[5])
        require(isinstance(draft, dict)
                and set(draft) == {"id", "revision", "capture", "text", "blocks"}
                and draft["id"] == row[4] and canonical_uuid(draft["id"])
                and canonical_uuid(draft["capture"])
                and type(draft["revision"]) is int and 0 < draft["revision"] <= 2**63-1
                and isinstance(draft["text"], str) and isinstance(draft["blocks"], list)
                and bool(draft["blocks"]), "Invalid original rich draft")
        captures = conn.execute("SELECT CASE WHEN length(id)<=36 THEN id ELSE NULL END FROM reply_captures ORDER BY id LIMIT 129").fetchall()
        require(0 < len(captures) <= 128 and all(canonical_uuid(r[0]) for r in captures)
                and (draft["capture"],) in captures, "Invalid capture custody")
        # Hash original SQL strings/order, not merely the decoded blocks.
        content = json.dumps([rows, clock, captures], ensure_ascii=True, separators=(",", ":")).encode()
        return dict(account=ACCOUNT, scope=SCOPE.copy(), root=ROOT_TS,
                    journal=dict(drafts=[draft], attempt=None, history=[]),
                    revision=clock[0][1], captureIds=[r[0] for r in captures],
                    contentSha256=hashlib.sha256(content).hexdigest())

    def _hold(self):
        require(self.connection is None, "Duplicate hold")
        self._paths()
        self.connection = sqlite3.connect(self.path.as_uri() + "?mode=rw", uri=True,
                                          timeout=1, isolation_level=None)
        try:
            self.connection.execute("BEGIN EXCLUSIVE")
            self.connection.execute("PRAGMA query_only=ON")
            self._paths()
            self.before = self._snapshot()
            self.hold_started = time.monotonic()
            self.evidence["held"] = True
            return self.before
        except BaseException:
            self._cleanup()
            raise

    def _release(self):
        require(self.connection is not None, "Release without hold")
        self._paths()
        after = self._snapshot()
        require(after == self.before, "Locked original journal changed")
        self.connection.rollback()
        self.connection.close()
        self.connection = None
        self.hold_started = None
        self.evidence["held"] = False
        return dict(after, unchanged=True)

    def _ack(self, ack):
        self._paths()
        path = self.directory / f"storage-{ack['sequence']}.json"
        pending = path.with_suffix(".json.pending")
        require(not path.exists() and not path.is_symlink()
                and not pending.exists() and not pending.is_symlink(), "Preexisting storage acknowledgment")
        with pending.open("xb") as stream:
            stream.write((json.dumps(ack, ensure_ascii=True) + "\n").encode())
            stream.flush()
            os.fsync(stream.fileno())
        os.link(pending, path)
        pending.unlink()

    def _native(self, native):
        require(self.expected is not None and native.process.pid == self.expected["pid"]
                and type(native.process.pid) is int
                and native.fixture == self.expected["fixture"], "Foreign storage process")

    def _history(self, requests):
        require(isinstance(requests, list) and len(requests) <= 2, "Extra storage commands")
        for index, request in enumerate(requests, 1):
            expected = dict(self.expected, sequence=index, action="hold" if index == 1 else "release")
            require(isinstance(request, dict) and request == expected
                    and all(type(request.get(k)) is int for k in ("version", "pid", "sequence")),
                    "Invalid command/sequence/owner")

    def service(self, native):
        require(not self.closed and not self.evidence["finished"], "Storage service outside active run")
        self._native(native)
        require(self.hold_started is None or time.monotonic() - self.hold_started <= 90,
                "Storage hold exceeded deadline")
        requests = native.receipts(PREFIX)
        self._history(requests)
        require(requests[:len(self.requests)] == self.requests and len(requests) >= len(self.requests),
                "Storage command history changed")
        for request in requests[len(self.requests):]:
            sequence = len(self.requests) + 1
            expected = dict(self.expected, sequence=sequence, action="hold" if sequence == 1 else "release")
            require(sequence <= 2 and request == expected
                    and all(type(request.get(k)) is int for k in ("version", "pid", "sequence")),
                    "Invalid command/sequence/owner")
            data = self._hold() if sequence == 1 else self._release()
            ack = dict(expected, ok=True, **data)
            self._ack(ack)
            self.requests.append(request.copy())
            self.evidence["requests"].append(request.copy())
            self.evidence["acknowledgments"].append(ack)

    def finish(self, native):
        self.service(native)
        require(len(self.requests) == 2 and self.connection is None and not self.evidence["held"],
                "Missing release/commands")
        self._paths()
        self.evidence["finished"] = True
        return self.evidence

    def validate_closed(self, native):
        self._native(native)
        requests = native.receipts(PREFIX)
        self._history(requests)
        require(self.evidence["finished"] and not self.evidence["held"] and self.connection is None
                and not self.evidence["cleanupErrors"] and requests == self.requests
                and len(self.requests) == 2, "Closed protocol changed/incomplete")
        self._paths()
        return self.evidence

    def _cleanup(self):
        if self.connection is not None:
            conn, self.connection = self.connection, None
            for stage, callback in (("rollback", conn.rollback), ("close", conn.close)):
                try:
                    callback()
                except BaseException as error:
                    self.evidence["cleanupErrors"].append(dict(stage=stage, error=str(error)))
        self.evidence["held"] = False
        self.hold_started = None

    def close(self):
        self._cleanup()
        self.closed = True

    def __enter__(self):
        require(not self.closed, "Storage controller already closed")
        return self

    def __exit__(self, kind, error, traceback):
        self.close()
        if kind is None:
            require(self.evidence["finished"] and not self.evidence["cleanupErrors"],
                    "Storage protocol did not finish cleanly")
        return False
