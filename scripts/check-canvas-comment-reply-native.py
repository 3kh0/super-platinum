"""Owned two-process Canvas reply workflow; all Slack responses are synthetic."""
import sys
sys.dont_write_bytecode = True
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
import time
from urllib.parse import urlsplit
import uuid
from canvas_native import CanvasCheck, capture_stage, private_checks
from canvas_evidence import audited_fixture, closed_receipts, png_signature
from native_fixture import NativeFixture, DEFAULT_BINARY
from native_check import expected_viewport

FIXTURE = "canvas-comment-reply"
FLAG = "SUPER_PLATINUM_CANVAS_COMMENT_REPLY_CHECK"
FIRST = "First reply \u65e5\u672c\u8a9e \U0001f98a e\u0301"
NEWER = "Newer local reply \u65e5\U0001f680"
UNKNOWN = "Unconfirmed reply \u65e5\U0001f98a"
FINAL = "A new explicit reply after review"
ROOT_TS = "1719800000.000100"
STAGES = {"exercise": ("saved", "reopened", "not-sent", "held-newer", "ack-newer", "stale", "uncertain"),
          "restart": ("restored", "reviewed", "sent")}
READ = ["files.info", "conversations.replies"]
REQUESTS = {"exercise": ["conversations.history", "conversations.replies"] * 2 + READ + READ + ["chat.postMessage", "conversations.replies"] + READ + ["conversations.replies"] + READ + ["chat.postMessage"],
            "restart": ["conversations.history", "conversations.replies"] + READ + READ + ["chat.postMessage", "conversations.replies"]}

def identity(value):
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except (ValueError, TypeError, AttributeError):
        return False

def fields(request):
    pairs = request.get("fields")
    if not isinstance(pairs, list) or any(not isinstance(p, list) or len(p) != 2 or not all(isinstance(v,str) for v in p) for p in pairs):
        raise RuntimeError("Missing exact prepared form")
    result = dict(pairs)
    if len(result) != len(pairs) or "token" in result:
        raise RuntimeError("Duplicate or secret request field")
    parsed = urlsplit(request.get("url", ""))
    if request.get("method") != "POST" or parsed.scheme != "https" or parsed.netloc != "fixture.slack.com" or parsed.fragment:
        raise RuntimeError("Wrong synthetic request route")
    return parsed.path.removeprefix("/api/"), result

def read_form(method):
    if method=="files.info":return {"file":"Ffixture"}
    form={"channel":"Cfixture","inclusive":"true","limit":"20" if method=="conversations.history" else "28","cached_latest_updates":"{}"}
    if method=="conversations.history":form.update(document_stream="latest_reply",ignore_replies="true",include_pin_count="true",no_user_profile="true",include_stories="true",include_free_team_extra_messages="true",include_date_joined="false")
    else:form["ts"]=ROOT_TS
    return form

def inventory(value):
    if not isinstance(value,dict) or set(value) != {"drafts", "attempt", "history"} or not isinstance(value["drafts"],list) or not isinstance(value["history"],list):
        raise RuntimeError("Incomplete reply inventory")
    def draft(d):
        if not isinstance(d,dict) or set(d)!={"id","revision","capture","text"} or not identity(d["id"]) or not identity(d["capture"]) or type(d["revision"]) is not int or d["revision"] < 1 or not isinstance(d["text"],str):
            raise RuntimeError("Malformed draft version")
    for d in value["drafts"]: draft(d)
    def receipt(r):
        if not isinstance(r,dict) or set(r)!={"id","record","draft"} or not identity(r["id"]):
            raise RuntimeError("Missing immutable reply identity")
        draft(r["draft"])
        record=r["record"]
        if not isinstance(record,dict) or set(record)!={"version","workspace","user","file","root","text","client_msg_id"} or record["version"]!=1 or record["file"]!="Ffixture" or record["root"].get("ts")!=ROOT_TS or record["root"].get("document_comment",{}).get("thread_id")!="fixture-comment-1" or record["client_msg_id"]!=r["id"] or record["text"]!=r["draft"]["text"] or not record["workspace"] or not record["user"]:
            raise RuntimeError("Wrong immutable reply destination or content")
    if value["attempt"] is not None: receipt(value["attempt"])
    for h in value["history"]:
        if not isinstance(h,dict) or set(h)!={"receipt","status","observed"} or h["status"] not in ("not_sent","acknowledged","reviewed_outcome_unknown"):
            raise RuntimeError("Malformed history")
        receipt(h["receipt"])

def geometry(value, viewport):
    def bounded(r):
        return (isinstance(r,dict) and all(type(r.get(k)) in (int,float) and math.isfinite(r[k]) for k in ("x","y","width","height","right","bottom"))
                and r["width"]>0 and r["height"]>0 and r["x"]>=-1 and r["y"]>=-1 and r["right"]<=viewport[0]+1 and r["bottom"]<=viewport[1]+1
                and abs(r["right"]-r["x"]-r["width"])<.01 and abs(r["bottom"]-r["y"]-r["height"])<.01)
    samples=value.get("samples")
    if (value.get("ok") is not True or value.get("viewport")!=viewport or value.get("noOverflow") is not True or value.get("sampledSequentially") is not True
            or not bounded(value.get("dialog")) or not bounded(value.get("panel")) or not bounded(value.get("finalRect"))
            or not isinstance(samples,list) or len(samples)<3 or any(s.get("hit") is not True or not bounded(s.get("rect")) for s in samples)
            or not any(s.get("id")=="canvas-comment-reply" for s in samples) or not any(s.get("id")=="canvas-reply-send" for s in samples)
            or type(value.get("sendDisabled")) is not bool or not isinstance(value.get("text"),str)):
        raise RuntimeError("Incomplete sequential control geometry")

