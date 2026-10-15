"""Synthetic closed SQLite acceptance and corruption cases; no native launches."""
from contextlib import closing
import json
from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest

D=runpy.run_path(str(Path(__file__).with_name('protected_enrollment_test_data.py')))
C=D['C'];uid=D['uid']
OLD=runpy.run_path(str(C['SCRIPTS']/'protected_drafts_storage.py'))


def database(temp,value):
    storage=Path(temp)/('protected-enrollment-'+value['nonce']);storage.mkdir()
    (storage/'owner.json').write_text(json.dumps(dict(version=1,nonce=value['nonce'])))
    worker=storage/'worker';worker.mkdir()
    (worker/'fixture.json').write_text(json.dumps(dict(version=1,account=C['ACCOUNT'],nonce=uid(900))))
    (worker/'fixture.lock').touch();db=worker/'drafts.sqlite'
    scenario=value['scenario'];failure=scenario=='failed-editor-channel';late=scenario=='late-typing-channel';success=scenario.startswith('enroll-') or scenario=='canceled-waiter-channel'
    with closing(sqlite3.connect(db)) as conn,conn:
        conn.executescript('''
CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT);
CREATE TABLE composer_draft_sync(destination TEXT PRIMARY KEY,storage_revision INTEGER,format_version INTEGER,state TEXT);
CREATE TABLE composer_draft_owner(singleton INTEGER,account TEXT);
CREATE TABLE composer_draft_adapter_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity_format(singleton INTEGER,version INTEGER);
CREATE TABLE composer_draft_continuity(team TEXT,content TEXT);
CREATE TABLE composer_draft_writer_lineage(writer_id TEXT,destination TEXT,content TEXT);
CREATE TABLE composer_draft_local_clock(destination TEXT,revision INTEGER,generation TEXT,writer_id TEXT,serial INTEGER);
CREATE TABLE composer_draft_protected_archive(enrollment TEXT,team TEXT,content TEXT);
CREATE TABLE upload_receipts(sequence INTEGER PRIMARY KEY AUTOINCREMENT);
''')
        for table in ('composer_draft_blocked','composer_draft_alternates','composer_draft_reviewed_writers','composer_draft_known_read_limit','composer_draft_baseline_evidence','composer_draft_execution_result','composer_draft_recovery_editing','composer_draft_release_head','composer_draft_continuity_reads','composer_draft_recovery_owners','composer_draft_status_reads','composer_draft_read_abandonment','composer_draft_read_late','upload_journal_identity'):
            conn.execute('CREATE TABLE '+table+'(value TEXT)')
        conn.execute('INSERT INTO composer_draft_owner VALUES(1,?)',(C['ACCOUNT'],))
        conn.execute('INSERT INTO composer_draft_adapter_format VALUES(1,1)')
        conn.execute('INSERT INTO composer_draft_continuity_format VALUES(1,7)')
        if success or failure:
            conn.execute("INSERT INTO composer_draft_continuity VALUES('T1',?)",(json.dumps(dict(epoch=1,lease_id=None,mode='Gap',sequence=0,pending=None,last_interrupted_id=None,gap_reason='activation',read_retry_at_ms=0)),))
        for i,target in enumerate(('channel','thread')):
            key=json.dumps(C['scope'](target)['key']);inv=value['finalInventory'][target]
            conn.execute('INSERT INTO composer_drafts VALUES(?,?)',(key,json.dumps(inv['local'])))
            if inv['row']:conn.execute('INSERT INTO composer_draft_sync VALUES(?,?,7,?)',(key,1,json.dumps(OLD['envelope'](inv['row']))))
            wid,generation=uid(600+i),uid(700+i);changed=late and target=='channel';serial=2 if changed else 1;clock=7 if changed else 4
            loc=C['local'](C['NEWER'] if changed else C['INITIAL'],target=='thread')
            record=dict(version=1,scope=C['scope'](target),writer=wid,origin=dict(LocalGeneration=generation),processed=serial,last=dict(serial=serial,local=loc,direct=None,continuation=dict(generation=generation,predecessor=1 if changed else 0,expected=C['local'](C['INITIAL'],target=='thread') if changed else C['local']('')),preserved=False,result=dict(version=1,scope=C['scope'](target),local=loc,blocked=None,alternates=[],row=None,origins=[],clock=clock,generation=generation)))
            conn.execute('INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)',(wid,key,json.dumps(record)))
            if success and target==value['target']:clock,generation,wid,serial=5,None,None,0
            if failure and target=='channel':clock,generation,wid,serial=6,uid(800),value['finalProjection']['retained'][0]['writerId'],0
            conn.execute('INSERT INTO composer_draft_local_clock VALUES(?,?,?,?,?)',(key,clock,generation,wid,serial))
        if failure:
            owner=value['finalProjection']['retained'][0];key=json.dumps(C['scope']('channel')['key'])
            record=dict(version=1,scope=C['scope']('channel'),writer=owner['writerId'],origin=dict(Enrollment=owner['enrollment']),processed=0,last=None)
            conn.execute('INSERT INTO composer_draft_writer_lineage VALUES(?,?,?)',(owner['writerId'],key,json.dumps(record)))
            row=D['enrolled']('channel')['row'];row['lineage']['enrollment']=owner['enrollment']
            archive=dict(version=1,scope=C['scope']('channel'),enrollment=owner['enrollment'],revision=1,row=json.dumps(OLD['envelope'](row)),local=C['local'](C['INITIAL']),blocked=None,alternates=[],writer_floors=[])
            conn.execute("INSERT INTO composer_draft_protected_archive VALUES(?,'T1',?)",(owner['enrollment'],json.dumps(archive)))
    return storage,db


