"""Offline protocol/semantic mutations; never launches the native harness."""
import copy
from contextlib import closing
import json
import math
from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest

C = runpy.run_path(str(Path(__file__).with_name("protected_drafts_receipts.py")))
R = runpy.run_path(str(Path(__file__).with_name("check-protected-drafts-native.py")))
globals().update({name: C[name] for name in ("local", "scope", "encoded", "review", "changed", "INITIAL", "REQUESTED", "NEWER", "ACCOUNT", "OTHER_ACCOUNT", "THREAD")})
NONCE = "a" * 32
STORAGE = Path(__file__).resolve().parent / ("protected-drafts-" + NONCE)


def uid(n): return f"00000000-0000-4000-8000-{n:012}"


def seed(target, failure):
    state = dict(scope=scope(target), client_msg_id=uid(10 if target == "channel" else 11), desired=encoded(INITIAL, target, target == "thread"),
                 local_revision=1, acknowledged_local_revision=0, acknowledged=None, workflow="Idle", tombstone=False,
                 retired_ids=[], consumption=None, check_deadline=None, check_ticket=None, observed_events={}, blocked_observation=None)
    return dict(scope=scope(target), clock=5, row=dict(scope=scope(target), revision=1 if failure else 2, state=state,
        lineage=dict(enrollment=uid(20 if target == "channel" else 21), verifiedEpoch=0 if failure else 2, basis="Unverified" if failure else "Local")),
        local=local(INITIAL, target == "thread"), blocked=None, alternates=[], origins=[], uncommitted=None)


def field_scope(p, target):
    thread = f'Some("{THREAD}")' if target == "thread" else "None"
    destination = "None" if target == "thread" and p["thread"] is None else f'Some(DraftKey {{ team: "T1", channel: "C2", thread: {thread} }})'
    return f'{p["viewId"]}:{p["epoch"]}:Some("{p["account"]}"):{destination}:{p[target + "Generation"]}'


def hit(r, rounded=False):
    if rounded:
        d = 6 - 5 / math.sqrt(2)
        points = [("center", 14, 14), ("top", 14, 1), ("right", 27, 14), ("bottom", 14, 27), ("left", 1, 14),
                  ("top-left", d, d), ("top-right", 28-d, d), ("bottom-right", 28-d, 28-d), ("bottom-left", d, 28-d)]
    else: points = [(f"{i}:{j}", r["width"]*x, r["height"]*y) for i, x in enumerate((.05, .5, .95)) for j, y in enumerate((.05, .5, .95))]
    v = dict(method="rounded-arc" if rounded else "rect-grid", ok=True, points=[dict(name=n, x=r["x"]+x, y=r["y"]+y,
             owned=True, element=dict(tag="BUTTON" if rounded else "DIV", id="", classes="")) for n, x, y in points])
    if rounded: v.update(computedRadii=["6px"]*4, radius=6, inset=1)
    return v


def measured(fixture, target, text, p, stage, input_counter):
    width, height = C["expected_viewport"](fixture)
    bounds = dict(field=dict(x=220, y=height-78, width=width-240, height=30), composer=dict(x=200, y=height-100, width=width-200, height=100),
                  pane=dict(x=200, y=0, width=width-200, height=height), header=dict(x=200, y=0, width=width-200, height=48))
    units = len(text.encode("utf-16-le"))//2
    start = len(text[:text.index("\U0001f680")].encode("utf-16-le"))//2 if "\U0001f680" in text else units
    n = dict(ok=True, text=text, display=text, codePoints=[ord(c) for c in text], utf16Length=units, scope=field_scope(p,target),
             visit=f"visit-{p['epoch']}", start=start, end=units, direction="backward" if text else "forward", composing=False,
             rect=dict(left=220, top=height-78, right=width-20, bottom=height-48, width=width-240, height=30), viewport=[width,height])
    current = dict(id=target+"-composer", scope=n["scope"], visit=n["visit"], wire=text, start=start, end=units, direction=n["direction"],
                   input=input_counter, composition=0, composing=False, display_start=start, display_end=units, display_len=units)
    locked = stage == "admitted"
    return dict(ok=True, target=target, targetMatches=True, **bounds, hitSamples={k:hit(bounds[k]) for k in ("field","composer","header")},
        viewport=[width,height], documentWidth=width, native=n, proof=None if locked else dict(status="captured",token="5",current=current),
        readOnly=locked, disabled=False, admission="7" if locked else None, fieldFocused=True, activeId=target+"-composer",
        scroll=dict(document=0,pane=0,composer=0))


