"""Disconnected Schedule receipt tests: no process, session, HTTP, or native UI."""
import copy
from contextlib import redirect_stdout
from datetime import datetime
import io
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

R = runpy.run_path(str(Path(__file__).with_name("check-managed-draft-schedule-native.py")))
from test_multi_external_select_receipts import png

QUERY = ("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
    "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current&_x_gantry=true"
    "&_x_num_retries=0&_x_version_ts=capture-required&fp=2123456789ab4def8123456789abcdef&slack_route=default")
UUID = "01234567-89ab-4def-8123-456789abcdef"
SUCCESSOR = "31234567-89ab-4def-8123-456789abcdef"


def op(target, scenario, identity=UUID, consume="11234567-89ab-4def-8123-456789abcdef", post_at=2000000040):
    return dict(id=identity, scope=R["scope"](target), client_msg_id=R["client"](target), local_revision=0,
        kind={"Schedule": {"post_at": post_at}}, snapshot=R["rich"](target, scenario in ("retry-channel", "expired-change-thread"), scenario == "expired-change-thread"),
        base=R["base"](target), consume_id=consume)



def anchor():
    expected = dict(scope="T1/C2/1", latestId="fixture-last-row", windowEnd=40)
    before = dict(expected=expected, connected=True, scope=expected["scope"], windowEnd=40,
        latestMounted=True, latestHeight=42, inputGeneration=0, scrollTop=300,
        scrollHeight=800, clientHeight=400, bottomGap=100, ready=True, atBottom=False)
    after = copy.deepcopy(before)
    after.update(scrollTop=400, bottomGap=0, atBottom=True)
    return dict(landed=True, before=before, after=after)

def projection(text, viewport):
    return dict(ok=True, text=text, display=text, codePoints=[ord(c) for c in text], utf16Length=len(text.encode("utf-16-le"))//2,
        viewport=viewport, composing=False, scope="T1/C2", visit="1", start=0, end=0, direction="none",
        rect=dict(left=10, top=10, right=110, bottom=40, width=100, height=30))


def proof(disabled=False, text="", bounds=None):
    bounds=bounds or dict(x=10,y=10,width=200,height=100)
    return dict(rect=bounds, visible=True, unobscured=True, disabled=disabled, text=text, focused=False,
        points=[dict(x=bounds["x"]+x*bounds["width"],y=bounds["y"]+y*bounds["height"],owned=True,
            hit=dict(tag="BUTTON",id="",classes="")) for x in (.15,.5,.85) for y in (.15,.5,.85)])


def action(viewport, disabled=False, text=""):
    return dict(ok=True,clicked=not disabled,control=proof(disabled,text),viewport=viewport,documentWidth=viewport[0])


def notice(viewport, text):
    return dict(ok=True, clicked=True, afterAbsent=True, before=dict(text=text, role="status",
        toast=proof(text=text), dismiss=proof(bounds=dict(x=170,y=45,width=28,height=28)),
        viewport=viewport, documentWidth=viewport[0]))


def review(payload, post_at=2000000040):
    return dict(ok=True, blocks=copy.deepcopy(payload["blocks"]),date=datetime.fromtimestamp(post_at).strftime("%Y-%m-%dT%H:%M"),postAt=post_at,generation="42",composing="false")


