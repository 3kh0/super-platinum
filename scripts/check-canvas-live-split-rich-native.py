#!/usr/bin/env python3
"""Offline rich continuation of a saved single-split pair whose owner stays live."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
from canvas_native import CanvasCheck, run
from canvas_heading import wire, field
from native_fixture import DEFAULT_BINARY
from native_check import expected_viewport

_spec = importlib.util.spec_from_file_location("owner_delete_shared", Path(__file__).with_name("check-canvas-owner-delete-native.py"))
shared = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(shared)
require, rows, lineage, child_id, rtml, escaped = shared.require, shared.rows, shared.lineage, shared.child_id, shared.rtml, shared.escaped
PREFIX = "canvas-live-split-rich"
URL = "https://example.test/"


def stages(c):
    result = ["initial"] + (["link-form"] + (["link-error"] if c.get("linkError") else []) if c["format"] == "link" else []) + ["child-draft"]
    if c["phase"] == "stale": return tuple(result + ["stale"])
    result += ["child-saving", "child-confirmed", "owner-mapped"]
    if c["ownerAction"] == "delete": return tuple(result + ["owner-confirmation", "owner-saving", "owner-deleted"])
    if c["ownerAction"] == "edit": result += ["owner-draft", "owner-saving", "owner-confirmed"]
    return tuple(result + (["remove-link-form"] + (["remove-link-error"] if c.get("linkError") else []) if c["format"] == "link" else []) + ["remove-draft", "remove-saving", "remove-confirmed"])


def text(c):
    return shared.expected_text(c["corpus"]) + (" typed" if c["cadence"] == "typed" else "")


def expected_rich(c):
    value = escaped(text(c))
    return "<b>"+value+"</b>" if c["format"] == "bold" else '<a href="'+URL+'">'+value+'</a>'


def expected_mark(c, remove=False):
    return {"name": "b" if c["format"] == "bold" else "a", "value": False if remove else (True if c["format"] == "bold" else json.dumps({"href": URL}, separators=(",", ":"))), "start": 0, "end": len(escaped(text(c)).encode("utf-16-le"))//2}


def decode_session(raw):
    session = wire(raw)
    helper = Path(__file__).resolve().parents[1] / "tmp/canvas-split-owner-delete-20261024/desktop/coalesced/history.mjs"
    result = subprocess.run(["node", str(helper)], input=json.dumps({"document": base64.b64encode(field(session, 5, 2)).decode()}), text=True, encoding="utf-8", capture_output=True, timeout=30, cwd=Path(__file__).resolve().parents[1])
    require(result.returncode == 0, "Prefix decoder failed: "+result.stderr[-1000:])
    decoded = json.loads(result.stdout)
    processed = [f[2].decode() for f in session if f[0] == 6]
    require(decoded["heads"] == [decoded["changes"][-1]["hash"]] and decoded["heads"][0] in processed, "Saved history head differs")
    return decoded, processed


def decode_row(raw): return decode_session(field(wire(raw), 38, 2))


def deleted(raw):
    values = [f for f in wire(raw) if f[0] == 3]
    require(len(values) <= 1 and (not values or values[0][1] == 0 and values[0][2] in (0, 1)), "Invalid deleted flag")
    return bool(values and values[0][2])


def prefix_history(decoded, processed, c):
    changes = decoded["changes"]
    tail = escaped(shared.expected_text(c["corpus"]))
    expected = ["", tail, tail+" typed"] + ([tail] if c["cadence"] == "revert" else [])
    require(len(changes) == len(expected) and [s["text"] for s in changes] == [s+"\ufeff" for s in expected], "Wrong exact typed/revert prefix")
    require(all(s["deleted"] is False and s["marks"] == [] for s in changes), "Prefix format/deletion differs")
    require(changes[0]["deps"] == [] and all(s["deps"] == [changes[i-1]["hash"]] for i, s in enumerate(changes) if i), "Prefix is not linear")
    require(changes[1]["hash"] not in processed, "Extracted original tail became processed")
    return decoded


def history_step(previous, current, text, marks, deleted=False):
    before, after = previous["changes"], current["changes"]
    require(len(after) == len(before)+1 and after[:-1] == before, "Changed or lost exact historical prefix")
    last = after[-1]
    # Original reloads can retain redundant ancestor dependencies. Require a
    # direct edge to the previous final head and forbid foreign/duplicate deps;
    # exact prior bytes plus one appended change still prove one continuation.
    deps = last["deps"]
    require(before[-1]["hash"] in deps and len(deps) == len(set(deps))
            and set(deps).issubset({c["hash"] for c in before}), "Independent report has foreign/missing dependencies")
    require(last["text"] == text+"\ufeff" and last["marks"] == marks and last["deleted"] is deleted, "Wrong independent report state")
    return current


def link_geometry(g, name, config):
    remove = name in ("remove-link-form", "remove-link-error")
    invalid = name in ("link-error", "remove-link-error")
    require(not invalid or config.get("linkError") is True, "Unrequested invalid-link stage")
    expected = [".rich-link-url", ".rich-link-apply", ".rich-link-cancel"] + ([".rich-link-remove"] if remove else [])
    require(g.get("kind") == "link-form" and g.get("ok") is True and g.get("remove") is remove and g.get("invalid") is invalid, "Link stage state differs")
    require([c.get("selector") for c in g["controls"]] == expected, "Link control inventory differs")
    for control in g["controls"]:
        require(control.get("enabled") is True and control.get("visible") is True, "Disabled/invisible link control")
        shared.geometry({"ok": True, "rect": control["rect"], "clip": g["clip"], "viewport": g["viewport"]})
    expected_layout = [".rich-editor", "#canvas-text-draft", ".canvas-text-hint", ".canvas-text-actions", "#canvas-text-cancel", "#canvas-text-save"] + ([".rich-link-error"] if invalid else [])
    require(set(g.get("layout", {})) == set(expected_layout), "Link editor geometry inventory differs")
    for selector in expected_layout:
        item = g["layout"][selector]
        require(item.get("visible") is True and item.get("enabled") is True, "Hidden/disabled link editor element")
        shared.geometry({"ok": True, "rect": item["rect"], "clip": g["clip"], "viewport": g["viewport"]})
    rich, draft, hint = [g["layout"][k]["rect"] for k in (".rich-editor", "#canvas-text-draft", ".canvas-text-hint")]
    gap = 4 if g["viewport"][1] <= 500 else 8
    require(g.get("gap") == gap and g.get("separated") is True and draft[3] >= 47
            and draft[1]+draft[3] <= rich[1]+rich[3]+1 and hint[1] >= draft[1]+draft[3]+gap-1, "Draft overlaps rich container/hint or lost minimum height")
    require(g.get("focus") == "rich-link-url", "Link input focus lost")
    if invalid:
        require(g.get("unchangedDraft") is True and g.get("errorText") == "Enter an http, https, mailto or slack link without spaces or control characters.", "Invalid URL did not preserve draft with exact readable refusal")
    else:
        require(g.get("errorText") is None, "Unexpected link error")


class Control(shared.Control):
    def __init__(self, root, config):
        super().__init__(root, config["phase"], config["corpus"], config["history"], coalesced=True)
        self.config = config
        self.current = None
        self.child_history = None
        self.owner_history = None
        self.tail_head = None
        self.counts = {"writes": 0, "acks": 0}
        self.evidence["liveRich"] = config
        self.evidence["decodedHistories"] = []
    def prepare(self, directory, fixture, checks):
        self.expected_fixture = fixture
        super().prepare(directory, fixture, checks)
        checks.update(SUPER_PLATINUM_CANVAS_LIVE_SPLIT_FORMAT=self.config["format"], SUPER_PLATINUM_CANVAS_LIVE_SPLIT_CADENCE=self.config["cadence"], SUPER_PLATINUM_CANVAS_LIVE_SPLIT_OWNER_ACTION=self.config["ownerAction"])
        checks["SUPER_PLATINUM_CANVAS_LIVE_SPLIT_LINK_ERROR"] = "1" if self.config.get("linkError") else "0"
        return self
    def before_capture(self, native, stage):
        require(stage.get("liveRich") == self.config and stage.get("coalesced") is True, "Native fixture configuration differs")
        name, g = stage["stage"], stage["geometry"]
        require(name in stages(self.config), "Unexpected live rich stage")
        require(g.get("ok") is True and g.get("viewport") == expected_viewport(self.expected_fixture), "Failed native geometry/viewport")
        current = rows(stage["canonical"])
        owner = "temp:C:Dte0000000000000000000000001" if self.history == "retained-zero" else "paragraph"
        child = child_id((self.initial or current)[owner])
        if self.initial is None:
            require(name == "initial", "Missing initial pair")
            self.initial, self.current = current, current
            require(rtml(current[child]) == escaped(text(self.config)), "Initial canonical child differs")
            decoded, processed = decode_row(current[child])
            self.child_history = prefix_history(decoded, processed, self.config)
            self.tail_head = decoded["changes"][1]["hash"]
            self.evidence["decodedHistories"].append({"stage": name, "target": child, "history": decoded})
            if self.config["ownerAction"] != "none": self.owner_history = decode_row(current[owner])[0]
        require(set(current) == set(self.current), "Canonical row inventory changed")
        target = child if name in ("child-confirmed", "remove-confirmed") else owner if name in ("owner-confirmed", "owner-deleted") else None
        require(all(current[k] == self.current[k] for k in current if k != target), "Unreported other-row mutation")
        require(lineage(current[owner]) == lineage(self.initial[owner]), "Original split lineage changed")
        if target == child:
            remove = name == "remove-confirmed"
            require(rtml(current[child]) == (escaped(text(self.config)) if remove else expected_rich(self.config)), "Exact child canonical formatting differs")
            decoded, processed = decode_row(current[child])
            require(self.tail_head not in processed, "Original tail incorrectly processed")
            self.child_history = history_step(self.child_history, decoded, escaped(text(self.config)), [expected_mark(self.config, remove)])
            self.evidence["decodedHistories"].append({"stage": name, "target": child, "history": decoded})
        elif target == owner:
            delete = name == "owner-deleted"
            require(self.config["ownerAction"] == ("delete" if delete else "edit"), "Unrequested owner operation")
            left = "Caf\u00e9 & \u65e5\u672c\U0001f600" if self.corpus == "unicode" else "Keep this"
            desired = escaped(left + ("" if delete else " edited"))
            require(rtml(current[owner]) == desired, "Independent owner body differs")
            decoded, _ = decode_row(current[owner])
            self.owner_history = history_step(self.owner_history, decoded, desired, [], delete)
            require(rtml(current[child]) == expected_rich(self.config), "Owner operation changed rich child")
            self.evidence["decodedHistories"].append({"stage": name, "target": owner, "history": decoded})
        require(deleted(current[owner]) is (name == "owner-deleted"), "Owner live/tombstone state differs")
        if name in ("link-form", "remove-link-form", "link-error", "remove-link-error"):
            link_geometry(g, name, self.config)
        elif "controls" in g:
            shared.geometry(g)
            expected = ["Edit", "Add before", "Add after", "Delete"]
            require([c["text"].strip() for c in g["controls"]] == expected, "Exact live/tombstone paragraph action policy differs")
            expected_target = owner if name in ("owner-mapped", "owner-confirmed") else child
            require(g.get("id") == expected_target, "Geometry targets wrong identity")
            if name in ("child-confirmed", "remove-confirmed", "owner-confirmed", "owner-deleted"):
                require(g.get("focus") is True, "Focus did not return to operation target")
        active, archive = self.journal()
        pending_delete = name == "owner-saving" and self.config["ownerAction"] == "delete"
        require(len(active) == int(pending_delete) and archive == [], "Unexpected live rich durable custody")
        if pending_delete:
            operation, body = active[0]
            checkpoint = json.loads(body)
            require(checkpoint["version"] == 1 and checkpoint["action"] == "Delete" and checkpoint["target"] == owner, "Wrong owner deletion checkpoint")
            source = {field(wire(bytes(s)), 1, 2).decode(): bytes(s) for s in checkpoint["sections"]}
            require(source == current, "Owner deletion checkpoint lost exact rich peer/source")
            if self.original_ticket is None: self.original_ticket = active
            require(active == self.original_ticket and stage["journal"]["active"] == {"operation": operation, "checkpoint": checkpoint}, "Native/SQLite deletion custody differs")
        else: require(stage["journal"]["active"] is None, "Native reports unexpected lock")
        require(stage["journal"]["archiveCount"] == 0, "Unexpected archived outcome")
        if name in ("child-saving", "remove-saving", "owner-saving"): self.counts["writes"] += 1
        if name in ("child-confirmed", "remove-confirmed", "owner-confirmed", "owner-deleted"): self.counts["acks"] += 1
        require(len(stage["writes"]) == self.counts["writes"] and len(stage["acknowledgements"]) == self.counts["acks"], "Missing/duplicate dispatched operation or ACK")
        require(all(w.get("httpFactoryExecuted") is False for w in stage["writes"]), "HTTP factory executed")
        require(stage["reservations"] == int(self.config["ownerAction"] == "delete" and name in ("owner-saving", "owner-deleted")), "Unexpected durable reservation count")
        self.current = current
        self.last_journal = (tuple(active), tuple(archive))
        self.evidence["observations"].append({"stage": name, "active": len(active), "archive": len(archive), **self.counts})
    def finish(self, native):
        require(len(self.evidence["observations"]) == len(stages(self.config)), "Missing held live rich stages")
        terminal = native.receipts(PREFIX+"-check: ")
        require(len(terminal) == 1 and terminal[0].get("liveRich") == self.config and rows(terminal[0]["canonical"]) == self.current, "Final live rich source/configuration differs")
        super().finish(native)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", choices=(PREFIX, PREFIX+"-narrow"), default=PREFIX)
    parser.add_argument("--corpus", choices=("ascii", "unicode"), default="ascii")
    parser.add_argument("--history", choices=("bootstrap", "retained", "retained-zero"), default="bootstrap")
    parser.add_argument("--cadence", choices=("typed", "revert"), required=True)
    parser.add_argument("--format", choices=("bold", "link"), required=True)
    parser.add_argument("--owner-action", choices=("none", "edit", "delete"), default="none")
    parser.add_argument("--link-error", action="store_true", help="Capture actual invalid URL refusal before applying/removing a link")
    parser.add_argument("--scenario", choices=("success", "stale"), default="success")
    args = parser.parse_args()
    require(args.scenario != "stale" or args.owner_action == "none", "Stale case cannot execute owner action")
    c = dict(corpus=args.corpus, history=args.history, cadence=args.cadence, format=args.format, ownerAction=args.owner_action, phase=args.scenario)
    require(not args.link_error or args.format == "link", "Link error requires link format")
    if args.link_error: c["linkError"] = True
    root, binary = args.captures.resolve(), args.binary.resolve()
    require(not root.exists(), "Refusing existing evidence directory")
    root.mkdir(parents=True)
    checksum = hashlib.sha256(binary.read_bytes()).hexdigest()
    result = {"ok": False, "binarySha256": checksum, "liveRich": c}
    try:
        spec = CanvasCheck(PREFIX, "SUPER_PLATINUM_CANVAS_LIVE_SPLIT_RICH_CHECK", ("noHttp", "cancelNoWrite", "duplicateGuard"), {"stages": len(stages(c))}, stages(c))
        control = Control(root, c)
        run(spec, args.fixture, binary, root/args.scenario, storage_factory=control)
        result["receipt"] = str((root/args.scenario/(args.fixture+".json")).relative_to(root))
        require(hashlib.sha256(binary.read_bytes()).hexdigest() == checksum, "Binary changed")
        result["ok"] = True
    finally:
        (root/"result.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")

if __name__ == "__main__": main()