class ClosedStorage(unittest.TestCase):
    def test_every_case_full_storage_and_read_only_validation(self):
        for scenario in C['SCENARIOS']:
            with self.subTest(scenario=scenario),tempfile.TemporaryDirectory() as temp:
                value=D['good'](scenario);storage,db=database(temp,value);before=db.read_bytes()
                self.assertTrue(C['validate_closed_storage'](storage,value)['ok']);self.assertEqual(db.read_bytes(),before)

    def test_duplicate_normalized_destinations_never_collapse(self):
        for scenario in C['SCENARIOS']:
            for table in ('composer_drafts','composer_draft_local_clock','composer_draft_sync'):
                with self.subTest(scenario=scenario,table=table),tempfile.TemporaryDirectory() as temp:
                    value=D['good'](scenario);storage,db=database(temp,value)
                    with closing(sqlite3.connect(db)) as conn,conn:
                        row=conn.execute('SELECT * FROM '+table+' LIMIT 1').fetchone()
                        if row is None:continue
                        row=list(row);row[0]=json.dumps(json.loads(row[0]),separators=(',',':'))
                        conn.execute('INSERT INTO '+table+' VALUES('+','.join('?' for _ in row)+')',row)
                    with self.assertRaises(RuntimeError):C['validate_closed_storage'](storage,value)

    def test_failed_pending_never_becomes_canonical_or_journal_ack(self):
        for sql,args in [('UPDATE composer_drafts SET content=? WHERE destination=?',(json.dumps(C['local'](C['NEWER'])),json.dumps(C['scope']('channel')['key']))),('UPDATE composer_draft_local_clock SET serial=1 WHERE writer_id=?',(uid(40),)),('DELETE FROM composer_draft_protected_archive',())]:
            with self.subTest(sql=sql),tempfile.TemporaryDirectory() as temp:
                value=D['good']('failed-editor-channel');storage,db=database(temp,value)
                with closing(sqlite3.connect(db)) as conn,conn:conn.execute(sql,args)
                with self.assertRaises(RuntimeError):C['validate_closed_storage'](storage,value)

    def test_no_stream_authority_unrelated_rows_or_wrong_owner(self):
        for sql,args in [("UPDATE composer_draft_continuity SET content=?",(json.dumps(dict(epoch=1,lease_id=uid(100),mode='Open',sequence=0,pending=None,last_interrupted_id=None,gap_reason='activation',read_retry_at_ms=0)),)),("INSERT INTO composer_draft_execution_result VALUES('unexpected')",()),('UPDATE composer_draft_owner SET account=?',(uid(999),)),('UPDATE composer_draft_sync SET format_version=5',())]:
            with self.subTest(sql=sql),tempfile.TemporaryDirectory() as temp:
                value=D['good']();storage,db=database(temp,value)
                with closing(sqlite3.connect(db)) as conn,conn:conn.execute(sql,args)
                with self.assertRaises(RuntimeError):C['validate_closed_storage'](storage,value)


if __name__=='__main__':unittest.main()
