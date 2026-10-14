"""Pure receipt rejection tests. Synthetic test dictionaries are not native evidence."""
import copy
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-draft-update-recovery-native.py")))


class Receipts(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.db = self.root / "draft-update-recovery.sqlite"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, scenario="positive-channel", fixture=None):
        fixture = fixture or R["FIXTURE"] + "-narrow"
        target = "thread" if scenario.endswith("thread") else "channel"
        viewport = expected_viewport(fixture)
        def op(which):
            base = R["remote"](which, R["BASE"], "10.000001")
            return dict(id="01234567-89ab-4def-8123-456789abcdef", scope=R["scope"](which),
                client_msg_id=base["client_msg_id"], local_revision=1, kind="Update",
                snapshot=R["snapshot"](R["REQUESTED"], which), base=base, consume_id=None)
        def row(which):
            operation = op(which)
            return dict(scope=R["scope"](which), canonical=dict(composer=dict(text=R["REQUESTED"], reply_broadcast=which=="thread"), attachments=[]),
                blocked=None, alternates=[], stored=dict(revision=1,state=dict(scope=R["scope"](which),
                client_msg_id=operation["client_msg_id"], desired=operation["snapshot"], local_revision=1,
                acknowledged_local_revision=0, acknowledged=operation["base"], workflow={"Uncertain":operation}, check_ticket=None, check_deadline=None)))
        operation = op(target)
        initial = {which: row(which) for which in ("channel", "thread")}
        held = copy.deepcopy(initial)
        state = held[target]["stored"]["state"]
        held[target]["stored"]["revision"] = 2
        state["check_ticket"] = dict(id="11234567-89ab-4def-8123-456789abcdef", operation=operation)
        state["check_deadline"] = dict(operation_id=operation["id"], operation=operation, until_ms=1030000)
        def typing(inventory):
            inventory[target]["canonical"]["composer"]["text"] = R["NEWER"]
            inventory[target]["stored"]["state"].update(desired=R["snapshot"](R["NEWER"], target),local_revision=2)
        if scenario == "typing-before-channel":
            typing(held)
        final = copy.deepcopy(held)
        unresolved = scenario in ("cooldown-channel", "pending-check-channel")
        if scenario != "pending-check-channel":
            final[target]["stored"]["revision"] = 3
            state = final[target]["stored"]["state"]
            state["check_ticket"] = None
            if scenario == "cooldown-channel":
                state["check_deadline"]["until_ms"] = 1120000
            else:
                state.update(workflow="Idle", acknowledged_local_revision=1,
                             acknowledged=R["remote"](target,R["REQUESTED"],"12.000001"))
        if scenario == "typing-during-commit-thread":
            typing(final)
        query=("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
               "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
               "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
               "&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        raw=dict(ok=True,draft=dict(id=operation["base"]["id"],client_msg_id=operation["client_msg_id"],last_updated_ts="12.000001",
            team_id="T1",user_id="U00000001",is_deleted=False,is_sent=False,date_scheduled=0,**R["snapshot"](R["REQUESTED"],target)))
        request=dict(method="POST",url="https://fixture.slack.com/api/drafts.info?"+query,path="/api/drafts.info",rawQuery=query,
            fields=[["draft_id",operation["base"]["id"]]],retrySafe=False,operation=operation,delivered=scenario!="pending-check-channel",
            rawResponse=None if scenario=="cooldown-channel" else raw,outcome="rate-limited" if scenario=="cooldown-channel" else "positive",
            nowMs=1000000,retryAfterSeconds=120 if scenario=="cooldown-channel" else None)
        value=dict(ok=True,fixture=fixture,nonce=self.nonce,pid=self.pid,scenario=scenario,phase="exercise",isolatedDatabase=str(self.db),
            target=target,noSession=True,noTransport=True,noProductionRoute=True,syntheticInput=True,actualPreparedRequests=True,
            canonicalMessagesUnchanged=True,preparedRequests=1,attempts=[request],routedInputs=int(scenario.startswith("typing-")),
            operation=operation,initial=initial,durable=final,details=dict(held=held),geometry=[],captures={},viewport=viewport)
        if scenario != "pending-check-channel":
            value["details"]["afterReceipt"] = copy.deepcopy(final)
        text = R["NEWER"] if scenario.startswith("typing-") else R["REQUESTED"]
        def native():
            return dict(ok=True,text=text,display=text,scope="fixture-scope",visit="1",codePoints=list(map(ord,text)),
                utf16Length=len(text.encode("utf-16-le"))//2,start=2,end=5,direction="backward",composing=False,
                rect=dict(left=10,top=100,right=210,bottom=150,width=200,height=50),viewport=viewport)
        def geometry_row(disabled=False):
            return dict(ok=True,kind="update",operation=operation["id"],text="Update uncertain Check",buttonText="Check",
                disabled=disabled,hit=True,threadPaneAbsent=True,rowUnobscured=True,buttonUnobscured=True,
                row=dict(x=10,y=100,width=220,height=100),button=dict(x=10,y=160,width=80,height=30),
                panel=dict(x=0,y=70,width=300,height=250),viewport=viewport,documentWidth=viewport[0])
        for stage in R["stages"](scenario,"exercise"):
            entry=geometry_row(stage=="checking" or (stage=="result" and scenario=="pending-check-channel")) if stage in ("initial","checking") or (stage=="result" and unresolved) else native()
            value["geometry"].append(dict(entry,stage=stage))
        if scenario.startswith("typing-"):
            value["details"]["input"]=native()
        if not unresolved:
            value["details"]["native"]=native()
        if scenario=="owner-rotation-channel":
            value["details"]["ownerRotation"]=dict(beforeEpoch=3,afterEpoch=4,managedBefore=2,managedAfter=2)
        self.artifacts(value)
        return value

    def artifacts(self,value):
        value["captures"]={}
        for stage in R["stages"](value["scenario"],value["phase"]):
            path=self.root/f"{value['fixture']}-{value['scenario']}-{value['phase']}-{stage}.png"
            path.write_bytes(png())
            value["captures"][stage]=str(path)

    def restart(self,exercise):
        value=copy.deepcopy(exercise)
        value.update(phase="restart",preparedRequests=0,attempts=[],routedInputs=0,initial=copy.deepcopy(value["durable"]))
        last=copy.deepcopy(value["geometry"][-1]);last["stage"]="restart"
        value["geometry"]=[last]
        if value["scenario"] in ("cooldown-channel","pending-check-channel"):
            last.update(disabled=False,text="Previous check unresolved" if value["scenario"]=="pending-check-channel" else "Check cooldown")
            value["details"]={"restartRefusal":copy.deepcopy(last)}
        else:
            value["operation"]=None
            value["details"]={"native":copy.deepcopy(last)}
        self.artifacts(value)
        return value

    def validate(self,value):
        return R["validate"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.db,value)

    def rejects(self,value):
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_all_cases_both_sizes_and_restart(self):
        for fixture in R["FIXTURES"]:
            for scenario in R["SCENARIOS"]:
                with self.subTest(fixture=fixture,scenario=scenario):
                    value=self.good(scenario,fixture)
                    self.validate(value)
                    self.validate(self.restart(value))

    def test_request_route_form_defaults_and_single_dispatch(self):
        for field,bad in (("method","GET"),("path","/api/drafts.update"),("fields",[["draft_id","wrong"]]),
                          ("retrySafe",True),("rawQuery",""),("url","https://fixture.slack.com/api/drafts.info")):
            value=self.good();value["attempts"][0][field]=bad;self.rejects(value)
        value=self.good();value["attempts"]*=2;self.rejects(value)

    def test_raw_readback_identity_version_content(self):
        for field,bad in (("id","wrong"),("last_updated_ts","10.000001"),("client_msg_id","wrong"),("blocks",[]),("is_sent",True)):
            value=self.good();value["attempts"][0]["rawResponse"]["draft"][field]=bad;self.rejects(value)

    def test_ticket_operation_and_held_response(self):
        value=self.good("pending-check-channel");value["attempts"][0]["delivered"]=True;self.rejects(value)
        value=self.good();value["details"]["held"]["channel"]["stored"]["state"]["check_ticket"]=None;self.rejects(value)
        value=self.good("pending-check-channel");value["durable"]["channel"]["stored"]["revision"]+=1;self.rejects(value)

    def test_strict_flags_numbers_and_scopes(self):
        for field,bad in (("preparedRequests",True),("pid",True),("noTransport",1),("noProductionRoute",False)):
            value=self.good();value[field]=bad;self.rejects(value)
        value=self.good();value["operation"]["local_revision"]=True;self.rejects(value)

    def test_revision_and_newer_typing(self):
        for field,bad in (("local_revision",1),("acknowledged_local_revision",2),("desired",None)):
            value=self.good("typing-before-channel");value["durable"]["channel"]["stored"]["state"][field]=bad;self.rejects(value)

    def test_native_unicode_selection_and_owner(self):
        value=self.good();value["details"]["native"]["utf16Length"]-=1;self.rejects(value)
        value=self.good("typing-during-commit-thread");value["details"]["native"]["visit"]="2";self.rejects(value)
        value=self.good("owner-rotation-channel");value["details"]["ownerRotation"]["managedAfter"]=3;self.rejects(value)

    def test_restart_no_request_or_mutation(self):
        for scenario in R["SCENARIOS"]:
            value=self.restart(self.good(scenario));value["attempts"]=[{}];self.rejects(value)
        value=self.restart(self.good());value["initial"]["thread"]["canonical"]["composer"]["text"]="changed";self.rejects(value)

    def test_cooldown_bound_and_refusal_copy(self):
        value=self.good("cooldown-channel");value["durable"]["channel"]["stored"]["state"]["check_deadline"]["until_ms"]=1030000;self.rejects(value)
        value=self.restart(self.good("pending-check-channel"));value["details"]["restartRefusal"]["text"]="Done";self.rejects(value)

    def test_geometry_boolean_nan_and_clipping(self):
        for bad in (True,float("nan"),-20,100000):
            value=self.good();value["geometry"][0]["row"]["y"]=bad;self.rejects(value)
        value=self.good();value["geometry"][-1]["rect"]["bottom"]=1000;self.rejects(value)

    def test_row_visibility_requires_each_exact_true_proof(self):
        for field in ("threadPaneAbsent","rowUnobscured","buttonUnobscured"):
            for bad in (False,None,1,"true","missing"):
                for location in ("initial","checking","restart-refusal"):
                    value=self.good("pending-check-channel")
                    if location=="restart-refusal":
                        value=self.restart(value);row=value["details"]["restartRefusal"]
                    else:
                        row=next(g for g in value["geometry"] if g["stage"]==location)
                    if bad=="missing":del row[field]
                    else:row[field]=bad
                    with self.subTest(field=field,bad=bad,location=location),self.assertRaisesRegex(RuntimeError,field):
                        self.validate(value)

    def test_whole_row_and_button_must_fit_panel_on_both_axes(self):
        # These rectangles remain inside the viewport: panel containment must
        # independently reject all four edges, including an exposed Check centre.
        for key in ("row","button"):
            for axis,extent in (("x","width"),("y","height")):
                for edge in ("start","end"):
                    value=self.good();row=value["geometry"][0]
                    row["panel"].update(x=20,y=70,width=300,height=250)
                    row["row"]["x"]=30;row["button"]["x"]=30
                    bounds,panel=row[key],row["panel"]
                    bounds[axis]=panel[axis]-2 if edge=="start" else panel[axis]+panel[extent]-bounds[extent]+2
                    with self.subTest(key=key,axis=axis,edge=edge),self.assertRaisesRegex(RuntimeError,f"Recovery {key} clipped on {axis}"):
                        self.validate(value)

    def test_row_and_button_cannot_escape_viewport(self):
        for key in ("row","button"):
            for axis,extent,limit in (("x","width",0),("y","height",1)):
                value=self.good();row=value["geometry"][0]
                row[key][axis]=row["viewport"][limit]-row[key][extent]+2
                with self.subTest(key=key,axis=axis),self.assertRaisesRegex(RuntimeError,"Recovery rectangle clipped: "+key):
                    self.validate(value)

    def test_complete_artifacts_and_owned_paths(self):
        value=self.good();edges=[dict(stage=s,edge=e) for s in R["stages"](value["scenario"],value["phase"]) for e in ("before","after")]
        def check():
            R["validate_artifacts"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.db,self.root,value,edges,None)
        check()
        value["captures"]["initial"]=str(self.root/".."/"outside.png")
        with self.assertRaises(RuntimeError):check()
        self.artifacts(value);edges.append(edges[-1])
        with self.assertRaises(RuntimeError):check()

    def test_exact_readiness(self):
        expected=R["ready_tuple"](self.nonce,self.pid,"positive-channel","exercise")
        for field,bad in (("pid",True),("phase","restart"),("nonce","b"*32),("extra",True)):
            value=dict(expected);value[field]=bad
            with self.assertRaises(RuntimeError):R["validate_ready"](value,expected)

    def test_png_truncation_and_corruption_are_rejected(self):
        value=self.good()
        edges=[dict(stage=s,edge=e) for s in R["stages"](value["scenario"],value["phase"]) for e in ("before","after")]
        path=Path(value["captures"]["initial"])
        original=path.read_bytes()
        damaged=bytearray(original);damaged[-5]^=1
        for data in (original[:24],original[:-12],bytes(damaged)):
            path.write_bytes(data)
            with self.assertRaises(RuntimeError):
                R["validate_artifacts"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.db,self.root,value,edges,None)


if __name__=="__main__":
    unittest.main()
