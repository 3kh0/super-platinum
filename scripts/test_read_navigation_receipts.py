"""Fail-closed navigation evidence validation; no native processes/session."""
import copy
from pathlib import Path
import runpy
import tempfile
import unittest
from native_check import expected_viewport
from test_multi_external_select_receipts import png
R = runpy.run_path(str(Path(__file__).with_name("check-read-navigation-native.py")))

class Receipts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture="read-navigation-narrow"):
        value = dict(ok=True, fixture=fixture, nonce=self.nonce, pid=self.pid, noSession=True,
                     noTransport=True, armedAdmissionOnly=True, noAcknowledgement=True,
                     checks=dict.fromkeys(R["CHECKS"], True), preparedRequests=3,
                     viewport=expected_viewport(fixture), captures={}, attempts=[], geometry=[],
                     saved={"ts":"1750000011.000001", "offset":-12.25})
        url = ("https://read-navigation.slack.com/api/conversations.mark?"
               "_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
               "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
               "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
               "&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        for stage in ("left-off", "newest-read", "divider"):
            value["attempts"].append(dict(stage=stage,method="POST",url=url,revision=0,scope=stage,
                previous="1750000012.000001",fields=[["token","fake-read-navigation"],["channel","C2"],["ts","1750000048.000001"]]))
        rect = dict(x=8,y=8,width=200,height=100)
        for stage in R["STAGES"]:
            sample=dict(scope=stage,key="key",visible=True,pending=False,dividerVisible=stage in ("left-off","divider"),
                        ghost=stage=="ghost",atBottom=stage in ("newest-read","newest-unseen"),
                        firstId=value["saved"]["ts"],offset=value["saved"]["offset"])
            item=dict(ok=True,stage=stage,viewport=value["viewport"],sample=sample,
                      canReadOpening=stage in ("left-off","newest-read","divider","saved-offset","explicit-target"),
                      canReadOngoing=stage=="divider",bounds=dict(ok=True,timeline=rect.copy(),composer=rect.copy(),documentWidth=value["viewport"][0]))
            if stage=="cold-boundary":
                item["syntheticPausedControls"]={"ok":True,"controls":[{"id":"unread-opening-continue","text":"Continue loading","hit":True},{"id":"unread-opening-latest","text":"Go to latest","hit":True}]}
            value["geometry"].append(item)
            path=self.root/f"{stage}.png";path.write_bytes(png());value["captures"][stage]=str(path)
        edges=[{"stage":stage,"edge":edge} for stage in R["STAGES"] for edge in ("before","after")]
        return value,edges

    def validate(self,value,edges):
        return R["validate_native"](value["fixture"],self.nonce,self.pid,self.root,value,edges)

    def test_both_complete_sizes(self):
        for name in R["FIXTURES"]:
            value,edges=self.good(name)
            self.assertEqual(len(self.validate(value,edges)),10)

    def test_all_required_flags_are_evidence_not_truthiness(self):
        value,edges=self.good()
        for flag in R["REQUIRED"]:
            for bad in (False,1,None):
                v=copy.deepcopy(value)
                target,key=(v["checks"],flag.split(".")[1]) if flag.startswith("checks.") else (v,flag)
                target[key]=bad
                with self.subTest(flag=flag,bad=bad),self.assertRaises(RuntimeError):self.validate(v,edges)

    def test_exact_requests_and_admission_guards(self):
        value,edges=self.good()
        changes=[("stage","ghost"),("method","GET"),("revision",True),("previous","1750000013.000001"),("scope",""),
                 ("url",value["attempts"][0]["url"]+"&token=secret"),
                 ("fields",[["token","fake-read-navigation"],["channel","C1"],["ts","1750000048.000001"]])]
        for key,bad in changes:
            v=copy.deepcopy(value);v["attempts"][0][key]=bad
            with self.subTest(key=key),self.assertRaises(RuntimeError):self.validate(v,edges)
        for bad in ([],value["attempts"]*2,list(reversed(value["attempts"]))):
            v=copy.deepcopy(value);v["attempts"]=bad
            with self.assertRaises(RuntimeError):self.validate(v,edges)

    def test_actual_geometry_and_policy_required(self):
        value,edges=self.good()
        for index,key,bad in ((0,"dividerVisible",False),(1,"atBottom",False),(2,"dividerVisible",True),(3,"visible",False),(4,"ghost",False),(5,"pending",True),(0,"scope","")):
            v=copy.deepcopy(value);v["geometry"][index]["sample"][key]=bad
            with self.subTest(index=index,key=key),self.assertRaises(RuntimeError):self.validate(v,edges)
        for index in (2,4,5,8,9):
            v=copy.deepcopy(value);v["geometry"][index]["canReadOngoing"]=True
            with self.assertRaises(RuntimeError):self.validate(v,edges)

    def test_saved_offset_and_paused_controls_required(self):
        value,edges=self.good()
        for key,bad in (("offset",float("nan")),("offset",900),("ts","wrong")):
            v=copy.deepcopy(value);v["saved"][key]=bad
            with self.assertRaises(RuntimeError):self.validate(v,edges)
        v=copy.deepcopy(value);v["geometry"][8]["syntheticPausedControls"]["controls"][0]["hit"]=False
        with self.assertRaises(RuntimeError):self.validate(v,edges)

    def test_owned_stages_paths_and_checkpoint_order(self):
        value,edges=self.good()
        for bad in (edges[:-1],list(reversed(edges)),edges+edges[-1:]):
            with self.assertRaises(RuntimeError):self.validate(value,bad)
        for bad in ("left-off.png",str(self.root/".."/"left-off.png"),str(self.root/"divider.png")):
            v=copy.deepcopy(value);v["captures"]["left-off"]=bad
            with self.assertRaises(RuntimeError):self.validate(v,edges)

    def test_readiness_owner_and_alias_are_exact(self):
        with self.assertRaises(RuntimeError):R["contract"]("read-navigation-extra")
        for nonce,pid in (("b"*31,42),("B"*32,42),("a"*32,True)):
            with self.assertRaises(RuntimeError):R["ready_tuple"]("read-navigation",nonce,pid)

if __name__ == "__main__":unittest.main()