def validate(fixture, phase, receipt):
    if receipt.get("phase")!=phase or any(receipt.get(k) is not True for k in ("ok","syntheticInput","noSession","noTransport","durableDraft","exactRequests","unknownNoReplay")):
        raise RuntimeError("Incomplete native reply receipt")
    inventory(receipt.get("initial")); inventory(receipt.get("final"))
    before, after=receipt["initial"],receipt["final"]
    ledger=receipt.get("ledger",{});prepared=ledger.get("prepared");requests=ledger.get("requests")
    expected=3 if phase=="exercise" else 1
    if type(receipt.get("preparedCount")) is not int or receipt["preparedCount"]!=expected or type(receipt.get("dispatchCount")) is not int or receipt["dispatchCount"]!=(2 if phase=="exercise" else 1) or not isinstance(prepared,list) or len(prepared)!=expected or not isinstance(requests,list):
        raise RuntimeError("Reply dispatch/preparation counts differ")
    if [fields(r)[0] for r in requests]!=REQUESTS[phase]:raise RuntimeError("Read/write request order differs")
    for request in requests:
        method, form=fields(request)
        if method!="chat.postMessage" and form!=read_form(method):raise RuntimeError("Canonical read form differs")
    texts=[FIRST,FIRST,UNKNOWN] if phase=="exercise" else [FINAL]
    ids=[]
    for r,text in zip(prepared,texts):
        method,form=fields(r)
        if method!="chat.postMessage" or set(form)!={"channel","text","thread_ts","client_msg_id","reply_broadcast"} or form["channel"]!="Cfixture" or form["thread_ts"]!=ROOT_TS or form["reply_broadcast"]!="false" or form["text"]!=text or not identity(form["client_msg_id"]):raise RuntimeError("Prepared reply payload differs")
        ids.append(form["client_msg_id"])
    if len(set(ids))!=len(ids):raise RuntimeError("Fresh deliberate replies reused attempt identity")
    writes=[r for r in requests if fields(r)[0]=="chat.postMessage"]
    if writes!=(prepared[1:] if phase=="exercise" else prepared):raise RuntimeError("Prepared/actual-dispatched forms differ")
    layouts=receipt.get("layouts")
    if not isinstance(layouts,list) or len(layouts)!=len(STAGES[phase]) or receipt.get("layout")!=layouts[-1]:raise RuntimeError("Missing capture stage geometry")
    for item in layouts:geometry(item,expected_viewport(fixture))
    if phase=="exercise":
        if before!={"drafts":[],"attempt":None,"history":[]} or after["attempt"] is None or after["attempt"]["id"]!=ids[-1] or len(after["drafts"])!=1 or after["drafts"][0]["text"]!=UNKNOWN or sorted(h["status"] for h in after["history"])!=["acknowledged","not_sent"]:raise RuntimeError("Exercise did not retain exact unknown attempt")
        if [l["text"] for l in layouts]!=[FIRST,FIRST,FIRST,NEWER,NEWER,NEWER,UNKNOWN] or layouts[-1]["sendDisabled"] is not True:raise RuntimeError("Newer input, reopened draft, or unknown lock differs")
    else:
        if before["attempt"] is None or before["attempt"]["record"]["text"]!=UNKNOWN or after["attempt"] is not None or any(d["text"] for d in after["drafts"]):raise RuntimeError("Restart/review final authority differs")
        if [l["text"] for l in layouts]!=[UNKNOWN,"",""] or layouts[0]["sendDisabled"] is not True:raise RuntimeError("Restored unknown became sendable")
        history={h["receipt"]["id"]:h for h in after["history"]}
        old=before["attempt"]
        if history.get(old["id"],{}).get("receipt")!=old or history[old["id"]]["status"]!="reviewed_outcome_unknown" or history.get(ids[0],{}).get("status")!="acknowledged":raise RuntimeError("Review lost original outcome identity")
        for h in before["history"]:
            if history.get(h["receipt"]["id"])!=h:raise RuntimeError("Review overwrote earlier history")

