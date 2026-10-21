"""Independent deletion recovery receipts and concrete contention regressions."""
import copy
import runpy
import unittest
from pathlib import Path

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete_storage.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_delete.py')))
DIRECTORY=B['DIRECTORY']


def sample(narrow=False):
    fixture,base=B['sample'](narrow=narrow)
    rows=[]
    mapping=(0,1,1,1,3,3,3,4,5,6)
    initial=base['stages'][0]['model']['comments']
    readset=initial['ledger']['requests'][:4]
    pending=copy.deepcopy(base['stages'][3]['deleteInventory'])
    ack=[]
    for seq in range(1,5):
        a=dict(base['readiness'],sequence=seq,action='hold' if seq%2 else 'release',ok=True,
            account=initial['account'],inventories=[copy.deepcopy(base['stages'][0]['journal'])],
            editInventories=[],editHistory=[],deleteInventories=[copy.deepcopy(pending)] if seq>=3 else [],
            deleteHistory=[copy.deepcopy(pending['attempt'])] if seq>=3 else [],contentSha256=('a' if seq<3 else 'b')*64)
        if seq%2==0:a['unchanged']=True
        ack.append(a)
    for i,(name,old) in enumerate(zip(V['STAGES'],mapping)):
        r=copy.deepcopy(base['stages'][old]);r['stage']=name;r['capture']=str(DIRECTORY/f'{fixture}-{name}.png')
        m=r['model']['comments'];n=V['READS'][i]
        m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,replies=n//4,cancelled=0,held=False)
        f=r['model']['fixture'];count=int(i>=4)
        f['policy']=copy.deepcopy(V['D']['policy_cycle']()*V['POLICIES'][i]);f['held']=i==4
        f['releases']=[copy.deepcopy(V['O']['RELEASE'])] if i>=5 else []
        if i in V['ACTIVE']:
            a=r['model']['delete']['active'];a.update(busy=i==4,phase='Pending' if i in (5,6) else None,
                localPending=i in (5,6),canRetryLocal=i in (5,6),canConfirm=i in (1,2,3),canCheck=False)
            p=r['geometry']['panel'];p['busy']=str(i==4).lower()
            controls=r['geometry']['controls']
            if i in (5,6):controls.insert(0,dict(copy.deepcopy(controls[0]),id=V['PREFIX']+'retry'))
            for c in controls:c['disabled']=i==4 or (i in (5,6) and c['id']==V['PREFIX']+'confirm')
        wanted=V['NOTICES'].get(i)
        r['geometry']['notice']=dict(text=wanted,role='status',visible=True,hit=True,rect=B['B']['B']['rect']()) if wanted else None
        p=r['proof'];length=V['COMMANDS'][i];p['storageControls']=copy.deepcopy(ack[:length])
        hold=1 if length<=2 else 3
        p['storage']=dict(hold=copy.deepcopy(ack[hold-1]),release=copy.deepcopy(ack[hold]) if length%2==0 else None) if length else None
        if i>=4:p['blockedConfirm']=copy.deepcopy(V['D']['BLOCKED'])
        if i>=5:p['release']=copy.deepcopy(V['O']['RELEASE'])
        if i in V['LOCKED']:r['journal']=None;r['deleteInventory']=None
        elif i in (4,6):r['deleteInventory']=copy.deepcopy(pending)
        rows.append(r)
    base.update(case='own-delete-storage',stages=rows,final=copy.deepcopy(rows[-1]['model']))
    return fixture,base


