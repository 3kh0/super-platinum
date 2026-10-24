#!/usr/bin/env python3
"""Bounded offline owner deletion, using the existing audited Canvas process runner."""
import argparse
import base64
from contextlib import closing
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
from canvas_native import CanvasCheck, run
from canvas_heading import wire, field
from native_fixture import DEFAULT_BINARY

ACCOUNT = "00000000-0000-4000-8000-000000009993"
PREFIX = "canvas-owner-delete"
STAGES = {
    "success": ("initial", "delete-confirmation", "delete-saving", "deleted", "child-edit", "child-saving", "child-confirmed"),
    "stale": ("initial", "delete-confirmation", "stale"),
    "omitted": ("initial", "delete-confirmation", "delete-saving", "owner-omitted"),
    "unknown": ("initial", "delete-confirmation", "delete-saving", "uncertain"),
    "seed": ("initial", "delete-confirmation", "delete-saving", "uncertain"),
    "restart": ("restored-locked", "refresh-locked"),
    "review": ("restored-locked", "review-original", "review-child", "review-current-child", "review-controls", "removed", "refreshed"),
    "storage-failure": ("restored-locked", "review-original", "review-child", "review-current-child", "review-controls", "storage-failed"),
    "reviewed": ("archive-restored",),
}

def require(value, message):
    if not value:
        raise RuntimeError(message)

def rows(value):
    raw = [base64.b64decode(s, validate=True) for s in value["sections"]]
    result = {field(wire(s), 1, 2).decode(): s for s in raw}
    require(len(result) == len(raw), "Duplicate canonical identity")
    return result

def lineage(raw):
    return [f[3] for f in wire(field(wire(raw), 38, 2)) if f[0] == 10]

def child_id(raw):
    infos = [f[2] for f in wire(field(wire(raw), 38, 2)) if f[0] == 10]
    require(len(infos) == 1, "Expected saved single split")
    return field(wire(infos[0]), 3, 2).decode()

def rtml(raw):
    return field(wire(field(wire(field(wire(raw), 12, 2)), 1, 2)), 1, 2).decode()

def expected_text(corpus):
    return "Second \u65e5\u672c\U0001f600 & paragraph" if corpus == "unicode" else "Second paragraph"

def escaped(text):
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def check_child_content(raw, corpus, coalesced):
    expected = escaped(expected_text(corpus)+" edited")
    require(rtml(raw) == ("<b>"+expected+"</b>" if coalesced else expected),
            "Independent child canonical formatting differs")