def snapshot(path):
    if not path.is_file():raise RuntimeError("Private SQLite file missing")
    with sqlite3.connect(path.as_uri()+"?mode=ro",uri=True) as db:
        if db.execute("PRAGMA integrity_check").fetchall()!=[("ok",)]:raise RuntimeError("Private SQLite integrity failed")
        tables=[r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        return {table:db.execute('SELECT * FROM "'+table.replace('"','""')+'" ORDER BY rowid').fetchall() for table in tables}

def persist_evidence(path,evidence,primary):
    try:path.write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf8")
    except Exception as error:
        if primary is None:raise
        primary.add_note("Reply evidence write failed: "+repr(error))

def run_phase(fixture,phase,binary,captures,ack):
    captures.mkdir(parents=True,exist_ok=False)
    spec=CanvasCheck(FIXTURE,FLAG,(),{},STAGES[phase])
    checks=private_checks(spec,ack)
    checks.update(SUPER_PLATINUM_CANVAS_REPLY_DB=str(ack/"replies.sqlite"),SUPER_PLATINUM_CANVAS_REPLY_PHASE=phase)
    evidence={"ok":False,"fixture":fixture,"phase":phase,"stages":[],"platform":sys.platform};native=None;primary=None
    try:
        native=NativeFixture(fixture,captures/(fixture+".log"),binary,checks)
        with audited_fixture(native,evidence) as audit:
            deadline=time.monotonic()+180;seen=0
            while True:
                native.require_running();audit.checkpoint("running")
                stages=native.receipts(FIXTURE+"-stage: ")
                for stage in stages[seen:]:
                    evidence["stages"].append(capture_stage(native,spec,fixture,stage,seen,captures,ack,audit=audit));seen+=1
                terminal=native.receipts(FIXTURE+"-check: ")
                if terminal:
                    if len(terminal)!=1:raise RuntimeError("Duplicate final reply receipt")
                    evidence["native"]=terminal[0];validate(fixture,phase,terminal[0])
                    if seen!=len(spec.stages):raise RuntimeError("Missing stage capture")
                    audit.capture(captures/(fixture+".png"),"final");break
                if time.monotonic()>deadline:raise TimeoutError("Reply fixture timed out")
                time.sleep(.05)
        expected=[{k:v for k,v in s.items() if k!="screenshot"} for s in evidence["stages"]]
        if closed_receipts(native.log,FIXTURE+"-stage: ")!=expected or closed_receipts(native.log,FIXTURE+"-check: ")!=[evidence["native"]]:raise RuntimeError("Closed reply log changed")
        validate(fixture,phase,evidence["native"])
        for c in evidence["captures"]:png_signature(Path(c["path"]),evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"]=True;evidence["database"]=snapshot(ack/"replies.sqlite");evidence["databaseSha256"]=hashlib.sha256((ack/"replies.sqlite").read_bytes()).hexdigest();evidence["ok"]=True
        return evidence
    except BaseException as error:
        primary=error;evidence["error"]=f"{type(error).__name__}: {error}";raise
    finally:
        if native is not None and native.log.exists():
            try:evidence["closedStageReceipts"]=closed_receipts(native.log,FIXTURE+"-stage: ");evidence["closedTerminalReceipts"]=closed_receipts(native.log,FIXTURE+"-check: ")
            except Exception as error:
                evidence["closedLogError"]=repr(error)
                if primary is not None:primary.add_note("Closed log read failed: "+repr(error))
                else:
                    evidence["ok"]=False;raise
        persist_evidence(captures/(fixture+".json"),evidence,primary)

def run(fixture,binary,captures):
    destination=captures/fixture
    if destination.exists():raise RuntimeError("Fresh reply evidence directory required")
    destination.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="sp-reply-owned-") as private:
        first=Path(private)/"exercise";second=Path(private)/"restart";first.mkdir();second.mkdir()
        exercise=run_phase(fixture,"exercise",binary,destination/"exercise",first)
        if exercise.get("processExited") is not True:raise RuntimeError("Original owner is still live")
        source=first/"replies.sqlite";target=second/"replies.sqlite"
        with target.open("xb") as output:output.write(source.read_bytes())
        if hashlib.sha256(target.read_bytes()).hexdigest()!=exercise["databaseSha256"] or snapshot(target)!=exercise["database"]:raise RuntimeError("Exact closed SQLite handoff differs")
        restart=run_phase(fixture,"restart",binary,destination/"restart",second)
        if restart["native"]["initial"]!=exercise["native"]["final"] or restart["binarySha256"]!=exercise["binarySha256"]:raise RuntimeError("Fresh process restored different inventory or binary")
        result={"ok":True,"fixture":fixture,"binarySha256":exercise["binarySha256"],"closedDatabaseHandoff":True,"exactInventoryRestored":True,"processCount":2,"captureCount":len(exercise["captures"])+len(restart["captures"]),"phaseReceipts":[str(destination/p/(fixture+".json")) for p in ("exercise","restart")],"limits":"Synthetic native input and injected responses. Sampled sequential control geometry; inspect PNGs. No real Slack or trusted OS typing."}
        persist_evidence(destination/"cohort.json",result,None)
        print(json.dumps(result),flush=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--binary",type=Path,default=DEFAULT_BINARY);parser.add_argument("--captures",type=Path,required=True);parser.add_argument("--fixture",action="append",choices=(FIXTURE,FIXTURE+"-narrow"));args=parser.parse_args()
    for fixture in args.fixture or (FIXTURE,FIXTURE+"-narrow"):run(fixture,args.binary.resolve(),args.captures.resolve())
if __name__=="__main__":main()
