"""Exact read-only closed-worker SQLite validation for local enrollment."""
import json
import sqlite3
import stat
import runpy
from pathlib import Path


def validate(storage,receipt,h):
    same,require,uuid4,local,scope=(h[n] for n in ("same","require","uuid4","local","scope"))
    old=runpy.run_path(str(h["SCRIPTS"]/"protected_drafts_storage.py"))
    decode,envelope=old["unique_json"],old["envelope"]
    account,initial=h["ACCOUNT"],h["INITIAL"];failure=receipt["scenario"]=="failed-editor-channel";late=receipt["scenario"]=="late-typing-channel"
    success=receipt["scenario"].startswith("enroll-") or receipt["scenario"]=="canceled-waiter-channel"
    def regular(path,directory=False):
        info=path.lstat();require(not path.is_symlink() and not getattr(info,"st_file_attributes",0)&0x400 and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)),"Linked/unowned isolated path");return info
    for parent in (storage,*storage.parents):regular(parent,True)
    require(storage.name=="protected-enrollment-"+receipt["nonce"],"Wrong owned enrollment directory")
    def metadata(path):
        require(regular(path).st_size<=1024*1024,"Oversized marker");return decode(path.read_text(encoding="utf-8"))
    same(metadata(storage/"owner.json"),dict(version=1,nonce=receipt["nonce"]),"Parent marker changed")
    worker=storage/"worker";regular(worker,True);marker=metadata(worker/"fixture.json")
    require(isinstance(marker,dict) and set(marker)=={"version","account","nonce"} and uuid4(marker["nonce"]),"Invalid isolated marker")
    same(marker["version"],1,"Marker version");same(marker["account"],account,"Marker account")
    for child in worker.iterdir():
        require(child.name in ("fixture.json","fixture.lock","drafts.sqlite","drafts.sqlite-journal","drafts.sqlite-wal","drafts.sqlite-shm"),"Unexpected worker file");regular(child)
    db=worker/"drafts.sqlite";require(0<regular(db).st_size<32*1024*1024,"Invalid bounded database")
    conn=sqlite3.connect(db.as_uri()+"?mode=ro",uri=True)
    try:
        conn.execute("PRAGMA query_only=ON");same(conn.execute("PRAGMA integrity_check").fetchall(),[("ok",)],"Database integrity")
        populated={"composer_drafts","composer_draft_sync","composer_draft_owner","composer_draft_adapter_format","composer_draft_continuity_format","composer_draft_continuity","composer_draft_writer_lineage","composer_draft_local_clock","composer_draft_protected_archive"}
        empty={"composer_draft_blocked","composer_draft_alternates","composer_draft_reviewed_writers","composer_draft_known_read_limit","composer_draft_baseline_evidence","composer_draft_execution_result","composer_draft_recovery_editing","composer_draft_release_head","composer_draft_continuity_reads","composer_draft_recovery_owners","composer_draft_status_reads","composer_draft_read_abandonment","composer_draft_read_late","upload_journal_identity","upload_receipts","sqlite_sequence"}
        same({r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")},populated|empty,"Unexpected SQLite schema")
        for table in empty:same(conn.execute('SELECT * FROM "'+table+'"').fetchall(),[],"Unexpected persisted work "+table)
        same(conn.execute("SELECT * FROM composer_draft_owner").fetchall(),[(1,account)],"Wrong persistent owner")
        same(conn.execute("SELECT * FROM composer_draft_adapter_format").fetchall(),[(1,1)],"Wrong adapter format")
        same(conn.execute("SELECT * FROM composer_draft_continuity_format").fetchall(),[(1,7)],"Wrong protected format")
        final=receipt["finalInventory"];key=lambda target:json.dumps(scope(target)["key"],sort_keys=True)
        canonical=conn.execute("SELECT destination,content FROM composer_drafts").fetchall();require(len(canonical)==2,"Wrong exact raw canonical row count")
        canonical={json.dumps(decode(k),sort_keys=True):decode(v) for k,v in canonical}
        same(canonical,{key(t):final[t]["local"] for t in final},"Closed canonical drafts differ")
        rows=conn.execute("SELECT destination,storage_revision,format_version,state FROM composer_draft_sync").fetchall()
        require(len(rows)==(1 if success else 0),"Wrong exact raw enrollment row count")
        rows={json.dumps(decode(k),sort_keys=True):dict(revision=r,format=f,envelope=decode(v)) for k,r,f,v in rows}
        same(rows,{key(t):dict(revision=inv["row"]["revision"],format=7,envelope=envelope(inv["row"])) for t,inv in final.items() if inv["row"]},"Closed full enrollment envelope differs")
        clocks=conn.execute("SELECT destination,revision,generation,writer_id,serial FROM composer_draft_local_clock").fetchall();require(len(clocks)==2,"Wrong exact raw local clock count")
        clocks={json.dumps(decode(k),sort_keys=True):[r,g,w,n] for k,r,g,w,n in clocks}
        same(set(clocks),{key("channel"),key("thread")},"Duplicate normalized clock key")
        raw_writers=conn.execute("SELECT writer_id,destination,content FROM composer_draft_writer_lineage").fetchall()
        require(len(raw_writers)==(3 if failure else 2),"Wrong exact writer lineage count")
        writers={}
        for wid,destination,raw in raw_writers:
            require(uuid4(wid) and wid not in writers,"Invalid or duplicate writer identity")
            item=decode(raw);same(item.get("scope",{}).get("key"),decode(destination),"Writer destination mismatch");writers[wid]=item
        for target in ("channel","thread"):
            candidates=[(wid,v) for wid,v in writers.items() if v.get("scope")==scope(target) and isinstance(v.get("origin"),dict) and set(v["origin"])=={"LocalGeneration"}]
            require(len(candidates)==1,"Missing unique original local writer")
            wid,item=candidates[0];generation=item["origin"]["LocalGeneration"];require(uuid4(generation),"Invalid local generation")
            changed=late and target=="channel";serial=2 if changed else 1;text=h["NEWER"] if changed else initial;clock=7 if changed else 4
            expected=dict(version=1,scope=scope(target),writer=wid,origin=dict(LocalGeneration=generation),processed=serial,last=dict(serial=serial,local=local(text,target=="thread"),direct=None,
                continuation=dict(generation=generation,predecessor=1 if changed else 0,expected=local(initial,target=="thread") if changed else local("")),preserved=False,
                result=dict(version=1,scope=scope(target),local=local(text,target=="thread"),blocked=None,alternates=[],row=None,origins=[],clock=clock,generation=generation)))
            same(item,expected,"Unexpected exact local writer receipt")
            writers.pop(wid)
            if failure and target=="channel":continue
            expected_clock=[5,None,None,0] if success and target==receipt["target"] else [clock,generation,wid,serial]
            same(clocks[key(target)],expected_clock,"Wrong final local generation ownership")
        archives=conn.execute("SELECT enrollment,team,content FROM composer_draft_protected_archive").fetchall()
        archive=None
        if failure:
            owner=receipt["finalProjection"]["retained"][0];wid=owner["writerId"];require(wid in writers,"Retained released writer absent")
            same(writers.pop(wid),dict(version=1,scope=scope("channel"),writer=wid,origin=dict(Enrollment=owner["enrollment"]),processed=0,last=None),"Failed local input incorrectly committed")
            clock=clocks[key("channel")];require(len(clock)==4 and uuid4(clock[1]),"Missing handoff generation");same(clock,[6,clock[1],wid,0],"Released writer clock changed on failed write")
            require(len(archives)==1,"Missing exact retained archive")
            enrollment,team,raw=archives[0];same(enrollment,owner["enrollment"],"Archive lineage differs");same(team,"T1","Archive team differs")
            archive=decode(raw);stored=decode(archive.get("row","{}"));require(uuid4(stored.get("state",{}).get("client_msg_id")),"Missing archived client identity")
            seed=h["empty_inventory"]("channel");actual=dict(seed,clock=5,row=dict(scope=scope("channel"),revision=1,state=stored["state"],lineage=dict(enrollment=enrollment,verifiedEpoch=0,basis="Unverified")))
            expected=h["enrolled"](seed,actual)
            same(stored,envelope(expected["row"]),"Archive envelope contains unexpected history")
            same(archive,dict(version=1,scope=scope("channel"),enrollment=enrollment,revision=1,row=archive["row"],local=local(initial),blocked=None,alternates=[],writer_floors=[]),"Full archived inventory changed")
        else:same(archives,[],"Unexpected archive")
        same(writers,{},"Extra writer lineage")
        teams=conn.execute("SELECT team,content FROM composer_draft_continuity").fetchall();team=None
        if success or failure:
            require(len(teams)==1 and teams[0][0]=="T1","Wrong continuity team")
            team=decode(teams[0][1]);same(team,dict(epoch=1,lease_id=None,mode="Gap",sequence=0,pending=None,last_interrupted_id=None,gap_reason="activation",read_retry_at_ms=0),"Enrollment fabricated Stream/dispatch authority")
        else:same(teams,[],"Refused enrollment fabricated continuity state")
        return dict(ok=True,database=str(db),workerMarker=marker,integrity="ok",canonical=canonical,protected=rows,clocks=clocks,writerCount=len(raw_writers),archive=archive,continuity=team,emptyTables=sorted(empty))
    finally:conn.close()