def check_coalesced_history(decoded, processed, corpus, prefix=None):
    changes = decoded["changes"]
    # CRDT text stores escaped RTML text, so ampersands occupy five units.
    tail = escaped(expected_text(corpus))
    require(len(changes) == (6 if prefix is not None else 4), "Wrong exact coalesced history length")
    require([c["text"] for c in changes[:4]] == [s+"\ufeff" for s in ("", tail, tail+" typed", tail)], "Wrong exact-revert report history")
    require(all(not c["deleted"] and not c["marks"] for c in changes[:4]), "Coalesced prefix mutated marks/deletion")
    require(changes[0]["deps"] == [] and all(c["deps"] == [changes[i-1]["hash"]] for i,c in enumerate(changes) if i), "History is not the exact sequential prefix")
    require(changes[1]["hash"] not in processed, "Original extracted tail incorrectly processed")
    require(decoded["heads"] == [changes[-1]["hash"]] and changes[-1]["hash"] in processed, "Current saved head not processed")
    if prefix is not None:
        require(changes[:4] == prefix["changes"], "Child lost or replaced the exact historical prefix")
        desired = tail+" edited"
        require(changes[4]["text"] == desired+"\ufeff" and changes[4]["deleted"] is False
                and changes[4]["marks"] == [], "Text phase must precede the separate Bold phase")
        require(processed == [*prefix["heads"], changes[-1]["hash"]],
                "Saved report must process only the final Bold head after the source head")
        require(changes[-1]["text"] == desired+"\ufeff" and changes[-1]["deleted"] is False
                and changes[-1]["marks"] == [{"name":"b", "value":True, "start":0, "end":len(desired.encode("utf-16-le"))//2}], "Wrong full-text Bold history")
    return decoded

def inspect_coalesced(raw, corpus, prefix=None):
    session = wire(field(wire(raw), 38, 2))
    document = base64.b64encode(field(session, 5, 2)).decode()
    processed = [f[2].decode() for f in session if f[0] == 6]
    helper = Path(__file__).resolve().parents[1] / "tmp/canvas-split-owner-delete-20261024/desktop/coalesced/history.mjs"
    result = subprocess.run(["node", str(helper)], input=json.dumps({"document":document}),
                            text=True, encoding="utf-8", capture_output=True, timeout=30,
                            cwd=Path(__file__).resolve().parents[1])
    require(result.returncode == 0, "Offline captured-history decoder failed: "+result.stderr[-1000:])
    decoded = check_coalesced_history(json.loads(result.stdout), processed, corpus, prefix)
    return decoded

def geometry(g):
    require(g.get("ok") is True, "Failed native geometry")
    if isinstance(g.get("clip"), list) and "rect" in g:
        x, y, w, h = g["rect"]
        cx, cy, cw, ch = g["clip"]
        vw, vh = g["viewport"]
        require(w > 0 and h > 0 and x >= max(0, cx)-1 and y >= max(0, cy)-1
                and x+w <= min(vw, cx+cw)+1 and y+h <= min(vh, cy+ch)+1,
                "Target is clipped by viewport or actual reader")
    if "controls" in g:
        require(len(g["controls"]) in (3, 4) and all(c["enabled"] is True and c["visible"] is True for c in g["controls"]), "Missing/disabled/clipped paragraph actions")

def exact_custody(active, archive, seed_ticket):
    """The same seed bytes survive both restart and the local unknown-outcome archive."""
    require(seed_ticket is not None, "Recovery phase has no pinned seed receipt")
    require((active == [seed_ticket] and archive == [])
            or (active == [] and archive == [(*seed_ticket, "local_unlock_outcome_unknown")]),
            "Recovery changed or lost the exact seed operation/body")

class Control:
    def __init__(self, root, phase, corpus, history, seed_ticket=None, coalesced=False):
        self.root, self.phase, self.corpus, self.history = root, phase, corpus, history
        self.coalesced = coalesced
        self.prefix_history = None
        self.tombstone = None
        self.db, self.snapshot = root / "owner-delete.sqlite", root / "canonical.json"
        self.evidence = {"phase": phase, "journal": str(self.db), "snapshot": str(self.snapshot), "observations": []}
        self.initial = None
        self.seed_ticket = seed_ticket
        if phase in ("restart", "review", "storage-failure", "reviewed"):
            require(seed_ticket is not None, "Recovery control requires the seed receipt")
        self.original_ticket = [seed_ticket] if seed_ticket is not None else None
        self.injected = False
        self.last_journal = None
        self.finished_journal = None
        self.finished_rows = None
        self.native = None
    def prepare(self, directory, fixture, checks):
        checks.update(SUPER_PLATINUM_CANVAS_RECOVERY_DB=str(self.db),
                      SUPER_PLATINUM_CANVAS_OWNER_DELETE_SNAPSHOT=str(self.snapshot),
                      SUPER_PLATINUM_CANVAS_OWNER_DELETE_PHASE=self.phase,
                      SUPER_PLATINUM_CANVAS_OWNER_DELETE_CORPUS=self.corpus,
                      SUPER_PLATINUM_CANVAS_OWNER_DELETE_HISTORY=self.history,
                      SUPER_PLATINUM_CANVAS_OWNER_DELETE_COALESCED="1" if self.coalesced else "0")
        return self
    def __enter__(self): return self
    def __exit__(self, *_):
        if self.injected:
            with closing(sqlite3.connect(self.db)) as db:
                db.execute("DROP TRIGGER fixture_owner_delete_review_failure")
                db.commit()
            self.injected = False
    def bind(self, native):
        require(self.native is None, "Storage controller already bound")
        self.native = native
    def service(self, native): pass
    def journal(self):
        require(self.db.is_file() and self.db.resolve().parent == self.root, "Missing owned journal")
        with closing(sqlite3.connect(self.db.as_uri()+"?mode=ro", uri=True)) as db:
            require(db.execute("SELECT singleton,account FROM identity").fetchall() == [(1, ACCOUNT)], "Journal identity changed")
            active = db.execute("SELECT operation,body FROM receipts").fetchall()
            archive = db.execute("SELECT operation,body,decision FROM local_reviews").fetchall()
        return active, archive
    def before_capture(self, native, stage):
        require(stage.get("coalesced", False) is self.coalesced, "Coalesced fixture flag differs")
        geometry(stage["geometry"])
        current = rows(stage["canonical"])
        owner = "temp:C:Dte0000000000000000000000001" if self.history == "retained-zero" else "paragraph"
        child = child_id((self.initial or current)[owner])
        if "controls" in stage["geometry"]:
            expected = ["Edit", "Add before", "Add after", "Delete"]
            require([c["text"].strip() for c in stage["geometry"]["controls"]] == expected, "Wrong exact paragraph action policy")

        if self.initial is None:
            self.initial = current
            if self.coalesced:
                require(rtml(current[child]) == escaped(expected_text(self.corpus)), "Exact revert did not preserve visible child")
                self.prefix_history = inspect_coalesced(current[child], self.corpus)
                self.evidence["coalescedPrefix"] = self.prefix_history
        if stage["stage"] == "deleted": self.tombstone = current[owner]
        require(set(current) == (set(self.initial)-{owner} if stage["stage"] == "owner-omitted" else set(self.initial)), "Owner tombstone or unrelated row missing")
        for key in current:
            if key not in (owner, child): require(current[key] == self.initial[key], "Unrelated raw section changed")
        if owner in current: require(lineage(current[owner]) == lineage(self.initial[owner]), "Owner field10 changed")
        if stage["stage"] != "child-confirmed": require(current[child] == self.initial[child], "Preserved child changed")
        if stage["stage"] in ("deleted", "child-edit", "child-saving", "child-confirmed", "uncertain", *STAGES["review"], *STAGES["restart"], *STAGES["reviewed"], "storage-failed"):
            require(field(wire(current[owner]), 3, 0) == 1, "Owner tombstone not retained")
        if stage["stage"] == "child-confirmed":
            check_child_content(current[child], self.corpus, self.coalesced)
            require(current[owner] == self.tombstone, "Child edit changed retained tombstone")
            if self.coalesced:
                self.evidence["coalescedFinal"] = inspect_coalesced(current[child], self.corpus, self.prefix_history)
            require(current[child] != self.initial[child], "Independent child save did not change history")
            require(stage["geometry"].get("focus") is True, "Child focus did not return")
        active, archive = self.journal()
        if self.seed_ticket is not None: exact_custody(active, archive, self.seed_ticket)
        expected_active = stage["stage"] in ("delete-saving", "uncertain", "owner-omitted", "restored-locked", "refresh-locked", "review-original", "review-child", "review-current-child", "review-controls", "storage-failed")
        require(len(active) == int(expected_active), "Wrong durable lock count")
        if active:
            operation, body = active[0]
            checkpoint = json.loads(body)
            require(checkpoint["version"] == 1 and checkpoint["action"] == "Delete"
                    and checkpoint["target"] == owner and checkpoint["sequence"] == 1, "Wrong inert deletion checkpoint")
            source = {field(wire(bytes(s)), 1, 2).decode(): bytes(s) for s in checkpoint["sections"]}
            require(source[child] == current[child] and lineage(source[owner]) == lineage(self.initial[owner]), "Checkpoint lost original peer proof")
            if self.original_ticket is None: self.original_ticket = active
            require(active == self.original_ticket, "Exact durable receipt changed")
            require(stage["journal"]["active"] == {"operation": operation, "checkpoint": checkpoint}, "Native and SQLite receipt differ")
        else: require(stage["journal"]["active"] is None, "Native reports stale durable lock")
        require(stage["journal"]["archiveCount"] == len(archive), "Archive count differs")
        if archive:
            require(len(archive) == 1 and archive[0][2] == "local_unlock_outcome_unknown", "Archive falsely claims success")
            if self.original_ticket is not None: require([archive[0][:2]] == self.original_ticket, "Archive lost exact retained request")
        if self.phase in ("restart", "review", "storage-failure", "reviewed"):
            require(not stage["writes"] and stage["reservations"] == 0, "Restart replayed deletion")
        self.last_journal = (tuple(active), tuple(archive))
        self.evidence["observations"].append({"stage": stage["stage"], "active": len(active), "archive": len(archive)})
        if self.phase == "storage-failure" and stage["stage"] == "review-controls":
            with closing(sqlite3.connect(self.db)) as db:
                db.execute("CREATE TRIGGER fixture_owner_delete_review_failure BEFORE INSERT ON local_reviews BEGIN SELECT RAISE(FAIL, 'fixture archive failure'); END")
                db.commit()
            self.injected = True
    def exact_rows(self):
        # Preserve all row columns, including scope and timestamps, across exit.
        # Read-only connection and explicit transaction give one consistent view.
        with closing(sqlite3.connect(self.db.as_uri()+"?mode=ro", uri=True)) as db:
            db.execute("BEGIN")
            return {name: db.execute("SELECT * FROM "+name+" ORDER BY 1,2,3").fetchall()
                    for name in ("receipts", "local_reviews")}
    def final_custody(self, active, archive):
        if self.phase in ("seed", "unknown", "omitted", "restart", "storage-failure"):
            require(self.original_ticket is not None and len(self.original_ticket) == 1
                    and active == self.original_ticket and archive == [], "Final active custody differs")
        elif self.phase in ("review", "reviewed"):
            require(self.original_ticket is not None and len(self.original_ticket) == 1
                    and active == [] and archive == [(*self.original_ticket[0], "local_unlock_outcome_unknown")],
                    "Final archived custody differs")
        else:
            require(self.phase in ("success", "stale") and active == [] and archive == [],
                    "Final retired custody differs")
        if self.seed_ticket is not None: exact_custody(active, archive, self.seed_ticket)
    def finish(self, native):
        require(native is self.native and self.finished_journal is None, "Storage finish owner/order differs")
        active, archive = self.journal()
        self.final_custody(active, archive)
        current = (tuple(active), tuple(archive))
        require(self.last_journal is not None and current == self.last_journal,
                "Journal changed after final held stage")
        self.finished_journal = current
        self.finished_rows = self.exact_rows()
        self.evidence.update(active=len(active), archive=len(archive), finalRows=self.finished_rows)
    def validate_closed(self, native):
        require(native is self.native and native is not None and native.process is not None
                and native.process.poll() is not None, "Storage process has not closed")
        require(self.finished_journal is not None and self.finished_rows is not None,
                "Closed storage validation before finish")
        require(not self.injected, "Storage failure trigger cleanup incomplete")
        active, archive = self.journal()
        self.final_custody(active, archive)
        current = (tuple(active), tuple(archive))
        require(current == self.finished_journal, "Closed journal receipt content/count changed")
        rows = self.exact_rows()
        require(rows == self.finished_rows, "Closed journal scope or row content changed")
        self.evidence.update(closedRows=rows, closedJournalValidated=True,
                             closedProcessExitCode=native.process.poll())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", choices=(PREFIX, PREFIX+"-narrow"), default=PREFIX)
    parser.add_argument("--scenario", choices=("success", "stale", "unknown", "omitted", "recovery", "storage-recovery"), default="success")
    parser.add_argument("--corpus", choices=("ascii", "unicode"), default="ascii")
    parser.add_argument("--history", choices=("bootstrap", "retained", "retained-zero"), default="bootstrap")
    parser.add_argument("--coalesced", action="store_true", help="Exact-revert child history before initial Save; actual Bold after owner deletion")
    args = parser.parse_args()
    require(not args.coalesced or args.scenario == "success", "Coalesced variant is bounded to success")
    root, binary = args.captures.resolve(), args.binary.resolve()
    require(not root.exists(), "Refusing an existing evidence directory")
    root.mkdir(parents=True)
    phases = ("seed", "restart", "storage-failure", "review", "reviewed") if args.scenario == "storage-recovery" else (("seed", "restart", "review", "reviewed") if args.scenario == "recovery" else (args.scenario,))
    initial_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    receipts = []
    result = {"ok": False, "binarySha256": initial_hash, "coalesced": args.coalesced, "phases": receipts}
    try:
        prior_archive = None
        seed_ticket = None
        for phase in phases:
            spec = CanvasCheck(PREFIX, "SUPER_PLATINUM_CANVAS_OWNER_DELETE_CHECK", ("noHttp",), {}, STAGES[phase])
            control = Control(root, phase, args.corpus, args.history, seed_ticket, coalesced=args.coalesced)
            run(spec, args.fixture, binary, root / phase, storage_factory=control)
            active, archive = control.journal()
            require(len(active) == control.evidence["active"] and len(archive) == control.evidence["archive"], "Journal changed during native exit")
            if phase == "seed":
                require(control.original_ticket is not None and len(control.original_ticket) == 1,
                        "Seed did not retain one exact operation/body")
                seed_ticket = tuple(control.original_ticket[0])
                result["seedReceipt"] = {"operation": seed_ticket[0], "body": seed_ticket[1]}
            if seed_ticket is not None: exact_custody(active, archive, seed_ticket)
            if prior_archive is not None: require(archive == prior_archive, "Restart changed exact archived request")
            if archive: prior_archive = archive
            receipts.append(str((root / phase / (args.fixture+".json")).relative_to(root)))
        require(hashlib.sha256(binary.read_bytes()).hexdigest() == initial_hash, "Binary changed")
        result["ok"] = True
    finally:
        (root / "result.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")

if __name__ == "__main__": main()