class OwnDeleteStorageTests(unittest.TestCase):
    def check(self,f,r,**kwargs):return V['validate'](f,r,DIRECTORY,png=False,**kwargs)
    def reject(self,change,i=5):
        f,r=sample();change(r['stages'][i])
        with self.assertRaises((ValueError,AssertionError,RuntimeError)):self.check(f,r)
    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_shared_runner_exact_case(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        stages,validate=runner['case_contract']('own-delete-storage')
        self.assertEqual(stages,V['STAGES']);f,r=sample();validate(f,r,DIRECTORY,png=False)
    def test_failed_begin_has_no_dispatch_or_record(self):
        self.reject(lambda r:r['model']['fixture']['deletes'].append(V['D']['wire']()),2)
        self.reject(lambda r:r['deleteInventory'].update(revision=1),3)
    def test_lock_identity_and_immutable_release(self):
        self.reject(lambda r:r['proof']['storageControls'][0].update(pid=999),2)
        self.reject(lambda r:r['proof']['storage']['release'].update(unchanged=False),3)
    def test_locked_stage_cannot_claim_independent_read(self):
        self.reject(lambda r:r.update(journal={}))
        self.reject(lambda r:r.update(deleteInventory={}))
    def test_pending_disk_cannot_claim_early_ack(self):
        self.reject(lambda r:r['deleteInventory']['attempt'].update(phase='Confirmed',raw=V['D']['ACK']),6)
        self.reject(lambda r:r['model']['comments']['comments'].update(deleted=[V['TS']]))
    def test_no_duplicate_or_foreign_delete(self):
        self.reject(lambda r:r['model']['fixture']['deletes'].append(V['D']['wire']()),6)
        self.reject(lambda r:r['model']['fixture']['deletes'][0]['args'].update(ts='1.000001'))
    def test_local_retry_only_original_ack(self):
        self.reject(lambda r:r['model']['delete']['active'].update(canConfirm=True))
        self.reject(lambda r:r['model']['delete']['active'].update(canRetryLocal=False))
        self.reject(lambda r:r['model']['fixture']['responses'][0]['raw'].update(channel='COTHER'))
    def test_visible_working_guidance_replaces_stale_failure(self):
        self.reject(lambda r:r['geometry']['notice'].update(text=V['BEGIN']),4)
        self.reject(lambda r:r['geometry']['controls'][-1].update(disabled=False),4)
    def test_reply_same_node_content_and_sqlite_preserved(self):
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=False),6)
        self.reject(lambda r:r['journal'].update(revision=999),6)
        self.reject(lambda r:r['proof']['storage']['hold']['inventories'][0].update(blocks=[]))
    def test_confirmed_retains_uuid_raw_and_removes_only_selected_row(self):
        self.reject(lambda r:r['deleteInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000099'),7)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(raw=None),7)
        self.reject(lambda r:r['model']['comments']['comments']['pageMessages'].append(V['O']['received']()),7)
        self.reject(lambda r:r['model']['comments']['comments']['page']['messages'].pop(),7)
    def test_canonical_read_and_actual_forget_reopen(self):
        self.reject(lambda r:r['model']['comments']['comments']['page'].pop('deleted'),8)
        self.reject(lambda r:r['proof'].update(runtimeForgotten=False),9)
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=True),9)
    def test_physical_png_dimensions_are_not_css_viewport(self):
        f,r=sample();old=V['H']['png_signature'];calls=[]
        V['H']['png_signature']=lambda p,d:calls.append(d)
        try:V['validate'](f,r,DIRECTORY,dimensions=(1942,1256))
        finally:V['H']['png_signature']=old
        self.assertEqual(calls,[(1942,1256)]*10)
    def test_closed_database_exact_history_and_no_edits(self):
        f,r=sample();last=r['stages'][-1]
        value=dict(account=last['model']['comments']['account'],inventories=[last['journal']],editInventories=[],editHistory=[],
            deleteInventories=[last['deleteInventory']],deleteHistory=[last['deleteInventory']['attempt']],contentSha256='a'*64)
        V['validate_closed'](value,r)
        for field in ('editHistory','deleteHistory','inventories'):
            changed=copy.deepcopy(value);changed[field].append({})
            with self.assertRaises(RuntimeError):V['validate_closed'](changed,r)


if __name__=='__main__':unittest.main()
