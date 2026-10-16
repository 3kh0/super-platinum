"""Temporary SQLite corruption tests of the actual post-exit oracle."""
import copy,json,runpy,sqlite3,tempfile,unittest
from pathlib import Path
from unittest.mock import patch

F=runpy.run_path(str(Path(__file__).with_name('owned_send_test_fixture.py')))
C=F['C'];S=runpy.run_path(str(Path(__file__).with_name('protected_owned_send_storage.py')))


def populated(conn,receipt):
    def table(name,columns,rows=()):
        conn.execute('CREATE TABLE '+name+'('+','.join(c+' '+t for c,t in columns)+')')
        for row in rows:conn.execute('INSERT INTO '+name+' VALUES('+','.join('?' for _ in columns)+')',row)
    raw=lambda x:json.dumps(x,ensure_ascii=False,separators=(',',':'))
    env=C['STORAGE']['envelope'];target=receipt['target'];other='thread' if target=='channel' else 'channel'
    records={v['stage']:v for v in receipt['snapshots']};initial=records['initial']['durable']['scopes'];final=records['result']['durable']['scopes']
    newer=records['newer']['durable']['scopes'][target];operation=records['result']['projection']['operation'];op=operation['operation'];intent=newer['row']['state']['consumption']
    committed=copy.deepcopy(newer);committed.update(operation['recorded']);account=receipt['account'];editors=records['result']['projection']['editors']
    def key(t):return raw(C['scope'](account,t)['key'])
    def inventory(v):return dict(version=1,scope=v['scope'],local=v['local'],blocked=None,alternates=[],row=None if v['row'] is None else [v['row']['revision'],raw(env(v['row']))],origins=[],clock=v['clock'],generation=None)
    table('composer_drafts',[('destination','TEXT'),('content','TEXT')],[(key(t),raw(final[t]['local'])) for t in final])
    row=final[other]['row']
    table('composer_draft_sync',[('destination','TEXT'),('storage_revision','INTEGER'),('format_version','INTEGER'),('state','TEXT')],[(key(other),row['revision'],7,raw(env(row)))])
    for name,col,value in [('composer_draft_owner','account',account),('composer_draft_adapter_format','version',1),('composer_draft_continuity_format','version',7)]:
        table(name,[('singleton','INTEGER'),(col,'TEXT' if isinstance(value,str) else 'INTEGER')],[(1,value)])
    clocks=[(key(other),initial[other]['clock'],None,None,0),(key(target),final[target]['clock'],F['uid'](500),editors[target]['writerId'],3)]
    table('composer_draft_local_clock',[('destination','TEXT'),('revision','INTEGER'),('generation','TEXT'),('writer_id','TEXT'),('serial','INTEGER')],clocks)
    writers=[]
    for t in final:
        v=newer if t==target else initial[t];serial=3 if t==target else 1;writer=editors[t]['writerId']
        value=dict(version=1,scope=v['scope'],writer=writer,origin=dict(Enrollment=initial[t]['row']['lineage']['enrollment']),processed=serial,
                   last=dict(serial=serial,local=v['local'],direct=intent if t==target else None,continuation=None,preserved=False,result=inventory(v)))
        writers.append((writer,key(t),raw(value)))
    table('composer_draft_writer_lineage',[('writer_id','TEXT'),('destination','TEXT'),('content','TEXT')],writers)
    enrollment=initial[target]['row']['lineage']['enrollment']
    archive=dict(version=1,scope=committed['scope'],enrollment=enrollment,revision=committed['row']['revision'],row=raw(env(committed['row'])),local=committed['local'],blocked=None,alternates=[],writer_floors=[])
    table('composer_draft_protected_archive',[('enrollment','TEXT'),('team','TEXT'),('content','TEXT')],[(enrollment,'T1',raw(archive))])
    jid='mutation:'+op['id']
    entry=dict(version=1,id=jid,scope=committed['scope'],enrollment=enrollment,operation=op,input=dict(Mutation=dict(Sent=dict(client_msg_id=op['client_msg_id'],draft_id=None,ts=C['ACK_TS']))),positive_accepted=True,original=inventory(committed))
    table('composer_draft_execution_result',[('id','TEXT'),('team','TEXT'),('content','TEXT')],[(jid,'T1',raw(entry))])
    team=dict(epoch=2,lease_id=F['uid'](501),mode='Open',sequence=0,pending=None,last_interrupted_id=None,gap_reason='new stream requires per-owner verification',read_retry_at_ms=0)
    table('composer_draft_continuity',[('team','TEXT'),('content','TEXT')],[('T1',raw(team))])
    table('composer_draft_migration_receipt',[('singleton','INTEGER'),('version','INTEGER'),('account','TEXT'),('id','TEXT'),('source','TEXT'),('result','TEXT')],
          [(1,1,account,F['uid'](502),'null',raw(dict(columns=['destination','storage_revision','format_version','state'],rows=[])))])
    empty=['composer_draft_blocked','composer_draft_alternates','composer_draft_reviewed_writers','composer_draft_known_read_limit','composer_draft_baseline_evidence','composer_draft_recovery_editing','composer_draft_release_head','composer_draft_continuity_reads','composer_draft_recovery_owners','composer_draft_status_reads','composer_draft_read_abandonment','composer_draft_read_late','upload_journal_identity']
    for name in empty:table(name,[('unused','TEXT')])
    # Creating the real SQLite internal sequence table requires AUTOINCREMENT.
    conn.execute('CREATE TABLE upload_receipts(id INTEGER PRIMARY KEY AUTOINCREMENT)')
    conn.commit()


