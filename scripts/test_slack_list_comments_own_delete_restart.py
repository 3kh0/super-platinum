"""Deletion restart preserves original inert records and exact closed DB bytes."""
import copy
from contextlib import closing
import json
from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest
from unittest import mock

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete_restart.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_delete.py')))
T=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit_restart.py')))
D,O=V['D'],V['O']
DIRECTORY=B['DIRECTORY']


def native_pair(mode='pending',narrow=False):
    fixture,base=B['sample'](narrow=narrow)
    _,failure=B['sample']('own-delete-failure',narrow)
    initial=base['stages'][0]['model']['comments'];scope=base['stages'][0]['journal']['scope']
    pending=copy.deepcopy(base['stages'][3]['deleteInventory'])
    pending['attempt']['phase']='Pending' if mode=='pending' else 'Uncertain'
    if mode=='unknown':pending['revision']=2
    empty=dict(scope=scope,ts=V['TS'],revision=0,attempt=None,retry_at=0)
    out=[]
    for restore in (False,True):
        case=f'own-delete-restart-{mode}-'+('restore' if restore else 'exercise')
        mapping=[(base,0),(failure,2),(failure,3),(base,2),(failure,2),(base,4),(base,6)] if restore else [(base,0),(base,1),(base,3) if mode=='pending' else (failure,2)]
        rows=[];ready=copy.deepcopy(base['readiness'])
        if restore:ready.update(pid=ready['pid']+1,nonce='c'*32)
        for i,(source,index) in enumerate(mapping):
            r=copy.deepcopy(source['stages'][index]);name=V['STAGES'][case][i];r.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'))
            m,f,delete=r['model']['comments'],r['model']['fixture'],r['model']['delete']
            reads=(4,4,8,8,8,12,16)[i] if restore else (4,8,12)[i]
            readset=initial['ledger']['requests'][:4]
            m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(reads//4))],completed=reads,replies=reads//4,cancelled=0,held=False)
            count=int(not restore and i==2);published=restore and i>=5
            f.update(policy=[] if restore else copy.deepcopy(D['policy_cycle']()*i),prepared=[D['wire']()]*count,deletes=[D['wire']()]*count,
                records=[D['source'](initial)]*count,responses=[dict(disposition='accepted' if mode=='pending' else 'unconfirmed',raw=copy.deepcopy(D['ACK']) if mode=='pending' else None,retryAfter=None)] if count else [],
                releases=[],held=bool(count and mode=='pending'),published=published,deleted=[V['TS']] if published else [],futureDeleted=V['TS'] if count or published else None)
            delete['retainedCount']=int(i>0)
            if delete['active']:
                a=delete['active'];a.update(busy=bool(count and mode=='pending'),phase=pending['attempt']['phase'] if restore or (i==2 and mode=='unknown') else None,
                    canConfirm=not restore and i==1,canCheck=restore or (i==2 and mode=='unknown'),canRetryLocal=False,localPending=False)
                r['geometry']['panel']['session']=str(a['id'])
            inv=copy.deepcopy(pending if restore or i==2 else empty)
            if published:
                inv['revision']=pending['revision']+1;inv['attempt'].update(phase='Observed',readback=dict(target=D['source'](initial)['target'],raw=D['canonical'](initial)))
            r['deleteInventory']=inv
            p=r['proof'];p.pop('release',None);p.pop('blockedConfirm',None)
            p.update(initialDeleteInventory=copy.deepcopy(pending if restore else empty),initialReplyInventory=copy.deepcopy(r['journal']) if restore else dict(scope=scope,revision=0,capture='',blocks=[],attempt=None),
                processId=ready['pid'],scenario=mode,phase='restore' if restore else 'exercise')
            if restore:p['replySeed']=None
            if (restore and i>=1) or (not restore and i==2):p['blockedConfirm']=copy.deepcopy(D['BLOCKED'])
            if restore and i>=4:p['reopenedBlockedConfirm']=copy.deepcopy(D['BLOCKED'])
            if published:p['publication']=dict(attemptId=pending['attempt']['id'],record=copy.deepcopy(pending['attempt']['record']),originalReceipt=None,deletes=0,fromPersistedRecord=True,deleted=V['TS'])
            rows.append(r)
        out.append(dict(ok=True,case=case,fixture=fixture,readiness=ready,stages=rows,final=copy.deepcopy(rows[-1]['model']),deleteInventory=copy.deepcopy(rows[-1]['deleteInventory']),initialRecords=base['initialRecords'],noLiveNetwork=True))
    return fixture,*out


def sql_proof(row,name='source'):
    inv=row['deleteInventory'];scope=row['journal']['scope']
    values=dict(drafts=[row['journal']],history=[],edit_drafts=[],edit_history=[],delete_state=[inv],delete_history=[inv['attempt']])
    tables=dict(metadata=[dict(account=row['model']['comments']['account'])])
    for table,items in values.items():
        tables[table]=[]
        for item in items:
            stored=dict(owner=json.dumps(scope if table=='drafts' else [scope,V['TS']]),body=json.dumps(item))
            if table=='delete_history':stored['id']=item['id']
            tables[table].append(stored)
    return dict(path='/private/'+name+'.sqlite',sha256='a'*64,account=row['model']['comments']['account'],tables=tables,inventories=values)


class OracleTests(unittest.TestCase):
    def check(self,f,r):V['validator'](r['case'])(f,r,DIRECTORY,png=False)
    def reject(self,mutate,index=1,restore=True,mode='pending'):
        f,a,b=native_pair(mode);r=b if restore else a;mutate(r['stages'][index])
        with self.assertRaises((RuntimeError,ValueError,AssertionError)):self.check(f,r)
    def test_all_four_cases_both_widths_and_sql_pair(self):
        for mode in ('pending','unknown'):
            for narrow in (False,True):
                f,a,b=native_pair(mode,narrow);self.check(f,a);self.check(f,b)
                old=sql_proof(a['stages'][-1]);copied=copy.deepcopy(old);copied['path']='/private/copy.sqlite'
                V['validate_pair'](mode,a,b,old,copied,sql_proof(b['stages'][-1],'final'))
    def test_unreleased_ack_does_not_confirm_pending_disk(self):
        self.reject(lambda r:r['deleteInventory']['attempt'].update(raw=D['ACK']),2,False)
        self.reject(lambda r:r['model']['fixture']['releases'].append(O['RELEASE']),2,False)
        self.reject(lambda r:r['model']['delete']['active'].update(busy=False),2,False)
    def test_unknown_is_not_acknowledged(self):
        self.reject(lambda r:r['model']['fixture']['responses'][0].update(raw=D['ACK']),2,False,'unknown')
        self.reject(lambda r:r['deleteInventory']['attempt'].update(phase='Confirmed'),2,False,'unknown')
    def test_restore_issues_no_policy_or_delete(self):
        for key in ('policy','prepared','deletes','records','responses','releases'):
            self.reject(lambda r,k=key:r['model']['fixture'][k].append({}))
    def test_restore_same_inert_inventory_and_no_replay_controls(self):
        self.reject(lambda r:r['deleteInventory'].update(revision=999))
        self.reject(lambda r:r['model']['delete']['active'].update(canConfirm=True))
        self.reject(lambda r:r['geometry']['controls'][-1].update(disabled=False))
    def test_negative_read_and_cancel_preserve_pending_and_body(self):
        self.reject(lambda r:r['deleteInventory']['attempt'].update(phase='Observed'),2)
        self.reject(lambda r:r['model']['comments']['comments']['pageMessages'].pop(),2)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000099'),3)
    def test_reopened_modal_still_locked_and_same_mount(self):
        self.reject(lambda r:r['proof'].pop('reopenedBlockedConfirm'),4)
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=False),4)
    def test_positive_marker_must_be_original_scope_not_partial_absence(self):
        self.reject(lambda r:r['deleteInventory']['attempt']['readback']['raw'].pop('deleted'),5)
        self.reject(lambda r:r['deleteInventory']['attempt']['readback']['raw']['messages'].append(O['received']()),5)
        self.reject(lambda r:r['proof']['publication'].update(deletes=1),5)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(raw=D['ACK']),5)
    def test_reply_styled_opaque_native_and_sqlite_unchanged(self):
        self.reject(lambda r:r['journal'].update(blocks=[]),2)
        self.reject(lambda r:r['geometry']['reply'].update(inert=False),2)
        self.reject(lambda r:r['geometry']['text'].update(ok=False,visibleLines=0),3)
    def test_final_canonical_reopen_requires_runtime_forget(self):
        self.reject(lambda r:r['proof'].update(runtimeForgotten=False),6)
        self.reject(lambda r:r['model']['comments']['comments']['page'].pop('deleted'),6)
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=True),6)
    def test_sql_foreign_authority_and_owner_refused(self):
        _,a,b=native_pair();row=a['stages'][-1]
        for mutate in (lambda s:s['inventories']['edit_history'].append({}),lambda s:s['tables']['delete_history'][0].update(owner='{}'),lambda s:s['tables']['drafts'][0].update(body='{}')):
            value=sql_proof(row);mutate(value)
            with self.assertRaises(RuntimeError):V['sql_custody'](value,row)
    def test_fresh_pair_identity_and_exact_database_hash(self):
        _,a,b=native_pair();old=sql_proof(a['stages'][-1]);copied=copy.deepcopy(old);copied['path']='/private/copied';final=sql_proof(b['stages'][-1])
        copied['sha256']='b'*64
        with self.assertRaises(RuntimeError):V['validate_pair']('pending',a,b,old,copied,final)
        copied['sha256']=old['sha256'];b['readiness']['pid']=a['readiness']['pid']
        with self.assertRaises(RuntimeError):V['validate_pair']('pending',a,b,old,copied,final)
    def test_physical_png_not_css(self):
        f,a,_=native_pair();old=V['H']['png_signature'];calls=[];V['H']['png_signature']=lambda p,d:calls.append(d)
        try:V['validator'](a['case'])(f,a,DIRECTORY,dimensions=(1942,1256))
        finally:V['H']['png_signature']=old
        self.assertEqual(calls,[(1942,1256)]*3)


