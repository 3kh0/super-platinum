"""Pure rejection tests for staged Send evidence; no native launch/session."""
import copy
import json
import math
from pathlib import Path
import runpy
import tempfile
import unittest

R = runpy.run_path(str(Path(__file__).with_name("check-managed-draft-send-native.py")))
from native_check import expected_viewport
from test_multi_external_select_receipts import png


def hit_samples(value, radius=6):
    result = {}
    for surface in ("field", "composer", "button"):
        rect = value[surface]
        width, height = rect["width"], rect["height"]
        if surface == "button":
            d = radius - (radius - 1) / math.sqrt(2) if radius > 1 else 1
            offsets = [("center", width/2, height/2), ("top", width/2, 1),
                ("right", width-1, height/2), ("bottom", width/2, height-1), ("left", 1, height/2),
                ("top-left", d, d), ("top-right", width-d, d),
                ("bottom-right", width-d, height-d), ("bottom-left", d, height-d)]
            proof = dict(ok=True, method="rounded-arc", computedRadii=[f"{radius}px"]*4, inset=1, radius=radius)
        else:
            offsets = [(f"{i}:{j}", width*x, height*y)
                for i,x in enumerate((.05,.5,.95)) for j,y in enumerate((.05,.5,.95))]
            proof = dict(ok=True, method="rect-grid")
        proof["points"] = [dict(name=name, x=rect["x"]+x, y=rect["y"]+y, owned=True,
            element=dict(tag="BUTTON" if surface == "button" else "DIV", id="", classes=""))
            for name,x,y in offsets]
        result[surface] = proof
    return result