class Storage(unittest.TestCase):
    def check(self,mutate=None,target='channel'):
        receipt=json.loads(json.dumps(F['good']('newer-before-prepare-'+target)))
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'fixture.sqlite'
            conn=sqlite3.connect(path)
            try:
                populated(conn,receipt)
                if mutate:mutate(conn,receipt)
                conn.commit()
                return S['validate_connection'](conn,receipt,C)
            finally:conn.close()

    def reject(self,mutate):
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):self.check(mutate)

    def test_both_complete_post_exit_logical_inventories(self):
        for target in ('channel','thread'):
            with self.subTest(target=target):self.assertEqual(self.check(target=target)['archive']['local'],C['local'](C['NEWER']))

    def test_missing_or_counterfeit_authority_tables(self):
        self.reject(lambda db,r:db.execute('DROP TABLE composer_draft_execution_result'))
        self.reject(lambda db,r:db.execute('CREATE TABLE counterfeit_permit(value TEXT)'))
        self.reject(lambda db,r:db.execute("INSERT INTO composer_draft_release_head VALUES('unconsumed')"))

    def test_canonical_and_unrelated_scope_bytes_cannot_drift(self):
        self.reject(lambda db,r:db.execute("UPDATE composer_drafts SET content='{}'"))
        self.reject(lambda db,r:db.execute('DELETE FROM composer_draft_sync'))

    def test_duplicate_semantic_destination_refused(self):
        def duplicate(db,r):
            key,raw=db.execute('SELECT destination,content FROM composer_drafts LIMIT 1').fetchone()
            db.execute('INSERT INTO composer_drafts VALUES(?,?)',(json.dumps(json.loads(key),indent=2),raw))
        self.reject(duplicate)

    def test_exact_archive_and_completed_call_full_record(self):
        def mutate(table,change):
            def apply(db,r):
                value=json.loads(db.execute('SELECT content FROM '+table).fetchone()[0]);change(value)
                db.execute('UPDATE '+table+' SET content=?',(json.dumps(value),))
            return apply
        self.reject(mutate('composer_draft_protected_archive',lambda v:v.update(revision=7)))
        self.reject(mutate('composer_draft_execution_result',lambda v:v.update(positive_accepted=False)))
        self.reject(mutate('composer_draft_execution_result',lambda v:v['original'].pop('generation')))
        self.reject(mutate('composer_draft_execution_result',lambda v:v.update(unrecognized_authority=True)))

    def test_writer_lease_and_pending_state_are_exact(self):
        self.reject(lambda db,r:db.execute('UPDATE composer_draft_local_clock SET serial=4 WHERE generation IS NOT NULL'))
        self.reject(lambda db,r:db.execute('DELETE FROM composer_draft_writer_lineage'))
        self.reject(lambda db,r:db.execute("UPDATE composer_draft_continuity SET content='{}'"))

    def test_fresh_store_history_cannot_claim_imported_state(self):
        self.reject(lambda db,r:db.execute("UPDATE composer_draft_migration_receipt SET source='{}'"))
        self.reject(lambda db,r:db.execute("UPDATE composer_draft_migration_receipt SET account='wrong'"))

    def test_duplicate_persisted_json_rejected(self):
        self.reject(lambda db,r:db.execute("UPDATE composer_drafts SET content='{\"composer\":{},\"composer\":{}}'"))

    def test_real_file_identity_changes_on_replacement(self):
        with tempfile.TemporaryDirectory() as root:
            a=Path(root)/'a';b=Path(root)/'b';a.write_bytes(b'a');b.write_bytes(b'a')
            with a.open('rb') as fa,b.open('rb') as fb:
                self.assertNotEqual(S['identity'](fa),S['identity'](fb))
                self.assertEqual(S['identity'](fa),S['identity'](fa))

    def full_file_audit(self,change=None):
        with tempfile.TemporaryDirectory(prefix="sp-fixture-storage-test-") as temporary:
            base=Path(temporary).resolve();receipt=F['good']();ticket=json.loads(receipt['ownedTicket'])
            root=base/('super-platinum-upgrade-'+ticket['nonce']);root.mkdir()
            owner=root/'owner';owner.write_text(receipt['account'],encoding='ascii')
            db=root/('drafts-'+receipt['account']+'.sqlite')
            conn=sqlite3.connect(db)
            try:populated(conn,receipt)
            finally:conn.close()
            with owner.open('rb') as f:ticket['marker']=S['identity'](f)
            with db.open('rb') as f:ticket['database']=S['identity'](f)
            receipt['ownedTicket']=json.dumps(ticket)
            storage=base/('protected-owned-send-'+receipt['nonce']);storage.mkdir()
            (storage/'owner.json').write_text(json.dumps(dict(version=1,nonce=receipt['nonce'])),encoding='utf-8')
            receipt['isolatedRoot']=str(storage)
            if change:change(root,db,owner,receipt)
            with patch('tempfile.gettempdir', side_effect=AssertionError('Parent temporary root must not be consulted')):
                return S['validate'](storage,receipt,C,temporary_root=base)

    def test_full_closed_file_identity_and_read_only_byte_audit(self):
        self.assertTrue(self.full_file_audit()['ok'])

    def test_sidecar_and_wal_header_refused_before_sqlite_open(self):
        def sidecar(root,db,owner,receipt):(root/(db.name+'-journal')).write_bytes(b'hot')
        with self.assertRaises(RuntimeError):self.full_file_audit(sidecar)
        def wal(root,db,owner,receipt):
            with db.open('r+b') as f:f.seek(18);f.write(b'\x02\x02')
        with self.assertRaises(RuntimeError):self.full_file_audit(wal)

    def test_ticket_identity_or_marker_replacement_refused(self):
        def wrong(root,db,owner,receipt):
            ticket=json.loads(receipt['ownedTicket']);ticket['database'][1]+=1;receipt['ownedTicket']=json.dumps(ticket)
        with self.assertRaises(RuntimeError):self.full_file_audit(wrong)
        def marker(root,db,owner,receipt):owner.write_text(F['uid'](900),encoding='ascii')
        with self.assertRaises(RuntimeError):self.full_file_audit(marker)


if __name__=='__main__':unittest.main()
