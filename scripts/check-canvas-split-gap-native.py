#!/usr/bin/env python3
"""Audited offline insertion/edit/delete of independent saved split gap rows."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tmp/canvas-split-gap-20261024/scripts"))
from gap_oracle import Oracle, expected_content, plain, rows, shared, stages, require
from canvas_native import CanvasCheck, run
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY

PREFIX = "canvas-split-gap"


def validate_geometry(g, fixture):
    require(g.get("ok") is True and g.get("viewport") == expected_viewport(fixture), "Wrong native geometry/viewport")
    if isinstance(g.get("clip"), list) and "rect" in g:
        shared.geometry(g)
    if "bounds" in g:
        require(g["bounds"] and all(x.get("visible") is True for x in g["bounds"]), "Editor element clipped")


class Control(shared.Control):
    def __init__(self, root, config, seed_ticket=None, restart_source=None):
        super().__init__(root, config["phase"], config["corpus"], config["history"], seed_ticket, coalesced=True)
        self.config = config
        self.oracle = Oracle(config)
        self.restart_source = restart_source
        self.tickets = []
        self.active_ticket = seed_ticket
        self.structural_count = 0
        self.text_session = None
        self.text_sequence = 0
        self.dispatches = set()
        self.evidence["gap"] = config
        self.evidence["checkpoints"] = []

    def prepare(self, directory, fixture, checks):
        self.expected_fixture = fixture
        super().prepare(directory, fixture, checks)
        checks.update(SUPER_PLATINUM_CANVAS_SPLIT_GAP_SIDE=self.config["side"],
                      SUPER_PLATINUM_CANVAS_SPLIT_GAP_COUNT=str(self.config["gaps"]),
                      SUPER_PLATINUM_CANVAS_SPLIT_GAP_RICH=self.config["rich"],
                      SUPER_PLATINUM_CANVAS_SPLIT_GAP_OWNER_DELETE="1" if self.config["ownerDelete"] else "0")
        return self

    def before_capture(self, native, stage):
        require(stage.get("gap") == self.config and stage.get("coalesced") is True, "Gap fixture configuration differs")
        name = stage["stage"]
        require(name == stages(self.config)[len(self.evidence["observations"])], "Unexpected stage order")
        validate_geometry(stage["geometry"], self.expected_fixture)
        previous_writes = len(self.oracle.writes)
        if self.phase == "restart":
            require(self.restart_source is not None and rows(stage["canonical"]) == self.restart_source, "Restart changed exact saved source")
            require(stage["writes"] == [] and stage["acknowledgements"] == [] and stage["reservations"] == 0, "Restart replayed a mutation")
            g = stage["geometry"]
            if name in ("review-intended", "review-observed"):
                section = "intended" if name == "review-intended" else "observed"
                text = plain(self.config, "gap", 1) if section == "intended" else plain(self.config, "owner" if self.config["side"] == "after-owner" else "child")
                require(g.get("section") == section and g.get("text") == text, "Recovery preview confused intended change and current anchor")
            if name == "review-controls":
                require(g.get("acknowledged") is False and g.get("cancelEnabled") is True and g.get("removeDisabled") is True,
                        "Inert review acknowledged/unlocked or disabled Cancel")
        else:
            self.oracle.observe(stage)
            if len(self.oracle.writes) != previous_writes:
                write = self.oracle.writes[-1]
                identity = write["session"], write["sequence"]
                require(identity not in self.dispatches, "Session/sequence was replayed")
                self.dispatches.add(identity)
                if "checkpoint" in write:
                    self.structural_count += 1
                else:
                    if self.text_session is None: self.text_session = write["session"]
                    require(write["session"] == self.text_session and write["sequence"] == self.text_sequence + 1,
                            "Independent text session/counter reset or alternate actor")
                    self.text_sequence = write["sequence"]
            require(stage["reservations"] == self.structural_count, "Structural reservation count differs")
            g = stage["geometry"]
            if "controls" in g:
                expected = ["Edit", "Add before", "Add after", "Delete"]
                require([x["text"].strip() for x in g["controls"]] == expected, "Wrong exact row action inventory")
                if name == "initial" or name == "owner-mapped" or name == "owner-edit-confirmed": target = self.oracle.owner
                elif name == "child-mapped" or name in ("child-edit-confirmed", "owner-deleted"): target = self.oracle.child
                elif name == "gap-2-deleted": target = self.oracle.gaps[1]
                elif name.startswith("gap-"): target = self.oracle.gaps[int(name[4])]
                else: target = None
                require(target is not None and g.get("id") == target, "Geometry targets wrong row")
                if re.fullmatch(r"gap-[123]-confirmed", name):
                    anchor = self.oracle.owner if self.config["side"] == "after-owner" else self.oracle.child
                    require(g.get("returnFocusId") == anchor and g.get("returnFocus") is True,
                            "Insertion did not return focus to its original anchor")
                elif name.endswith("-confirmed"):
                    require(g.get("focus") is True, "Target focus did not return")
        active, archive = self.journal()
        require(archive == [], "Inert review unexpectedly archived/unlocked request")
        structural_save = self.oracle.pending is not None and self.oracle.pending["kind"] in ("insert", "delete")
        locked = self.phase == "restart" or name == "unknown" or structural_save
        require(len(active) == int(locked), "Wrong durable custody count")
        if active:
            operation, body = active[0]
            cp = json.loads(body)
            if structural_save:
                require(cp == self.oracle.pending["write"]["checkpoint"], "Durable body differs from exact prepared write")
                ticket = (operation, body)
                require(ticket not in self.tickets, "Operation receipt reused")
                self.tickets.append(ticket)
                self.active_ticket = ticket
                self.evidence["checkpoints"].append({"operation": operation, "checkpoint": cp})
            require((operation, body) == self.active_ticket, "Active exact operation/body changed")
            require(stage["journal"]["active"] == {"operation": operation, "checkpoint": cp}, "Native and SQLite custody differ")
        else:
            require(stage["journal"]["active"] is None, "Native retained a retired lock")
        require(stage["journal"]["archiveCount"] == 0, "Native archive count differs")
        self.original_ticket = [self.active_ticket] if self.active_ticket is not None else None
        self.last_journal = (tuple(active), tuple(archive))
        self.evidence["observations"].append({"stage": name, "active": len(active), "archive": len(archive),
                                               "writes": len(stage["writes"]), "acks": len(stage["acknowledgements"])})

    def finish(self, native):
        require(len(self.evidence["observations"]) == len(stages(self.config)), "Missing held gap stages")
        terminal = native.receipts(PREFIX + "-check: ")
        expected = self.restart_source if self.phase == "restart" else self.oracle.current
        require(len(terminal) == 1 and terminal[0].get("gap") == self.config and rows(terminal[0]["canonical"]) == expected,
                "Terminal configuration/canonical differs")
        self.evidence["decodedHistories"] = self.oracle.histories
        super().finish(native)


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    p.add_argument("--captures", type=Path, required=True)
    p.add_argument("--fixture", choices=(PREFIX, PREFIX + "-narrow"), default=PREFIX)
    p.add_argument("--corpus", choices=("ascii", "unicode"), default="ascii")
    p.add_argument("--history", choices=("bootstrap", "retained", "retained-zero"), default="bootstrap")
    p.add_argument("--side", choices=("after-owner", "before-child"), required=True)
    p.add_argument("--gaps", type=int, choices=(1, 3), default=1)
    p.add_argument("--rich", choices=("none", "bold", "link"), default="none")
    p.add_argument("--owner-delete", action="store_true")
    p.add_argument("--scenario", choices=("success", "stale", "unknown", "recovery"), default="success")
    a = p.parse_args(argv)
    require(not a.owner_delete or a.gaps == 3 and a.scenario == "success", "Owner deletion requires repeated-gap success")
    require(a.scenario == "success" or a.gaps == 1 and a.rich == "none", "Refusal/restart bounded to first plain gap")
    return a


def main(argv=None):
    a = parse_args(argv)
    root, binary = a.captures.resolve(), a.binary.resolve()
    require(not root.exists(), "Refusing existing evidence directory")
    require(binary.is_file(), "Native binary missing")
    root.mkdir(parents=True)
    checksum = hashlib.sha256(binary.read_bytes()).hexdigest()
    phases = ("unknown", "restart") if a.scenario == "recovery" else (a.scenario,)
    result = {"ok": False, "binarySha256": checksum, "receipts": []}
    ticket = snapshot = None
    try:
        for phase in phases:
            config = dict(corpus=a.corpus, history=a.history, side=a.side, gaps=a.gaps, rich=a.rich, ownerDelete=a.owner_delete, phase=phase)
            control = Control(root, config, ticket, snapshot)
            spec = CanvasCheck(PREFIX, "SUPER_PLATINUM_CANVAS_SPLIT_GAP_CHECK", ("noHttp", "cancelNoWrite", "duplicateGuard"), {"stages": len(stages(config))}, stages(config))
            run(spec, a.fixture, binary, root / phase, storage_factory=control)
            result["receipts"].append(str((root / phase / (a.fixture + ".json")).relative_to(root)))
            if phase == "unknown":
                require(control.active_ticket is not None, "Unknown result lost exact durable operation")
                ticket, snapshot = control.active_ticket, control.oracle.current
                result["seedReceipt"] = {"operation": ticket[0], "body": ticket[1]}
            require(hashlib.sha256(binary.read_bytes()).hexdigest() == checksum, "Binary changed")
        result["ok"] = True
    finally:
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
