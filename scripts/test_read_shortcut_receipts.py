"""Pure Escape evidence validators; no native launch or real session/storage."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-read-shortcuts-native.py")))


class Receipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = R["FIXTURE"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture = fixture or self.fixture
        value = dict(noSession=True, noTransport=True, syntheticAcknowledgements=True, syntheticKeyboard=True,
                     checks=dict.fromkeys(R["CHECKS"], True), selection=dict(ok=True, start=3, end=8, direction="backward"))
        value.update(ok=True, fixture=fixture, nonce=self.nonce, pid=self.pid,
                     preparedRequests=5, viewport=expected_viewport(fixture), captures={}, attempts=[])
        query = ("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
               "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
               "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
               "&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        for stage in R["REQUEST_STAGES"]:
            thread=stage=="thread-read"
            target=dict(kind="thread" if thread else "conversation",team="T1",channel="C2")
            if thread: target["rootTs"]=R["THREAD_ROOT"]
            endpoint="subscriptions.thread.mark" if thread else "conversations.mark"
            fields=R["request_fields"](stage)
            value["attempts"].append(dict(stage=stage,target=target,ts=dict(fields)["ts"],method="POST",
                url="https://reading-shortcuts.slack.com/api/"+endpoint+"?"+query,fields=fields))
        value["guardEvents"]=[dict(kind=kind,requestCount=5,**({} if kind=="menu" else {"prevented":kind=="completion"}))
            for kind in ("control","meta","alt","repeat","composing","key229","completion","menu")]
        for event in value["guardEvents"][:6]:
            facts=dict(ctrlKey=False,metaKey=False,altKey=False,repeat=False,isComposing=False,keyCode=0)
            key={"control":"ctrlKey","meta":"metaKey","alt":"altKey","repeat":"repeat","composing":"isComposing","key229":"keyCode"}[event["kind"]]
            facts[key]=229 if event["kind"]=="key229" else True;event["facts"]=facts
        value["staleEvents"]=[dict(kind=kind,requestCount=5) for kind in ("scope","account","focus-return")]
        rect=dict(x=8,y=8,width=100,height=50)
        value["geometry"]=[]
        for stage,count in zip(R["STAGES"],R["COUNTS"]):
            thread=stage in ("thread-read","focused-primary")
            value["geometry"].append(dict(ok=True,stage=stage,viewport=value["viewport"],documentWidth=value["viewport"][0],
                timeline=rect.copy(),composer=rect.copy(),threadVisible=thread,thread=rect.copy() if thread else None,
                threadComposer=rect.copy() if thread else None,activeId="channel-composer",requestCount=count,
                channelText="" if stage=="composer-empty" else R["DRAFT"],threadText=R["REPLY"] if thread else None))
        for stage in R["STAGES"]:
            path=self.root/f"{stage}.png";path.write_bytes(png());value["captures"][stage]=str(path)
        edges=[{"stage":stage,"edge":edge} for stage in R["STAGES"] for edge in ("before","after")]
        return value,edges

    def validate(self, value, edges):
        return R["validate_native"](value["fixture"], self.nonce, self.pid, self.root, value, edges)

    def test_normal_and_narrow_complete_evidence(self):
        for fixture in R["FIXTURES"]:
            value, edges = self.good(fixture)
            self.assertEqual(len(self.validate(value, edges)), 10)

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

    def test_actual_requests_wrong_target_stage_timestamp_or_form_rejected(self):
        value,edges=self.good()
        for mutation in ("missing","extra","method","target","root","timestamp","read","token","stage","duplicate"):
            changed=copy.deepcopy(value)
            if mutation=="missing":del changed["attempts"]
            elif mutation=="extra":changed["attempts"].append(changed["attempts"][0])
            elif mutation=="method":changed["attempts"][0]["method"]="GET"
            elif mutation=="target":changed["attempts"][0]["target"]["channel"]="C1"
            elif mutation=="root":changed["attempts"][3]["target"]["rootTs"]="1.000001"
            elif mutation=="timestamp":changed["attempts"][0]["fields"][-1][1]="1750000048.000001"
            elif mutation=="read":changed["attempts"][3]["fields"][-1][1]="0"
            elif mutation=="token":changed["attempts"][0]["fields"][0][1]="wrong"
            elif mutation=="stage":changed["attempts"][0]["stage"]="blocked-keys"
            else:changed["attempts"][0]["fields"].append(["channel","C2"])
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_full_request_url_and_default_query_required(self):
        value,edges=self.good();url=value["attempts"][0]["url"]
        for bad in (None,url.split("?")[0],url+"&_x_id=extra",url+"#fragment",url.replace("reading-shortcuts.slack.com","example.test"),
                    url.replace("conversations.mark","conversations.history"),url.replace("_x_num_retries=0","_x_num_retries=1")):
            changed=copy.deepcopy(value);changed["attempts"][0]["url"]=bad
            with self.subTest(url=bad),self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_guard_and_stale_receipts_cannot_hide_extra_dispatch(self):
        value,edges=self.good()
        for key in ("guardEvents","staleEvents"):
            changed=copy.deepcopy(value);changed[key][0]["requestCount"]=6
            with self.assertRaises(RuntimeError):self.validate(changed,edges)
            changed=copy.deepcopy(value);changed[key].pop()
            with self.assertRaises(RuntimeError):self.validate(changed,edges)
        changed=copy.deepcopy(value);changed["guardEvents"][6]["prevented"]=False
        with self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_synthetic_guard_facts_must_represent_each_tested_modifier(self):
        value,edges=self.good()
        for index,key,bad in ((0,"ctrlKey",False),(0,"altKey",True),(5,"keyCode",0),(4,"isComposing",1)):
            changed=copy.deepcopy(value);changed["guardEvents"][index]["facts"][key]=bad
            with self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_geometry_text_thread_and_selection_types_rejected(self):
        value,edges=self.good()
        for index,key,bad in ((0,"composer",None),(0,"documentWidth",True),(0,"documentWidth",float("nan")),
                             (0,"thread",{}),(5,"threadVisible",1),(5,"threadText",None),(0,"channelText","lost"),
                             (0,"requestCount",False),(0,"timeline",dict(x=-100,y=0,width=1,height=1)),
                             (5,"threadComposer",dict(x=0,y=0,width=0,height=1))):
            changed=copy.deepcopy(value);changed["geometry"][index][key]=bad
            with self.subTest(key=key,bad=bad),self.assertRaises(RuntimeError):self.validate(changed,edges)
        for key,bad in (("start",False),("direction","forward"),("end",9)):
            changed=copy.deepcopy(value);changed["selection"][key]=bad
            with self.assertRaises(RuntimeError):self.validate(changed,edges)

    def test_capture_paths_cannot_escape_or_substitute_another_stage(self):
        value, edges = self.good()
        for path in (str(self.root / ".." / "initial.png"), str(self.root / "metadata-read.png"), "initial.png", None):
            changed = copy.deepcopy(value)
            changed["captures"]["initial"] = path
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

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
