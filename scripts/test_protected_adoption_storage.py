"""Pure synthetic closed-database oracle tests. No application/native launches."""
from contextlib import closing
import copy
import json
from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest

P=Path(__file__).resolve().parent
SCRIPTS=next(p/"scripts" for p in P.parents if (p/"Cargo.toml").is_file())
M=runpy.run_path(str(P/"protected_adoption_storage.py"))
BASE=runpy.run_path(str(SCRIPTS/"protected_drafts_receipts.py"))
OLD=runpy.run_path(str(SCRIPTS/"protected_drafts_storage.py"))
ACCOUNT="00000000-0000-4000-8000-000000000351"
INITIAL="Local draft ready for recovery"
NEWER="Later local input \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
FINAL=NEWER+" \u2014 after retry"
uid=lambda n:f"00000000-0000-4000-8000-{n:012x}"
def scope(t):return dict(account_id=ACCOUNT,key=dict(team="T1",channel="C2",thread="1719800400.000000" if t=="thread" else None))
H={n:BASE[n] for n in ("same","require","uuid4","local","encoded")}
H.update(SCRIPTS=SCRIPTS,ACCOUNT=ACCOUNT,INITIAL=INITIAL,NEWER=NEWER,FINAL=FINAL,scope=scope)
local=H["local"]


def receipt(scenario):
    target="thread" if scenario=="adopt-thread" else "channel"
    writes=2 if scenario=="failed-managed-write-channel" else 1 if scenario in ("adopt-channel","adopt-thread","cancel-boundaries-channel") else 0
    preserved=scenario in ("late-input-channel","native-refusals-channel")
    final={}
    for i,t in enumerate(("channel","thread")):
        text=(FINAL if writes==2 else NEWER if writes else INITIAL) if t==target else INITIAL
        inv=dict(scope=scope(t),clock=4,local=local(text,t=="thread"),row=None,blocked=None,alternates=[],origins=[],uncommitted=None)
        if t==target:
            inv["clock"]=5+2*writes
            inv["row"]=dict(scope=scope(t),revision=1+writes,lineage=dict(enrollment=uid(100),verifiedEpoch=0,basis="Unverified"),state=dict(scope=scope(t),client_msg_id=uid(101),desired=H["encoded"](text,t,t=="thread"),local_revision=1+writes,acknowledged_local_revision=0,acknowledged=None,workflow="Idle",tombstone=False,retired_ids=[],consumption=None,check_deadline=None,check_ticket=None,observed_events={},blocked_observation=None))
            if preserved:
                inv["alternates"]=[dict(writerId=uid(200+i),revision=3 if scenario=="native-refusals-channel" else 2,draft=local(NEWER if scenario=="late-input-channel" else INITIAL,t=="thread"))]
                inv["origins"]=[dict(writerId=uid(200+i),origin=dict(LocalGeneration=uid(300+i)))]
        final[t]=inv
    retained=[] if scenario=="late-input-channel" else [dict(scope=scope(target),writerId=uid(400),enrollment=uid(100))]
    return dict(scenario=scenario,target=target,nonce="a"*32,finalInventory=final,finalProjection=dict(retained=retained))


