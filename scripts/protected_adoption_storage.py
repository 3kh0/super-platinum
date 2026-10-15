"""Exact read-only SQLite oracle for the isolated native adoption cohort.

Canonical state and writer receipts are derived from the frozen real operations;
no native result boolean or route label authorizes a database expectation.
"""
import copy
import json
import runpy
import sqlite3
import stat

TARGETS = ("channel", "thread")
SCENARIOS = ("adopt-channel", "adopt-thread", "late-input-channel", "native-refusals-channel",
             "owner-replacement-channel", "cancel-boundaries-channel", "failed-managed-write-channel")
EMPTY = {
    "composer_draft_blocked", "composer_draft_reviewed_writers", "composer_draft_known_read_limit",
    "composer_draft_baseline_evidence", "composer_draft_execution_result", "composer_draft_protected_archive",
    "composer_draft_recovery_editing", "composer_draft_release_head", "composer_draft_continuity_reads",
    "composer_draft_recovery_owners", "composer_draft_status_reads", "composer_draft_read_abandonment",
    "composer_draft_read_late", "upload_journal_identity", "upload_receipts", "sqlite_sequence",
}
POPULATED = {"composer_drafts", "composer_draft_sync", "composer_draft_owner", "composer_draft_adapter_format",
             "composer_draft_continuity_format", "composer_draft_continuity", "composer_draft_writer_lineage",
             "composer_draft_local_clock", "composer_draft_alternates"}
TEAM = dict(epoch=1, lease_id=None, mode="Gap", sequence=0, pending=None,
            last_interrupted_id=None, gap_reason="activation", read_retry_at_ms=0)


def facts(receipt, h):
    require, same = h["require"], h["same"]
    scenario = receipt["scenario"]
    require(scenario in SCENARIOS, "Unknown adoption storage scenario")
    target = "thread" if scenario == "adopt-thread" else "channel"
    same(receipt["target"], target, "Wrong adoption target")
    writes = 2 if scenario == "failed-managed-write-channel" else (1 if scenario in ("adopt-channel", "adopt-thread", "cancel-boundaries-channel") else 0)
    preserved = scenario in ("late-input-channel", "native-refusals-channel")
    text = h["FINAL"] if writes == 2 else h["NEWER"] if writes else h["INITIAL"]
    return dict(scenario=scenario, target=target, writes=writes, preserved=preserved,
                registered=scenario != "late-input-channel", text=text,
                local_serial=3 if scenario == "native-refusals-channel" else 2,
                predecessor_text=h["INITIAL"] + " temporary" if scenario == "native-refusals-channel" else h["INITIAL"],
                alternate=h["NEWER"] if scenario == "late-input-channel" else h["INITIAL"])


def expected_inventory(receipt, h):
    """Source-derived full final inventory; only issued UUIDs are taken as evidence."""
    f = facts(receipt, h)
    same, require, uuid4, local, scope = (h[n] for n in ("same", "require", "uuid4", "local", "scope"))
    actual = receipt["finalInventory"]
    same(set(actual), set(TARGETS), "Wrong final inventory scopes")
    out = {}
    for target in TARGETS:
        loc = local(f["text"] if target == f["target"] else h["INITIAL"], target == "thread")
        inv = dict(scope=scope(target), clock=4, row=None, local=loc,
                   blocked=None, alternates=[], origins=[], uncommitted=None)
        if target == f["target"]:
            row = actual[target].get("row", {})
            state, lineage = row.get("state", {}), row.get("lineage", {})
            require(uuid4(state.get("client_msg_id")) and uuid4(lineage.get("enrollment")), "Missing issued enrollment identities")
            inv["clock"] = 5 + 2*f["writes"]
            inv["row"] = dict(scope=scope(target), revision=1+f["writes"],
                lineage=dict(enrollment=lineage["enrollment"], verifiedEpoch=0, basis="Unverified"),
                state=dict(scope=scope(target), client_msg_id=state["client_msg_id"],
                    desired=h["encoded"](f["text"], target, target == "thread"), local_revision=1+f["writes"],
                    acknowledged_local_revision=0, acknowledged=None, workflow="Idle", tombstone=False,
                    retired_ids=[], consumption=None, check_deadline=None, check_ticket=None,
                    observed_events={}, blocked_observation=None))
            if f["preserved"]:
                alts, origins = actual[target].get("alternates"), actual[target].get("origins")
                require(isinstance(alts, list) and len(alts) == 1 and isinstance(origins, list) and len(origins) == 1, "Missing exact preserved local alternate")
                wid = alts[0].get("writerId")
                origin = origins[0].get("origin", {})
                require(uuid4(wid) and isinstance(origin, dict) and set(origin) == {"LocalGeneration"} and uuid4(origin["LocalGeneration"]), "Preserved input lost local writer origin")
                inv["alternates"] = [dict(writerId=wid, revision=f["local_serial"], draft=local(f["alternate"], target == "thread"))]
                inv["origins"] = [dict(writerId=wid, origin=origin)]
        out[target] = inv
    same(actual, out, "Final full adoption inventory differs from real operation sequence")
    return out


