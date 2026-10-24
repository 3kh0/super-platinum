#!/usr/bin/env python3
"""Audited offline rich continuation of a coalesced saved single-split owner."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tmp/canvas-coalesced-owner-rich-20261024/scripts"))
from owner_rich_oracle import Oracle, owner_plain, shared, rich, rows, stages, require
from canvas_native import CanvasCheck, run
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY

PREFIX = "canvas-coalesced-owner-rich"


class Control(shared.Control):
    def __init__(self,root,config,seed_ticket=None,restart_source=None):
        super().__init__(root,config["phase"],config["corpus"],config["history"],seed_ticket,coalesced=True)
        self.config,self.oracle,self.restart_source=config,Oracle(config),restart_source
        self.active_ticket=seed_ticket
        self.text_session=None;self.text_sequence=0;self.dispatches=set();self.structural_count=0
        self.evidence["ownerRich"]=config

    def prepare(self,directory,fixture,checks):
        self.expected_fixture=fixture
        super().prepare(directory,fixture,checks)
        checks.update(SUPER_PLATINUM_CANVAS_COALESCED_OWNER_CADENCE=self.config["parentCadence"],
                      SUPER_PLATINUM_CANVAS_COALESCED_OWNER_GAP="1" if self.config["gap"] else "0")
        return self

    def before_capture(self,native,stage):
        require(stage.get("ownerRich")==self.config and stage.get("coalesced") is True,"Wrong owner-rich fixture config")
        name=stage["stage"];require(name==stages(self.config)[len(self.evidence["observations"])],"Wrong held stage order")
        g=stage["geometry"]
        require(g.get("ok") is True and g.get("viewport")==expected_viewport(self.expected_fixture),"Failed native geometry/viewport")
        shared.geometry(g)
        if "bounds" in g:require(g["bounds"] and all(x.get("visible") is True for x in g["bounds"]),"Clipped editor control")
        if self.phase=="restart":
            require(self.restart_source is not None and rows(stage["canonical"])==self.restart_source,"Restart changed exact canonical rows")
            require(stage["writes"]==[] and stage["acknowledgements"]==[] and stage["reservations"]==0,"Restart replayed a write")
            if name in ("review-original","review-child","review-current-child"):
                section=name.removeprefix("review-")
                text=owner_plain(self.config) if section=="original" else shared.expected_text(self.corpus)
                require(g.get("section")==section and g.get("text")==text,"Recovery preview lost original rich owner or preserved child")
            if name=="review-controls":
                require(g.get("acknowledged") is False and g.get("cancelEnabled") is True and g.get("removeDisabled") is True,"Review acknowledged/unlocked")
        else:
            before=len(self.oracle.writes);self.oracle.observe(stage)
            if name=="initial" and self.config["parentCadence"]=="erase":
                require(g.get("emptyContent") is True and g.get("emptyLabel") is None,"Erased owner row is not the actual blank reader representation")
            if len(self.oracle.writes)>before:
                w=self.oracle.writes[-1];identity=w["session"],w["sequence"]
                require(identity not in self.dispatches,"Replayed actor/sequence");self.dispatches.add(identity)
                if "checkpoint" in w:self.structural_count+=1
                else:
                    if self.text_session is None:self.text_session=w["session"]
                    require(w["session"]==self.text_session and w["sequence"]==self.text_sequence+1,"Text actor/counter reset")
                    self.text_sequence=w["sequence"]
            require(stage["reservations"]==self.structural_count,"Unexpected durable reservation")
            if name in ("link-form","remove-link-form"):rich.link_geometry(g,name,self.config)
            elif "controls" in g:
                target=self.oracle.child if name in ("child-mapped","child-later-confirmed") else self.oracle.gap if name in ("gap-mapped","gap-later-confirmed") else self.oracle.owner
                require(g.get("id")==target,"Geometry targets wrong row")
                require([x["text"].strip() for x in g["controls"]]==["Edit","Add before","Add after","Delete"],"Exact mapped action policy differs")
                if name.endswith("-confirmed"):require(g.get("focus") is True,"Focus did not return to saved target")
        active,archive=self.journal();require(archive==[],"Review unexpectedly archived/unlocked")
        structural_save=self.oracle.pending is not None and self.oracle.pending["kind"]=="delete"
        locked=self.phase=="restart" or name=="unknown" or structural_save
        require(len(active)==int(locked),"Wrong exact durable custody count")
        if active:
            operation,body=active[0];cp=json.loads(body)
            if structural_save:
                require(cp==self.oracle.pending["write"]["checkpoint"],"Journal changed exact dispatched body")
                require(self.active_ticket is None,"Second structural claim")
                self.active_ticket=(operation,body)
            require((operation,body)==self.active_ticket,"Operation/body changed across hold/restart")
            require(stage["journal"]["active"]=={"operation":operation,"checkpoint":cp},"Native/SQLite custody differs")
        else:require(stage["journal"]["active"] is None,"Native retains unexpected lock")
        require(stage["journal"]["archiveCount"]==0,"Native archive count differs")
        self.original_ticket=[self.active_ticket] if self.active_ticket else None
        self.last_journal=(tuple(active),tuple(archive))
        self.evidence["observations"].append({"stage":name,"active":len(active),"archive":len(archive),"writes":len(stage["writes"]),"acks":len(stage["acknowledgements"])})

    def finish(self,native):
        require(len(self.evidence["observations"])==len(stages(self.config)),"Missing held stage")
        terminal=native.receipts(PREFIX+"-check: ")
        expected=self.restart_source if self.phase=="restart" else self.oracle.current
        require(len(terminal)==1 and terminal[0].get("ownerRich")==self.config and rows(terminal[0]["canonical"])==expected,"Terminal source/config differs")
        self.evidence["decodedHistories"]=self.oracle.histories
        super().finish(native)


def parse_args(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary",type=Path,default=DEFAULT_BINARY)
    p.add_argument("--captures",type=Path,required=True)
    p.add_argument("--fixture",choices=(PREFIX,PREFIX+"-narrow"),default=PREFIX)
    p.add_argument("--corpus",choices=("ascii","unicode"),default="ascii")
    p.add_argument("--history",choices=("bootstrap","retained","retained-zero"),default="bootstrap")
    p.add_argument("--parent-cadence",choices=("correction","revert","erase"),required=True)
    p.add_argument("--gap",action="store_true")
    p.add_argument("--scenario",choices=("success","stale","unknown","recovery"),default="success")
    return p.parse_args(argv)


def main(argv=None):
    a=parse_args(argv);root,binary=a.captures.resolve(),a.binary.resolve()
    require(not root.exists(),"Refusing existing evidence directory");require(binary.is_file(),"Missing binary");root.mkdir(parents=True)
    checksum=hashlib.sha256(binary.read_bytes()).hexdigest()
    phases=("unknown","restart") if a.scenario=="recovery" else (a.scenario,)
    result={"ok":False,"binarySha256":checksum,"receipts":[]};ticket=snapshot=None
    try:
        for phase in phases:
            config=dict(corpus=a.corpus,history=a.history,parentCadence=a.parent_cadence,format="bold" if a.corpus=="ascii" else "link",gap=a.gap,phase=phase)
            control=Control(root,config,ticket,snapshot)
            spec=CanvasCheck(PREFIX,"SUPER_PLATINUM_CANVAS_COALESCED_OWNER_RICH_CHECK",("noHttp","cancelNoWrite","duplicateGuard"),{"stages":len(stages(config))},stages(config))
            run(spec,a.fixture,binary,root/phase,storage_factory=control)
            result["receipts"].append(str((root/phase/(a.fixture+".json")).relative_to(root)))
            if phase=="unknown":
                require(control.active_ticket is not None,"Unknown lost exact durable owner")
                ticket,snapshot=control.active_ticket,control.oracle.current
                result["seedReceipt"]={"operation":ticket[0],"body":ticket[1]}
            require(hashlib.sha256(binary.read_bytes()).hexdigest()==checksum,"Binary changed")
        result["ok"]=True
    finally:(root/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")


if __name__=="__main__":main()