class DeleteTransferTests(unittest.TestCase):
    write_wrapper=T['TransferTests'].write_wrapper
    transfer=T['TransferTests'].transfer
    rejected=T['TransferTests'].rejected
    def setUp(self):
        T['TransferTests'].setUp(self)
        f,a,_=native_pair();self.case='own-delete-restart-pending-restore';self.exercise=a['case']
        ready=self.value['readiness'];a['readiness']=ready
        for row in a['stages']:
            row['proof']['processId']=ready['pid'];path=self.directory/f'{f}-{row["stage"]}.png';path.write_bytes(T['png']());row['capture']=str(path)
        self.value.update(case=self.exercise,native=a);self.write_wrapper()
        lines=['slack-list-comments-compose-waiting: '+json.dumps(ready)]
        lines+=['slack-list-comments-compose-stage: '+json.dumps(dict(stage=row['stage'],edge=edge)) for row in a['stages'] for edge in ('before','after')]
        lines+=['slack-list-comments-compose-check: '+json.dumps(a)];self.log.write_text('\n'.join(lines)+'\n')
        proof=sql_proof(a['stages'][-1])
        with closing(sqlite3.connect(self.source)) as db:
            for table in T['S']['TABLES']:db.execute(f'DELETE FROM {table}')
            for name,columns in T['S']['DELETE_TABLES'].items():db.execute(f'CREATE TABLE {name}({",".join(c+" TEXT NOT NULL" for c in columns)})')
            for table,rows in proof['tables'].items():
                for row in rows:db.execute(f'INSERT INTO {table}({",".join(row)}) VALUES({",".join("?" for _ in row)})',tuple(row.values()))
            db.commit()
    def test_exact_seven_table_copy_without_edit_authority(self):
        before=self.source.read_bytes();value=self.transfer();self.assertEqual(before,self.source.read_bytes());self.assertEqual(before,self.destination.read_bytes())
        self.assertEqual(value['source']['inventories']['delete_state'][0]['attempt']['phase'],'Pending')
    def test_edit_authority_refused_before_copy(self):
        with closing(sqlite3.connect(self.source)) as db:db.execute('INSERT INTO edit_drafts VALUES(?,?)',('foreign','{}'));db.commit()
        self.rejected();self.assertFalse(self.destination.exists())
    def test_wrong_original_uuid_or_body_refused_before_copy(self):
        with closing(sqlite3.connect(self.source)) as db:
            row=json.loads(db.execute('SELECT body FROM delete_history').fetchone()[0]);row['raw']=D['ACK'];db.execute('UPDATE delete_history SET body=?',(json.dumps(row),));db.commit()
        self.rejected();self.assertFalse(self.destination.exists())
    def test_wrong_operation_and_unclosed_process_refused(self):
        self.value['processExited']=False;self.write_wrapper();self.rejected()
    def test_nonempty_sidecar_refused(self):
        Path(str(self.source)+'-wal').write_bytes(b'pending');self.rejected()


