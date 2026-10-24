#!/usr/bin/env python3
"""Audited offline deletion of a saved single-split child with its owner retained."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tmp/canvas-live-split-child-delete-20261024/scripts"))
from child_delete_oracle import Oracle, config, stages, shared, rich, rows, require
from canvas_native import CanvasCheck, run
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY

PREFIX = "canvas-live-split-child-delete"


class Control(shared.Control):
    def __init__(self, root, c, seed_ticket=None, restart_source=None):
        super().__init__(root,c["phase"],c["corpus"],c["history"],seed_ticket,coalesced=c["variant"] != "plain")
        self.config,self.oracle,self.restart_source = c,Oracle(c),restart_source
        self.active_ticket = seed_ticket
        self.text_session = None
        self.text_sequence = self.structural_count = 0
        self.dispatches = set()
        self.evidence["childDelete"] = c

    def prepare(self,directory,fixture,checks):
        self.expected_fixture = fixture
        super().prepare(directory,fixture,checks)
        checks["SUPER_PLATINUM_CANVAS_CHILD_DELETE_VARIANT"] = self.config["variant"]
        return self

    def before_capture(self,native,stage):
        require(stage.get("childDelete") == self.config and stage.get("coalesced") is self.coalesced, "Wrong child-delete fixture config")
        name = stage["stage"]
        require(name == stages(self.config)[len(self.evidence["observations"])], "Wrong held stage order")
        g = stage["geometry"]
        require(g.get("ok") is True and g.get("viewport") == expected_viewport(self.expected_fixture), "Failed exact native geometry/viewport")
        shared.geometry(g)
        if "bounds" in g: require(g["bounds"] and all(x.get("visible") is True for x in g["bounds"]), "Clipped editor control")
        if self.phase == "restart":
            require(self.restart_source is not None and rows(stage["canonical"]) == self.restart_source, "Restart replaced exact retained/omitted source")
            require(stage["writes"] == [] and stage["acknowledgements"] == [] and stage["reservations"] == 0, "Restart replayed request")
            if name == "review-draft":
                require(g.get("section") == "draft" and g.get("text") == shared.expected_text(self.corpus), "Review lost original child draft")
            if name == "review-controls":
                require(g.get("acknowledged") is False and g.get("cancelEnabled") is True and g.get("removeDisabled") is True, "Review acknowledged/unlocked")
        else:
            count = len(self.oracle.writes)
            self.oracle.observe(stage)
            if len(self.oracle.writes) > count:
                w = self.oracle.writes[-1]; identity = w["session"],w["sequence"]
                require(identity not in self.dispatches, "Replayed actor/sequence"); self.dispatches.add(identity)
                if "checkpoint" in w: self.structural_count += 1
                else:
                    if self.text_session is None: self.text_session = w["session"]
                    require(w["session"] == self.text_session and w["sequence"] == self.text_sequence + 1, "Text session/counter reset")
                    self.text_sequence = w["sequence"]
            require(stage["reservations"] == self.structural_count, "Wrong durable reservation count")
            if name == "link-form": rich.link_geometry(g,name,self.config)
            elif "controls" in g:
                target = self.oracle.child if name in ("initial","cancelled") else self.oracle.added if name == "add-confirmed" else self.oracle.owner
                expected = ["Edit","Add before","Add after","Delete"]
                require(g.get("id") == target and [x["text"].strip() for x in g["controls"]] == expected, "Wrong exact mapped target/action policy")
                if name == "add-confirmed":
                    require(g.get("returnFocusId") == self.oracle.owner and g.get("returnFocus") is True, "Add did not return focus to original anchor")
                elif name in ("cancelled","deleted","owner-confirmed","format-confirmed"):
                    require(g.get("focus") is True, "Focus did not return to intended row")
        active,archive = self.journal()
        require(archive == [], "Unexpected archive/unlock")
        pending = self.oracle.pending
        structural_save = pending is not None and pending["kind"] in ("delete","insert")
        locked = self.phase == "restart" or name in ("unknown","omitted") or structural_save
        require(len(active) == int(locked), "Wrong exact durable custody count")
        if active:
            operation,body = active[0]; cp = json.loads(body)
            if structural_save:
                require(cp == pending["write"]["checkpoint"] and self.active_ticket is None, "Changed dispatched body or concurrent durable claim")
                self.active_ticket = (operation,body)
            require((operation,body) == self.active_ticket, "Durable operation/body changed across hold/restart")
            require(stage["journal"]["active"] == {"operation":operation,"checkpoint":cp}, "Native/SQLite custody differs")
        else:
            require(stage["journal"]["active"] is None, "Native retained unexpected lock")
            self.active_ticket = None
        require(stage["journal"]["archiveCount"] == 0, "Archive count differs")
        self.original_ticket = [self.active_ticket] if self.active_ticket else None
        self.last_journal = (tuple(active),tuple(archive))
        self.evidence["observations"].append(dict(stage=name,active=len(active),archive=len(archive),writes=len(stage["writes"]),acks=len(stage["acknowledgements"])))

    def finish(self,native):
        require(len(self.evidence["observations"]) == len(stages(self.config)), "Missing held stage")
        terminal = native.receipts(PREFIX + "-check: ")
        expected = self.restart_source if self.phase == "restart" else self.oracle.current
        require(len(terminal) == 1 and terminal[0].get("childDelete") == self.config and rows(terminal[0]["canonical"]) == expected, "Terminal source/config differs")
        if self.phase == "restart": require(terminal[0].get("noReplay") is True, "Restart did not attest inert review")
        self.evidence["decodedHistories"] = self.oracle.histories
        super().finish(native)


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary",type=Path,default=DEFAULT_BINARY)
    p.add_argument("--captures",type=Path,required=True)
    p.add_argument("--fixture",choices=(PREFIX,PREFIX+"-narrow"),default=PREFIX)
    p.add_argument("--variant",choices=("plain","revert","rich-gap"),required=True)
    p.add_argument("--scenario",choices=("success","stale","unknown","omitted","unknown-restart","omitted-restart"),default="success")
    return p.parse_args(argv)


def main(argv=None):
    a = parse_args(argv); root,binary = a.captures.resolve(),a.binary.resolve()
    require(not root.exists(), "Refusing existing evidence directory"); require(binary.is_file(), "Missing binary")
    root.mkdir(parents=True)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    phases = (a.scenario.removesuffix("-restart"),"restart") if a.scenario.endswith("-restart") else (a.scenario,)
    result = dict(ok=False,binarySha256=digest,receipts=[])
    ticket = snapshot = None
    try:
        for phase in phases:
            c = config(a.variant,phase); control = Control(root,c,ticket,snapshot)
            spec = CanvasCheck(PREFIX,"SUPER_PLATINUM_CANVAS_LIVE_SPLIT_CHILD_DELETE_CHECK",("noHttp","cancelNoWrite","duplicateGuard"),{"stages":len(stages(c))},stages(c))
            run(spec,a.fixture,binary,root/phase,storage_factory=control)
            result["receipts"].append(str((root/phase/(a.fixture+".json")).relative_to(root)))
            if phase in ("unknown","omitted"):
                require(control.active_ticket is not None, "Unconfirmed outcome lost exact custody")
                ticket,snapshot = control.active_ticket,control.oracle.current
                result["seedReceipt"] = dict(operation=ticket[0],body=ticket[1])
            require(hashlib.sha256(binary.read_bytes()).hexdigest() == digest, "Binary changed")
        result["ok"] = True
    finally:
        (root/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")


if __name__ == "__main__": main()
