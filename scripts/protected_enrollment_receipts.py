"""Strict enrollment receipt oracle; staged copies cannot launch the harness."""
from pathlib import Path
import runpy
import copy

SCRIPTS = next(parent / "scripts" for parent in Path(__file__).resolve().parents
               if (parent / "Cargo.toml").is_file() and (parent / "scripts/native_fixture.py").is_file())
BASE = runpy.run_path(str(SCRIPTS / "protected_drafts_receipts.py"))
for name in ("NativeFixture", "DEFAULT_BINARY", "ROOT", "expected_viewport", "sha", "png",
             "validate_exit", "normalize", "validate_pointer_cleanup", "closed_receipts", "strict_equal",
             "number", "rectangle", "require", "same", "native", "uuid4", "keys"):
    globals()[name] = BASE[name]

FIXTURES = ("protected-enrollment", "protected-enrollment-narrow")
SCENARIOS = ("enroll-channel", "enroll-thread", "late-typing-channel", "owner-change-channel",
             "failed-editor-channel", "canceled-waiter-channel")
FLAG = "SUPER_PLATINUM_PROTECTED_ENROLLMENT_CHECK"
PREFIX, WAITING, STAGE = ("protected-enrollment-" + name + ": " for name in ("check", "waiting", "stage"))
CONTRACT_READY = True


def require_contract():
    require(CONTRACT_READY and Path(__file__).resolve().parent == SCRIPTS,
            "Enrollment preparation cannot launch before contract completion and promotion")


def phases(scenario):
    require(scenario in SCENARIOS, "Unknown enrollment scenario")
    return ("exercise",)


def stages(scenario, phase="exercise"):
    require(phase in phases(scenario), "Unknown enrollment phase")
    if scenario.startswith("enroll-"):
        return ("initial", "dialog", "prepared", "result", "reloaded")
    if scenario == "failed-editor-channel":
        return ("initial", "input", "dialog", "result", "reloaded")
    return ("initial", "dialog", "enqueued" if scenario == "canceled-waiter-channel" else "prepared",
            "interleaved", "result", "reloaded")


def validate_closed_storage(storage,receipt):
    module=runpy.run_path(str(Path(__file__).with_name("protected_enrollment_storage.py")))
    return module["validate"](storage,receipt,globals())


def pages(scenario, stage):
    require(stage in stages(scenario), "Unknown enrollment stage")
    if stage == "dialog":
        return ("channel", "thread", "pending") if scenario == "failed-editor-channel" else ("channel", "thread")
    if stage in ("prepared", "enqueued") or stage == "interleaved" and scenario != "canceled-waiter-channel":
        return ("modal",)
    return ("status",)


def capture_names(scenario, phase="exercise"):
    return tuple(stage + "-" + page if stage == "dialog" else stage
                 for stage in stages(scenario, phase) for page in pages(scenario, stage))


ACCOUNT="00000000-0000-4000-8000-000000000251"
THREAD="1719800400.000000"
INITIAL="Local draft ready for recovery"
NEWER="Later local input "+chr(0x2014)+" "+"".join(map(chr,(0x65e5,0x672c,0x8a9e)))+" "+chr(0x1f680)+" e"+chr(0x301)
local=BASE["local"]
encoded=BASE["encoded"]
G=runpy.run_path(str(Path(__file__).with_name("protected_enrollment_geometry.py")))
G["install"](globals())


def scope(target):return dict(account_id=ACCOUNT,key=dict(team="T1",channel="C2",thread=THREAD if target=="thread" else None))


def empty_inventory(target,clock=4,text=INITIAL):
    return dict(scope=scope(target),clock=clock,row=None,local=local(text,target=="thread"),blocked=None,alternates=[],origins=[],uncommitted=None)


