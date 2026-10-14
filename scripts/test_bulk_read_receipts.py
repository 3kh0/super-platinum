"""Pure bulk-read evidence validators; no native launch or real session/storage."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-bulk-read-native.py")))


class Receipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = R["FIXTURE"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture=fixture or self.fixture
        value=dict(noSession=True,noTransport=True,syntheticInput=True,actualPreparedRequests=True,checks=dict.fromkeys(R["CHECKS"],True))
        value.update(ok=True,fixture=fixture,nonce=self.nonce,pid=self.pid,preparedRequests=13,viewport=expected_viewport(fixture),captures={},ledger=[],reconciliations=[])
        query=("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
               "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
               "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
               "&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        def snapshot(cleared):
            return dict(channels={c["id"]:{**c,"name":"fixture"} for c in R["raw_counts"](["C1","C2"],cleared)["channels"]},
                        activityUnread=0 if cleared else 2,threadCounts={},dockCounts=0)
        initial,cleared=snapshot(False),snapshot(True)
        value.update(initialState=initial,finalState=cleared,finalRaw=R["raw_preferences"](False),
            expiry=dict(durationSeconds=6.0,elapsedSeconds=6.05,requestCountBefore=9,requestCountAfter=9),
            ownerRotation=dict(beforeEpoch=0,afterEpoch=1,requestCountBefore=11,requestCountAfter=11))
        previous=initial
        for index,kind in enumerate(R["KINDS"]):
            endpoint={"preference-read":"users.prefs.get","preference-write":"users.prefs.set","clear":"users.markAllRead","undo":"users.markAllRead","counts":"client.counts"}[kind]
            if kind=="preference-read":fields=[["prefs",R["PREFERENCE"]]];response={"ok":True,"prefs":R["raw_preferences"](index==0)}
            elif kind=="preference-write":fields=[["name",R["PREFERENCE"]],["value","false"],["_x_reason","prefs-api/setUserPrefByApi"]];response={"ok":True}
            elif kind=="counts":fields=[];response=R["raw_counts"](["C1","C2"],index!=6)
            else:
                fields=([["undo_ts",R["UNDO_TS"]]] if kind=="undo" else [])+[["_x_reason","UndoShiftEscapeMarkAllRead" if kind=="undo" else "ShiftEscapeConfirmDialog"]]
                response=None if index==11 else {"ok":True} if kind=="undo" else {"ok":True,"undo_ts":R["UNDO_TS"]}
            entry=dict(sequence=index,kind=kind,method="POST",path="/api/"+endpoint,rawQuery=query,
                url="https://fixture.slack.com/api/"+endpoint+"?"+query,fields=fields,retrySafe=kind=="counts",response=response)
            if kind in ("clear","undo"):entry["outcome"]="unknown" if index==11 else "ack"
            value["ledger"].append(entry)
            if kind=="counts":
                after=initial if index==6 else cleared
                value["reconciliations"].append(dict(sequence=index,accepted=True,scope="counts-only",before=copy.deepcopy(previous),after=copy.deepcopy(after),response=copy.deepcopy(response)))
                previous=after
        rect=dict(x=8,y=8,width=100,height=50)
        value["geometry"]=[]
        for index,(stage,count) in enumerate(zip(R["STAGES"],R["COUNTS"])):
            confirm=stage in ("confirmation","confirmation-checked")
            controls=[dict(name=name,rect=rect.copy(),hit=True,disabled=False,
                checked=stage=="confirmation-checked" if name=="bulk-read-dont-ask" else None) for name in R["control_names"](stage)]
            value["geometry"].append(dict(ok=True,stage=stage,viewport=value["viewport"],documentWidth=value["viewport"][0],
                surface=rect.copy(),surfaceKind="confirmation" if confirm else "composer" if stage in ("initial","cancelled","expired") else "status",
                controls=controls,text="Clear all unreads? Cancel" if confirm else "",focusedControl={"confirmation":"bulk-read-confirm-submit",
                    "confirmation-checked":"bulk-read-dont-ask","cancelled":"channel-composer","expired":"channel-composer"}.get(stage),requestCount=count,
                state=copy.deepcopy(initial if index<=4 or stage=="undone" else cleared)))
            path=self.root/f"{stage}.png";path.write_bytes(png());value["captures"][stage]=str(path)
        edges=[{"stage":stage,"edge":edge} for stage in R["STAGES"] for edge in ("before","after")]
        return value,edges

    def validate(self, value, edges):
        return R["validate_native"](value["fixture"], self.nonce, self.pid, self.root, value, edges)

    def test_normal_and_narrow_complete_evidence(self):
        for fixture in R["FIXTURES"]:
            value, edges = self.good(fixture)
            self.assertEqual(len(self.validate(value, edges)), 14)

    def test_every_required_flag_missing_or_false_rejected(self):
        value, edges = self.good()
        for flag in R["REQUIRED"]:
            for absent in (False, True):
                changed = copy.deepcopy(value)
                target, key = (changed["checks"], flag.split(".")[1]) if flag.startswith("checks.") else (changed, flag)
                if absent:
                    del target[key]
                else:
                    target[key] = False
                with self.subTest(flag=flag, absent=absent), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)

    def test_actual_ledger_missing_reordered_replayed_wrong_forms_rejected(self):
        value,edges=self.good()
        for mutation in ("missing","extra","method","sequence","retry","preference","undo","clear","unknown"):
            changed=copy.deepcopy(value)
            if mutation=="missing":del changed["ledger"]
            elif mutation=="extra":changed["ledger"].append(changed["ledger"][3])
            elif mutation=="method":changed["ledger"][0]["method"]="GET"
            elif mutation=="sequence":changed["ledger"][0]["sequence"]=False
            elif mutation=="retry":changed["ledger"][3]["retrySafe"]=True
            elif mutation=="preference":changed["ledger"][1]["fields"][0]=["prefs",'{"mark_msgs_read_immediately":false}']
            elif mutation=="undo":changed["ledger"][5]["fields"][0][1]="1719800701.000001"
            elif mutation=="clear":changed["ledger"][3]["fields"].append(["channel","C2"])
            else:changed["ledger"][11]["response"]={"ok":True}
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_full_query_host_endpoint_and_rawquery_must_match(self):
        value,edges=self.good();url=value["ledger"][0]["url"]
        for bad in (None,url.split("?")[0],url+"&_x_id=extra",url+"#fragment",url.replace("fixture.slack.com","example.test"),url.replace("users.prefs.get","client.counts")):
            changed=copy.deepcopy(value);changed["ledger"][0]["url"]=bad
            with self.subTest(url=bad),self.assertRaises(RuntimeError):self.validate(changed,edges)
        changed=copy.deepcopy(value);changed["ledger"][0]["rawQuery"]=None
        with self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_raw_preference_false_is_boolean_and_unrelated_keys_preserved(self):
        value,edges=self.good()
        for bad in (0,"false",None):
            changed=copy.deepcopy(value);changed["finalRaw"][R["PREFERENCE"]]=bad
            with self.assertRaises(RuntimeError):self.validate(changed,edges)
        changed=copy.deepcopy(value);del changed["finalRaw"]["unknown_pref"]
        with self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_counts_only_refresh_requires_actual_response_and_exact_transition(self):
        value,edges=self.good()
        for mutation in ("missing","response","scope","before","after","membership","duplicate"):
            changed=copy.deepcopy(value)
            if mutation=="missing":changed["reconciliations"].pop()
            elif mutation=="response":changed["reconciliations"][0]["response"]["activity_v2"]["at_user"]=2
            elif mutation=="scope":changed["reconciliations"][0]["scope"]="all-feeds"
            elif mutation=="before":changed["reconciliations"][0]["before"]["activityUnread"]=0
            elif mutation=="after":changed["reconciliations"][0]["after"]["channels"]["C2"]["unread_count"]=False
            elif mutation=="membership":changed["ledger"][4]["response"]["channels"].pop()
            else:changed["ledger"][4]["response"]["channels"].append(changed["ledger"][4]["response"]["channels"][0])
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_expiry_and_owner_rotation_need_time_identity_and_zero_extra_requests(self):
        value,edges=self.good()
        for key,field,bad in (("expiry","elapsedSeconds",5.99),("expiry","durationSeconds",3),("expiry","elapsedSeconds",float("nan")),
                              ("expiry","requestCountAfter",10),("ownerRotation","beforeEpoch",False),
                              ("ownerRotation","afterEpoch",0),("ownerRotation","requestCountAfter",12)):
            changed=copy.deepcopy(value);changed[key][field]=bad
            with self.subTest(field=field),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_checkbox_undo_refresh_hit_and_geometry_contract(self):
        value,edges=self.good()
        for mutation in ("checkbox","undo","refresh","hit","overflow","state","count"):
            changed=copy.deepcopy(value)
            if mutation=="checkbox":changed["geometry"][3]["controls"][1]["checked"]=False
            elif mutation=="undo":changed["geometry"][5]["controls"].pop(0)
            elif mutation=="refresh":changed["geometry"][-1]["controls"][0]["name"]="bulk-read-undo"
            elif mutation=="hit":changed["geometry"][1]["controls"][0]["hit"]=1
            elif mutation=="overflow":changed["geometry"][0]["surface"]["width"]=10000
            elif mutation=="state":changed["geometry"][4]["state"]=changed["finalState"]
            else:changed["geometry"][0]["requestCount"]=False
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_capture_paths_cannot_escape_or_substitute_another_stage(self):
        value, edges = self.good()
        for path in (str(self.root / ".." / "initial.png"), str(self.root / "confirmation.png"), "initial.png", None):
            changed = copy.deepcopy(value)
            changed["captures"]["initial"] = path
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_expired_notice_disappears_with_counts_preserved(self):
        value,edges=self.good()
        expired=value["geometry"][9]
        self.assertEqual(expired["stage"],"expired")
        self.assertEqual(expired["surfaceKind"],"composer")
        self.assertEqual([c["name"] for c in expired["controls"]],["channel-composer"])
        self.assertEqual(expired["requestCount"],9)
        self.assertEqual(expired["state"],value["reconciliations"][0]["after"])
        self.validate(value,edges)
        changed=copy.deepcopy(value);changed["geometry"][9]["surfaceKind"]="status"
        with self.assertRaisesRegex(RuntimeError,"Bulk stage expired: invalid surfaceKind"):
            self.validate(changed,edges)
        changed=copy.deepcopy(value);changed["geometry"][9]["controls"][0]["name"]="bulk-read-dismiss"
        with self.assertRaisesRegex(RuntimeError,"Bulk stage expired: control channel-composer"):
            self.validate(changed,edges)

    def test_confirmation_focus_checkbox_focus_and_cancel_return_are_exact(self):
        value,edges=self.good()
        for index in (1,2,3,9):
            for bad in (None,"bulk-read-close","channel-composer" if index in (1,3) else "bulk-read-confirm-submit"):
                changed=copy.deepcopy(value);changed["geometry"][index]["focusedControl"]=bad
                with self.subTest(stage=changed["geometry"][index]["stage"],bad=bad),self.assertRaisesRegex(RuntimeError,"focusedControl"):
                    self.validate(changed,edges)

    def test_stage_errors_name_the_mismatched_field(self):
        value,edges=self.good()
        for field,bad in (("state",value["finalState"]),("requestCount",5),("viewport",[1,1])):
            changed=copy.deepcopy(value);changed["geometry"][4][field]=bad
            with self.assertRaisesRegex(RuntimeError,"Bulk stage sending: invalid "+field):
                self.validate(changed,edges)

    def test_geometry_and_stage_sequence_cannot_be_partial(self):
        value, edges = self.good()
        changed = copy.deepcopy(value)
        changed["geometry"][1]["ok"] = False
        with self.assertRaises(RuntimeError):
            self.validate(changed, edges)
        for bad in (edges[:-1], edges + edges[-1:], list(reversed(edges))):
            with self.assertRaises(RuntimeError):
                self.validate(value, bad)

    def test_readiness_exact_and_cannot_overwrite(self):
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        for name, value in (("pid", 43), ("nonce", "b" * 32), ("fixture", R["FIXTURE"]), ("version", True)):
            changed = {**expected, name: value}
            with self.assertRaises(RuntimeError):
                R["validate_ready"](changed, expected)
        R["publish_ready"](self.root, expected)
        with self.assertRaises(FileExistsError):
            R["publish_ready"](self.root, expected)

    def test_closed_log_late_duplicate_and_pointer_skip_rejected(self):
        value, edges = self.good()
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        lines = [R["WAITING"] + json.dumps(expected)]
        lines.extend(R["STAGE"] + json.dumps(edge) for edge in edges)
        lines.append(R["PREFIX"] + json.dumps(value))
        path = self.root / "closed.log"
        path.write_text("\n".join(lines) + "\n")
        R["validate_closed_log"](path, expected, self.root, value)
        path.write_text(path.read_text() + lines[-1] + "\n")
        with self.assertRaises(RuntimeError):
            R["validate_closed_log"](path, expected, self.root, value)
        with self.assertRaises(RuntimeError):
            R["validate_pointer_cleanup"]({"movement_observed": True, "restore_observed": False,
                                            "restore_attempted": True})

    def test_png_truncation_wrong_dimensions_and_fresh_output(self):
        value, edges = self.good()
        path = Path(value["captures"]["initial"])
        with self.assertRaises(RuntimeError):
            R["png_signature"](path, [99, 200])
        path.write_bytes(path.read_bytes()[:-12])
        with self.assertRaises(RuntimeError):
            self.validate(value, edges)
        (self.root / self.fixture).mkdir()
        with self.assertRaises(FileExistsError):
            R["run"](self.fixture, self.root / "never-launched.exe", self.root)


if __name__ == "__main__":
    unittest.main()
