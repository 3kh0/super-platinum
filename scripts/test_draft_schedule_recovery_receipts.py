"""Pure Schedule recovery receipt rejection tests; no native launch or session."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

R = runpy.run_path(str(Path(__file__).with_name("check-draft-schedule-recovery-native.py")))
from test_multi_external_select_receipts import png



def anchor():
    expected = dict(scope="T1/C2/1", latestId="fixture-last-row", windowEnd=40)
    before = dict(expected=expected, connected=True, scope=expected["scope"], windowEnd=40,
        latestMounted=True, latestHeight=42, inputGeneration=0, scrollTop=300,
        scrollHeight=800, clientHeight=400, bottomGap=100, ready=True, atBottom=False)
    after = copy.deepcopy(before)
    after.update(scrollTop=400, bottomGap=0, atBottom=True)
    return dict(landed=True, before=before, after=after)

class Receipts(unittest.TestCase):

    def test_home_anchor_requires_real_ready_bottom_and_identity(self):
        for mutation in ("missing", "landed", "disconnected", "latest", "zero-height", "nonfinite",
                         "gap", "bottom", "input", "scope", "window", "extra", "bool-counter", "expected"):
            value = self.good()
            mounted = value["details"]["nativeInitial"]
            a = mounted["homeAnchor"]
            if mutation == "missing": del mounted["homeAnchor"]
            elif mutation == "landed": a["landed"] = 1
            elif mutation == "disconnected": a["before"]["connected"] = False
            elif mutation == "latest": a["before"]["latestMounted"] = False
            elif mutation == "zero-height": a["before"]["latestHeight"] = 0
            elif mutation == "nonfinite": a["after"]["scrollTop"] = float("nan")
            elif mutation == "gap": a["after"]["bottomGap"] = 1
            elif mutation == "bottom": a["after"].update(scrollTop=397, bottomGap=3, atBottom=False)
            elif mutation == "input": a["after"]["inputGeneration"] += 1
            elif mutation == "scope": a["after"]["scope"] = "another"
            elif mutation == "window": a["after"]["windowEnd"] += 1
            elif mutation == "extra": a["after"]["fabricated"] = True
            elif mutation == "bool-counter": a["before"]["inputGeneration"] = False
            else: a["after"]["expected"]["latestId"] = "another-row"
            with self.subTest(mutation=mutation): self.rejects(value)

    def test_home_anchor_bottom_tolerance_is_bounded_and_not_premount_requirement(self):
        for gap in (-1, 2):
            value = self.good()
            value["details"]["nativeInitial"]["homeAnchor"]["after"].update(scrollTop=400-gap, bottomGap=gap)
            self.validate(value)

    def test_every_positive_scope_remount_and_restart_requires_anchor(self):
        for field in ("nativeByScopeInitial", "nativeByScopeFinal"):
            for target in ("channel", "thread"):
                value = self.good("positive-thread")
                del value["details"][field][target]["homeAnchor"]
                with self.subTest(field=field, target=target): self.rejects(value)
        value = self.restart(self.good("pending-check-channel"))
        del value["details"]["nativeInitial"]["homeAnchor"]
        self.rejects(value)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.database = self.root / "draft-schedule-recovery.sqlite"
        self.nonce, self.pid = "a" * 32, 42

    def operation(self, target="channel"):
        return dict(id="11234567-89ab-4def-8123-456789abcdef", scope=R["scope"](target),
            client_msg_id=R["client"](target), local_revision=0, kind={"Schedule": {"post_at": 500}},
            snapshot=R["snapshot"](R["REQUESTED"], target, True), base=R["base"](target),
            consume_id="01234567-89ab-4def-8123-456789abcdef")

    def attempt(self, scenario="positive-channel"):
        op = self.operation("thread" if scenario.endswith("thread") else "channel")
        query=("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
            "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current&_x_gantry=true"
            "&_x_num_retries=0&_x_version_ts=capture-required&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        pending = scenario == "pending-check-channel"
        raw = None if pending else R["raw_response"](op, scenario)
        value = dict(sequence=1, method="POST", url="https://fixture.slack.com/api/drafts.info?"+query,
            fields=[["draft_id", op["base"]["id"]]], retrySafe=False, operation=op, nowMs=100000,
            delivered=not pending, rawResponse=raw, admitted={}, retryAfterSeconds=120 if scenario == "cooldown-channel" else None)
        if not pending:
            value["result"] = dict(requested=True, confirmed=scenario not in R["UNCONFIRMED_SCENARIOS"],
                cooldownUntilMs=220003 if scenario == "cooldown-channel" else None,
                issue="RateLimited" if scenario == "cooldown-channel" else "UnsupportedResponse" if scenario in R["UNCONFIRMED_SCENARIOS"] else None,
                hasMore=False, raw=copy.deepcopy(raw))
        return value

    def good(self, scenario="positive-thread", fixture=None):
        fixture=fixture or R["FIXTURE"]+"-narrow";viewport=R["expected_viewport"](fixture)
        target="thread" if scenario.endswith("thread") else "channel"
        op=self.operation(target)
        initial={which:R["initial_inventory"](which,self.operation(which)) for which in ("channel","thread")}
        initial["withheld"]=[];withheld=[]
        if scenario=="positive-channel":
            for index in range(3):
                scope=dict(account_id=R["ACCOUNT"],key=dict(team="T1",channel="C1" if index<2 else "C2",
                    thread=None if index==0 else R["THREAD"] if index==1 else "1719800300.000000"))
                text=("Withheld unknown ID schedule","Withheld file schedule","Withheld extra routing schedule")[index]
                frozen=R["snapshot"](text,"channel" if index==0 else "thread",True)
                frozen["destinations"][0]["channel_id"]=scope["key"]["channel"]
                if index:frozen["destinations"][0]["thread_ts"]=scope["key"]["thread"]
                if index==1:frozen["file_ids"]=["FWithheld"]
                if index==2:frozen["destinations"][0]["unproven_route"]="retained"
                identity=f"00000000-0000-4000-8000-{114+index:012}"
                base=None if index==0 else dict(scope=scope,id="DrScheduleFile" if index==1 else "DrScheduleRouting",
                    client_msg_id=identity,last_updated_ts="10.000001",snapshot=frozen)
                item=dict(id=f"11234567-89ab-4def-8123-456789abcde{index}",scope=scope,client_msg_id=identity,
                    local_revision=1 if index==0 else 0,kind={"Schedule":{"post_at":500}},snapshot=frozen,base=base,
                    consume_id=f"01234567-89ab-4def-8123-456789abcde{index}")
                withheld.append(item);initial["withheld"].append(R["withheld_inventory"](item,index))
        admitted=copy.deepcopy(initial);admitted[target]["stored"]["revision"]=2
        admitted[target]["stored"]["state"].update(check_ticket=dict(id="21234567-89ab-4def-8123-456789abcdef",operation=op),
            check_deadline=dict(operation_id=op["id"],operation=op,until_ms=130000))
        request=self.attempt(scenario);request["admitted"]=copy.deepcopy(admitted)
        final=copy.deepcopy(admitted)
        if scenario.startswith("typing-"):
            final[target]["canonical"]=R["local"](R["NEWER"],target);final[target]["stored"]["revision"]+=1
            final[target]["stored"]["state"].update(desired=R["snapshot"](R["NEWER"],target),local_revision=3)
        typed=copy.deepcopy(final)
        if scenario!="pending-check-channel":
            final[target]["stored"]["revision"]+=1;state=final[target]["stored"]["state"];state["check_ticket"]=None
            if scenario not in R["UNCONFIRMED_SCENARIOS"]:
                state.update(client_msg_id="00000000-0000-4000-8000-000000000121",acknowledged=None,acknowledged_local_revision=1,
                    workflow="Idle",retired_ids=[op["base"]["id"]],consumption=None,
                    local_completion=dict(intent=R["consumption"](op),successor_client_msg_id="00000000-0000-4000-8000-000000000121"))
            if scenario=="cooldown-channel":state["check_deadline"]["until_ms"]=220003
        lineage=scenario in ("successor-channel","reenroll-channel")
        held=copy.deepcopy(final if lineage else admitted)
        if lineage:
            state=final[target]["stored"]["state"];del state["local_completion"]
            if scenario=="successor-channel":
                final[target]["stored"]["revision"]=5
                state.update(local_revision=4,consumption=dict(id="31234567-89ab-4def-8123-456789abcdef",scope=R["scope"](target),
                    client_msg_id=state["client_msg_id"],local_revision=2,kind={"Schedule":{"post_at":501}},snapshot=R["snapshot"](R["CURRENT"],target),base=None))
            else:
                final[target]["stored"]["revision"]=1
                state.update(client_msg_id="00000000-0000-4000-8000-000000000122",local_revision=1,acknowledged_local_revision=0,
                    retired_ids=[],check_deadline=None,check_ticket=None)
        def projection():
            managed={which:R["view"](initial[which]) for which in ("channel","thread")}
            editors={which:dict(sameCaptured=True,serial=0,view=copy.deepcopy(managed[which]),pending=None,running=False,unsaved=False) for which in managed}
            if lineage:editors[target]=None
            return dict(account=R["ACCOUNT"],epoch=1,team="T1",viewId=1,managed=managed,editors=editors,
                composer=R["local"](R["CURRENT"],"channel")["composer"],threadComposer=R["local"](R["CURRENT"],"thread")["composer"],toast=None,
                queue=dict(generation=1,matchesCapturedScope=True,filter="Drafts",items={},loading=False,loaded=False,hasMore=False,limit=100,error=None,busy=[]))
        first=projection();reference=copy.deepcopy(first)
        details=dict(held=held,heldProjection=copy.deepcopy(first),afterReceipt=final)
        if scenario.startswith("typing-"):
            reference["managed"][target]=R["view"](typed[target]);reference["editors"][target]["view"]=R["view"](typed[target])
            reference["editors"][target]["serial"]=1
            reference["threadComposer"]=R["local"](R["NEWER"],target)["composer"]
            details["inputProjection"]=copy.deepcopy(reference)
        if scenario in R["RACE_SCENARIOS"]:
            before=copy.deepcopy(reference)
            if scenario=="owner-rotation-channel":reference["epoch"]+=1
            elif scenario=="writer-replacement-channel":
                reference["editors"][target].update(sameCaptured=False,view=R["view"](held[target]))
                reference["managed"][target]=R["view"](held[target])
            elif scenario=="queue-reset-channel":reference["queue"]["generation"]+=2
            elif scenario=="queue-newer-channel":
                reference["queue"]["generation"]+=1;reference["queue"]["items"]={op["base"]["id"]:R["queue_row"](op,True)}
            elif lineage:reference["managed"][target]=R["view"](final[target])
            details.update(beforeInterleave=before,afterInterleave=copy.deepcopy(reference),mutation=dict(scenario=scenario,before=before,
                after=copy.deepcopy(reference),durableBefore=held,durableAfter=final if lineage else held))
        result=copy.deepcopy(reference)
        if scenario not in ("owner-rotation-channel","writer-replacement-channel","successor-channel","reenroll-channel"):
            if scenario not in R["UNCONFIRMED_SCENARIOS"]:
                result["toast"]="Schedule confirmed. Your current text was kept."
                if scenario not in ("queue-reset-channel","queue-newer-channel"):
                    result["queue"]["generation"]+=1;result["queue"]["items"]={op["base"]["id"]:R["queue_row"](op)}
            if scenario!="pending-check-channel":
                result["managed"][target]=R["view"](final[target]);result["editors"][target]["view"]=R["view"](final[target])
        details.update(resultProjection=result,beforeScheduledClick=copy.deepcopy(result))
        last=copy.deepcopy(result);last["queue"]["filter"]="Scheduled"
        if scenario=="storage-retry-channel":details.update(failedStorage=admitted,failedStorageProjection=copy.deepcopy(first))
        messages=dict(channel=[],thread=[],pending=dict(channel=[],thread=[]),pending_delivery={})
        value=dict(ok=True,fixture=fixture,scenario=scenario,phase="exercise",nonce=self.nonce,pid=self.pid,target=target,
            isolatedDatabase=str(self.database),noSession=True,noTransport=True,noCache=True,noProductionRoute=True,syntheticInput=True,
            actualPreparedRequests=True,canonicalMessagesUnchanged=True,preparedRequests=1,refusedInputs=0,routedInputs=int(scenario.startswith("typing-")),
            viewport=viewport,operation=op,withheld=withheld,initial=initial,durable=final,attempts=[request],details=details,
            initialProjection=first,finalProjection=last,messagesInitial=messages,messagesFinal=copy.deepcopy(messages))
        self.add_geometry(value)
        return value

    def add_geometry(self,value):
        target,case,viewport,details=value["target"],value["scenario"],value["viewport"],value["details"]
        def native(text,selected=False):
            return dict(ok=True,text=text,display=text,codePoints=list(map(ord,text)),utf16Length=len(text.encode("utf-16-le"))//2,
                start=len(text[:text.index("🚀")].encode("utf-16-le"))//2 if selected else 0,
                end=len(text.encode("utf-16-le"))//2 if selected else 0,direction="backward" if selected else "none",composing=False,
                scope="fixture-scope",visit="1",viewport=viewport,rect=dict(left=40,top=220,right=440,bottom=260,width=400,height=40))
        def row(disabled=False,clicked=False,text="Schedule unconfirmed"):
            return dict(ok=True,kind="schedule",operation=value["operation"]["id"],disabled=disabled,clickRequested=clicked,clicked=clicked,
                hit=True,threadPaneAbsent=True,rowUnobscured=True,buttonUnobscured=True,headerUnobscured=True,
                buttonText="Checking…" if disabled else "Check schedule",text=text,viewport=viewport,documentWidth=viewport[0],
                row=dict(x=30,y=70,width=400,height=100),button=dict(x=40,y=130,width=140,height=30),
                panel=dict(x=20,y=60,width=500,height=280),header=dict(x=20,y=10,width=500,height=40))
        def surface(mode,text=""):
            count=len(value["finalProjection"]["queue"]["items"]) if mode=="scheduled" else 1
            empty=mode=="scheduled" and count==0
            return dict(ok=True,kind=mode,contentKind="tab" if mode=="tab" else "empty-body" if empty else "row",
                rowUnobscured=True,headerUnobscured=True,threadPaneAbsent=True,
                clickRequested=mode=="tab",clicked=mode=="tab",checkCount=0,selectedBefore="true" if mode=="scheduled" else "false",
                text="" if empty else text,actions=["Continue editing locally"] if mode=="local" else [],buttonText="Scheduled 1",
                rowCount=count,viewport=viewport,documentWidth=viewport[0],
                row=dict(x=20,y=60,width=500,height=280) if empty else dict(x=30,y=70,width=400,height=100),
                panel=dict(x=20,y=60,width=500,height=280),header=dict(x=20,y=10,width=500,height=40),tab=dict(x=20,y=50,width=120,height=20))
        def composer():
            return dict(ok=True,kind="composer",fieldUnobscured=True,composerUnobscured=True,headerUnobscured=True,fieldFocused=True,
                activeId=target+"-composer",scope="fixture-scope",visit="1",viewport=viewport,documentWidth=viewport[0],
                field=dict(x=40,y=220,width=400,height=40),composer=dict(x=30,y=200,width=500,height=100),header=dict(x=30,y=20,width=500,height=50))
        details.update(nativeInitial=native(R["CURRENT"]),actualClick=row(clicked=True),scheduledClick=surface("tab"))
        details["nativeInitial"]["homeAnchor"] = anchor()
        if case in ("positive-channel","positive-thread"):
            details.update(nativeByScopeInitial={which:native(R["CURRENT"]) for which in ("channel","thread")},
                nativeByScopeFinal={which:native(R["CURRENT"]) for which in ("channel","thread")})
            for key in ("nativeByScopeInitial", "nativeByScopeFinal"):
                for mounted in details[key].values(): mounted["homeAnchor"] = anchor()
        if case.startswith("typing-") or case in R["RACE_SCENARIOS"]:
            text=R["NEWER"] if case.startswith("typing-") else R["CURRENT"]
            details.update(nativeBeforeRelease=native(text,True),nativeFinal=native(text,True))
            if case.startswith("typing-"):details["input"]=native(text,True)
        if case=="storage-retry-channel":details["retryClick"]=row(clicked=True,text="Schedule status could not be saved")
        if case=="cooldown-channel":details["cooldownClick"]=row(clicked=True,text="The saved cooldown applies")
        if case in R["UNCONFIRMED_SCENARIOS"] and case!="pending-check-channel":
            details["feedback"]=row(text="cooldown" if case=="cooldown-channel" else "still unconfirmed")
        geometry=[]
        for stage in R["stages"](case,"exercise"):
            if stage in ("initial","checking","retry"):
                entry=row(disabled=stage=="checking",text="could not be saved" if stage=="retry" else "Schedule unconfirmed")
            elif stage in ("input","interleaved"):entry=composer()
            elif stage.startswith("withheld-"):
                entry=surface("withheld",("Withheld unknown ID schedule","Withheld file schedule","Withheld extra routing schedule")[int(stage[-1])])
            elif stage=="result":
                if case in R["UNCONFIRMED_SCENARIOS"]:
                    entry=row(disabled=case=="pending-check-channel",text="cooldown" if case=="cooldown-channel" else "still unconfirmed")
                elif case.startswith("typing-") or case in R["RACE_SCENARIOS"]:entry=composer()
                else:entry=surface("local",R["CURRENT"])
            else:entry=surface("scheduled",("Newer queued schedule" if case=="queue-newer-channel" else R["REQUESTED"])+"\nFrozen checklist item")
            entry["stage"]=stage;geometry.append(entry)
        value["geometry"]=geometry

    def validate(self,value):
        return R["validate"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.database,value)

    def rejects(self,value):
        with self.assertRaises(RuntimeError):self.validate(value)

    def restart(self,exercise):
        value=copy.deepcopy(exercise);target=value["target"]
        value.update(phase="restart",initial=copy.deepcopy(value["durable"]),preparedRequests=0,attempts=[],routedInputs=0)
        first=value["initialProjection"]
        first["managed"][target]=R["view"](value["durable"][target]);first["editors"][target]["view"]=R["view"](value["durable"][target])
        value["finalProjection"]=copy.deepcopy(first)
        clicked=copy.deepcopy(value["details"]["actualClick"])
        refusal=copy.deepcopy(clicked);refusal.update(clickRequested=False,clicked=False,
            text="cooldown" if value["scenario"]=="cooldown-channel" else "unresolved")
        value["details"]=dict(nativeInitial=value["details"]["nativeInitial"],restartClick=clicked,restartRefusal=refusal)
        value["geometry"]=[dict(refusal,stage="restart")]
        return value

    def test_all_sixteen_cases_in_both_sizes(self):
        for fixture in R["FIXTURES"]:
            for scenario in R["SCENARIOS"]:
                with self.subTest(fixture=fixture,scenario=scenario):
                    exercise=self.good(scenario,fixture);self.validate(exercise)
                    if scenario in R["RESTART_SCENARIOS"]:
                        restarted=self.restart(exercise);self.validate(restarted)
                        R["validate_restart_pair"](scenario,exercise["durable"],restarted["durable"])

    def test_public_validator_rejects_flag_request_and_raw_response_mutations(self):
        for field in ("ok","noSession","noTransport","noCache","noProductionRoute","syntheticInput",
                      "actualPreparedRequests","canonicalMessagesUnchanged"):
            for bad in (False,1,None):
                value=self.good();value[field]=bad
                with self.subTest(field=field,bad=bad):self.rejects(value)
        for mutation in ("extra-request","token","replay","time","blocks","broadcast","sent"):
            value=self.good();request=value["attempts"][0]
            if mutation=="extra-request":value["attempts"].append(copy.deepcopy(request))
            elif mutation=="token":request["fields"].append(["token","fake"])
            elif mutation=="replay":request["url"]=request["url"].replace("drafts.info","drafts.update")
            elif mutation=="time":request["rawResponse"]["draft"]["date_scheduled"]+=1
            elif mutation=="blocks":request["rawResponse"]["draft"]["blocks"]=[]
            elif mutation=="broadcast":request["rawResponse"]["draft"]["destinations"][0]["broadcast"]=False
            else:request["rawResponse"]["draft"]["is_sent"]=True
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_public_admission_requires_complete_durable_ticket_before_actual_read(self):
        for mutation in ("missing","ticket","payload","deadline","revision","sibling"):
            value=self.good();admitted=value["attempts"][0]["admitted"];state=admitted["thread"]["stored"]["state"]
            if mutation=="missing":del value["attempts"][0]["admitted"]
            elif mutation=="ticket":state["check_ticket"]=None
            elif mutation=="payload":state["check_ticket"]["operation"]=dict(state["check_ticket"]["operation"],local_revision=1)
            elif mutation=="deadline":state["check_deadline"]["until_ms"]=130001
            elif mutation=="revision":admitted["thread"]["stored"]["revision"]=1
            else:admitted["channel"]["canonical"]["composer"]["text"]="changed"
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_public_completion_retains_exact_current_draft_and_new_authority(self):
        for mutation in ("release","desired","completion","identity","revision","selection","retired","required-null"):
            value=self.good();row=value["durable"]["thread"];state=row["stored"]["state"]
            if mutation=="release":row["stored"]=None
            elif mutation=="desired":state["desired"]=value["operation"]["snapshot"]
            elif mutation=="completion":del state["local_completion"]
            elif mutation=="identity":state["client_msg_id"]=value["operation"]["client_msg_id"]
            elif mutation=="revision":row["stored"]["revision"]+=1
            elif mutation=="selection":row["canonical"]["composer"]["selection"]["end"]=0
            elif mutation=="retired":state["retired_ids"]=[]
            else:del state["blocked_observation"]
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_public_negative_read_never_fabricates_resolution_or_loses_cooldown(self):
        for case in R["UNCONFIRMED_SCENARIOS"]:
            value=self.good(case);value["durable"]["channel"]["stored"]["state"]["workflow"]="Idle";self.rejects(value)
        value=self.good("pending-check-channel");value["durable"]["channel"]["stored"]["state"]["check_ticket"]=None;self.rejects(value)
        value=self.good("cooldown-channel");value["durable"]["channel"]["stored"]["state"]["check_deadline"]["until_ms"]-=1;self.rejects(value)

    def test_public_typing_and_backward_native_selection_are_independently_preserved(self):
        for case in ("typing-read-thread","typing-commit-thread"):
            for mutation in ("text","selection","visit","scope","unicode","focus","routed"):
                value=self.good(case);after=value["details"]["nativeFinal"]
                if mutation=="text":value["durable"]["thread"]["canonical"]["composer"]["text"]=R["CURRENT"]
                elif mutation=="selection":after["direction"]="forward"
                elif mutation=="visit":after["visit"]="other"
                elif mutation=="scope":after["scope"]="other"
                elif mutation=="unicode":after["codePoints"]=[65]
                elif mutation=="focus":value["geometry"][-2]["fieldFocused"]=False
                else:value["routedInputs"]=0
                with self.subTest(case=case,mutation=mutation):self.rejects(value)

    def test_public_races_refuse_stale_projection_and_require_real_mutation(self):
        for case in R["RACE_SCENARIOS"]:
            value=self.good(case);value["details"]["resultProjection"]["queue"]["items"]={"wrong":{}};self.rejects(value)
        value=self.good("owner-rotation-channel");value["details"]["mutation"]["after"]["epoch"]-=1;self.rejects(value)
        value=self.good("writer-replacement-channel");value["details"]["mutation"]["after"]["editors"]["channel"]["sameCaptured"]=True;self.rejects(value)
        value=self.good("writer-replacement-channel");value["details"]["mutation"]["after"]["managed"]["channel"]=R["view"](value["initial"]["channel"]);self.rejects(value)
        for case in ("successor-channel","reenroll-channel"):
            value=self.good(case);value["durable"]["channel"]=copy.deepcopy(value["details"]["held"]["channel"]);self.rejects(value)
            value=self.good(case);value["details"]["resultProjection"]["toast"]="stale completion";self.rejects(value)

    def test_public_storage_retry_and_unsupported_schedules_never_gain_authority(self):
        value=self.good("storage-retry-channel");value["details"]["failedStorage"]["channel"]["stored"]["state"]["check_ticket"]=None;self.rejects(value)
        value=self.good("storage-retry-channel");value["details"]["failedStorageProjection"]["queue"]["items"]={"invented":{}};self.rejects(value)
        for index in range(3):
            value=self.good("positive-channel");value["geometry"][index+1]["checkCount"]=1;self.rejects(value)
            value=self.good("positive-channel");value["durable"]["withheld"][index]["stored"]["state"]["check_ticket"]={};self.rejects(value)

    def test_public_geometry_clipping_occlusion_and_automatic_filter_change(self):
        for field in ("threadPaneAbsent","rowUnobscured","buttonUnobscured","headerUnobscured"):
            value=self.good();value["geometry"][0][field]=False;self.rejects(value)
        for axis in ("x","y"):
            value=self.good();value["geometry"][0]["row"][axis]=-30;self.rejects(value)
        value=self.good();value["details"]["actualClick"]["clicked"]=False;self.rejects(value)
        value=self.good();value["details"]["scheduledClick"]["selectedBefore"]="true";self.rejects(value)
        value=self.good();value["details"]["beforeScheduledClick"]["queue"]["filter"]="Scheduled";self.rejects(value)
        value=self.good();value["messagesFinal"]["pending_delivery"]={"invented":"Unconfirmed"};self.rejects(value)

    def test_public_restart_proves_no_replay_and_exact_whole_inventory(self):
        for case in R["RESTART_SCENARIOS"]:
            exercise=self.good(case);restarted=self.restart(exercise)
            for mutation in ("request","click","ticket","extra-authority"):
                value=copy.deepcopy(restarted)
                if mutation=="request":value["attempts"]=[exercise["attempts"][0]]
                elif mutation=="click":value["details"]["restartClick"]["clicked"]=False
                elif mutation=="ticket":value["durable"]["channel"]["stored"]["state"]["check_deadline"]=None
                else:value["initial"]["channel"]["stored"]["state"]["invented_authority"]=True
                with self.subTest(case=case,mutation=mutation):self.rejects(value)
            changed=copy.deepcopy(restarted["durable"]);changed["channel"]["canonical"]["composer"]["selection"]["start"]=0
            with self.assertRaises(RuntimeError):R["validate_restart_pair"](case,exercise["durable"],changed)

    def test_artifacts_require_owned_paths_exact_edges_and_complete_png(self):
        value=self.good();names=R["stages"](value["scenario"],value["phase"])
        value["captures"]={}
        for stage in names:
            path=self.root/f"{value['fixture']}-{value['scenario']}-{value['phase']}-{stage}.png"
            path.write_bytes(png());value["captures"][stage]=str(path)
        edges=[dict(stage=stage,edge=edge) for stage in names for edge in ("before","after")]
        def check():
            return R["validate_artifacts"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,
                self.database,self.root,value,edges,None)
        check();path=value["captures"]["initial"]
        value["captures"]["initial"]=str(self.root/".."/"outside.png")
        with self.assertRaises(RuntimeError):check()
        value["captures"]["initial"]=path;edges.reverse()
        with self.assertRaises(RuntimeError):check()
        edges.reverse();edges.append(edges[-1])
        with self.assertRaises(RuntimeError):check()
        edges.pop();Path(path).write_bytes(png()[:-12])
        with self.assertRaises(RuntimeError):check()
        Path(path).write_bytes(png());value["captures"]["invented"]=path
        with self.assertRaises(RuntimeError):check()

    def test_empty_schedule_body_cannot_claim_a_rendered_message_or_partial_surface(self):
        for mutation in ("missing","invented-message","partial-body","text","rowcount","occlusion"):
            value=self.good("absent-channel");surface=value["geometry"][-1]
            if mutation=="missing":del surface["contentKind"]
            elif mutation=="invented-message":surface["contentKind"]="empty-message"
            elif mutation=="partial-body":surface["row"]["width"]-=10
            elif mutation=="text":surface["text"]="No scheduled messages"
            elif mutation=="rowcount":surface["rowCount"]=1
            else:surface["rowUnobscured"]=False
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_exact_frozen_rich_schedule_identity_and_routing(self):
        for target in ("channel", "thread"):
            op = self.operation(target)
            R["operation"](op, target)
            for field, bad in (("kind", {"Schedule": {"post_at": 501}}), ("local_revision", 1),
                    ("snapshot", R["snapshot"](R["CURRENT"], target)), ("consume_id", None), ("id", "not-a-uuid")):
                changed = copy.deepcopy(op);changed[field] = bad
                with self.subTest(target=target, field=field), self.assertRaises(RuntimeError):
                    R["operation"](changed, target)
        op=self.operation("thread");del op["snapshot"]["destinations"][0]["broadcast"]
        with self.assertRaises(RuntimeError):R["operation"](op,"thread")

    def test_one_actual_read_preserves_all_query_and_form_fields(self):
        for case in R["SCENARIOS"]:
            request=self.attempt(case);R["read_attempt"](request, request["operation"], case)
        for key,bad in (("method","GET"),("sequence",True),("retrySafe",True),("fields",[]),
                ("fields",[["draft_id","DrScheduleChannel"],["blocks","replay"]]),
                ("url","https://fixture.slack.com/api/drafts.info"),
                ("url","https://fixture.slack.com/api/drafts.update")):
            request=self.attempt();request[key]=bad
            with self.subTest(key=key,bad=bad),self.assertRaises(RuntimeError):
                R["read_attempt"](request,request["operation"],"positive-channel")

    def test_pending_absent_changed_and_rate_limited_results_are_distinct(self):
        for case in R["UNCONFIRMED_SCENARIOS"]:
            request=self.attempt(case);request["delivered"]=not request["delivered"]
            with self.subTest(case=case),self.assertRaises(RuntimeError):R["read_attempt"](request,request["operation"],case)
        request=self.attempt("pending-check-channel");request["result"]={}
        with self.assertRaises(RuntimeError):R["read_attempt"](request,request["operation"],"pending-check-channel")
        request=self.attempt("cooldown-channel");request["result"]["cooldownUntilMs"]=219999
        with self.assertRaises(RuntimeError):R["read_attempt"](request,request["operation"],"cooldown-channel")
        request=self.attempt("changed-payload-channel");request["result"]["confirmed"]=True
        with self.assertRaises(RuntimeError):R["read_attempt"](request,request["operation"],"changed-payload-channel")

    def test_whole_inventory_requires_required_nulls_and_byte_boundaries(self):
        value=R["initial_inventory"]("thread",self.operation("thread"));R["inventory_shape"](value)
        for key in R["SYNC_FIELDS"]:
            changed=copy.deepcopy(value);del changed["stored"]["state"][key]
            with self.subTest(missing=key),self.assertRaises(RuntimeError):R["inventory_shape"](changed)
        for mutation in ("unknown","null-selection","split-codepoint","bool-revision"):
            changed=copy.deepcopy(value)
            if mutation=="unknown":changed["stored"]["state"]["invented_authority"]={}
            elif mutation=="null-selection":changed["canonical"]["composer"]["selection"]=None
            elif mutation=="split-codepoint":changed["canonical"]["composer"]["selection"]["start"]=len("Retained local draft ".encode())+1
            else:changed["stored"]["revision"]=True
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):R["inventory_shape"](changed)

    def test_only_pending_and_cooldown_restart(self):
        self.assertEqual(len(R["SCENARIOS"]), 16)
        self.assertEqual(sum(len(R["phases"](case)) for case in R["SCENARIOS"]), 18)
        for case in R["SCENARIOS"]:
            expected = ("exercise", "restart") if case in ("cooldown-channel", "pending-check-channel") else ("exercise",)
            self.assertEqual(R["phases"](case), expected)
        with self.assertRaises(RuntimeError):
            R["phases"]("invented-case")

    def test_exact_owned_readiness_and_no_unplanned_restart(self):
        value = R["ready_tuple"](self.nonce, self.pid, "positive-channel", "exercise")
        self.assertEqual(value, dict(version=1, nonce=self.nonce, pid=self.pid, scenario="positive-channel", phase="exercise"))
        for key, bad in (("pid", True), ("version", True), ("nonce", "b" * 32), ("phase", "restart")):
            changed = dict(value, **{key: bad})
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                R["validate_ready"](changed, value)
        with self.assertRaises(RuntimeError):
            R["validate_ready"](dict(value, extra=True), value)
        for nonce, pid, case, phase in (("A" * 32, 42, "positive-channel", "exercise"),
                (self.nonce, True, "positive-channel", "exercise"),
                (self.nonce, 0, "positive-channel", "exercise"),
                (self.nonce, 42, "positive-channel", "restart")):
            with self.assertRaises(RuntimeError):
                R["ready_tuple"](nonce, pid, case, phase)

    def test_readiness_publishes_exclusively_and_preparation_cannot_launch(self):
        value = R["ready_tuple"](self.nonce, self.pid, "positive-channel", "exercise")
        R["publish_ready"](self.database, value)
        path = self.root / "ready-exercise.json"
        self.assertEqual(json.loads(path.read_text(encoding="utf-8")), value)
        with self.assertRaises(FileExistsError):
            R["publish_ready"](self.database, value)
        self.assertFalse((self.root / "ready-exercise.staged").exists())
        if not R["CONTRACT_READY"]:
            with self.assertRaisesRegex(RuntimeError, "before promotion"):
                R["run"]("draft-schedule-recovery", "positive-channel", self.root / "missing.exe", self.root / "captures")
            with self.assertRaisesRegex(RuntimeError, "before promotion"):
                R["run_phase"]("draft-schedule-recovery", "positive-channel", "exercise", self.nonce,
                    self.database, self.root / "missing.exe", self.root / "captures", "0" * 64)
            self.assertFalse((self.root / "captures").exists())


if __name__ == "__main__":
    unittest.main()