def good(scenario="consume-channel", fixture="protected-drafts"):
    failure = scenario.startswith("failed-")
    target = "thread" if scenario.endswith("thread") else "channel"
    initial = {name:seed(name,failure) for name in ("channel","thread")}
    typed = changed(initial[target],REQUESTED,target=="thread")
    consumed = copy.deepcopy(typed)
    consumed["clock"]+=2;consumed["row"]["revision"]+=1;consumed["local"]=local("")
    consumed["row"]["state"].update(desired=None,local_revision=3,consumption=dict(id=uid(100),scope=scope(target),client_msg_id=typed["row"]["state"]["client_msg_id"],local_revision=2,kind="Send",snapshot=encoded(REQUESTED,target,target=="thread"),base=None))
    newer=changed(typed if failure else consumed,NEWER,scenario=="expired-clear-thread")
    snapshots=[];details={};held=[];actions=[]
    for stage in C["stages"](scenario):
        counter=0 if stage=="initial" else 2 if stage=="newer" or stage=="settled" and scenario in ("expired-clear-thread","canceled-clear-channel") else 1
        epoch=10+(1 if stage=="away" else 2 if failure and stage in ("returned","retried","newer") else 0)
        text=INITIAL if stage=="initial" else NEWER if counter==2 else "" if stage=="settled" else REQUESTED
        inv=copy.deepcopy(initial)
        inv[target]=newer if counter==2 else typed if failure and stage=="retried" or not failure and stage=="input" else initial[target] if failure or stage=="initial" else consumed
        p=dict(account=OTHER_ACCOUNT if stage=="away" else ACCOUNT,epoch=epoch,viewId=3,team="T1",channel="C2",thread=THREAD if target=="thread" else None,
            channelGeneration=1,threadGeneration=1,fieldScopes={},sessionPresent=False,transportPresent=False,cachePresent=False,isolated=True,
            book={},editors={},routed=counter,refused=0,inflight=stage=="admitted" or scenario in ("expired-clear-thread","canceled-clear-channel") and stage in ("expired","canceled","newer"),
            fixtureActions=[],nativeAdmission=[],result=None,operations={"Ok":[]})
        p["fieldScopes"]={name:field_scope(p,name) for name in ("channel","thread")}
        for name in ("channel","thread"):
            loc=initial[name]["local"] if name!=target or stage=="initial" else local(text,name=="thread" and (scenario=="expired-clear-thread" or stage not in ("settled","newer")))
            p[name+"Composer"]=local("")["composer"] if name=="thread" and p["thread"] is None else loc["composer"];p["book"][name]=loc if loc["composer"]["text"] else None
            failed=failure and name==target and stage in ("input","failed","returned")
            p["editors"][name]=None if stage=="away" else dict(retentionId="protected-1" if name=="channel" else "protected-2",writerId=uid(40 if name=="channel" else 41),enrollment=initial[name]["row"]["lineage"]["enrollment"],serial=counter if name==target else 0,review=review(inv[name]),uncommitted=local(REQUESTED) if failed else None,pending=local(REQUESTED) if failed else None,running=False,parked=False,error="Storage" if failed else None,failedInputs=[local(REQUESTED)] if failed else [])
        input_counter=0 if stage=="initial" or failure and stage in ("away","returned","retried") else 1 if failure else counter
        g=measured(fixture,target,text,p,stage,input_counter)
        if stage=="input": actions.append(dict(kind="input",key=scope(target)["key"],serial=1,local=local(REQUESTED,target=="thread")))
        if stage=="admitted":
            before=snapshots[-1]["geometry"]["proof"]["current"]
            held=[dict(kind="capture",evidence=dict(before=before)),dict(kind="lock",evidence=dict(token="7",before=before))]
            actions.append(dict(kind="consumed",before=review(typed),after=review(consumed),serial=1))
        if stage=="away":
            details["away"]=dict(kind="away",account=OTHER_ACCOUNT,epoch=11,retentionId="protected-1",strongCount=2,failedInputs=[local(REQUESTED)],pending=local(REQUESTED));actions.append(details["away"])
        if stage=="returned": details["returned"]=dict(kind="returned",account=ACCOUNT,epoch=12,retentionId="protected-1");actions.append(details["returned"])
        if stage=="retried": details["retry"]=dict(kind="input-retried",retentionId="protected-1",serialBefore=1,serialAfter=1,review=review(typed),pending=None,failedInputs=[]);actions.append(details["retry"])
        if stage in ("settled","canceled"):
            if stage=="canceled": actions.append(dict(kind="waiter-canceled"))
            details["result"]=dict(kind="canceled") if stage=="canceled" else dict(kind="completed",operationId=1,operationStatus="ConsumedAwaitingDelivery",nativeClear="Cleared" if scenario.startswith("consume-") else "Deferred",before=review(typed),after=review(consumed),serial=1)
            if scenario.startswith("consume-"): held.append(dict(kind="clear-ack",evidence=dict(token="7",before=held[0]["evidence"]["before"],after=g["proof"]["current"])))
            if stage=="settled": held.append(dict(kind="release-requested",evidence=dict(token="7")))
            if stage=="canceled": details["canceledWaiter"]={"kind":"canceled"}
        if stage=="newer": actions.append(dict(kind="input",key=scope(target)["key"],serial=2,local=local(NEWER,scenario=="expired-clear-thread")))
        p["fixtureActions"]=copy.deepcopy(actions);p["nativeAdmission"]=copy.deepcopy(held);p["result"]=copy.deepcopy(details.get("result"))
        if not failure and stage not in ("initial","input"):
            completed=stage=="settled" or stage=="newer" and scenario.startswith("consume-")
            p["operations"]["Ok"]=[dict(id=1,scope=scope(target),epoch=10,serial=1,request="Send",
                local=local(REQUESTED,target=="thread"),book=local(REQUESTED,target=="thread"),phase="NativeClear",
                status="ConsumedAwaitingDelivery" if completed else "Running",native=copy.deepcopy(held[0]["evidence"]["before"]),
                review=review(typed),consumed=dict(before=review(typed),after=review(consumed),serial=1,
                    nativeClear=("Cleared" if scenario.startswith("consume-") else "Deferred") if completed else "NotAttempted"),
                preservation=None,httpPresent=False,receiptPresent=False,live=True)]
        snapshots.append(dict(stage=stage,geometry=g,durable=inv,projection=p))
    details["typed"]=copy.deepcopy(snapshots[1]["geometry"]);details["typed"]["proof"]=None
    details["nativeInitial"]=copy.deepcopy(snapshots[0]["geometry"]["native"])
    anchor=dict(expected=dict(scope="timeline",latestId="message",windowEnd=3),connected=True,scope="timeline",windowEnd=3,latestMounted=True,latestHeight=32,inputGeneration=0,scrollTop=100,scrollHeight=300,clientHeight=200,bottomGap=0,ready=True,atBottom=True)
    details["nativeInitial"]["homeAnchor"]=dict(landed=True,before=anchor,after=copy.deepcopy(anchor))
    if not failure:
        g=snapshots[1]["geometry"];button=dict(x=630,y=C["expected_viewport"](fixture)[1]-37,width=28,height=28)
        details["clicked"]=dict(ok=True,kind="composer",target=target,clickRequested=True,clicked=True,disabled=False,fieldUnobscured=True,composerUnobscured=True,buttonUnobscured=True,targetMatches=True,scope=g["native"]["scope"],visit=g["native"]["visit"],fieldFocused=False,buttonFocused=True,viewport=g["viewport"],documentWidth=g["documentWidth"],**{k:g[k] for k in ("field","composer","pane","header")},button=button,hitSamples={"field":g["hitSamples"]["field"],"composer":g["hitSamples"]["composer"],"button":hit(button,True)})
    return dict(ok=True,fixture=fixture,scenario=scenario,phase="exercise",nonce=NONCE,pid=123,isolatedRoot=str(STORAGE),target=target,viewport=C["expected_viewport"](fixture),captures={},snapshots=snapshots,details=details,finalInventory=copy.deepcopy(snapshots[-1]["durable"]),finalProjection=copy.deepcopy(snapshots[-1]["projection"]),evidenceScope="protected local checkpoint/consume/native-clear; fixture construction has no HTTP capability")