def request(operation, index=1, outcome="scheduled"):
    encode=lambda v:json.dumps(v,ensure_ascii=False,separators=(",",":"),sort_keys=True)
    raw=None
    if outcome=="scheduled":
        raw=dict(ok=True,draft=dict(id=operation["base"]["id"],client_msg_id=operation["client_msg_id"],last_updated_ts=R["ACK_TS"],
            date_created=10,date_scheduled=operation["kind"]["Schedule"]["post_at"],is_sent=False,is_deleted=False,is_from_composer=True,
            **copy.deepcopy(operation["snapshot"])))
    if outcome=="rejected":raw=dict(ok=False,error="invalid_blocks")
    return dict(sequence=index,method="POST",url="https://fixture.slack.com/api/drafts.update?"+QUERY,
        form=[["blocks",encode(operation["snapshot"]["blocks"])],["destinations",encode(operation["snapshot"]["destinations"])],
            ["file_ids","[]"],["is_from_composer","true"],["date_scheduled",str(operation["kind"]["Schedule"]["post_at"])],
            ["draft_id",operation["base"]["id"]],["client_last_updated_ts","1999990000000"]],retrySafe=False,
        operation=copy.deepcopy(operation),consume=R["consumption"](operation),rawResponse=raw,error="transport" if outcome=="uncertain" else None,
        outcome=outcome,delivered=outcome!="pending")


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

    def test_retry_remount_anchor_and_native_identity_are_required(self):
        for mutation in ("missing", "anchor", "visit", "text"):
            value = self.good("rejected-retry-channel")
            if mutation == "missing": del value["details"]["retryHomeMount"]
            elif mutation == "anchor": del value["details"]["retryHomeMount"]["homeAnchor"]
            elif mutation == "visit": value["details"]["retryHomeMount"]["visit"] = "another"
            else: value["details"]["retryHomeMount"]["text"] = "older"
            with self.subTest(mutation=mutation): self.rejects(value)
        value = self.restart(self.good("held-response-thread"))
        del value["details"]["nativeInitial"]["homeAnchor"]
        self.rejects(value)

    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.database=self.root/"managed-draft-schedule.sqlite"
        self.nonce="a"*32;self.pid=42

    def geometry(self, value):
        scenario,phase=value["scenario"],value["phase"]
        viewport=value["viewport"];rows=[]
        for stage in R["stages"](scenario,phase):
            base=stage.removesuffix("-date")
            rejected=scenario in ("retry-channel","expired-change-thread")
            surface="composer" if base in ("restart","stale") or base=="initial" and not rejected else "panel" if base in ("initial","result","rejected") else "dialog"
            names=("panel","header","row") if surface=="panel" else ("field","footer","schedule")
            controls={name:proof() for name in names};busy=base in ("pending","retry-pending")
            body_view=body_scroll=None
            if surface=="dialog":
                body_view="date" if stage.endswith("-date") else "editor"
                offset=330 if body_view=="date" else 0
                rects=dict(dialog=(10,10,400,330),header=(20,15,380,30),title=(20,20,200,20),close=(370,16,28,28),
                    destination=(20,45,380,20),body=(20,70,380,210),footer=(20,300,380,30),primary=(220,300,180,30),
                    editorLabel=(20,70-offset,200,15),toolbar=(20,90-offset,380,25),editor=(20,120-offset,380,130),
                    dateLabel=(20,400-offset,200,15),date=(20,420-offset,200,30),hint=(20,460-offset,380,35))
                controls={key:proof(bounds=dict(zip(("x","y","width","height"),rect))) for key,rect in rects.items()}
                for key in (("date","dateLabel","hint") if body_view=="editor" else ("editor","editorLabel","toolbar")):
                    controls[key]["visible"]=R["rectangle"](controls[key]["rect"],viewport)
                    controls[key]["unobscured"]=False
                    for point in controls[key]["points"]:point.update(owned=False,hit=None)
                controls["primary"]["disabled"]=busy or base=="stale";controls["close"]["disabled"]=busy
                review_mode=rejected or base in ("retry-review","retry-pending")
                controls["title"]["text"]="Review schedule" if review_mode else "Schedule message"
                controls["destination"]["text"]="To #ship"+(" \u00b7 Thread reply" if scenario.endswith("thread") else "")
                controls["primary"]["text"]="Saving\u2026" if busy else "Save new schedule" if scenario=="expired-change-thread" else "Retry same schedule" if review_mode else "Schedule message"
                controls["editorLabel"]["text"]="Message";controls["dateLabel"]["text"]="Date and time"
                controls["hint"]["text"]="Uses your local time zone. "+("Your separate composer draft will be kept." if review_mode else "Newer typing will be kept.")
                controls["date"]["disabled"]=busy or (review_mode and scenario!="expired-change-thread")
                sample=dict(generation=value["frozen"]["generation"],scrollTop=0,scrollHeight=540,clientHeight=210,rect=controls["body"]["rect"])
                body_scroll=dict(requested=body_view,before=copy.deepcopy(sample),after=dict(sample,scrollTop=offset))
            current={k:copy.deepcopy(v) for k,v in value["frozen"].items() if k!="ok"} if surface=="dialog" else None
            rows.append(dict(stage=stage,ok=True,surface=surface,controls=controls,viewport=viewport,documentWidth=viewport[0],
                current=current,active=dict(id="",tag="BODY",text=""),text="Schedule message",busy=str(busy).lower() if surface=="dialog" else None,
                threadPaneAbsent=True if surface=="panel" else None,bodyView=body_view,bodyScroll=body_scroll))
        value["geometry"]=rows

    def good(self, scenario="rich-thread", fixture=None):
        fixture=fixture or R["FIXTURE"]+"-narrow";viewport=R["expected_viewport"](fixture)
        target="thread" if scenario.endswith("thread") else "channel"
        rejected=scenario in ("retry-channel","expired-change-thread");actual_retry=scenario=="rejected-retry-channel"
        held=scenario.startswith("held-");lost=scenario.startswith("lost-");stale=scenario.startswith("stale-")
        newer=scenario.startswith("newer-") or held or lost
        operation=op(target,scenario)
        initial={which:R["seed"](which) for which in ("channel","thread")}
        if rejected:
            old=op(target,"retry-channel","21234567-89ab-4def-8123-456789abcdef",post_at=1999900020 if scenario.startswith("expired-") else 2000000040)
            if scenario.startswith("expired-"):old["consume_id"]="41234567-89ab-4def-8123-456789abcdef"
            initial[target]=R["seed"](target,old)
        details=dict(nativeInitial=projection(initial[target]["canonical"]["composer"]["text"],viewport),
            open=action(viewport,text="Review schedule" if rejected else ""),
            reviewInitial=review(R["rich"](target,True) if rejected else R["snapshot"](R["BASE"],target),old["kind"]["Schedule"]["post_at"] if rejected else 2000000040))
        details["nativeInitial"]["homeAnchor"] = anchor()
        if scenario=="expired-change-thread":
            details.update(expiredRetry=action(viewport,True,"Retry same schedule"),change=action(viewport,text="Change schedule"))
        frozen=review(operation["snapshot"])
        final=copy.deepcopy(initial);attempts=[]
        if stale:
            details.update(contextBefore=dict(account=R["ACCOUNT"],epoch=1,viewId=1,team="T1"),
                staleRetained=dict(ok=True,generation=frozen["generation"],dialogConnected=True,submitConnected=True,
                    primary=proof(text="Schedule message"),viewport=viewport,documentWidth=viewport[0]),
                staleRetired=dict(ok=True,generation=frozen["generation"],dialogDisconnected=True,submitDisconnected=True,currentDialogAbsent=True),
                staleNativeBefore=projection(R["BASE"],viewport),staleNativeAfter=projection(R["BASE"],viewport))
            details["staleNativeAfter"].update(scope="T1/C2/remounted",visit="2")
            details["contextAfter"]=dict(details["contextBefore"]);details["contextAfter"]["epoch" if scenario=="stale-owner-channel" else "viewId"]+=1
        else:
            details["submit"]=action(viewport,text="Save new schedule" if scenario=="expired-change-thread" else "Retry same schedule" if rejected else "Schedule message")
            first=copy.deepcopy(operation)
            if actual_retry:first["id"]="21234567-89ab-4def-8123-456789abcdef"
            final=R["pending"](initial,target,first,rejected);details["pending"]=copy.deepcopy(final)
            if actual_retry:
                details["firstOperation"]=first;attempts.append(request(first,1,"rejected"))
                final=R["completed"](final,target,first,rejected=True);details["afterRejection"]=copy.deepcopy(final)
                details["nativeAfterRejection"]=projection("",viewport)
                final=R["typed"](final,target);details["afterRejectionTyping"]=copy.deepcopy(final)
                details.update(newerNative=projection(R["NEWER"],viewport),retryOpen=action(viewport,text="Review schedule"),
                    retryReview=review(operation["snapshot"]),retrySubmit=action(viewport,text="Retry same schedule"))
                final[target]["stored"]["revision"]+=2;final[target]["stored"]["state"]["workflow"]={"InFlight":operation}
                details["retryPending"]=copy.deepcopy(final)
            elif newer:
                final=R["typed"](final,target);details.update(afterNewer=copy.deepcopy(final),newerNative=projection(R["NEWER"],viewport))
            attempts.append(request(operation,len(attempts)+1,"pending" if held else "uncertain" if lost else "scheduled"))
            if not held:final=R["completed"](final,target,operation,successor=SUCCESSOR,uncertain=lost)
            if not held and not rejected:details["nativeAfter"]=projection(R["NEWER"] if newer or actual_retry else "",viewport)
            if actual_retry:
                details["retryHomeMount"] = copy.deepcopy(details["nativeAfter"])
                details["retryHomeMount"]["homeAnchor"] = anchor()
            if lost:details["blockedReplay"]=action(viewport,True)
            if not held:
                details["completionToast"] = notice(viewport, R["UNCERTAIN_NOTICE"] if lost else R["SCHEDULED_NOTICE"])
                if actual_retry: details["rejectionToast"] = notice(viewport, R["REJECTED_NOTICE"])
        messages=dict(channel=[],thread=[],pending=dict(channel=[],thread=[]),pending_delivery={})
        queue={} if stale or held or lost else {operation["base"]["id"]:{k:v for k,v in attempts[-1]["rawResponse"]["draft"].items() if k!="is_from_composer"}}
        value=dict(ok=True,fixture=fixture,scenario=scenario,phase="exercise",nonce=self.nonce,pid=self.pid,isolatedDatabase=str(self.database),
            target=target,viewport=viewport,noSession=True,noTransport=True,noCache=True,noProductionRoute=True,syntheticInput=True,
            actualDialogControl=True,actualPreparedRequests=True,preparedRequests=len(attempts),attempts=attempts,routedInputs=int(newer or actual_retry),refusedInputs=0,
            operation=None if stale else operation,consumption=None if stale else R["consumption"](operation),frozen=frozen,
            initial=initial,durable=final,messagesInitial=messages,messagesFinal=copy.deepcopy(messages),queueInitial={},queueFinal=queue,
            details=details,captures={})
        self.geometry(value)
        return value

    def restart(self, first):
        value=copy.deepcopy(first);target=value["target"];scenario=value["scenario"]
        value.update(phase="restart",attempts=[],preparedRequests=0,routedInputs=0,frozen=None,queueInitial={},queueFinal={})
        if scenario.startswith("held-"):
            value["durable"][target]["stored"]["revision"]+=1
            value["durable"][target]["stored"]["state"]["workflow"]={"Uncertain":value["operation"]}
        value["initial"]=copy.deepcopy(value["durable"])
        value["details"]=dict(nativeInitial=projection(value["initial"][target]["canonical"]["composer"]["text"],value["viewport"]))
        value["details"]["nativeInitial"]["homeAnchor"] = anchor()
        if scenario.startswith(("held-","lost-")):value["details"]["blockedReplay"]=action(value["viewport"],True)
        else:value.update(operation=None,consumption=None)
        self.geometry(value)
        return value

    def validate(self,value):
        R["validate"](value["fixture"],value["scenario"],value["phase"],self.nonce,self.pid,self.database,value)

    def rejects(self,value):
        with self.assertRaises(RuntimeError):self.validate(value)

    def test_all_scenarios_both_sizes_and_exact_restart_pairs(self):
        for fixture in R["FIXTURES"]:
            for scenario in R["SCENARIOS"]:
                with self.subTest(fixture=fixture,scenario=scenario):
                    first=self.good(scenario,fixture);self.validate(first)
                    second=self.restart(first);self.validate(second)
                    R["validate_restart_pair"](scenario,first,second)

    def test_staged_runner_refuses_launch(self):
        self.assertEqual(R["CONTRACT_READY"], Path(__file__).resolve().parent == R["SCRIPTS"])
        with patch.dict(R["require_contract"].__globals__, CONTRACT_READY=False):
            with self.assertRaises(RuntimeError):R["require_contract"]()
            with self.assertRaises(RuntimeError):R["run"](R["FIXTURE"],"rich-channel",None,None)
            with self.assertRaises(RuntimeError):R["run_phase"](*([None]*8))

    def test_stale_retirement_requires_actual_connected_then_disconnected_nodes(self):
        for scenario in ("stale-owner-channel", "stale-visit-thread"):
            for mutation in ("missing", "connected", "retained-generation", "covered", "disabled", "retired-generation", "dialog", "submit", "current", "clicked"):
                value = self.good(scenario)
                details = value["details"]
                if mutation == "missing": del details["staleRetired"]
                elif mutation == "connected": details["staleRetained"]["submitConnected"] = False
                elif mutation == "retained-generation": details["staleRetained"]["generation"] = "other"
                elif mutation == "covered": details["staleRetained"]["primary"]["points"][0]["owned"] = False
                elif mutation == "disabled": details["staleRetained"]["primary"]["disabled"] = True
                elif mutation == "retired-generation": details["staleRetired"]["generation"] = "other"
                elif mutation == "dialog": details["staleRetired"]["dialogDisconnected"] = False
                elif mutation == "submit": details["staleRetired"]["submitDisconnected"] = 1
                elif mutation == "current": details["staleRetired"]["currentDialogAbsent"] = False
                else: details["staleRetired"]["clickedDetached"] = True
                with self.subTest(scenario=scenario, mutation=mutation): self.rejects(value)

    def test_stale_retirement_preserves_native_text_and_requires_new_scope_and_visit(self):
        for mutation in ("old-text", "new-text", "before-scope", "after-scope", "after-visit", "old-click", "extra-stage"):
            value = self.good("stale-visit-thread")
            details = value["details"]
            if mutation == "old-text": details["staleNativeBefore"]["text"] = "wrong"
            elif mutation == "new-text": details["staleNativeAfter"]["text"] = "wrong"
            elif mutation == "before-scope": details["staleNativeBefore"]["scope"] = "wrong"
            elif mutation == "after-scope": details["staleNativeAfter"]["scope"] = details["staleNativeBefore"]["scope"]
            elif mutation == "after-visit": details["staleNativeAfter"]["visit"] = details["staleNativeBefore"]["visit"]
            elif mutation == "old-click": details["staleClick"] = action(value["viewport"],True,"Schedule message")
            else: value["geometry"].append(dict(value["geometry"][-1],stage="stale-date"))
            with self.subTest(mutation=mutation): self.rejects(value)

    def test_reviewed_dialog_generation_cannot_change_between_stages(self):
        for scenario, stage in (("rich-channel", "pending"), ("retry-channel", "pending"),
                                ("expired-change-thread", "pending"), ("rejected-retry-channel", "retry-pending")):
            value = self.good(scenario)
            for row in value["geometry"]:
                if row["stage"].removesuffix("-date") == stage:
                    row["current"]["generation"] = "43"
                    for when in ("before", "after"): row["bodyScroll"][when]["generation"] = "43"
            with self.subTest(scenario=scenario, stage=stage): self.rejects(value)

    def test_explicit_retry_allows_its_new_generation_but_preserves_that_mount(self):
        value = self.good("rejected-retry-channel")
        value["details"]["retryReview"]["generation"] = "43"
        for row in value["geometry"]:
            if row["stage"].startswith("retry-"):
                row["current"]["generation"] = "43"
                for when in ("before", "after"): row["bodyScroll"][when]["generation"] = "43"
        self.validate(value)
        value["details"]["retryReview"]["generation"] = "44"
        self.rejects(value)

    def test_completion_notice_requires_exact_copy_role_click_and_absence(self):
        for scenario, field in (("rich-channel", "completionToast"), ("lost-ack-channel", "completionToast"),
                                ("rejected-retry-channel", "rejectionToast")):
            for mutation in ("missing", "extra", "ok", "clicked", "absent", "role", "copy", "measured-copy", "viewport", "overflow"):
                value = self.good(scenario)
                receipt = value["details"][field]
                if mutation == "missing": del value["details"][field]
                elif mutation == "extra": receipt["fabricated"] = None
                elif mutation == "ok": receipt["ok"] = 1
                elif mutation == "clicked": receipt["clicked"] = False
                elif mutation == "absent": receipt["afterAbsent"] = False
                elif mutation == "role": receipt["before"]["role"] = "alert"
                elif mutation == "copy": receipt["before"]["text"] = "Done"
                elif mutation == "measured-copy": receipt["before"]["toast"]["text"] = "Done"
                elif mutation == "viewport": receipt["before"]["viewport"] = [1, 1]
                else: receipt["before"]["documentWidth"] = value["viewport"][0]+2
                with self.subTest(scenario=scenario, mutation=mutation): self.rejects(value)

    def test_notice_requires_both_complete_nine_point_control_proofs(self):
        for field in ("toast", "dismiss"):
            for mutation in ("covered", "point-count", "coordinate", "owned-type", "missing-hit", "disabled", "clipped"):
                value = self.good()
                control = value["details"]["completionToast"]["before"][field]
                if mutation == "covered": control["points"][4]["owned"] = False
                elif mutation == "point-count": control["points"].pop()
                elif mutation == "coordinate": control["points"][0]["x"] += 1
                elif mutation == "owned-type": control["points"][0]["owned"] = 1
                elif mutation == "missing-hit": control["points"][0]["hit"] = None
                elif mutation == "disabled": control["disabled"] = True
                else: control.update(proof(bounds=dict(x=-10,y=10,width=100,height=30)), visible=False)
                with self.subTest(field=field, mutation=mutation): self.rejects(value)

    def test_notice_dismiss_must_be_inside_actual_toast(self):
        for bounds in (dict(x=211,y=20,width=28,height=28), dict(x=20,y=111,width=28,height=28)):
            value = self.good()
            value["details"]["completionToast"]["before"]["dismiss"] = proof(bounds=bounds)
            self.rejects(value)

    def test_no_fabricated_notice_in_stale_held_or_restart(self):
        for scenario in ("stale-owner-channel", "held-response-thread", "rich-channel"):
            value = self.good(scenario)
            if scenario == "rich-channel": value = self.restart(value)
            value["details"]["completionToast"] = notice(value["viewport"], R["SCHEDULED_NOTICE"])
            self.rejects(value)
        value = self.good()
        value["details"]["rejectionToast"] = notice(value["viewport"], R["REJECTED_NOTICE"])
        self.rejects(value)

    def test_dismissal_does_not_exempt_result_panel_occlusion(self):
        value = self.good()
        panel = next(row for row in value["geometry"] if row["stage"] == "result")["controls"]["panel"]
        panel["points"][-1].update(owned=False, hit=dict(tag="DIV", id="status-toast", classes="toast"))
        panel["unobscured"] = False
        self.rejects(value)

    def test_lifecycle_passes_complete_receipts_to_restart_validator(self):
        first=self.good("held-response-thread");second=self.restart(first)
        lifecycle=R["run"].__globals__
        def fake_phase(fixture,scenario,phase,nonce,database,binary,directory,digest):
            value=first if phase=="exercise" else second
            return dict(ok=True,native=copy.deepcopy(value))
        with patch.dict(lifecycle,require_contract=lambda:None,sha=lambda _:"f"*64,run_phase=fake_phase),redirect_stdout(io.StringIO()):
            R["run"](first["fixture"],first["scenario"],self.root/"binary.exe",self.root/"captures")
        output=self.root/"captures"/f"{first['fixture']}-{first['scenario']}"/f"{first['fixture']}-{first['scenario']}.json"
        evidence=json.loads(output.read_text())
        self.assertIs(evidence["ok"],True);self.assertIs(evidence["twoProcessRestart"],True)
        self.assertIs(evidence["exactDurableReadback"],True)

    def test_readiness_exact_owner_tuple_and_single_publication(self):
        expected=R["ready_tuple"](self.nonce,self.pid,"rich-channel","exercise")
        R["validate_ready"](expected,expected)
        for actual in (dict(expected,pid=True),dict(expected,version=True),dict(expected,extra=None)):
            with self.assertRaises(RuntimeError):R["validate_ready"](actual,expected)
        R["publish_ready"](self.database,expected)
        with self.assertRaises(FileExistsError):R["publish_ready"](self.database,expected)

    def test_exact_flags_and_count_types(self):
        for field in ("ok","noSession","noTransport","noCache","noProductionRoute","actualDialogControl","syntheticInput","preparedRequests","refusedInputs"):
            value=self.good();value[field]=1 if type(value[field]) is bool else True
            with self.subTest(field=field):self.rejects(value)

    def test_full_dictionary_required_null_unknown_and_sibling(self):
        for field in ("check_ticket","check_deadline","blocked_observation","consumption"):
            value=self.good();del value["initial"]["channel"]["stored"]["state"][field];self.rejects(value)
        value=self.good();value["durable"]["channel"]["stored"]["state"]["workflow"]={"Uncertain":value["operation"]};self.rejects(value)
        value=self.good();value["durable"]["thread"]["stored"]["state"]["extra_authority"]=None;self.rejects(value)

    def test_exact_cas_and_local_revisions(self):
        for scenario in R["SCENARIOS"]:
            value=self.good(scenario);value["durable"][value["target"]]["stored"]["revision"]+=1
            with self.subTest(scenario=scenario):self.rejects(value)

    def test_canonical_bytes_broadcast_and_newer_selection(self):
        for mutate in (lambda v:v["durable"]["thread"]["canonical"]["composer"]["selection"].update(end=1),
                       lambda v:v["durable"]["thread"]["canonical"]["composer"].update(reply_broadcast=True),
                       lambda v:v["details"]["nativeAfter"].update(direction="backward")):
            value=self.good("newer-thread");mutate(value);self.rejects(value)

    def test_request_route_defaults_order_token_and_raw_ack(self):
        for mutate in (lambda a:a.update(url=a["url"].replace("drafts.update","chat.postMessage")),
            lambda a:a.update(url=a["url"].replace("_x_num_retries=0","_x_num_retries=1")),
            lambda a:a["form"].reverse(),lambda a:a["form"].append(["token","real"]),
            lambda a:a.update(retrySafe=True),lambda a:a["rawResponse"]["draft"].update(last_updated_ts="other")):
            value=self.good();mutate(value["attempts"][0]);self.rejects(value)

    def test_no_replay_and_pending_not_fabricated_ack(self):
        value=self.good("held-response-thread");value["attempts"][0]["delivered"]=True;self.rejects(value)
        value=self.good("lost-ack-channel");value["details"]["blockedReplay"]["clicked"]=True;self.rejects(value)
        value=self.restart(self.good("lost-ack-channel"));del value["details"]["blockedReplay"];self.rejects(value)

    def test_rich_metadata_routing_and_immutable_schedule(self):
        for scenario in ("rich-thread","retry-channel","expired-change-thread"):
            value=self.good(scenario);value["operation"]["snapshot"]["blocks"][0]["elements"].pop();self.rejects(value)
            value=self.good(scenario);value["frozen"]["postAt"]+=60;self.rejects(value)
        value=self.good("rich-thread");value["operation"]["snapshot"]["destinations"][0]["broadcast"]=False;self.rejects(value)

    def test_expired_retry_and_explicit_change_required(self):
        value=self.good("expired-change-thread");value["details"]["expiredRetry"]["control"]["disabled"]=False;self.rejects(value)
        value=self.good("expired-change-thread");value["operation"]["consume_id"]=value["initial"]["thread"]["stored"]["state"]["consumption"]["id"];self.rejects(value)

    def test_real_rejection_then_retry_distinct_operation_same_intent(self):
        value=self.good("rejected-retry-channel");value["attempts"][0]["rawResponse"]={"ok":True};self.rejects(value)
        value=self.good("rejected-retry-channel");value["details"]["firstOperation"]["id"]=value["operation"]["id"];self.rejects(value)
        value=self.good("rejected-retry-channel");value["details"]["retryPending"]["channel"]["canonical"]["composer"]["text"]="";self.rejects(value)

    def test_datetime_and_actual_mutation_time(self):
        value=self.good();value["frozen"]["date"]="2033-05-18T01:00";self.rejects(value)
        value=self.good();value["attempts"][0]["form"][-1][1]=str(value["frozen"]["postAt"]*1000);self.rejects(value)

    def test_stale_owner_or_visit_is_specific_observed_change(self):
        for scenario in ("stale-owner-channel","stale-visit-thread"):
            value=self.good(scenario);value["details"]["contextAfter"]=value["details"]["contextBefore"];self.rejects(value)

    def test_queue_exact_and_no_chat_or_delivery_projection(self):
        value=self.good();value["queueFinal"]["unexpected"]=copy.deepcopy(next(iter(value["queueFinal"].values())));self.rejects(value)
        value=self.good();value["messagesFinal"]["pending_delivery"]["1.000001"]="Unconfirmed";self.rejects(value)
        value=self.restart(self.good());value["queueFinal"]={"fabricated":{}};self.rejects(value)

    def test_hit_point_count_coordinates_ownership_and_diagnostics(self):
        for mutate in (lambda p:p["points"].pop(),lambda p:p["points"][0].update(owned=False),
            lambda p:p["points"][0].update(x=float("nan")),lambda p:p["points"][0].update(x=123),
            lambda p:p["points"][0].update(hit=None),lambda p:p.update(unobscured=1)):
            value=self.good();mutate(value["geometry"][1]["controls"]["primary"]);self.rejects(value)

    def test_two_axis_clipping_and_wrong_stage_surface(self):
        value=self.good();value["geometry"][1]["controls"]["primary"]=proof(bounds=dict(x=0,y=10,width=200,height=100));self.rejects(value)
        value=self.good();value["geometry"][1]["controls"]["primary"]=proof(bounds=dict(x=10,y=0,width=200,height=100));self.rejects(value)
        value=self.good();value["geometry"][2]["surface"]="composer";self.rejects(value)

    def test_panel_overlay_and_full_dialog_chrome(self):
        for change in (lambda v:v["geometry"][-1].update(threadPaneAbsent=False),
            lambda v:v["geometry"][1]["controls"].pop("footer"),
            lambda v:v["geometry"][1]["controls"]["destination"].update(text="To #general"),
            lambda v:v["geometry"][1]["controls"]["primary"].update(text="Send now"),
            lambda v:v["geometry"][1]["controls"].update(footer=proof(bounds=dict(x=20,y=100,width=380,height=30)))):
            value=self.good();change(value);self.rejects(value)

    def test_exact_editor_date_stage_pairs(self):
        self.assertEqual(R["stages"]("rich-channel","exercise"),
            ("initial","review","review-date","pending","pending-date","result"))
        self.assertEqual(R["stages"]("stale-owner-channel","exercise"),
            ("initial","review","review-date","stale"))
        self.assertEqual(R["stages"]("held-response-thread","exercise"),
            ("initial","review","review-date","pending","pending-date"))
        for mutation in ("missing","order","repeated"):
            value=self.good()
            if mutation=="missing":value["geometry"].pop(2)
            elif mutation=="order":value["geometry"][1:3]=reversed(value["geometry"][1:3])
            else:value["geometry"][2]=copy.deepcopy(value["geometry"][1])
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_body_scroll_is_exact_finite_bounded_and_same_generation(self):
        for mutation in ("missing","requested","generation","top","nan","height","rect","before","bool","not-bottom"):
            value=self.good();row=value["geometry"][2];scroll=row["bodyScroll"]
            if mutation=="missing":del row["bodyScroll"]
            elif mutation=="requested":scroll["requested"]="editor"
            elif mutation=="generation":scroll["after"]["generation"]="another"
            elif mutation=="top":scroll["after"]["scrollTop"]=10000
            elif mutation=="nan":scroll["after"]["scrollTop"]=float("nan")
            elif mutation=="height":scroll["after"]["scrollHeight"]+=1
            elif mutation=="rect":scroll["before"]["rect"]=dict(scroll["before"]["rect"],x=0)
            elif mutation=="before":scroll["before"]["scrollTop"]=1
            elif mutation=="bool":scroll["after"]["clientHeight"]=True
            else:scroll["after"]["scrollTop"]-=20
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_selected_controls_must_be_inside_body_not_merely_viewport(self):
        for index,control in ((1,"editor"),(1,"toolbar"),(1,"editorLabel"),(2,"date"),(2,"dateLabel"),(2,"hint")):
            value=self.good();value["geometry"][index]["controls"][control]=proof(bounds=dict(x=15,y=80,width=100,height=20))
            with self.subTest(control=control):self.rejects(value)

    def test_unselected_controls_remain_truthfully_measured(self):
        value=self.good();self.validate(value)
        self.assertIs(value["geometry"][1]["controls"]["date"]["visible"],False)
        self.assertIs(value["geometry"][2]["controls"]["editor"]["unobscured"],False)
        for mutation in ("visible","unobscured","ownership","points","rectangle"):
            value=self.good();control=value["geometry"][1]["controls"]["date"]
            if mutation=="visible":control["visible"]=True
            elif mutation=="unobscured":control["unobscured"]=True
            elif mutation=="ownership":control["points"][0]["owned"]=True
            elif mutation=="points":control["points"].pop()
            else:control["rect"]["width"]=-1
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_body_scroll_preserves_fixed_chrome_focus_and_payload(self):
        for mutation in ("focus","title","destination","generation","blocks","chrome"):
            value=self.good();row=value["geometry"][2]
            if mutation=="focus":row["active"]["id"]="date"
            elif mutation in ("title","destination"):row["controls"][mutation]["text"]="changed"
            elif mutation=="generation":row["current"]["generation"]="next"
            elif mutation=="blocks":row["current"]["blocks"][0]["elements"].pop()
            else:row["controls"]["header"]["focused"]=True
            with self.subTest(mutation=mutation):self.rejects(value)

    def test_body_hit_descendant_can_change_after_real_scroll(self):
        value=self.good()
        for name in ("body","dialog"):
            for point in value["geometry"][2]["controls"][name]["points"]:
                point["hit"]=dict(tag="INPUT",id="managed-schedule-date",classes="")
        self.validate(value)

    def test_nondialog_cannot_claim_scroll_evidence(self):
        value=self.good();value["geometry"][0]["bodyView"]="editor";self.rejects(value)
        value=self.restart(self.good());value["geometry"][0]["bodyScroll"]={};self.rejects(value)

    def test_restart_only_allowed_workflow_normalization(self):
        first=self.good("held-response-thread");second=self.restart(first)
        second["initial"]["thread"]["stored"]["revision"]+=1
        with self.assertRaises(RuntimeError):R["validate_restart_pair"](first["scenario"],first,second)

    def test_closed_artifacts_full_png_edges_and_path_ownership(self):
        value=self.good();names=R["stages"](value["scenario"],"exercise");dimensions=[30,20]
        edges=[dict(stage=s,edge=e) for s in names for e in ("before","after")]
        for stage in names:
            path=self.root/f"{value['fixture']}-{value['scenario']}-exercise-{stage}.png"
            path.write_bytes(png(*dimensions));value["captures"][stage]=str(path)
        args=(value["fixture"],value["scenario"],"exercise",self.nonce,self.pid,self.database,self.root,value,edges,dimensions)
        R["validate_artifacts"](*args)
        path=value["captures"]["initial"];value["captures"]["initial"]=str(self.root/".."/Path(path).name)
        with self.assertRaises(RuntimeError):R["validate_artifacts"](*args)
        value["captures"]["initial"]=path;edges.reverse()
        with self.assertRaises(RuntimeError):R["validate_artifacts"](*args)


if __name__=="__main__":unittest.main()
