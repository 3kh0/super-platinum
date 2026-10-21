import copy
import unittest
import workflow_management_channel_workflows as W

def sample(case,stage):
    req=[];res=[]
    for n,(method,channel) in enumerate(W.schedule(case,stage),1):
        req.append(dict(serial=n,method=method,fields=list(map(list,W.fields(method,channel).items())),retrySafe=method=='bookmarks.list',owner=dict(W.A.OWNER,channel=channel,channelGeneration=8,tab='Workflows',socket=0)))
        raw=W.previews() if method=='workflows.triggers.preview' else W.bookmarks(channel)
        res.append(dict(serial=n,result=dict(error='offline') if case=='first-tab-failure' and n==2 else dict(raw=raw)))
        if case=='first-tab-rate-limit' and n==2:res[-1]['result']=dict(error='rate-limited',retryAfterSeconds=12)
    retired=case=='first-tab-retirement' and stage in ('switched','late-settled','returned')
    held=case=='first-tab-retirement' and stage in ('workflows-held','switched')
    settlements=[dict(serial=n,returned=not(retired and n==2)) for n in range(1,len(req)+1) if not(case=='first-tab-retirement' and stage=='workflows-held' and n==2)]
    return dict(requests=req,responses=res,settlements=settlements,releases=[dict(serial=2,receiverRetired=True,delivered=False)] if case=='first-tab-retirement' and stage in ('late-settled','returned') else [],held=[dict(serial=2,receiverRetired=retired)] if held else [],about=[{}]*4 if stage=='returned' else [],networkCalls=0)

class LedgerTests(unittest.TestCase):
    def test_all_stage_schedules(self):
        for case,stages in W.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):W.ledger(case,stage,sample(case,stage))
    def reject(self,change,case='first-tab-success',stage='refreshed'):
        v=sample(case,stage);change(v)
        with self.assertRaises((RuntimeError,AssertionError)):W.ledger(case,stage,v)
    def test_wrong_reason(self):self.reject(lambda v:v['requests'][0]['fields'].__setitem__(-1,['_x_reason','bookmarks-list-api']))
    def test_wrong_trigger_order(self):self.reject(lambda v:v['requests'][1]['fields'].__setitem__(0,['trigger_ids',','.join(reversed(W.IDS))]))
    def test_duplicate_wire_field(self):self.reject(lambda v:v['requests'][0]['fields'].append(v['requests'][0]['fields'][0]))
    def test_live_network(self):self.reject(lambda v:v.update(networkCalls=1))
    def test_foreign_owner(self):self.reject(lambda v:v['requests'][0]['owner'].update(channel='OTHER'))
    def test_mutated_original_bookmark(self):self.reject(lambda v:v['responses'][0]['result']['raw']['bookmarks'][0].update(title='changed'))
    def test_duplicate_response(self):self.reject(lambda v:v['responses'][1].update(serial=1))
    def test_extra_read(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][-1])))
    def test_preview_not_transport_retry_safe(self):self.reject(lambda v:v['requests'][1].update(retrySafe=True))
    def test_false_empty_success(self):self.reject(lambda v:v['responses'][1].update(result=dict(raw=dict(ok=True,triggers=[]))),'first-tab-failure','workflows-failed')
    def test_retired_receiver_not_dropped(self):self.reject(lambda v:v['held'][0].update(receiverRetired=False),'first-tab-retirement','switched')
    def test_late_delivery_forged(self):self.reject(lambda v:v['releases'][0].update(delivered=True),'first-tab-retirement','late-settled')
    def test_canceled_receipt_claims_return(self):self.reject(lambda v:v['settlements'][1].update(returned=True),'first-tab-retirement','switched')
    def test_rate_header_not_offline(self):self.reject(lambda v:v['responses'][1].update(result=dict(error='offline')),'first-tab-rate-limit','rate-limited')
    def test_rate_header_exact(self):self.reject(lambda v:v['responses'][1]['result'].update(retryAfterSeconds=0),'first-tab-rate-limit','expired')
    def test_cooldown_remount_extra_read(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])),'first-tab-rate-limit','reentered-blocked')
    def test_private_source_empty(self):
        self.assertEqual(W.bookmarks(W.A.PRIVATE)['bookmarks'],[])
        self.assertEqual(W.CHANNELS[W.A.PRIVATE]['properties']['channel_workflows'],[])
        self.assertEqual(W.CHANNELS[W.A.PRIVATE]['properties']['tabz'][0]['type'],'workflows')
    def test_real_metadata_and_collection_order(self):
        featured,bookmarked=W.rows()
        self.assertEqual([r['id'] for r in featured],['TR_REVIEW','FtDenied01'])
        self.assertEqual([r['id'] for r in bookmarked],['FtFolder01','TR_REVIEW','FtDenied01'])
        self.assertNotEqual(bookmarked[1]['title'],W.A.bookmark()['title'])
        self.assertEqual(featured[1]['title'],'Private featured \u4e2d')
        self.assertEqual(bookmarked[2]['title'],'Private workflow')
        self.assertEqual(bookmarked[0]['source']['parent_id'],'BkParent01')

