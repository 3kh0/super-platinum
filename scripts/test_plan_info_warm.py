"""Portable private-cache oracle tests; constructed data is not native acceptance."""
import copy,json,pathlib,sqlite3,tempfile,unittest
import plan_info_warm as O
from contextlib import closing

SCOPE=dict(account='55766095-90cd-4da2-b439-49474e9cb530',team='TWORKFLOW',user='UWORKFLOW',url='https://example.test',enterprise=None)
RECORD=dict(version=1,source='accepted-plan-info',raw={'is_plan_frozen':True,'reconnect_grace_period_start_ts':42},**SCOPE)

class CacheEvidence(unittest.TestCase):
    def test_exact_scope(self):
        self.assertEqual(O.envelope(copy.deepcopy(RECORD),SCOPE),RECORD)
        for key,value in [('account','foreign'),('team','TOTHER'),('user','UOTHER'),('url','https://other.test'),('enterprise','EOTHER'),('version',True),('source','warm-value'),('raw',None)]:
            with self.subTest(key=key):
                bad=copy.deepcopy(RECORD);bad[key]=value
                with self.assertRaises(RuntimeError):O.envelope(bad,SCOPE)
        bad=dict(RECORD,token='unwanted')
        with self.assertRaises(RuntimeError):O.envelope(bad,SCOPE)

    def database(self,directory,record=RECORD,team=None):
        p=pathlib.Path(directory)/O.DB_NAME
        with closing(sqlite3.connect(p)) as db:
            db.execute('CREATE TABLE workspaces(team_id TEXT,url TEXT,self_user_id TEXT,sidebar TEXT)')
            for name in ('meta','channels','usergroups','users','messages','channel_state'):
                db.execute('CREATE TABLE '+name+'(id TEXT)')
            db.execute('INSERT INTO workspaces VALUES(?,?,?,?)',(team or SCOPE['team'],O.WORKSPACE_URL,SCOPE['user'],json.dumps({'plan_info':record})))
            db.commit()
        return p

    def test_closed_sqlite_read_is_exact_and_readonly(self):
        with tempfile.TemporaryDirectory() as d:
            p=self.database(d);before=p.read_bytes();result=O.read_database(p,SCOPE,RECORD)
            self.assertEqual(p.read_bytes(),before);self.assertTrue(result['readOnly']);self.assertEqual(result['planInfo'],RECORD)

    def test_unrelated_workspace_and_missing_original_refused(self):
        for record,team in [(None,None),(RECORD,'TOTHER'),(dict(RECORD,raw={}),None)]:
            with self.subTest(team=team,record=record),tempfile.TemporaryDirectory() as d:
                p=self.database(d,record,team)
                with self.assertRaises(RuntimeError):O.read_database(p,SCOPE,RECORD)

    def test_foreign_table_and_corruption_refused(self):
        with tempfile.TemporaryDirectory() as d:
            p=self.database(d)
            with closing(sqlite3.connect(p)) as db:db.execute('CREATE TABLE foreign_data(id TEXT)');db.commit()
            with self.assertRaises(RuntimeError):O.read_database(p,SCOPE,RECORD)
        with tempfile.TemporaryDirectory() as d:
            p=pathlib.Path(d)/O.DB_NAME;p.write_bytes(b'not a database')
            with self.assertRaises(sqlite3.DatabaseError):O.read_database(p,SCOPE,RECORD)



import runpy
from unittest.mock import patch
import test_workflow_activity_native as Base
from activity_grace_clock import LabelSpec
from workflow_activity_shared_frozen import GRACE