class RunnerTests(unittest.TestCase):
    def test_routes_only_exact_four_cases(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        for case,stages in V['STAGES'].items():self.assertEqual(runner['case_contract'](case)[0],stages)
        with self.assertRaises(RuntimeError):runner['case_contract']('own-delete-restart-unsent-restore')
    def test_delete_pair_cannot_start_second_before_first_exits(self):
        pair=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-own-edit-restart-native.py')));fn=pair['run_pair'];calls=[]
        def first(*args,**kwargs):calls.append(args[3]);return dict(ok=True,processExited=False,closedLogRevalidated=True,cleanupErrors=[])
        with tempfile.TemporaryDirectory() as name:
            with mock.patch.dict(fn.__globals__,build_proof=lambda *a:{}),mock.patch.dict(fn.__globals__['C'],run_fixture=first),mock.patch('builtins.print'):
                with self.assertRaisesRegex(RuntimeError,'First process must close'):fn('pending','slack-list',Path(name)/'unused.exe',Path(name)/'captures',operation='delete')
        self.assertEqual(calls,['own-delete-restart-pending-exercise'])
    def test_invalid_mode_operation_never_creates_capture_directory(self):
        pair=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-own-edit-restart-native.py')))
        with tempfile.TemporaryDirectory() as name:
            for operation,mode in (('delete','unsent'),('edit','pending'),('other','unknown')):
                target=Path(name)/'captures'
                with self.assertRaises(RuntimeError):pair['run_pair'](mode,'slack-list',Path(name)/'unused',target,operation=operation)
                self.assertFalse(target.exists())


if __name__=='__main__':unittest.main()