def database(temp,value):
    root=Path(temp)/("protected-adoption-"+value["nonce"]);root.mkdir()
    (root/"owner.json").write_text(json.dumps(dict(version=1,nonce=value["nonce"])),encoding="utf-8")
    worker=root/"worker";worker.mkdir()
    (worker/"fixture.json").write_text(json.dumps(dict(version=1,account=ACCOUNT,nonce=uid(900))),encoding="utf-8")
    (worker/"fixture.lock").touch();db=worker/"drafts.sqlite"
    f=M["facts"](value,H);final=value["finalInventory"]
    with closing(sqlite3.connect(db)) as c,c:
        c.executescript("""
CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT);
CREATE TABLE composer_draft_sync(destination TEXT PRIMARY KEY,storage_revision INTEGER,format_version INTEGER,state TEXT);
CREATE TABLE composer_draft_owner(singleton INTEGER,account TEXT);
CREATE TABLE composer_draft_adapter_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity(team TEXT,content TEXT);
CREATE TABLE composer_draft_writer_lineage(writer_id TEXT,destination TEXT,content TEXT);
CREATE TABLE composer_draft_local_clock(destination TEXT,revision INTEGER,generation TEXT,writer_id TEXT,serial INTEGER);
CREATE TABLE composer_draft_alternates(destination TEXT,writer_id TEXT,revision INTEGER,content TEXT);
CREATE TABLE upload_receipts(sequence INTEGER PRIMARY KEY AUTOINCREMENT);
""")
        for table in M["EMPTY"]-{"upload_receipts","sqlite_sequence"}:c.execute("CREATE TABLE "+table+"(value TEXT)")
        c.execute("INSERT INTO composer_draft_owner VALUES(1,?)",(ACCOUNT,))
        c.execute("INSERT INTO composer_draft_adapter_format VALUES(1,1)")
        c.execute("INSERT INTO composer_draft_continuity_format VALUES(1,7)")
        c.execute("INSERT INTO composer_draft_continuity VALUES('T1',?)",(json.dumps(M["TEAM"]),))
        for i,t in enumerate(("channel","thread")):
            key=json.dumps(scope(t)["key"]);inv=final[t];wid,generation=uid(200+i),uid(300+i)
            c.execute("INSERT INTO composer_drafts VALUES(?,?)",(key,json.dumps(inv["local"])))
            if inv["row"]:c.execute("INSERT INTO composer_draft_sync VALUES(?,?,7,?)",(key,inv["row"]["revision"],json.dumps(OLD["envelope"](inv["row"]))))
            clock=(inv["clock"],None,None,0) if t==f["target"] else (4,generation,wid,1)
            c.execute("INSERT INTO composer_draft_local_clock VALUES(?,?,?,?,?)",(key,*clock))
            preserved=f["preserved"] and t==f["target"]
            seed=dict(scope=scope(t),clock=4,row=None,local=local(INITIAL,t=="thread"),blocked=None,alternates=[],origins=[],uncommitted=None)
            result=M["record_inventory"](inv if preserved else seed,OLD["envelope"],None if preserved else generation)
            if result["row"]:result["row"][1]=json.dumps(result["row"][1])
            record=dict(version=1,scope=scope(t),writer=wid,origin=dict(LocalGeneration=generation),processed=f["local_serial"] if preserved else 1,last=dict(serial=f["local_serial"] if preserved else 1,local=local(f["alternate"] if preserved else INITIAL,t=="thread"),direct=None,continuation=dict(generation=generation,predecessor=f["local_serial"]-1 if preserved else 0,expected=local(f["predecessor_text"],t=="thread") if preserved else local("")),preserved=preserved,result=result))
            c.execute("INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)",(wid,key,json.dumps(record)))
            for a in inv["alternates"]:c.execute("INSERT INTO composer_draft_alternates VALUES(?,?,?,?)",(key,a["writerId"],a["revision"],json.dumps(a["draft"])))
        if f["registered"]:
            inv=final[f["target"]];last=None
            if f["writes"]:
                result=M["record_inventory"](inv,OLD["envelope"]);result["row"][1]=json.dumps(result["row"][1])
                last=dict(serial=f["writes"],local=inv["local"],direct=None,continuation=None,preserved=False,result=result)
            record=dict(version=1,scope=scope(f["target"]),writer=uid(400),origin=dict(Enrollment=uid(100)),processed=f["writes"],last=last)
            c.execute("INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)",(uid(400),json.dumps(scope(f["target"])["key"]),json.dumps(record)))
    return root,db


def mutate_record(db,wid,edit):
    with closing(sqlite3.connect(db)) as c,c:
        value=json.loads(c.execute("SELECT content FROM composer_draft_writer_lineage WHERE writer_id=?",(wid,)).fetchone()[0]);edit(value)
        c.execute("UPDATE composer_draft_writer_lineage SET content=? WHERE writer_id=?",(json.dumps(value),wid))