class Receipts(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.db = self.root / "managed-draft-send.sqlite"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, scenario="ack-newer-channel", fixture=None):
        fixture = fixture or R["FIXTURE"] + "-narrow"
        target = "thread" if scenario.endswith("thread") else "channel"
        viewport = expected_viewport(fixture)
        positive, held = scenario.startswith("ack-newer-"), scenario.startswith("held-response-")
        consume = dict(id="01234567-89ab-4def-8123-456789abcdef",scope=R["scope"](target),client_msg_id=R["client"](target),
            local_revision=1,kind="Send",snapshot=R["snapshot"](R["REQUESTED"],target,target=="thread"),base=R["base"](target))
        op = dict(consume,id="11234567-89ab-4def-8123-456789abcdef",consume_id=consume["id"])
        def inventory(which,text,broadcast,revision,local_revision,workflow="Idle",consumption=None):
            byte_end=len(text.encode("utf-8"))
            return dict(scope=R["scope"](which),canonical=dict(composer=dict(text=text,reply_broadcast=broadcast,
                selection=dict(start=byte_end,end=byte_end)),attachments=[]),
                blocked=None,alternates=[],stored=dict(revision=revision,state=dict(scope=R["scope"](which),
                client_msg_id=R["client"](which),desired=R["snapshot"](text,which,broadcast) if text else None,
                local_revision=local_revision,acknowledged_local_revision=0,acknowledged=R["base"](which),workflow=workflow,
                consumption=consumption,tombstone=False,retired_ids=[],check_deadline=None,check_ticket=None,observed_events={},blocked_observation=None)))
        initial={which:inventory(which,R["BASE"],which=="thread",1,0) for which in ("channel","thread")}
        def pair(text,broadcast,revision,local_revision,workflow="Idle",consumption=None):
            value=copy.deepcopy(initial)
            value[target]=inventory(target,text,broadcast,revision,local_revision,workflow,consumption)
            return value
        before=pair(R["REQUESTED"],target=="thread",2,1)
        pending=pair("",False,4,2,{"InFlight":op},consume)
        newer=pair(R["NEWER"],False,5,3,{"InFlight":op},consume)
        final=copy.deepcopy(newer)
        if positive:final[target]["stored"]=None
        elif not held:
            final[target]["stored"]["revision"]=6
            final[target]["stored"]["state"]["workflow"]={"Uncertain":op}
        def native(text):
            return dict(ok=True,text=text,display=text,scope="fixture-scope",visit="1",codePoints=list(map(ord,text)),
                utf16Length=len(text.encode("utf-16-le"))//2,start=0,end=0,direction="none",composing=False,
                rect=dict(left=50,top=210,right=250,bottom=240,width=200,height=30),viewport=viewport)
        def control(clicked=False):
            value = dict(ok=True,kind="composer",target=target,clickRequested=clicked,clicked=clicked,disabled=False,
                fieldFocused=not clicked,buttonFocused=clicked,
                fieldUnobscured=True,composerUnobscured=True,buttonUnobscured=True,targetMatches=True,
                field=dict(x=50,y=210,width=200,height=30),composer=dict(x=40,y=190,width=300,height=80),
                button=dict(x=280,y=220,width=40,height=30),pane=dict(x=30,y=20,width=350,height=300),
                header=dict(x=30,y=20,width=350,height=40),scope="fixture-scope",visit="1",viewport=viewport,documentWidth=viewport[0])
            value["hitSamples"] = hit_samples(value)
            return value
        query=("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
            "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current&_x_gantry=true"
            "&_x_num_retries=0&_x_version_ts=capture-required&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        form=[["blocks",json.dumps(consume["snapshot"]["blocks"],ensure_ascii=False,sort_keys=True,separators=(",",":"))],
            ["channel","C2"],["client_msg_id",R["client"](target)],["draft_id",R["base"](target)["id"]]]
        if target=="thread":form += [["thread_ts",R["THREAD"]],["reply_broadcast","true"]]
        raw=dict(ok=True,channel="C2",ts=R["ACK_TS"],draft_id=R["base"](target)["id"],message=dict(client_msg_id=R["client"](target),
            user="U00000001",channel="C2",ts=R["ACK_TS"],text=R["REQUESTED"],blocks=consume["snapshot"]["blocks"]))
        if target=="thread":raw["message"]["thread_ts"]=R["THREAD"]
        attempt=dict(sequence=1,method="POST",url="https://fixture.slack.com/api/chat.postMessage?"+query,form=form,retrySafe=False,
            operation=op,consume=consume,rawResponse=raw if positive else None,error="transport" if not positive and not held else None,
            outcome="sent" if positive else "pending" if held else "uncertain",delivered=not held)
        messages_initial={"channel":[],"thread":[],"pending":{"channel":[],"thread":[]},"pending_delivery":{}}
        messages=copy.deepcopy(messages_initial)
        optimistic_ts="1800000000.000001"
        messages[target]=[R["frozen_message"](target,optimistic_ts)]
        messages["pending"][target]=[optimistic_ts]
        final_messages=copy.deepcopy(messages)
        if positive:
            final_messages[target][0]["ts"]=R["ACK_TS"]
            final_messages["pending"][target]=[]
            if target=="thread":final_messages["channel"]=copy.deepcopy(final_messages[target])
        elif not held:
            final_messages["pending_delivery"][optimistic_ts]="Unconfirmed"
        details=dict(nativeInitial=native(R["BASE"]),nativeInput=native(R["REQUESTED"]),nativeConsumed=native(""),
            nativeNewer=native(R["NEWER"]),nativeBeforeRelease=native(R["NEWER"]),nativeFinal=native(R["NEWER"]),
            beforeReleaseFocus=dict(ok=True,fieldFocused=True,buttonFocused=False,activeId=target+"-composer"),
            finalFocus=dict(ok=True,fieldFocused=True,buttonFocused=False,activeId=target+"-composer"),
            beforeSend=before,heldAfterConsume=pending,heldAfterTyping=newer,
            actualClick=control(True),duplicateClick=control(True),messagesHeld=messages,messagesFinal=final_messages,optimisticTs=optimistic_ts)
        value=dict(ok=True,fixture=fixture,scenario=scenario,phase="exercise",nonce=self.nonce,pid=self.pid,isolatedDatabase=str(self.db),
            target=target,viewport=viewport,noSession=True,noTransport=True,noCache=True,noProductionRoute=True,syntheticInput=True,
            actualSendControl=True,actualPreparedRequests=True,preparedRequests=1,attempts=[attempt],routedInputs=2,refusedInputs=0,
            operation=op,consumption=consume,initial=initial,durable=final,messagesInitial=messages_initial,details=details,
            geometry=[dict(control(),stage=stage,native=native(text)) for stage,text in zip(R["stages"](scenario,"exercise"),
                (R["BASE"],R["REQUESTED"],"",R["NEWER"],R["NEWER"]))])
        self.artifacts(value)
        return value

    def artifacts(self,value):
        value["captures"]={}
        for stage in R["stages"](value["scenario"],value["phase"]):
            path=self.root/f"{value['fixture']}-{value['scenario']}-{value['phase']}-{stage}.png"
            path.write_bytes(png());value["captures"][stage]=str(path)

    def restart(self,exercise):
        value=copy.deepcopy(exercise);target=value["target"]
        if value["scenario"].startswith("held-response-"):
            stored=value["durable"][target]["stored"]
            stored["state"]["workflow"]={"Uncertain":value["operation"]};stored["revision"]+=1
        value.update(phase="restart",initial=copy.deepcopy(value["durable"]),preparedRequests=0,attempts=[],routedInputs=0)
        last=value["geometry"][-1];last["stage"]="restart";value["geometry"]=[last]
        details={"nativeInitial":copy.deepcopy(value["details"]["nativeFinal"])}
        if value["scenario"].startswith("ack-newer-"):
            value["operation"]=None;value["consumption"]=None
        else:
            last.update(fieldFocused=False,buttonFocused=True)
            details.update(beforeBlocked=copy.deepcopy(value["durable"]),afterBlocked=copy.deepcopy(value["durable"]),
                nativeBeforeBlocked=copy.deepcopy(details["nativeInitial"]),nativeAfterBlocked=copy.deepcopy(details["nativeInitial"]),
                blockedClick=value["details"]["duplicateClick"],messagesAfterBlocked=copy.deepcopy(value["messagesInitial"]),
                blockedFocus=dict(ok=True,fieldFocused=False,buttonFocused=True,activeId=""),
                blockedFeedback=dict(ok=True,modelText=R["BLOCKED_TEXT"],renderedText=R["BLOCKED_TEXT"],rendered=True,
                    rect=dict(x=50,y=100,width=300,height=60),viewport=value["viewport"]))
        value["details"]=details
        self.artifacts(value)
        return value

    def validate(self,value):
        return R["validate"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.db,value)

    def rejects(self,value):
        with self.assertRaises(RuntimeError):self.validate(value)

    def test_every_case_both_sizes_and_exact_restart_pair(self):
        for fixture in R["FIXTURES"]:
            for scenario in R["SCENARIOS"]:
                with self.subTest(fixture=fixture,scenario=scenario):
                    value=self.good(scenario,fixture);self.validate(value)
                    restarted=self.restart(value);self.validate(restarted)
                    R["validate_restart_pair"](scenario,value["durable"],restarted["durable"])

    def test_single_request_route_query_form_and_retry_classification(self):
        for key,bad in (("method","GET"),("sequence",True),("retrySafe",True),("form",[]),
            ("url","https://fixture.slack.com/api/chat.postMessage"),("url","https://other.test/api/chat.postMessage")):
            value=self.good();value["attempts"][0][key]=bad;self.rejects(value)
        value=self.good();value["attempts"]*=2;self.rejects(value)
        value=self.good("ack-newer-thread");value["attempts"][0]["form"][-1][1]="false";self.rejects(value)

    def test_frozen_identity_base_and_content_cannot_follow_newer_typing(self):
        for key,bad in (("local_revision",3),("consume_id","wrong"),("snapshot",None),("kind","Update")):
            value=self.good();value["operation"][key]=bad;self.rejects(value)
        value=self.good();value["consumption"]["base"]["id"]="wrong";self.rejects(value)

    def test_positive_lost_held_response_evidence_are_distinct(self):
        for scenario,key,bad in (("ack-newer-channel","rawResponse",None),("lost-ack-channel","error",None),
            ("held-response-thread","delivered",True),("held-response-channel","outcome","uncertain")):
            value=self.good(scenario);value["attempts"][0][key]=bad;self.rejects(value)

    def test_newer_thread_input_does_not_inherit_consumed_broadcast(self):
        value=self.good("ack-newer-thread");value["durable"]["thread"]["canonical"]["composer"]["reply_broadcast"]=True;self.rejects(value)

    def test_durable_admission_revision_and_immutable_pending_state(self):
        for detail,key,bad in (("heldAfterConsume","local_revision",1),("heldAfterTyping","consumption",None),
            ("beforeSend","workflow",{"InFlight":{}})):
            value=self.good();value["details"][detail]["channel"]["stored"]["state"][key]=bad;self.rejects(value)
        value=self.good();value["details"]["heldAfterConsume"]["channel"]["stored"]["revision"]=3;self.rejects(value)

    def test_only_exact_held_restart_normalization_is_allowed(self):
        value=self.good("held-response-thread");restarted=self.restart(value)
        for change in ("revision","content","operation","extra"):
            changed=copy.deepcopy(restarted["durable"]);stored=changed["thread"]["stored"]
            if change=="revision":stored["revision"]+=1
            elif change=="content":changed["thread"]["canonical"]["composer"]["text"]="replacement"
            elif change=="operation":stored["state"]["workflow"]["Uncertain"]["id"]="wrong"
            else:stored["state"]["extra"]=True
            with self.subTest(change=change),self.assertRaises(RuntimeError):
                R["validate_restart_pair"](value["scenario"],value["durable"],changed)

    def test_restart_block_requires_actual_click_and_no_mutation_or_replay(self):
        for change in ("click","request","durable","native"):
            value=self.restart(self.good("lost-ack-thread"))
            if change=="click":value["details"]["blockedClick"]["clicked"]=False
            elif change=="request":value["attempts"]=[{}]
            elif change=="durable":value["details"]["afterBlocked"]["thread"]["canonical"]["composer"]["text"]="wrong"
            else:value["details"]["nativeAfterBlocked"]["visit"]="other"
            self.rejects(value)

    def test_unicode_selection_and_native_scope_survive_response(self):
        for key,bad in (("utf16Length",1),("visit","other"),("scope","other"),("direction","forward")):
            value=self.good();value["details"]["nativeFinal"][key]=bad;self.rejects(value)

    def test_geometry_requires_whole_composer_and_button_unobscured(self):
        for flag in ("fieldUnobscured","composerUnobscured","buttonUnobscured","targetMatches"):
            for bad in (False,None,1,"true"):
                value=self.good();value["geometry"][0][flag]=bad;self.rejects(value)
            value=self.good();del value["geometry"][0][flag];self.rejects(value)
        for key in ("composer","field","button","header"):
            for axis in ("x","y"):
                value=self.good();value["geometry"][0][key][axis]=0;self.rejects(value)

    def test_actual_message_projection_keeps_frozen_text_and_positive_ts_only(self):
        for change in ("duplicate","newer","timestamp","broadcast"):
            value=self.good("ack-newer-thread")
            messages=value["details"]["messagesFinal"]
            if change=="duplicate":messages["thread"]*=2
            elif change=="newer":messages["thread"][0]["text"]=R["NEWER"]
            elif change=="timestamp":messages["thread"][0]["ts"]="pending-1"
            else:messages["channel"]=[]
            self.rejects(value)

    def test_flags_are_strict_and_no_production_route_is_required(self):
        for key,bad in (("preparedRequests",True),("pid",True),("noSession",1),("noCache",False),("actualSendControl",False)):
            value=self.good();value[key]=bad;self.rejects(value)

    def test_artifact_paths_edges_and_full_png(self):
        value=self.good();edges=[dict(stage=s,edge=e) for s in R["stages"](value["scenario"],value["phase"]) for e in ("before","after")]
        def check():
            return R["validate_artifacts"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.db,self.root,value,edges,None)
        check();value["captures"]["initial"]=str(self.root/".."/"outside.png")
        with self.assertRaises(RuntimeError):check()
        self.artifacts(value);edges.append(edges[-1])
        with self.assertRaises(RuntimeError):check()
        edges.pop();path=Path(value["captures"]["pending"]);path.write_bytes(path.read_bytes()[:-12])
        with self.assertRaises(RuntimeError):check()

    def test_exact_readiness_and_preparation_cannot_launch(self):
        expected=R["ready_tuple"](self.nonce,self.pid,"ack-newer-channel","exercise")
        for key,bad in (("pid",True),("phase","restart"),("extra",True)):
            value=dict(expected);value[key]=bad
            with self.assertRaises(RuntimeError):R["validate_ready"](value,expected)
        if not R["CONTRACT_READY"]:
            with self.assertRaisesRegex(RuntimeError,"cannot launch before promotion"):
                R["run"](R["FIXTURE"],"ack-newer-channel",self.root/"not-launched.exe",self.root)

    def test_complete_dictionary_keys_and_null_fields_cannot_disappear(self):
        for location in ("inventory","canonical","composer","selection","stored","state"):
            for change in ("missing","extra"):
                value=self.good("held-response-channel");row=value["durable"]["channel"]
                container={"inventory":row,"canonical":row["canonical"],"composer":row["canonical"]["composer"],
                    "selection":row["canonical"]["composer"]["selection"],"stored":row["stored"],"state":row["stored"]["state"]}[location]
                if change=="missing":
                    field="check_ticket" if location=="state" else next(iter(container));del container[field]
                else:container["unknown_authority"]=True
                with self.subTest(location=location,change=change):self.rejects(value)

    def test_canonical_selection_is_exact_byte_end_and_revision_steps_are_exact(self):
        value=self.good();value["durable"]["channel"]["canonical"]["composer"]["selection"]["end"]=len(R["NEWER"]);self.rejects(value)
        for scenario,location in (("held-response-channel","heldAfterTyping"),("lost-ack-channel","durable")):
            value=self.good(scenario);inventory=value[location] if location=="durable" else value["details"][location]
            inventory["channel"]["stored"]["revision"]+=1;self.rejects(value)
        value=self.good();value["initial"]["thread"]["stored"]["state"]["workflow"]={"Uncertain":{}};self.rejects(value)

    def test_focus_restoration_and_shared_blocked_feedback_are_required(self):
        for name in ("beforeReleaseFocus","finalFocus"):
            value=self.good();value["details"][name]["fieldFocused"]=False;self.rejects(value)
        value=self.good();value["details"]["actualClick"]["buttonFocused"]=False;self.rejects(value)
        value=self.good();value["refusedInputs"]=1;self.rejects(value)
        for field,bad in (("rendered",False),("modelText","ignored"),("renderedText","ignored"),("ok",1)):
            value=self.restart(self.good("lost-ack-channel"));value["details"]["blockedFeedback"][field]=bad;self.rejects(value)
        value=self.restart(self.good("lost-ack-channel"));del value["details"]["blockedFeedback"];self.rejects(value)

    def test_frozen_message_has_exact_routing_blocks_sender_and_default_shape(self):
        for scenario in ("ack-newer-channel","ack-newer-thread","held-response-thread","lost-ack-channel"):
            for field,bad in (("channel","C9"),("thread_ts","wrong"),("subtype","wrong"),("user","UOTHER"),
                              ("type","other"),("blocks",[]),("extra_authority",True),("files",[{}])):
                value=self.good(scenario);value["details"]["messagesFinal"][value["target"]][0][field]=bad
                with self.subTest(scenario=scenario,field=field):self.rejects(value)
        value=self.good();del value["details"]["messagesHeld"]["channel"][0]["bot_id"];self.rejects(value)

    def test_positive_removes_optimistic_pending_and_delivery_evidence(self):
        for mutation in ("pending","delivery","optimistic-message"):
            value=self.good("ack-newer-thread");held=value["details"]["messagesHeld"];final=value["details"]["messagesFinal"]
            ts=held["thread"][0]["ts"]
            if mutation=="pending":final["pending"]["thread"].append(ts)
            elif mutation=="delivery":final["pending_delivery"][ts]="Unconfirmed"
            else:
                extra=copy.deepcopy(held["thread"][0]);extra["client_msg_id"]="unrelated-client";final["thread"].append(extra)
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_held_and_lost_delivery_states_cannot_be_invented_or_dropped(self):
        for scenario in ("held-response-channel","lost-ack-thread"):
            for mutation in ("pending","status","extra-pending","extra-delivery"):
                value=self.good(scenario);target=value["target"];final=value["details"]["messagesFinal"];ts=final[target][0]["ts"]
                if mutation=="pending":final["pending"][target]=[]
                elif mutation=="status":final["pending_delivery"][ts]="Failed" if scenario.startswith("lost") else "Unconfirmed"
                elif mutation=="extra-pending":final["pending"][target].append("1800000000.000002")
                else:final["pending_delivery"]["1800000000.000002"]="Unconfirmed"
                with self.subTest(scenario=scenario,mutation=mutation):self.rejects(value)
        value=self.good("lost-ack-channel");value["details"]["messagesFinal"]["pending_delivery"]={};self.rejects(value)

    def test_unrelated_message_vectors_and_absent_bags_are_preserved(self):
        for mutation in ("wrong-copy","unrelated-change","missing-bag","initial-match"):
            value=self.good("held-response-thread");final=value["details"]["messagesFinal"]
            if mutation=="wrong-copy":final["channel"]=copy.deepcopy(final["thread"])
            elif mutation=="unrelated-change":final["channel"].append(dict(client_msg_id="other",text="invented"))
            elif mutation=="missing-bag":final["channel"]=None
            else:value["messagesInitial"]["channel"]=copy.deepcopy(final["thread"])
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_pending_inventory_keys_and_initial_optimistic_identity_are_exact(self):
        for name in ("pending","pending_delivery"):
            value=self.good();del value["details"]["messagesHeld"][name];self.rejects(value)
        value=self.good("held-response-channel");value["details"]["messagesHeld"]["channel"][0]["ts"]="pending-1";self.rejects(value)
        value=self.good();value["details"]["optimisticTs"]="1800000000.000002";self.rejects(value)
        value=self.good();del value["details"]["optimisticTs"];self.rejects(value)

    def test_hit_samples_require_complete_ordered_nine_point_proofs(self):
        for surface in ("field", "composer", "button"):
            for mutation in ("missing", "extra", "short", "long", "reorder", "duplicate", "method", "point-field"):
                value=self.good(); samples=value["geometry"][0]["hitSamples"]; proof=samples[surface]
                if mutation=="missing":del samples[surface]
                elif mutation=="extra":proof["invented"]=True
                elif mutation=="short":proof["points"].pop()
                elif mutation=="long":proof["points"].append(copy.deepcopy(proof["points"][0]))
                elif mutation=="reorder":proof["points"].reverse()
                elif mutation=="duplicate":proof["points"][1]=copy.deepcopy(proof["points"][0])
                elif mutation=="method":proof["method"]="center-only"
                else:del proof["points"][0]["element"]
                with self.subTest(surface=surface,mutation=mutation):self.rejects(value)
        value=self.good();del value["geometry"][0]["hitSamples"];self.rejects(value)

    def test_hit_samples_require_strict_success_and_actual_hit_elements(self):
        for surface in ("field", "composer", "button"):
            for key in ("ok", "owned"):
                for bad in (False, 1, None):
                    value=self.good();proof=value["details"]["actualClick"]["hitSamples"][surface]
                    (proof if key=="ok" else proof["points"][0])[key]=bad
                    with self.subTest(surface=surface,key=key,bad=bad):self.rejects(value)
            for bad in (None, {}, dict(tag="",id="",classes=""), dict(tag="DIV",id=1,classes="")):
                value=self.good();value["geometry"][0]["hitSamples"][surface]["points"][0]["element"]=bad;self.rejects(value)

    def test_hit_samples_reject_wrong_coordinates_and_old_rounded_corners(self):
        for surface in ("field", "composer", "button"):
            for coordinate in (True, float("nan"), float("inf"), -1, 0, 9999):
                value=self.good();value["geometry"][0]["hitSamples"][surface]["points"][0]["x"]=coordinate
                with self.subTest(surface=surface,coordinate=coordinate):self.rejects(value)
            value=self.good();value["geometry"][0]["hitSamples"][surface]["points"][0]["y"]+=.01;self.rejects(value)
        value=self.good();control=value["geometry"][0];point=control["hitSamples"]["button"]["points"][5]
        point.update(x=control["button"]["x"]+.05*control["button"]["width"],
                     y=control["button"]["y"]+.05*control["button"]["height"])
        self.rejects(value)

    def test_rounded_hit_samples_fail_closed_on_unsupported_radii(self):
        cases=[("radius",bad) for bad in (True,-1,16,float("nan"),float("inf"))]
        cases += [("inset",bad) for bad in (True,0,2)]
        cases += [("computedRadii",bad) for bad in (None,[],["6px"]*3,["6%"]*4,
            ["6px 7px"]*4,["6px","5px","6px","6px"],["7px"]*4,["calc(6px)"]*4,["٦px"]*4)]
        for key,bad in cases:
            value=self.good();value["geometry"][0]["hitSamples"]["button"][key]=bad
            with self.subTest(key=key,bad=bad):self.rejects(value)

    def test_real_28_pixel_button_and_supported_radius_boundaries(self):
        for radius in (0,1,6,14):
            value=self.good();control=value["geometry"][0];control["button"].update(width=28,height=28)
            control["hitSamples"]=hit_samples(control,radius)
            with self.subTest(radius=radius):self.validate(value)
        value=self.good();control=value["geometry"][0]
        control["hitSamples"]["button"]["computedRadii"]=["6px 6px"]*4;self.validate(value)

    def test_blocked_restart_does_not_project_messages_pending_or_delivery(self):
        for mutation in ("message","pending","delivery","missing"):
            value=self.restart(self.good("lost-ack-thread"));content=value["details"]["messagesAfterBlocked"]
            if mutation=="message":content["thread"].append(R["frozen_message"]("thread","1800000000.000002"))
            elif mutation=="pending":content["pending"]["thread"].append("1800000000.000002")
            elif mutation=="delivery":content["pending_delivery"]["1800000000.000002"]="Unconfirmed"
            else:del value["details"]["messagesAfterBlocked"]
            with self.subTest(mutation=mutation):self.rejects(value)


if __name__=="__main__":unittest.main()