def enrolled(before,actual):
    target="thread" if before["scope"]["key"]["thread"] else "channel"
    row=actual.get("row",{});state=row.get("state",{});lineage=row.get("lineage",{})
    require(uuid4(state.get("client_msg_id")) and uuid4(lineage.get("enrollment")),"Missing actual issued enrollment identities")
    expected=copy.deepcopy(before);expected["clock"]+=1
    expected["row"]=dict(scope=scope(target),revision=1,lineage=dict(enrollment=lineage["enrollment"],verifiedEpoch=0,basis="Unverified"),
        state=dict(scope=scope(target),client_msg_id=state["client_msg_id"],desired=encoded(INITIAL,target,target=="thread"),local_revision=1,
                   acknowledged_local_revision=0,acknowledged=None,workflow="Idle",tombstone=False,retired_ids=[],consumption=None,
                   check_deadline=None,check_ticket=None,observed_events={},blocked_observation=None))
    same(actual,expected,"Wrong exact first enrollment inventory")
    return expected


def review(inv):return {k:copy.deepcopy(inv[k]) for k in ("row","local","blocked","alternates","origins","uncommitted")}


def displayed(inv,status="Ready",pending=False):
    entries={k:dict(kind="managed" if v["row"] else "local",inventory=review(v) if v["row"] else v,uncommitted=None) for k,v in inv.items()}
    if status!="Ready":entries={"channel":None,"thread":None}
    return dict(status=status,entries=entries,recovery=0 if status=="Ready" else None,pendingEnrollment=pending)


def field_scope(p,target):
    thread='Some("'+THREAD+'")' if target=="thread" else "None"
    dest="None" if target=="thread" and p["thread"] is None else 'Some(DraftKey { team: "T1", channel: "C2", thread: '+thread+' })'
    return str(p["viewId"])+":"+str(p["epoch"])+':Some("'+ACCOUNT+'"):'+dest+":"+str(p[target+"Generation"])


def snapshot_native(value,actual,target,generation):
    keys(value,("id","scope","visit","input","composition","composing","wire","start","end","direction","display_start","display_end","display_len"),"shared native snapshot")
    expected=dict(id=target+"-composer",scope=actual["scope"],visit=actual["visit"],input=generation,composition=0,composing=False,wire=actual["text"],
                  start=actual["start"],end=actual["end"],direction=actual["direction"],display_start=actual["start"],display_end=actual["end"],display_len=actual["utf16Length"])
    same(value,expected,"Actual native controller snapshot differs")


def message(scenario,stage):
    if stage=="reloaded":return "Saved drafts reloaded."
    if stage=="interleaved" and scenario=="canceled-waiter-channel":return "Stopped waiting. Check status to recover the enrollment result."
    if stage!="result":return "Review saved drafts before enabling recovery."
    if scenario in ("late-typing-channel","failed-editor-channel"):return "This draft has changed or has input to recover. Review it again before enabling recovery."
    if scenario=="owner-change-channel":return "The draft or its location changed. Reload saved drafts and review it again."
    if scenario=="canceled-waiter-channel":return "The earlier enrollment finished. Reload saved drafts to see the current result."
    return "Enrollment finished. Reload saved drafts to see the current result."