def validate(value):
    C["validate"](value["fixture"],value["scenario"],"exercise",NONCE,123,STORAGE,value)


class Protocol(unittest.TestCase):
    def test_public_all_five_both_sizes(self):
        for case in C["SCENARIOS"]:
            for fixture in C["FIXTURES"]:
                with self.subTest(case=case,fixture=fixture): validate(good(case,fixture))

    def test_retention_identity_matches_actual_constructor(self):
        for value in ("protected-1", "protected-2", "protected-18446744073709551614"):
            self.assertTrue(C["retention_id"](value), value)
        for value in (None, True, 1, "", uid(30), "protected-0", "protected-01", "protected--1",
                      "protected-1 ", " protected-1", "Protected-1", "protected-?", "protected-18446744073709551615",
                      "protected-18446744073709551616", "protected-999999999999999999999"):
            self.assertFalse(C["retention_id"](value), value)
        self.reject(lambda v:v["snapshots"][0]["projection"]["editors"]["thread"].update(retentionId="protected-1"))
        self.reject(lambda v:v["snapshots"][0]["projection"]["editors"]["channel"].update(writerId="protected-1"))
        self.reject(lambda v:v["snapshots"][1]["projection"]["editors"]["channel"].update(retentionId="protected-99"))

    def test_canceled_waiter_requires_same_operation_settled_without_http(self):
        self.reject(lambda v:v["snapshots"][-1]["projection"]["operations"]["Ok"][0].update(id=2), "canceled-clear-channel")
        self.reject(lambda v:v["snapshots"][-1]["projection"]["operations"]["Ok"][0].update(status="Finished"), "canceled-clear-channel")
        self.reject(lambda v:v["snapshots"][-1]["projection"]["operations"]["Ok"][0].update(httpPresent=True), "canceled-clear-channel")
        self.reject(lambda v:v["snapshots"][-1]["projection"]["operations"]["Ok"][0]["consumed"].update(nativeClear="Cleared"), "canceled-clear-channel")

    def test_stage_matrix(self):
        self.assertEqual(sum(len(C["stages"](s)) for s in C["SCENARIOS"])*2,58)
        self.assertTrue(all(C["phases"](s)==("exercise",) for s in C["SCENARIOS"]))

    def test_readiness_exact(self):
        expected=R["ready_tuple"](NONCE,123,"consume-thread","exercise")
        R["validate_ready"](expected,expected)
        for field,value in (("pid",True),("nonce","b"*32),("phase","restart")):
            bad=copy.deepcopy(expected);bad[field]=value
            with self.assertRaises(RuntimeError):R["validate_ready"](bad,expected)

    def test_prep_cannot_launch(self):
        if not C["CONTRACT_READY"]:
            with self.assertRaises(RuntimeError): C["require_contract"]()

    def reject(self, edit, case="consume-channel"):
        value=good(case);edit(value)
        with self.assertRaises(RuntimeError):validate(value)

    def test_partial_or_covered_geometry(self):
        self.reject(lambda v:v["snapshots"][2]["geometry"]["header"].update(y=-20))
        self.reject(lambda v:v["snapshots"][2]["geometry"]["hitSamples"]["field"]["points"][0].update(owned=False))
        self.reject(lambda v:v["snapshots"][2]["geometry"]["hitSamples"]["field"]["points"].pop())

    def test_frozen_consume_content_and_scope(self):
        self.reject(lambda v:v["snapshots"][2]["durable"]["channel"]["row"]["state"]["consumption"].update(kind="Delete"))
        self.reject(lambda v:v["snapshots"][2]["durable"]["thread"]["local"]["composer"].update(text="wrong sibling"))

    def test_readonly_expiry_and_actual_ack(self):
        self.reject(lambda v:v["snapshots"][2]["geometry"].update(readOnly=False))
        self.reject(lambda v:v["snapshots"][3]["projection"]["nativeAdmission"][2]["evidence"]["after"].update(input=2))
        self.reject(lambda v:v["snapshots"][-1]["geometry"]["native"].update(direction="forward"),"expired-clear-thread")

    def test_scope_and_native_counter(self):
        self.reject(lambda v:v["snapshots"][1]["geometry"]["native"].update(scope="wrong-main-field"))
        self.reject(lambda v:v["snapshots"][1]["geometry"]["proof"]["current"].update(input=9))

    def test_cancel_keeps_actual_consumption(self):
        self.reject(lambda v:v["snapshots"][3]["durable"]["channel"]["row"]["state"].update(consumption=None),"canceled-clear-channel")

    def test_failed_input_retention_and_retry_serial(self):
        self.reject(lambda v:v["details"]["away"].update(strongCount=3),"failed-write-return-channel")
        self.reject(lambda v:v["details"]["retry"].update(serialAfter=2),"failed-write-return-channel")
        self.reject(lambda v:v["snapshots"][4]["projection"]["editors"]["channel"].update(writerId=uid(999)),"failed-write-return-channel")

    def test_http_capability_and_unrelated_fields_rejected(self):
        self.reject(lambda v:v["snapshots"][1]["projection"].update(transportPresent=True))
        self.reject(lambda v:v["snapshots"][0]["durable"]["channel"]["row"]["state"].update(unknown=True))

    def test_home_navigation_not_just_composer_mount(self):
        self.reject(lambda v:v["details"]["nativeInitial"]["homeAnchor"].update(landed=False))

    def test_closed_sqlite_full_inventory_and_corruption(self):
        storage_module=runpy.run_path(str(Path(__file__).with_name("protected_drafts_storage.py")))
        for scenario in C["SCENARIOS"]:
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory(prefix="protected-oracle-test-") as temp:
                value=good(scenario)
                storage=Path(temp)/("protected-drafts-"+NONCE);storage.mkdir()
                (storage/"owner.json").write_text(json.dumps(dict(version=1,nonce=NONCE)))
                worker=storage/"worker";worker.mkdir()
                (worker/"fixture.json").write_text(json.dumps(dict(version=1,account=ACCOUNT,nonce=uid(900))))
                (worker/"fixture.lock").touch()
                database=worker/"drafts.sqlite"
                with closing(sqlite3.connect(database)) as conn, conn:
                    conn.executescript('''
CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT);
CREATE TABLE composer_draft_sync(destination TEXT PRIMARY KEY,storage_revision INTEGER,format_version INTEGER,state TEXT);
CREATE TABLE composer_draft_owner(singleton INTEGER,account TEXT);
CREATE TABLE composer_draft_adapter_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity(team TEXT,content TEXT);
CREATE TABLE composer_draft_writer_lineage(writer_id TEXT,destination TEXT,content TEXT);
CREATE TABLE composer_draft_local_clock(destination TEXT,revision INTEGER,generation TEXT,writer_id TEXT,serial INTEGER);
CREATE TABLE upload_receipts(sequence INTEGER PRIMARY KEY AUTOINCREMENT);
''')
                    for table in ("composer_draft_blocked", "composer_draft_alternates", "composer_draft_reviewed_writers", "composer_draft_known_read_limit",
                        "composer_draft_baseline_evidence", "composer_draft_execution_result", "composer_draft_protected_archive", "composer_draft_recovery_editing",
                        "composer_draft_release_head", "composer_draft_continuity_reads", "composer_draft_recovery_owners", "composer_draft_status_reads",
                        "composer_draft_read_abandonment", "composer_draft_read_late", "upload_journal_identity"):
                        conn.execute('CREATE TABLE '+table+'(value TEXT)')
                    conn.execute("INSERT INTO composer_draft_owner VALUES(1,?)",(ACCOUNT,))
                    conn.execute("INSERT INTO composer_draft_adapter_format VALUES(1,1)")
                    conn.execute("INSERT INTO composer_draft_continuity_format VALUES(1,7)")
                    failure=scenario.startswith("failed-")
                    team=dict(epoch=1 if failure else 2,lease_id=None if failure else uid(800),mode="Gap" if failure else "Open",sequence=0,pending=None,last_interrupted_id=None,gap_reason="activation" if failure else "new stream requires per-owner verification",read_retry_at_ms=0)
                    conn.execute("INSERT INTO composer_draft_continuity VALUES('T1',?)",(json.dumps(team),))
                    for index,target in enumerate(("channel","thread")):
                        inventory=value["finalInventory"][target];key=json.dumps(scope(target)["key"])
                        conn.execute("INSERT INTO composer_drafts VALUES(?,?)",(key,json.dumps(inventory["local"])))
                        conn.execute("INSERT INTO composer_draft_sync VALUES(?,?,7,?)",(key,inventory["row"]["revision"],json.dumps(storage_module["envelope"](inventory["row"]))))
                        conn.execute("INSERT INTO composer_draft_local_clock VALUES(?,?,NULL,NULL,0)",(key,inventory["clock"]))
                        writer=value["finalProjection"]["editors"][target]["writerId"]
                        record=dict(version=1,scope=scope(target),writer=writer,origin=dict(Enrollment=inventory["row"]["lineage"]["enrollment"]),processed=0,last=None)
                        if target==value["target"]:
                            result=storage_module["inventory_record"](inventory)
                            # Actual private record contains JSON text, not a nested dictionary.
                            result["row"][1]=json.dumps(result["row"][1],sort_keys=True,ensure_ascii=True)
                            record.update(processed=2,last=dict(serial=2,local=inventory["local"],direct=inventory["row"]["state"]["consumption"],continuation=None,preserved=False,result=result))
                        conn.execute("INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)",(writer,key,json.dumps(record)))
                        writer,generation=uid(600+index),uid(700+index)
                        loc=local(INITIAL,target=="thread")
                        record=dict(version=1,scope=scope(target),writer=writer,origin=dict(LocalGeneration=generation),processed=1,last=dict(serial=1,local=loc,direct=None,continuation=dict(generation=generation,predecessor=0,expected=local("")),preserved=False,result=dict(version=1,scope=scope(target),local=loc,blocked=None,alternates=[],row=None,origins=[],clock=4,generation=generation)))
                        conn.execute("INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)",(writer,key,json.dumps(record)))
                self.assertTrue(C["validate_closed_storage"](storage,value)["ok"])
                before=database.read_bytes()
                C["validate_closed_storage"](storage,value)
                self.assertEqual(database.read_bytes(),before,"Read-only validator changed SQLite bytes")
                # Distinct raw TEXT primary keys may decode to the same scope.
                # No normalized dictionary may silently discard the extra row.
                for table in ("composer_drafts", "composer_draft_sync", "composer_draft_local_clock"):
                    with closing(sqlite3.connect(database)) as conn, conn:
                        row=list(conn.execute('SELECT * FROM '+table+' LIMIT 1').fetchone())
                        duplicate=json.dumps(json.loads(row[0]),separators=(",",":"))
                        self.assertNotEqual(duplicate,row[0])
                        row[0]=duplicate
                        conn.execute('INSERT INTO '+table+' VALUES('+','.join('?' for _ in row)+')',row)
                    with self.assertRaises(RuntimeError): C["validate_closed_storage"](storage,value)
                    with closing(sqlite3.connect(database)) as conn, conn:
                        conn.execute('DELETE FROM '+table+' WHERE destination=?',(duplicate,))
                    self.assertTrue(C["validate_closed_storage"](storage,value)["ok"])
                with closing(sqlite3.connect(database)) as conn, conn: conn.execute("UPDATE composer_draft_sync SET format_version=5")
                with self.assertRaises(RuntimeError): C["validate_closed_storage"](storage,value)

    def test_persisted_duplicate_fields_rejected(self):
        m=runpy.run_path(str(Path(__file__).with_name("protected_drafts_storage.py")))
        with self.assertRaises(RuntimeError):m["unique_json"]('{"state":{"a":1,"a":2}}')


if __name__ == "__main__": unittest.main()