def record_inventory(inv, envelope, generation=None):
    return dict(version=1, scope=inv["scope"], local=inv["local"], blocked=None,
        alternates=[[a["writerId"], a["revision"], a["draft"]] for a in inv["alternates"]],
        row=[inv["row"]["revision"], envelope(inv["row"])] if inv["row"] else None,
        origins=[[o["writerId"], o["origin"]] for o in inv["origins"]],
        clock=inv["clock"], generation=generation)


def validate(storage, receipt, h):
    same, require, uuid4, local, scope = (h[n] for n in ("same", "require", "uuid4", "local", "scope"))
    common = runpy.run_path(str(h["SCRIPTS"] / "protected_drafts_storage.py"))
    decode, envelope = common["unique_json"], common["envelope"]
    final, f = expected_inventory(receipt, h), facts(receipt, h)
    require(storage.is_absolute(), "Relative isolated storage path")
    def regular(path, directory=False):
        info = path.lstat()
        require(not path.is_symlink() and not getattr(info, "st_file_attributes", 0) & 0x400 and
                (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)), "Linked/reparse isolated path")
        return info
    for parent in (storage, *storage.parents): regular(parent, True)
    same(storage.name, "protected-adoption-" + receipt["nonce"], "Wrong owned adoption directory")
    def metadata(path):
        require(regular(path).st_size <= 1024*1024, "Oversized fixture marker")
        return decode(path.read_text(encoding="utf-8"))
    same(metadata(storage/"owner.json"), dict(version=1, nonce=receipt["nonce"]), "Parent ownership changed")
    worker = storage/"worker"; regular(worker, True)
    marker = metadata(worker/"fixture.json")
    require(isinstance(marker, dict) and set(marker) == {"version", "account", "nonce"} and uuid4(marker["nonce"]), "Invalid isolated worker marker")
    same(marker["version"], 1, "Marker format"); same(marker["account"], h["ACCOUNT"], "Wrong isolated account")
    for child in worker.iterdir():
        require(child.name in ("fixture.json", "fixture.lock", "drafts.sqlite", "drafts.sqlite-journal", "drafts.sqlite-wal", "drafts.sqlite-shm"), "Unexpected worker file")
        regular(child)
    db = worker/"drafts.sqlite"; require(0 < regular(db).st_size < 32*1024*1024, "Invalid bounded database")
    conn = sqlite3.connect(db.as_uri()+"?mode=ro", uri=True)
    try:
        conn.execute("PRAGMA query_only=ON")
        same(conn.execute("PRAGMA integrity_check").fetchall(), [("ok",)], "Database integrity failure")
        same({r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}, POPULATED|EMPTY, "Unexpected SQLite schema")
        for table in EMPTY: same(conn.execute('SELECT * FROM "'+table+'"').fetchall(), [], "Unexpected persisted work: "+table)
        same(conn.execute("SELECT * FROM composer_draft_owner").fetchall(), [(1,h["ACCOUNT"])], "Persistent account changed")
        same(conn.execute("SELECT * FROM composer_draft_adapter_format").fetchall(), [(1,1)], "Adapter format changed")
        same(conn.execute("SELECT * FROM composer_draft_continuity_format").fetchall(), [(1,7)], "Protected format changed")
        key = lambda target: json.dumps(scope(target)["key"], sort_keys=True)
        def keyed(rows, count):
            require(len(rows) == count, "Wrong exact raw destination row count")
            result = {}
            for row in rows:
                dest = json.dumps(decode(row[0]), sort_keys=True)
                require(dest not in result, "Duplicate normalized destination")
                result[dest] = row[1:]
            return result
        canonical = {k:decode(v[0]) for k,v in keyed(conn.execute("SELECT destination,content FROM composer_drafts").fetchall(),2).items()}
        same(canonical, {key(t):final[t]["local"] for t in TARGETS}, "Closed canonical input differs")
        rows = {k:dict(revision=v[0],format=v[1],envelope=decode(v[2])) for k,v in keyed(conn.execute("SELECT destination,storage_revision,format_version,state FROM composer_draft_sync").fetchall(),1).items()}
        same(rows, {key(f["target"]):dict(revision=final[f["target"]]["row"]["revision"],format=7,envelope=envelope(final[f["target"]]["row"]))}, "Full closed protected envelope differs")
        clocks = keyed(conn.execute("SELECT destination,revision,generation,writer_id,serial FROM composer_draft_local_clock").fetchall(),2)
        same(set(clocks), {key(t) for t in TARGETS}, "Clock scopes changed")
        raw_writers = conn.execute("SELECT writer_id,destination,content FROM composer_draft_writer_lineage").fetchall()
        require(len(raw_writers) == (3 if f["registered"] else 2), "Wrong actual writer registration count")
        writers = {}
        for wid, dest, raw in raw_writers:
            require(uuid4(wid) and wid not in writers, "Invalid/duplicate writer identity")
            item = decode(raw)
            same(item.get("scope",{}).get("key"), decode(dest), "Writer destination differs")
            last = item.get("last")
            if isinstance(last,dict) and isinstance(last.get("result",{}).get("row"),list):
                pair=last["result"]["row"]; require(len(pair)==2 and isinstance(pair[1],str), "Invalid retained row envelope")
                pair[1]=decode(pair[1])
            writers[wid]=item
        for target in TARGETS:
            candidates=[(wid,r) for wid,r in writers.items() if r.get("scope")==scope(target) and isinstance(r.get("origin"),dict) and set(r["origin"])=={"LocalGeneration"}]
            require(len(candidates)==1, "Missing unique original local writer")
            wid,item=candidates[0]; generation=item["origin"]["LocalGeneration"]
            require(uuid4(generation), "Invalid original local generation")
            preserved=f["preserved"] and target==f["target"]
            seed=dict(scope=scope(target),clock=4,row=None,local=local(h["INITIAL"],target=="thread"),blocked=None,alternates=[],origins=[],uncommitted=None)
            if preserved:
                same(final[target]["alternates"][0]["writerId"],wid,"Alternate lost original local writer")
                same(final[target]["origins"][0]["origin"],dict(LocalGeneration=generation),"Alternate generation changed")
            expected=dict(version=1,scope=scope(target),writer=wid,origin=dict(LocalGeneration=generation),processed=f["local_serial"] if preserved else 1,
                last=dict(serial=f["local_serial"] if preserved else 1, local=local(f["alternate"] if preserved else h["INITIAL"],target=="thread"),direct=None,
                    continuation=dict(generation=generation,predecessor=f["local_serial"]-1 if preserved else 0,expected=local(f["predecessor_text"],target=="thread") if preserved else local("")),
                    preserved=preserved,result=record_inventory(final[target] if preserved else seed,envelope,None if preserved else generation)))
            same(writers.pop(wid),expected,"Exact original local writer receipt differs")
            expected_clock=(final[target]["clock"],None,None,0) if target==f["target"] else (4,generation,wid,1)
            same(clocks[key(target)],expected_clock,"Final local generation/serial ownership differs")
        if f["registered"]:
            retained=receipt["finalProjection"]["retained"]
            require(isinstance(retained,list) and len(retained)==1,"Missing exact retained registered Editor")
            entry=retained[0]; wid=entry.get("writerId")
            require(uuid4(wid) and wid in writers,"Retained managed writer absent")
            inv=final[f["target"]]
            same(entry.get("scope"),scope(f["target"]),"Retained Editor scope changed")
            same(entry.get("enrollment"),inv["row"]["lineage"]["enrollment"],"Retained enrollment changed")
            expected=dict(version=1,scope=scope(f["target"]),writer=wid,origin=dict(Enrollment=inv["row"]["lineage"]["enrollment"]),processed=f["writes"],last=None)
            if f["writes"]:
                expected["last"]=dict(serial=f["writes"],local=inv["local"],direct=None,continuation=None,preserved=False,result=record_inventory(inv,envelope))
            same(writers.pop(wid),expected,"Managed writer receipt or exact retry differs")
        else: same(receipt["finalProjection"]["retained"],[],"Refused registration invented an Editor")
        same(writers,{},"Unrelated writer remains")
        raw_alternates=conn.execute("SELECT destination,writer_id,revision,content FROM composer_draft_alternates ORDER BY destination,writer_id").fetchall()
        expected_alternates=[(key(t),a["writerId"],a["revision"],a["draft"]) for t in TARGETS for a in final[t]["alternates"]]
        normalized=[(json.dumps(decode(dest),sort_keys=True),wid,revision,decode(raw)) for dest,wid,revision,raw in raw_alternates]
        same(normalized,expected_alternates,"Closed preserved input/identity differs")
        teams=conn.execute("SELECT team,content FROM composer_draft_continuity").fetchall()
        require(len(teams)==1 and teams[0][0]=="T1","Unexpected continuity teams")
        same(decode(teams[0][1]),TEAM,"Local enrollment invented stream/remote authority")
        return dict(ok=True,database=str(db),workerMarker=marker,integrity="ok",canonical=canonical,protected=rows,
                    clocks=clocks,writerCount=len(raw_writers),alternates=normalized,continuity=TEAM,emptyTables=sorted(EMPTY))
    finally:
        conn.close()