class ClosedAdoptionStorage(unittest.TestCase):
    def test_all_seven_full_closed_inventories_read_only(self):
        for scenario in M["SCENARIOS"]:
            with self.subTest(scenario=scenario),tempfile.TemporaryDirectory() as temp:
                value=receipt(scenario);root,db=database(temp,value);before=db.read_bytes()
                self.assertTrue(M["validate"](root,value,H)["ok"]);self.assertEqual(db.read_bytes(),before)

    def test_canonical_sync_clock_duplicates_do_not_collapse(self):
        for table in ("composer_drafts","composer_draft_sync","composer_draft_local_clock"):
            with self.subTest(table=table),tempfile.TemporaryDirectory() as temp:
                value=receipt("adopt-channel");root,db=database(temp,value)
                with closing(sqlite3.connect(db)) as c,c:
                    row=list(c.execute("SELECT * FROM "+table+" LIMIT 1").fetchone());row[0]=json.dumps(json.loads(row[0]),separators=(",",":"))
                    c.execute("INSERT INTO "+table+" VALUES("+",".join("?" for _ in row)+")",row)
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_preserved_alternate_never_becomes_canonical_or_other_origin(self):
        for scenario in ("late-input-channel","native-refusals-channel"):
            changes=[("DELETE FROM composer_draft_alternates",()),("UPDATE composer_draft_alternates SET revision=4",()),("UPDATE composer_draft_alternates SET writer_id=?",(uid(999),)),("UPDATE composer_draft_local_clock SET revision=6 WHERE generation IS NULL",())]
            if scenario.startswith("late"):
                changes.append(("UPDATE composer_drafts SET content=? WHERE destination=?",(json.dumps(local(NEWER)),json.dumps(scope("channel")["key"]))))
            for sql,args in changes:
                with self.subTest(scenario=scenario,sql=sql),tempfile.TemporaryDirectory() as temp:
                    value=receipt(scenario);root,db=database(temp,value)
                    with closing(sqlite3.connect(db)) as c,c:c.execute(sql,args)
                    with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_writer_command_nulls_predecessor_and_origin_are_exact(self):
        edits=[lambda r:r["last"].pop("direct"),lambda r:r["last"].update(direct={}),lambda r:r["last"]["continuation"].update(predecessor=0),lambda r:r["last"].update(preserved=False),lambda r:r.update(origin=dict(Enrollment=uid(100)))]
        for edit in edits:
            with tempfile.TemporaryDirectory() as temp:
                value=receipt("late-input-channel");root,db=database(temp,value);mutate_record(db,uid(200),edit)
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_native_aba_requires_two_routed_inputs_and_exact_temporary_predecessor(self):
        for mutation in ("processed", "serial", "predecessor", "expected", "alternate"):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as temp:
                value=receipt("native-refusals-channel");root,db=database(temp,value)
                if mutation=="alternate":
                    with closing(sqlite3.connect(db)) as c,c:c.execute("UPDATE composer_draft_alternates SET revision=2")
                else:
                    def edit(r):
                        if mutation=="processed":r["processed"]=2
                        elif mutation=="serial":r["last"]["serial"]=2
                        elif mutation=="predecessor":r["last"]["continuation"]["predecessor"]=1
                        else:r["last"]["continuation"]["expected"]=local(INITIAL)
                    mutate_record(db,uid(200),edit)
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_retry_cannot_register_another_writer_or_replay_serial(self):
        for mutation in ("extra","serial","row","null","input"):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as temp:
                value=receipt("failed-managed-write-channel");root,db=database(temp,value)
                if mutation=="extra":
                    with closing(sqlite3.connect(db)) as c,c:
                        row=list(c.execute("SELECT * FROM composer_draft_writer_lineage WHERE writer_id=?",(uid(400),)).fetchone());row[0]=uid(999);c.execute("INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)",row)
                else:
                    def edit(r):
                        if mutation=="serial":r["processed"]=3
                        elif mutation=="row":r["last"]["result"]["row"][1]=json.dumps(dict(state={}))
                        elif mutation=="null":r["last"]=None
                        else:r["last"]["local"]=local(NEWER)
                    mutate_record(db,uid(400),edit)
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_unrelated_work_stream_authority_and_envelope_fields_refused(self):
        for mutation in ("stream","unknown","owner","schema","boolean"):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as temp:
                value=receipt("adopt-thread");root,db=database(temp,value)
                with closing(sqlite3.connect(db)) as c,c:
                    if mutation=="stream":c.execute("UPDATE composer_draft_continuity SET content=?",(json.dumps(dict(M["TEAM"],mode="Open",lease_id=uid(999))),))
                    elif mutation=="owner":c.execute("UPDATE composer_draft_owner SET account=?",(uid(999),))
                    elif mutation=="schema":c.execute("CREATE TABLE unexpected(value TEXT)")
                    else:
                        raw=json.loads(c.execute("SELECT state FROM composer_draft_sync").fetchone()[0])
                        if mutation=="unknown":raw["extra"]=None
                        else:raw["verified_epoch"]=False
                        c.execute("UPDATE composer_draft_sync SET state=?",(json.dumps(raw),))
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_terminal_inventory_does_not_authorize_fabricated_sql(self):
        for mutation in ("clock","cleared","basis","uncertain","alternate","client"):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as temp:
                value=receipt("adopt-channel");root,db=database(temp,value);inv=value["finalInventory"]["channel"]
                if mutation=="clock":inv["clock"]+=1
                elif mutation=="cleared":inv["local"]=local("")
                elif mutation=="basis":inv["row"]["lineage"]["basis"]="Local"
                elif mutation=="uncertain":inv["row"]["state"]["workflow"]={"Uncertain":{}}
                elif mutation=="alternate":inv["alternates"]=[{}]
                else:inv["row"]["state"]["client_msg_id"]="not-an-issued-uuid"
                with self.assertRaises(RuntimeError):M["validate"](root,value,H)

    def test_marker_identity_duplicate_json_and_missing_worker_refused(self):
        for mode in ("owner","worker","duplicate","missing"):
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temp:
                value=receipt("adopt-channel");root,db=database(temp,value)
                if mode=="owner":(root/"owner.json").write_text(json.dumps(dict(version=1,nonce="b"*32)))
                elif mode=="worker":(root/"worker/fixture.json").write_text(json.dumps(dict(version=1,nonce=uid(900),account=uid(999))))
                elif mode=="missing":(root/"worker/fixture.json").unlink()
                else:
                    with closing(sqlite3.connect(db)) as c,c:c.execute("UPDATE composer_drafts SET content=?",('{"composer":{},"composer":{}}',))
                with self.assertRaises((RuntimeError,FileNotFoundError)):M["validate"](root,value,H)

if __name__=="__main__":unittest.main()