def sample(narrow=False):
    fixture,receipt=Base.channel_sample(narrow,spec=GRACE);base=copy.deepcopy(receipt['stages'][:3]);rows=[]
    seed=base[0]['proof']['channel']['seed'];saved=dict(version=1,source='accepted-plan-info',raw=seed['extras']['plan_info'],**O.SCOPE)
    reload=dict(path=str(Base.DIRECTORY/O.DB_NAME),saved=saved,restored=copy.deepcopy(saved),saveClosedBeforeLoad=True,startupLoad=True,liveRawRestored=False,previousEpoch=0,currentEpoch=1,oldLiveCurrent=False,oldSourceCurrent=False,foreignAccountMatches=False,boot=seed['boot'],init=seed['init'],extras={'ok':True},defaultCacheInstalled=False)
    actions=[dict(kind='empty-positive',proof=dict(raw=dict(ok=True,plan_info={}),originalCurrent=True,accepted=True,error=None)),dict(kind='null-positive',proof=dict(raw=dict(ok=True,plan_info={'reconnect_grace_period_start_ts':None}),originalCurrent=True,accepted=True,error=None)),dict(kind='foreign-refused',proof=dict(raw=dict(id='TFOREIGN',is_plan_frozen=False,reconnect_grace_period_start_ts=99),ignored=True)),dict(kind='late-refused',proof=dict(raw=dict(ok=True,plan_info=dict(is_plan_frozen=False,reconnect_grace_period_start_ts=1,late=True)),originalCurrent=False,accepted=False,error='Slack API returned error: channel_initialization_unavailable'))]
    for i,name in enumerate(O.STAGES):
        row=copy.deepcopy(base[min(i,2)]);row['stage']=name;m=row['model'];ch=row['proof']['channel'];m.update(account=O.ACCOUNT,epoch=1)
        m['metadata'].update(reads=0 if i==0 else 12 if i==1 else 16,title=Base.R['TITLE'],description=Base.R['DESCRIPTION'],dirty=i>0)
        if i>=2:ch['documents'][0]['revision']+=1
        m['activity']=dict(active=2<=i<7,detail={'workflow_event':{'workflow_execution_id':'E_REVIEW'}} if i>=2 else None,documentRevision=ch['documents'][0]['revision'] if i else None)
        m['export']=dict(attempts=[],retained=0,current=dict(shown=False,disabled=i>=7,notice=None))
        if i>=2:
            m['ledger']['requests'].append(dict(serial=2,method='workflows.activity.get',args=dict(workflow_id='WF_REVIEW',workflow_execution_id='E_REVIEW',_x_reason='Load instance steps for activity tab')));m['ledger']['finished']=2
        raw=copy.deepcopy(saved['raw'])
        if i>=4:raw['reconnect_grace_period_start_ts']=None
        provenance='retained-cache' if i<3 else 'live'
        value=dict(reload=copy.deepcopy(reload),actions=copy.deepcopy(actions[:max(0,min(i-2,4))]),oldDisplayCurrent=i<3,oldSourceCurrent=False if i<6 else None,account=O.ACCOUNT,epoch=1,live=None if i<3 else dict(current=True,raw=raw),display=dict(current=True,provenance=provenance,raw=raw),retained=dict(saved,raw=raw))
        row['proof']['planInfoWarm']=value
        if 2<=i<7:
            ch['labels'].update(plan=raw,planProvenance=provenance)
            ch['labels']['rows'].sort(key=lambda r:r['id'])
            for label in ch['labels']['rows']:
                label['raw']['is_member']=label['id']!='CGRARCH01'
                if i>=4:label['policy'].update(graceEndMs=None,inGrace=False,archiveIcon=True,archivedSuffix=label['id'] in GRACE.archived_ids)
            spec=LabelSpec(i<4);native=Base.channel_labels_sample(spec=spec)
            native['identity']=dict(ok=True,sameRoot=True,sameDetail=True,rows=[dict(id=node,channel=id,sameButton=True,sameName=True,connected=True,text=text) for node,id,text,_ in spec.vectors()])
            if i==2:row['proof']['warmNative']=native
            else:row['proof']['exercise']={'native':native,'projection':copy.deepcopy(ch['labels'])}
        if i in (1,7):row['geometry']['fields']=dict(title=Base.R['TITLE'],description=Base.R['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False)
        if i==7:row['proof']['exercise']={'native':dict(ok=True,originalRootConnected=True,originalDetailConnected=False,title=Base.R['TITLE'],description=Base.R['DESCRIPTION'])}
        rows.append(row)
    receipt.update(stages=rows,captures={name:str(Base.DIRECTORY/f'{fixture}-{name}.png') for name in O.STAGES},final=copy.deepcopy(rows[-1]['model']),planInfoWarm=copy.deepcopy(value))
    return fixture,receipt


class ReceiptEvidence(unittest.TestCase):
    def validate(self,f,v):return O.validate(f,v,Base.DIRECTORY,Base.R,platform='win32',png=False)
    def reject(self,i,change):
        f,v=sample();change(v['stages'][i])
        with self.assertRaises((RuntimeError,TypeError,KeyError)):self.validate(f,v)

    def test_original_two_width_vectors(self):
        for narrow in (False,True):
            f,v=sample(narrow);self.validate(f,v)

    def test_cache_never_promotes_live_source(self):
        self.reject(2,lambda r:r['proof']['planInfoWarm'].update(live=dict(current=True,raw={})))

    def test_empty_equal_raw_retires_cached_witness(self):
        self.reject(3,lambda r:r['proof']['planInfoWarm'].update(oldDisplayCurrent=True))

    def test_null_replacement_is_not_empty_or_absent(self):
        self.reject(4,lambda r:r['proof']['planInfoWarm']['display']['raw'].update(reconnect_grace_period_start_ts=42))

    def test_old_owner_refusal_is_retained(self):
        self.reject(6,lambda r:r['proof']['planInfoWarm']['actions'][-1]['proof'].update(accepted=True))

    def test_foreign_scope_cannot_mutate(self):
        self.reject(5,lambda r:r['proof']['planInfoWarm']['actions'][-1]['proof'].update(ignored=False))

    def test_native_node_replacement_refused(self):
        self.reject(4,lambda r:r['proof']['exercise']['native']['identity'].update(sameRoot=False))

    def test_settled_projection_requires_current_live_original_source(self):
        for key,value in [('current',False),('planProvenance','retained-cache'),('plan',{})]:
            with self.subTest(key=key):
                self.reject(3,lambda r:r['proof']['exercise']['projection'].update({key:value}))
        self.reject(3,lambda r:r['proof']['exercise'].pop('projection'))
        self.reject(4,lambda r:r['proof']['exercise']['projection']['rows'][0]['policy'].update(graceEndMs=42))
        self.reject(3,lambda r:r['proof']['exercise']['projection']['rows'][0]['raw'].update(name='foreign'))

    def test_settled_projection_validates_capture_time_without_exact_timestamp_equality(self):
        f,v=sample()
        for row in v['stages'][3:7]:
            for policy in row['proof']['exercise']['projection']['rows']:
                policy['policy']['capturedNowMs']+=1000
        self.validate(f,v)
        self.reject(3,lambda r:r['proof']['exercise']['projection']['rows'][0]['policy'].update(capturedNowMs=0))

    def test_sequential_identity_text_drift_is_still_refused(self):
        self.reject(3,lambda r:r['proof']['exercise']['native']['identity']['rows'][0].update(text='#activity-known'))

    def test_dirty_draft_and_no_extra_reads(self):
        self.reject(4,lambda r:r['model']['metadata'].update(reads=20))
        self.reject(7,lambda r:r['geometry']['fields'].update(description='lost'))

    def test_real_cache_lifecycle_and_missing_extras_required(self):
        self.reject(0,lambda r:r['proof']['planInfoWarm']['reload'].update(saveClosedBeforeLoad=False))
        self.reject(0,lambda r:r['proof']['planInfoWarm']['reload'].update(extras={'ok':True,'plan_info':{}}))

    def test_label_source_cannot_claim_live_while_cached(self):
        self.reject(2,lambda r:r['proof']['channel']['labels'].update(planProvenance='live'))

    def test_null_action_cannot_retain_grace_icon(self):
        self.reject(4,lambda r:r['proof']['exercise']['native']['rows'][0].update(iconKind='Tag'))

    def test_late_receipt_cannot_restore_old_epoch_or_envelope(self):
        self.reject(6,lambda r:r['proof']['planInfoWarm'].update(epoch=0))
        self.reject(6,lambda r:r['proof']['planInfoWarm']['retained']['raw'].update(late=True))

    def test_same_original_reload_and_raw_history(self):
        self.reject(4,lambda r:r['proof']['planInfoWarm']['reload']['saved']['raw'].update(opaque='changed'))

    def test_exact_wrapper_and_private_database(self):
        wrapper=runpy.run_path(str(pathlib.Path(__file__).with_name('check-plan-info-warm-native.py')))
        with tempfile.TemporaryDirectory() as d,patch('native_fixture.NativeFixture') as native:
            directory=pathlib.Path(d).resolve();module=wrapper['harness']();checks={module.FLAG:'1','SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE':O.CASE,'SUPER_PLATINUM_UI_CAPTURE_DIR':str(directory)}
            module.NativeFixture(O.FIXTURES[0],directory/'native.log','binary',checks)
            self.assertEqual(native.call_args.args[3][O.DB_FLAG],str(directory/O.DB_NAME))
            self.assertEqual(native.call_args.args[3][O.FLAG],'1')
            for extra in ({O.FLAG:'1'},{O.DB_FLAG:'foreign'},{'SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE':'browse'}):
                with self.assertRaises(RuntimeError):module.NativeFixture(O.FIXTURES[0],directory/'native.log','binary',dict(checks,**extra))
            (directory/O.DB_NAME).write_bytes(b'existing')
            with self.assertRaises(RuntimeError):module.NativeFixture(O.FIXTURES[0],directory/'native.log','binary',checks)

if __name__=='__main__':unittest.main()