def rate_native(stage):
    idle=stage in ('reentered-blocked','expired');blocked=stage in ('rate-limited','reentered-blocked')
    status='idle' if idle else 'failed' if blocked else 'ready'
    owner=None if idle else dict(id=12,account=W.F.OWNER['account'],accountEpoch=W.F.OWNER['epoch'],team='TWORKFLOW',channel=W.A.DEST,channelGeneration=8,socket=0)
    product=dict(current=True,status=status,phase=None,serial=0 if idle else 1,owner=owner,
      cooldown=dict(blocked=blocked,retryAfterSeconds=12 if blocked else None,indefinite=False),
      channelRaw=None if idle else W.channels('first-tab-rate-limit')[0],bookmarksRaw=None if idle else W.bookmarks(W.A.DEST),previewsRaw=W.previews() if status=='ready' else None,
      reads=[] if idle else [dict(serial=1,owner=12,method='bookmarks.list',raw=W.bookmarks(W.A.DEST)),dict(serial=1,owner=12,method='workflows.triggers.preview',**(dict(failure='rate-limited',retryAfterSeconds=12) if blocked else dict(raw=W.previews())))],
      featured=[],bookmarked=[])
    f,b=W.rows() if status=='ready' else ([],[]);product.update(featured=f,bookmarked=b)
    controls=[dict(id='channel-workflows-'+k,nodeId=i,disabled=blocked,visible=True,rect={}) for i,k in enumerate(['refresh']+(['retry'] if status!='ready' else []),1)]
    sections=[dict(kind=kind,role='list',rows=[dict(trigger=r['id'],status=r['kind'],title=r['title'],description=r['description'],visible=True,actions=1 if r['kind']=='available' else 0,opener=dict(id='channel-workflow-open-'+kind+'-'+str(rows.index(r)),disabled=False) if r['kind']=='available' else None,brokenImages=0,rect={}) for r in rows]) for kind,rows in [('featured',f),('bookmarked',b)]] if status=='ready' else [None,None]
    text='Slack asked us to wait before refreshing workflows.' if blocked else 'Retry to load workflows.' if idle else ''
    return dict(workflows=dict(panel=dict(connected=True),product=product,tab=dict(text='Workflows',pressed='true',visible=True,rect={}),controls=controls,sections=sections,feedback=[dict(text=text)] if text else [],empty=[]))