def validate(fixture,scenario,phase,nonce,pid,storage,receipt):
    keys(receipt,("ok","fixture","scenario","phase","nonce","pid","isolatedRoot","target","viewport","captures","snapshots","details","finalInventory","finalProjection","evidenceScope"),"terminal receipt")
    require(fixture in FIXTURES,"Unknown enrollment fixture")
    target="thread" if scenario.endswith("thread") else "channel";failure=scenario=="failed-editor-channel";late=scenario=="late-typing-channel"
    replaced=scenario in ("owner-change-channel","canceled-waiter-channel");success=scenario.startswith("enroll-") or scenario=="canceled-waiter-channel"
    for k,v in dict(ok=True,fixture=fixture,scenario=scenario,phase=phase,nonce=nonce,pid=pid,isolatedRoot=str(storage),target=target,
                    viewport=expected_viewport(fixture),evidenceScope="protected local discovery/enrollment consent; fixture construction has no HTTP capability").items():same(receipt[k],v,"Receipt identity "+k)
    records=receipt["snapshots"];require(isinstance(records,list),"Missing stage records")
    same([v.get("stage") for v in records],list(stages(scenario,phase)),"Wrong semantic stage order")
    snapshots={v["stage"]:v for v in records};initial=snapshots["initial"];p0=initial["projection"]
    inv0={k:empty_inventory(k,6 if failure and k=="channel" else 4) for k in ("channel","thread")}
    same(initial["durable"],inv0,"Wrong whole isolated seed")
    final=copy.deepcopy(inv0)
    if success:final[target]=enrolled(inv0[target],receipt["finalInventory"][target])
    if late:final[target]=empty_inventory(target,7,NEWER)
    same(receipt["finalInventory"],final,"Unexpected final inventory or sibling mutation")
    details=receipt["details"];expected_details={"nativeInitial","open","submit","result","reload"}
    if late or failure:expected_details.add("typed")
    if replaced:expected_details.add("replacement")
    if scenario=="canceled-waiter-channel":expected_details|={"cancel","canceled","check"}
    keys(details,expected_details,"scenario details")
    native(details["nativeInitial"],INITIAL,expected_viewport(fixture));BASE["SCHEDULE"]["home_anchor"](details["nativeInitial"])
    BASE["same_native"](details["nativeInitial"],initial["native"]["native"])
    generation=snapshots["dialog"]["pages"][0]["generation"]
    require(BASE["retention_id"](generation),"Invalid actual dialog identity")
    G["clicked"](details["open"],"open","Review draft",expected_viewport(fixture),None)
    G["clicked"](details["submit"],"submit","Enable recovery",expected_viewport(fixture),generation)
    G["clicked"](details["reload"],"reload","Reload saved drafts",expected_viewport(fixture),None)
    if late or failure:
        G["observed"](details["typed"],fixture,target,NEWER)
        same(details["typed"]["before"],{k:initial["native"]["native"][k] for k in ("text","start","end","direction")},"Input did not start from original native selection")
        typed=details["typed"]["native"];same(typed["start"],len(NEWER[:NEWER.index(chr(0x1f680))].encode("utf-16-le"))//2,"Wrong backward selection start")
        same(typed["end"],len(NEWER.encode("utf-16-le"))//2,"Wrong backward selection end");same(typed["direction"],"backward","Wrong actual input direction")
        same(details["typed"]["fieldFocused"],True,"Typing did not focus actual field")
    if scenario=="canceled-waiter-channel":
        G["clicked"](details["cancel"],"cancel","Cancel waiting",expected_viewport(fixture),generation)
        G["clicked"](details["check"],"check","Check status",expected_viewport(fixture),None)
        same(details["canceled"],dict(kind="canceled",pending=True),"Actual cancellation did not retain pending result")
    if success:
        same(details["result"],dict(kind="enrolled",current=scenario!="canceled-waiter-channel",account=ACCOUNT,expected=inv0[target],review=review(final[target])),"Wrong real enrollment result")
    else:same(details["result"],dict(kind="refused",phase="capture" if failure else "prepare-or-admit",error="Stale" if scenario=="owner-change-channel" else "ReviewRequired"),"Wrong actual refusal phase/reason")
    for record in records:
        keys(record,("stage","pages","native","composer","durable","projection"),"stage record")
        stage=record["stage"];after_replace=replaced and stage in ("interleaved","result","reloaded")
        after_input=failure and stage!="initial" or late and stage in ("interleaved","result","reloaded")
        text=NEWER if after_input else INITIAL
        expected_inv=final if late and after_input or success and stage in ("enqueued","interleaved","result","reloaded") else inv0
        # The enqueued gate observes the real detached FIFO completion by a Current read.
        same(record["durable"],expected_inv,"Wrong complete stage inventory at "+stage)
        G["observed"](record["native"],fixture,target,text)
        same(record["native"]["before"],{k:record["native"]["native"][k] for k in ("text","start","end","direction")},"Read-only native observation changed text or selection")
        require(isinstance(record["pages"],list),"Missing visible pages")
        same([v.get("page") for v in record["pages"]],list(pages(scenario,stage)),"Wrong ordered page set")
        for page in record["pages"]:G["surface"](page,fixture,scenario,stage,page["page"],target,generation,message(scenario,stage))
        p=record["projection"];validate_projection(p,p0,target,text,after_input,after_replace,failure)
        same(record["native"]["native"]["scope"],p["fieldScopes"][target],"Actual field scope differs from model")
        if record["composer"] is not None:
            BASE["geometry"](record["composer"],fixture,target,text,focused=record["composer"]["fieldFocused"],proof=True)
            BASE["same_native"](record["native"]["native"],record["composer"]["native"])
            snapshot_native(record["composer"]["proof"]["current"],record["native"]["native"],target,1 if after_input else 0)
        else:require(pages(scenario,stage)!=("status",),"Status lacks full composer proof")
        baseline=snapshots["interleaved"]["native"]["native"] if after_replace else details["typed"]["native"] if after_input else initial["native"]["native"]
        BASE["same_native"](baseline,record["native"]["native"])
        is_final=stage in ("result","reloaded");pending=scenario=="canceled-waiter-channel" and stage in ("enqueued","interleaved")
        status="NeedsLoad" if success and stage in ("enqueued","interleaved","result") or after_replace and stage!="reloaded" else "Ready"
        shown=final if stage=="reloaded" else inv0
        same(p["discovery"],displayed(shown,status,pending),"Wrong generation-bound displayed discovery")
        expected_result=details["result"] if is_final else details["canceled"] if scenario=="canceled-waiter-channel" and stage=="interleaved" else None
        same(p["result"],expected_result,"Result appeared at wrong stage")
    validate_actions(snapshots,details,scenario,target,inv0,final,generation)
    if replaced:
        keys(details["replacement"],("before","after"),"actual shell replacement")
        before,after=details["replacement"]["before"],details["replacement"]["after"]
        same(before,snapshots["enqueued" if scenario=="canceled-waiter-channel" else "prepared"]["projection"] if scenario!="canceled-waiter-channel" else dict(snapshots["enqueued"]["projection"],result=details["canceled"],fixtureActions=snapshots["enqueued"]["projection"]["fixtureActions"]+[dict(kind="cancel-click",pending=True)]),"Wrong pre-replacement state")
        same(after,snapshots["interleaved"]["projection"],"Replacement evidence differs from observed shell")
        require(initial["native"]["native"]["visit"]!=snapshots["interleaved"]["native"]["native"]["visit"],"Shell replacement did not remount actual field")
    same(receipt["finalProjection"],snapshots["reloaded"]["projection"],"Final projection differs")


def validate_projection(p,p0,target,text,after_input,after_replace,failure):
    keys(p,("account","epoch","viewId","team","channel","thread","channelGeneration","threadGeneration","fieldScopes","sessionPresent","transportPresent","cachePresent","isolated","book","channelComposer","threadComposer","discovery","retained","localWriters","routed","refused","fixtureActions","nativeAdmission","result"),"whole renderer projection")
    for k,v in dict(account=ACCOUNT,team="T1",channel="C2",thread=THREAD if target=="thread" else None,sessionPresent=False,transportPresent=False,cachePresent=False,isolated=True,routed=1 if after_input else 0,refused=0).items():same(p[k],v,"Wrong renderer field "+k)
    for k in ("epoch","viewId","channelGeneration","threadGeneration"):require(type(p[k]) is int and p[k]>=0,"Invalid owner generation")
    same(p["epoch"],p0["epoch"]+(1 if after_replace else 0),"Wrong account epoch")
    if after_replace:require(p["viewId"]!=p0["viewId"],"Replacement reused view identity")
    else:
        for k in ("viewId","channelGeneration","threadGeneration"):same(p[k],p0[k],"Unexpected owner/navigation change")
    same(p["fieldScopes"],{k:field_scope(p,k) for k in ("channel","thread")},"Model field scopes do not bind exact owner")
    book={k:local(text if k==target else INITIAL,k=="thread") for k in ("channel","thread")}
    same(p["book"],book,"Draft book changed outside intended input")
    same(p["channelComposer"],book["channel"]["composer"],"Channel composer differs")
    same(p["threadComposer"],book["thread"]["composer"] if target=="thread" else local("")["composer"],"Inactive/active thread composer differs")
    writers=[dict(key=scope(k)["key"],serial=2 if after_input and not failure and k==target else 1,pending=None,running=False,error=None,failedInputs=[]) for k in ("channel","thread") if not(failure and k=="channel")]
    same(p["localWriters"],writers,"Local writer ownership/serial/result differs")
    if failure:
        require(isinstance(p0["retained"],list) and len(p0["retained"])==1,"Missing unique actual retained failed Editor")
        owner=p0["retained"][0];require(BASE["retention_id"](owner.get("retentionId")) and uuid4(owner.get("writerId")) and uuid4(owner.get("enrollment")),"Invalid retained identities")
        expected=dict(retentionId=owner["retentionId"],scope=scope("channel"),writerId=owner["writerId"],enrollment=owner["enrollment"],serial=1 if after_input else 0,
                      pending=local(NEWER) if after_input else None,failedInputs=[local(NEWER)] if after_input else [],running=False,parked=after_input,error="Storage" if after_input else None)
        same(p["retained"],[expected],"Failed input or actual retained writer was lost/replaced")
    else:same(p["retained"],[],"Enrollment created an unrequested Editor")


def validate_actions(snapshots,details,scenario,target,initial,final,generation):
    failure=scenario=="failed-editor-channel";late=scenario=="late-typing-channel";canceled=scenario=="canceled-waiter-channel"
    p0=snapshots["initial"]["projection"];first=dict(kind="current-load",epoch=p0["epoch"],viewId=p0["viewId"])
    input_action=dict(kind="input",key=scope(target)["key"],serial=1 if failure else 2,local=local(NEWER,target=="thread"))
    opening=[dict(kind="open-click"),first,dict(kind="dialog",id=generation,displayed=displayed(initial),pending=[local(NEWER)] if failure else [])]
    consent=dict(kind="consent-click",id=generation);prepare=dict(kind="prepared",expected=initial[target]);enqueue=dict(kind="enqueue",expected=initial[target])
    replacement=None
    if scenario in ("owner-change-channel","canceled-waiter-channel"):
        before=details["replacement"]["before"];after=details["replacement"]["after"]
        replacement=dict(kind="shell-replaced",before={k:before[k] for k in ("epoch","viewId")},after={k:after[k] for k in ("epoch","viewId")})
    for stage,record in snapshots.items():
        events=[first]
        if failure and stage!="initial":events.append(input_action)
        if stage not in ("initial","input"):events+=opening
        if stage not in ("initial","input","dialog"):
            events.append(consent)
            if not failure:events.append(prepare)
            if scenario.startswith("enroll-") and stage in ("result","reloaded") or canceled:events.append(enqueue)
            if late and stage in ("interleaved","result","reloaded"):events.append(input_action)
            if replacement and stage in ("interleaved","result","reloaded"):
                if canceled:events.append(dict(kind="cancel-click",pending=True))
                events.append(replacement)
            if canceled and stage in ("result","reloaded"):events.append(dict(kind="check-click"))
        if stage=="reloaded":events += [dict(kind="reload-click"),dict(kind="current-load",epoch=record["projection"]["epoch"],viewId=record["projection"]["viewId"])]
        same(record["projection"]["fixtureActions"],events,"Actual action/handler sequence differs at "+stage)
        trace=record["projection"]["nativeAdmission"]
        if stage in ("initial","input"):same(trace,[],"Consent occurred before actual open");continue
        capture=snapshots["dialog"]["projection"]["nativeAdmission"]
        require(isinstance(capture,list) and len(capture)==1,"Expected one original native capture")
        keys(capture[0],("kind","evidence"),"native capture event");same(capture[0]["kind"],"capture","Wrong admission source")
        keys(capture[0]["evidence"],("before",),"native capture evidence")
        snapshot_native(capture[0]["evidence"]["before"],snapshots["dialog"]["native"]["native"],target,1 if failure else 0)
        locked=canceled and stage in ("enqueued","interleaved","result","reloaded") or scenario.startswith("enroll-") and stage in ("result","reloaded")
        if locked:
            require(isinstance(trace,list) and len(trace)==3,"Consent lock/release missing or unexpected clear")
            token=trace[1].get("evidence",{}).get("token")
            require(isinstance(token,str) and token.isdigit() and int(token)>0,"Missing actual lease token")
            same(trace,capture+[dict(kind="lock",evidence=dict(token=token,before=capture[0]["evidence"]["before"])),dict(kind="release-requested",evidence=dict(token=token))],"Unexpected admission/clear sequence")
        else:same(trace,capture,"Refusal fabricated lock/enqueue/clear")