class CooldownTests(unittest.TestCase):
    def verify(self,stage,n):W.product_proof('first-tab-rate-limit',stage,n,dict(channel=W.A.DEST,tab='Workflows',channelGeneration=8),[1440,1000],lambda *_:None)
    def test_original_rate_remount_expiry_and_explicit_retry(self):
        for stage in ('rate-limited','reentered-blocked','expired','recovered'):self.verify(stage,rate_native(stage))
    def reject(self,stage,change):
        n=rate_native(stage);change(n['workflows'])
        with self.assertRaises((RuntimeError,AssertionError)):self.verify(stage,n)
    def test_remount_does_not_admit_owner(self):self.reject('reentered-blocked',lambda n:n['product'].update(owner=dict(id=1)))
    def test_remount_does_not_admit_serial(self):self.reject('reentered-blocked',lambda n:n['product'].update(serial=1))
    def test_expiry_does_not_send_read(self):self.reject('expired',lambda n:n['product']['reads'].append(dict(method='bookmarks.list')))
    def test_blocked_refresh_disabled(self):self.reject('rate-limited',lambda n:n['controls'][0].update(disabled=False))
    def test_blocked_retry_disabled(self):self.reject('reentered-blocked',lambda n:n['controls'][1].update(disabled=False))
    def test_expired_retry_enabled(self):self.reject('expired',lambda n:n['controls'][1].update(disabled=True))
    def test_actual_header_receipt_preserved(self):self.reject('rate-limited',lambda n:n['product']['reads'][1].update(retryAfterSeconds=None))
    def test_retry_fresh_complete_read_cycle(self):self.reject('recovered',lambda n:n['product']['reads'].pop(0))
    def test_rate_tab_config_only_new_case(self):
        self.assertNotIn('tabz',W.channels()[0]['properties'])
        self.assertEqual(W.channels('first-tab-rate-limit')[0]['properties']['tabz'],[dict(id='workflows',type='workflows',label='')])
    def test_disabled_clicks_are_same_original_nodes_and_quiet(self):
        o=dict(native=rate_native('rate-limited'),model={},firstPublish={},server={})
        controls=[{k:c[k] for k in ('id','nodeId','disabled')} for c in o['native']['workflows']['controls']]
        e=dict(blocked=dict(before=o,after=o,activation=dict(ok=True,controls=controls),quiet=dict(before=o,after=o,observedMs=500)))
        W.rate_exercise('rate-limited',e,o,{})
        e['blocked']['activation']['controls'][0]['nodeId']=999
        with self.assertRaises((RuntimeError,AssertionError)):W.rate_exercise('rate-limited',e,o,{})
    def test_expiry_only_changes_gate_not_acquisition_or_server(self):
        before=dict(native=rate_native('reentered-blocked'),model={},firstPublish={},server=dict(requests=[1,2]))
        after=copy.deepcopy(before);after['native']=rate_native('expired')
        e=dict(before=before,after=after,waitedMs=1000)
        W.rate_exercise('expired',e,after,before)
        after['server']['requests'].append(3)
        with self.assertRaises((RuntimeError,AssertionError)):W.rate_exercise('expired',e,after,before)
    def test_retry_requires_actual_expired_before(self):
        before=dict(native=rate_native('expired'))
        W.rate_exercise('recovered',dict(before=before),{},before)
        with self.assertRaises((RuntimeError,AssertionError)):W.rate_exercise('recovered',dict(before={}),{},before)

class DocumentCustodyTests(unittest.TestCase):
    def setUp(self):
        self.original=dict(id=10,revision=15,pending=None,title=W.F.TITLE,description=W.F.DESCRIPTION,editor=dict(id='WfREVIEW01',is_published=False),trigger=dict(id='TR_REVIEW',name='Review requests'))
        self.product=dict(attempts=[dict(id=16,complete=True,resolved=True,closed=True,readback=dict(matches=True,workflow=dict(id='WfREVIEW01',is_published=True,last_published_version_id='VFIRST'),trigger=dict(id='TR_REVIEW',name='Published')))])
        self.adopted=dict(self.original,revision=32,editor=self.product['attempts'][0]['readback']['workflow'],trigger=self.product['attempts'][0]['readback']['trigger'])
    def test_original_pending_then_verified_adoption_then_exact_browse(self):
        for stage in ('draft','finish','details'):W.document_proof(stage,self.original,self.original,None,self.product)
        for stage in ('confirmed','checked','selected'):W.document_proof(stage,dict(self.original,pending=16),self.original,None,self.product)
        baseline=W.document_proof('workflows',self.adopted,self.original,None,self.product)
        for stage in ('refreshed','returned'):W.document_proof(stage,self.adopted,self.original,baseline,self.product)
    def test_unrelated_pending_refused(self):
        with self.assertRaises((RuntimeError,AssertionError)):W.document_proof('confirmed',dict(self.original,pending=99),self.original,None,self.product)
    def test_unverified_adoption_refused(self):
        self.product['attempts'][0]['resolved']=False
        with self.assertRaises((RuntimeError,AssertionError)):W.document_proof('workflows',self.adopted,self.original,None,self.product)
    def test_adopted_editor_must_be_exact_readback(self):
        altered=copy.deepcopy(self.adopted);altered['editor']['retained_unknown']='forged'
        with self.assertRaises((RuntimeError,AssertionError)):W.document_proof('workflows',altered,self.original,None,self.product)
    def test_after_navigation_every_document_field_retained(self):
        for key,value in [('id',99),('revision',33),('pending',16),('title','lost'),('description','lost'),('editor',{}),('trigger',{})]:
            with self.subTest(key=key),self.assertRaises((RuntimeError,AssertionError)):
                W.document_proof('returned',dict(self.adopted,**{key:value}),self.original,self.adopted,self.product)

if __name__=='__main__':unittest.main()
