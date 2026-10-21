import copy
import unittest
import workflow_management_channel_workflow_details as D
import workflow_management_channel_workflow_details_ledger as L

OWNER=dict(id=7,account=D.F.OWNER['account'],accountEpoch=D.F.OWNER['epoch'],team='TWORKFLOW',channel=D.A.DEST,channelGeneration=8,socket=1)
def selection(trigger,identity):
    f,b=D.W.rows();row=(f if trigger==L.A else b)[0]
    return dict(id=identity,browseSerial=1,owner=copy.deepcopy(OWNER),group='featured' if trigger==L.A else 'bookmarked',index=0,triggerId=trigger,source=copy.deepcopy(row['source']),previewRaw=copy.deepcopy(row['previewRaw']),context=dict(location='bookmark',channelId=D.A.DEST,bookmarkId=trigger),threadRoot=None,profileUser=None,groupId=None)
def ledger(case,stage):
    count,pending,cancelled,held=L.state(case,stage)
    ids={'first-details-success':[11,11,12,12],'first-details-errors':[11]*6,'first-details-close-held':[11,12,12],'first-details-supersession':[11,12,12,13,13],'first-details-account-retirement':[11],'first-details-rate-limit':[11,12,12]}[case]
    attempts=[1,2,3,3,4,4] if case=='first-details-errors' else [1]*len(ids)
    req=[];responses=[]
    for n,(method,trigger) in enumerate(L.plan(case)[:count],1):
        req.append(dict(serial=n,method=method,fields=list(map(list,L.fields(method,trigger).items())),retrySafe=False,owner=selection(trigger,ids[n-1]),attempt=attempts[n-1]))
        responses.append(dict(serial=n,result=L.result(case,n,method,trigger)))
    releases=[dict(serial=n,receiverRetired=True,delivered=False) for n,_,_,late in L.holds(case) if stage not in L.SETUP and L.STAGES[case].index(stage)>=L.STAGES[case].index(late)]
    return dict(requests=req,responses=responses,settlements=[dict(serial=n,returned=n not in cancelled) for n in range(1,count+1) if n not in pending],releases=releases,held=held,networkCalls=0)
def native(case,stage):
    s=ledger(case,stage);trigger=D.stage_trigger(case,stage);status=D.expected_status(stage)
    selected=12 if stage in ('reopened-blocked','expired') else s['requests'][-1]['owner']['id'];sel=selection(trigger,selected)
    req=[r for r in s['requests'] if r['owner']['id']==selected];attempt=max((r['attempt'] for r in req),default=0)
    returned={r['serial']:r['returned'] for r in s['settlements']};reads=[];tr=None;wf=None
    for r in req:
        if returned.get(r['serial']) is not True:continue
        res=s['responses'][r['serial']-1]['result'];v=dict(serial=r['attempt'],method=r['method'])
        if 'raw' in res:v['raw']=res['raw']
        elif res['error']=='rate-limited':v.update(failure='rate-limited',retryAfterSeconds=12)
        else:v['failure']='read failed'
        reads.append(v)
        if r['attempt']==attempt and 'raw' in res:
            if r['method']==L.INFO:tr=res['raw']
            else:wf=res['raw']
    blocked=D.blocked(case,stage)
    p=dict(selection=sel,serial=attempt,status=status,current=True,phase='workflow' if stage in ('get-held','b-held') else 'trigger' if status=='loading' else None,reads=reads,triggerRaw=tr,workflowRaw=wf,cooldown=dict(blocked=blocked,retryAfterSeconds=12 if blocked else None,indefinite=False),projection=L.projection(trigger) if status=='ready' else None)
    def node(id,text='',disabled=False):return dict(id=id,nodeId=20,connected=True,visible=True,rect={},text=text,disabled=disabled)
    controls=[node('channel-workflow-details-close')]+([node('channel-workflow-details-retry','Retry',blocked)] if status in ('failed','idle') else [])
    if status=='ready':content=[node('channel-workflow-details-title',L.projection(trigger)['title']),node('channel-workflow-details-description',L.projection(trigger)['description'])]
    elif status=='loading':content=[node('channel-workflow-details-loading','Loading workflow details\u2026')]
    elif status=='failed':
        title,message=('Permission denied','You don\u2019t have permissions to view workflow details.') if stage=='denied' else ('Workflow not found','This workflow no longer exists.') if stage=='not-found' else ('Something\u2019s gone wrong','Oops, there was an error loading this workflow.')
        content=[node('channel-workflow-details-error-title',title),node('channel-workflow-details-error-message',message)]
    else:content=[]
    pane=dict(product=p,originalNodes=True,pane=dict(connected=True,ariaLabel='Workflow details',role=None),brokenImages=0,controls=controls,content=content,feedback=[],steps=[node('', 'Review the original request')] if status=='ready' else [],headings=['Workflow'],opener=dict(id=f"channel-workflow-open-{'featured' if trigger==L.A else 'bookmarked'}-0",nodeId=18,connected=True))
    return dict(workflowDetails=p),dict(details=pane,workflows=dict(sections=[dict(kind=g,rows=[dict(opener=dict(nodeId=18))]) for g in ('featured','bookmarked')])),copy.deepcopy(s)

class LedgerTests(unittest.TestCase):
    def test_all_exact_stage_schedules(self):
        for case,stages in L.STAGES.items():
            for stage in L.SETUP+stages:
                with self.subTest(case=case,stage=stage):L.ledger(case,stage,ledger(case,stage),OWNER)
    def reject(self,change,case='first-details-success',stage='details-a'):
        value=ledger(case,stage);change(value)
        with self.assertRaises((RuntimeError,AssertionError)):L.ledger(case,stage,value,OWNER)
    def test_wrong_reason(self):self.reject(lambda v:v['requests'][0]['fields'].__setitem__(-1,['_x_reason','preview']))
    def test_duplicate_field(self):self.reject(lambda v:v['requests'][0]['fields'].append(v['requests'][0]['fields'][0]))
    def test_no_workflow_details(self):self.reject(lambda v:v['requests'][0]['fields'].__setitem__(1,['include_workflow_details','false']))
    def test_wrong_get_identity(self):self.reject(lambda v:v['requests'][1]['fields'].__setitem__(0,['workflow_id','OTHER']))
    def test_transport_retry_safe_not_invented(self):self.reject(lambda v:v['requests'][0].update(retrySafe=True))
    def test_raw_opaque_rewritten(self):self.reject(lambda v:v['responses'][0]['result']['raw']['trigger']['opaque'].update(order=[1,3]))
    def test_access_denied_not_ready(self):self.reject(lambda v:v['responses'][0].update(result=dict(raw=L.trigger_raw(L.A))),'first-details-errors','denied')
    def test_original_context_not_arbitrary(self):self.reject(lambda v:v['requests'][0]['owner']['context'].update(channelId='OTHER'))
    def test_private_row_cannot_grant_details(self):self.reject(lambda v:v['requests'][0]['owner'].update(triggerId='FtDenied01'))
    def test_same_trigger_different_origin_rejected(self):self.reject(lambda v:v['requests'][0]['owner'].update(group='bookmarked',index=1))
    def test_foreign_account(self):self.reject(lambda v:v['requests'][0]['owner']['owner'].update(account='foreign'))
    def test_foreign_channel_generation(self):self.reject(lambda v:v['requests'][0]['owner']['owner'].update(channelGeneration=9))
    def test_foreign_socket_generation(self):self.reject(lambda v:v['requests'][0]['owner']['owner'].update(socket=0))
    def test_original_nonzero_socket_generation(self):L.ledger('first-details-success','details-a',ledger('first-details-success','details-a'),OWNER)
    def test_preview_raw_mutation(self):self.reject(lambda v:v['requests'][0]['owner']['previewRaw'].update(name='changed'))
    def test_extra_get(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][1])))
    def test_live_fallback(self):self.reject(lambda v:v.update(networkCalls=1))
    def test_offline_not_fake_raw(self):self.reject(lambda v:v['responses'][3].update(result=dict(raw=L.workflow_raw(L.A))),'first-details-errors','failed')
    def test_held_false_completion(self):self.reject(lambda v:v['settlements'].append(dict(serial=1,returned=True)),'first-details-close-held','info-held')
    def test_retired_receiver_open(self):self.reject(lambda v:v['held'][0].update(receiverRetired=False),'first-details-close-held','closed-info')
    def test_late_accepted(self):self.reject(lambda v:v['releases'][0].update(delivered=True),'first-details-supersession','late-a')
    def test_cancellation_not_ack(self):self.reject(lambda v:v['settlements'][0].update(returned=True),'first-details-account-retirement','late-settled')
    def test_rate_header_exact(self):self.reject(lambda v:v['responses'][0]['result'].update(retryAfterSeconds=0),'first-details-rate-limit','rate-limited')
    def test_no_auto_read_after_expiry(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])),'first-details-rate-limit','expired')
    def test_source_metadata_precedence(self):
        self.assertEqual(L.projection(L.A)['title'],'Info review \u4e2d')
        self.assertEqual(L.projection(L.B)['title'],'Folder details caf\u00e9')
        self.assertNotEqual(L.projection(L.A)['title'],D.W.rows()[0][0]['title'])
        self.assertEqual(L.projection(L.A)['steps'],L.workflow_raw(L.A)['workflow']['steps'])
    def test_82_actual_details_pngs(self):self.assertEqual(sum(map(len,L.STAGES.values()))*2,82)

class NativeTests(unittest.TestCase):
    def verify(self,case,stage,m,n,s):D.product_proof(case,stage,m,n,s,OWNER,[1440,1000],lambda *_:None)
    def test_all_present_panes(self):
        for case,stages in L.STAGES.items():
            for stage in stages:
                if stage in D.ABSENT:continue
                with self.subTest(case=case,stage=stage):self.verify(case,stage,*native(case,stage))
    def reject(self,change,case='first-details-success',stage='details-a'):
        m,n,s=native(case,stage);change(m,n,s)
        with self.assertRaises((RuntimeError,AssertionError)):self.verify(case,stage,m,n,s)
    def test_wrong_painted_title(self):self.reject(lambda m,n,s:n['details']['content'][0].update(text='First publication 中'))
    def test_projection_get_title_overrides_source(self):self.reject(lambda m,n,s:m['workflowDetails']['projection'].update(title=L.workflow_raw(L.A)['workflow']['title']))
    def test_wrong_parent_step(self):self.reject(lambda m,n,s:n['details']['steps'][0].update(text='Inline step must be replaced'))
    def test_unowned_pane(self):self.reject(lambda m,n,s:n['details'].update(originalNodes=False))
    def test_no_start_button(self):self.reject(lambda m,n,s:n['details']['controls'].append(dict(id='workflow-start',disabled=False)))
    def test_private_opener(self):self.reject(lambda m,n,s:n['details']['opener'].update(id='channel-workflow-open-featured-1'))
    def test_replaced_opener_node(self):self.reject(lambda m,n,s:n['details']['opener'].update(nodeId=77))
    def test_original_socket_pinned_by_separate_browse_requests(self):
        request_owner=dict(account=OWNER['account'],epoch=OWNER['accountEpoch'],team=OWNER['team'],channel=OWNER['channel'],channelGeneration=OWNER['channelGeneration'],tab='Workflows',socket=OWNER['socket'])
        ledger=dict(requests=[dict(owner=copy.deepcopy(request_owner)) for _ in range(2)])
        D.original_owner(OWNER,ledger,dict(channelGeneration=8))
        ledger['requests'][1]['owner']['socket']=0
        with self.assertRaises(RuntimeError):D.original_owner(OWNER,ledger,dict(channelGeneration=8))
    def test_blocked_retry_enabled(self):self.reject(lambda m,n,s:n['details']['controls'][1].update(disabled=False),'first-details-rate-limit','reopened-blocked')
    def test_blocked_close_still_enabled(self):self.reject(lambda m,n,s:n['details']['controls'][0].update(disabled=True),'first-details-rate-limit','rate-limited')
    def test_hidden_feedback(self):self.reject(lambda m,n,s:n['details']['content'][0].update(visible=False),'first-details-errors','denied')
    def test_generic_error_not_permission_denied(self):self.reject(lambda m,n,s:n['details']['content'][0].update(text='Permission denied'),'first-details-errors','failed')
    def test_stale_after_close(self):
        m,n,s=native('first-details-success','details-a')
        with self.assertRaises(RuntimeError):self.verify('first-details-success','closed',m,n,s)
    def test_no_fabricated_empty_ready(self):self.reject(lambda m,n,s:m['workflowDetails'].update(status='ready'),'first-details-errors','not-found')
    def test_snapshot_raw_keeps_denied(self):self.reject(lambda m,n,s:m['workflowDetails']['triggerRaw'].update(ok=True),'first-details-errors','denied')
    def test_real_dimension_validator_invoked(self):
        m,n,s=native('first-details-success','details-a');calls=[]
        D.product_proof('first-details-success','details-a',m,n,s,OWNER,[1024,640],lambda r,v:calls.append(v))
        self.assertGreaterEqual(len(calls),4);self.assertTrue(all(v==[1024,640] for v in calls))

def observation():
    return dict(model=dict(workflowDetails=None,detailsCustody=dict(threadRoot=None,threadGeneration=0,profileUser=None,groupId=None)),firstPublish=dict(attempts=[]),native=dict(detailsCustody=dict(browse=dict(nodeId=1),thread=None,composer=None,threadComposer=None)),server=dict(channelWorkflows=dict(details=dict(requests=[],responses=[]))))
class SuccessorTests(unittest.TestCase):
    def sample(self):
        def node(id,n):return dict(id=id,nodeId=n,connected=True,value=None,selectionStart=None,selectionEnd=None)
        header=dict(id='channel-details-trigger',nodeId=60,connected=True,text='#private-review',visible=True,rect={})
        successor=dict(header=header,timeline=node('message-timeline',61),composer=node('channel-composer',62),active=node('',63),name='private-review',modalAbsent=True)
        native=dict(channelSuccessor=successor,details=None,workflows=None,present=False,modal=None,detailsCustody=dict(browse=None,composer=copy.deepcopy(successor['composer'])))
        destination=dict(channel=D.A.PRIVATE,tab='Messages',channelGeneration=9,team=OWNER['team'],account=OWNER['account'],accountEpoch=OWNER['accountEpoch'])
        return native,destination
    def verify(self,n,d,stage='channel-successor'):
        D.successor_proof('first-details-supersession',stage,n,d,OWNER,[1280,780],lambda r,v:None)
    def test_actual_sidebar_successor_and_late(self):
        n,d=self.sample();self.verify(n,d);self.verify(n,d,'late-b')
    def test_wrong_channel_or_tab_rejected(self):
        for k,v in [('channel',D.A.DEST),('tab','Workflows'),('channelGeneration',8),('accountEpoch',1)]:
            n,d=self.sample();d[k]=v
            with self.assertRaises(RuntimeError):self.verify(n,d)
    def test_detached_or_replaced_composer_rejected(self):
        for k,v in [('connected',False),('nodeId',99),('id','foreign')]:
            n,d=self.sample();n['channelSuccessor']['composer'][k]=v
            with self.assertRaises(RuntimeError):self.verify(n,d)
    def test_original_pane_or_modal_not_allowed(self):
        for k in ['details','workflows','modal']:
            n,d=self.sample();n[k]={}
            with self.assertRaises(RuntimeError):self.verify(n,d)
    def test_hidden_or_wrong_header_rejected(self):
        for k,v in [('visible',False),('text','#review-destination'),('nodeId',0)]:
            n,d=self.sample();n['channelSuccessor']['header'][k]=v
            with self.assertRaises(RuntimeError):self.verify(n,d)
    def test_wrong_active_name_rejected(self):
        n,d=self.sample();n['channelSuccessor']['name']='review-destination'
        with self.assertRaises(RuntimeError):self.verify(n,d)
    def test_no_successor_in_other_stages(self):
        n,d=self.sample()
        with self.assertRaises(RuntimeError):self.verify(n,d,'closed')
        n.pop('channelSuccessor');self.verify(n,d,'closed')
    def test_missing_successor_rejected(self):
        n,d=self.sample();n.pop('channelSuccessor')
        with self.assertRaises(RuntimeError):self.verify(n,d)

class ExposureTests(unittest.TestCase):
    @staticmethod
    def box(l,t,r,b):return dict(left=l,top=t,right=r,bottom=b,width=r-l,height=b-t)
    def sample(self):
        r=self.box(10,10,310,60);p=self.box(200,0,500,500);region=self.box(10,10,200,60)
        return dict(rect=r,connected=True,visible=True,exposure=dict(viewport=r,occluder=p,regions=[region],centerHit=True,hitPoint=dict(x=105,y=35),occluded=False))
    def test_actual_serialized_fractional_coordinate_arithmetic(self):
        r=dict(bottom=136.6666717529297,height=32,left=829.1458740234375,right=904.0000457763672,top=104.66667175292967,width=74.85417175292969)
        p=self.box(920,0,1280,780)
        v=dict(rect=r,connected=True,visible=True,exposure=dict(viewport=r,occluder=p,regions=[r],centerHit=True,hitPoint=dict(x=866.5729598999023,y=120.66667175292967),occluded=False))
        D.exposure_proof(v,[1280,780])
    def test_integral_json_number_representation(self):
        self.assertTrue(D.calculated_geometry({'width':75},{'width':75.0}))
    def test_geometry_drift_not_rounding(self):
        v=self.sample();v['exposure']['regions'][0]['width']+=0.001
        with self.assertRaises(RuntimeError):D.exposure_proof(v,[500,500])
    def test_boolean_not_coordinate(self):
        self.assertFalse(D.calculated_geometry({'x':True},{'x':1.0}))
    def test_nonfinite_not_coordinate(self):
        self.assertFalse(D.calculated_geometry({'x':float('nan')},{'x':1.0}))
    def test_partial_exposure(self):D.exposure_proof(self.sample(),[500,500])
    def test_occluded_background_honestly_reported(self):
        v=self.sample();v['exposure'].update(regions=[],occluder=self.box(0,0,500,500),centerHit=False,hitPoint=None,occluded=True);v['visible']=False
        self.assertIsNone(D.exposure_proof(v,[500,500]))
    def test_covered_hitpoint_rejected(self):
        v=self.sample();v['exposure']['hitPoint']['x']=250
        with self.assertRaises(RuntimeError):D.exposure_proof(v,[500,500])
    def test_fake_visible_rejected(self):
        v=self.sample();v['exposure'].update(hitPoint=None,occluded=True)
        with self.assertRaises(RuntimeError):D.exposure_proof(v,[500,500])
    def test_disconnected_rejected(self):
        v=self.sample();v['connected']=False
        with self.assertRaises(RuntimeError):D.exposure_proof(v,[500,500])
    def test_fabricated_exposed_region_rejected(self):
        v=self.sample();v['exposure']['regions'][0]['right']=310
        with self.assertRaises(RuntimeError):D.exposure_proof(v,[500,500])
    def test_original_browse_width_not_squeezed(self):
        r=self.box(220,60,650,360);p=self.box(308,20,668,380)
        native=dict(workflows={},details={},detailsLayout=dict(browse=r,pane=dict(position='absolute',rect=p)))
        D.layout_proof(native,native,[668,380],lambda *args:None)
        changed=copy.deepcopy(native);changed['detailsLayout']['browse']['width']-=10
        with self.assertRaises(RuntimeError):D.layout_proof(changed,native,[668,380],lambda *args:None)
        changed=copy.deepcopy(native);changed['detailsLayout']['pane']['position']='relative'
        with self.assertRaises(RuntimeError):D.layout_proof(changed,native,[668,380],lambda *args:None)

class HeaderSamplingTests(unittest.TestCase):
    def pair(self):
        before=observation();before['native'].update(viewport=[668,380],details={'originalNodes':True})
        header=dict(id='channel-details-trigger',nodeId=41,text='#review-destination',visible=True,rect=dict(left=234,right=368,top=13,bottom=34,width=134,height=21))
        before['native']['destination']={'header':header}
        later=copy.deepcopy(header);later['rect'].update(left=224,right=358)
        before['native']['workflows']={'header':later}
        after=copy.deepcopy(before);after['native']['destination']['header']=copy.deepcopy(later)
        return before,after
    def test_corroborated_same_header_horizontal_scroll(self):
        a,b=self.pair();self.assertTrue(D.captured_observation(a,b))
    def test_changed_model_not_ignored(self):
        a,b=self.pair();b['model']['changed']=True;self.assertFalse(D.captured_observation(a,b))
    def test_changed_node_not_ignored(self):
        a,b=self.pair();b['native']['destination']['header']['nodeId']=42;self.assertFalse(D.captured_observation(a,b))
    def test_uncorroborated_position_not_ignored(self):
        a,b=self.pair();b['native']['destination']['header']['rect']['left']=225;self.assertFalse(D.captured_observation(a,b))
    def test_vertical_or_width_drift_not_ignored(self):
        for key in ('top','width'):
            a,b=self.pair();a['native']['destination']['header']['rect'][key]+=1;self.assertFalse(D.captured_observation(a,b))
    def test_other_native_geometry_not_ignored(self):
        a,b=self.pair();b['native']['other']={'rect':{}};self.assertFalse(D.captured_observation(a,b))

class ExerciseTests(unittest.TestCase):
    def test_original_late_drop_is_not_adoption(self):
        o=observation();e=dict(before=copy.deepcopy(o),after=copy.deepcopy(o),release=dict(serial=1,receiverRetired=True,delivered=False),quiet=dict(before=copy.deepcopy(o),after=copy.deepcopy(o),observedMs=500))
        D.exercise_proof('first-details-account-retirement','late-settled',e,o,o)
    def test_late_delivered_rejected(self):
        o=observation();e=dict(before=o,after=o,release=dict(serial=1,receiverRetired=True,delivered=True))
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-account-retirement','late-settled',e,o,o)
    def test_late_native_change_rejected(self):
        before=observation();after=copy.deepcopy(before);after['native']['detailsCustody']['browse']=dict(nodeId=9)
        e=dict(before=before,after=after,release=dict(serial=1,receiverRetired=True,delivered=False))
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-account-retirement','late-settled',e,after,before)
    def test_quiet_must_be_exact_observed(self):
        o=observation();q=dict(before=o,after=copy.deepcopy(o),observedMs=500);q['after']['model']['extra']=True
        with self.assertRaises(RuntimeError):D.quiet(q,o)
    def test_quiet_interval_not_guessed(self):
        o=observation()
        with self.assertRaises(RuntimeError):D.quiet(dict(before=o,after=o,observedMs=50),o)
    def test_private_actual_node_and_zero_effect(self):
        o=observation();o['native']['workflows']=dict(sections=[dict(rows=[{},dict(nodeId=55)])])
        e=dict(private=dict(before=o,after=o,activation=dict(ok=True,actions=0,trigger='FtDenied01',nodeId=55),quiet=dict(before=o,after=o,observedMs=500)))
        D.exercise_proof('first-details-success','browse',e,o,observation())
        e['private']['activation']['nodeId']=56
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-success','browse',e,o,observation())
    def test_expiry_no_automatic_request(self):
        before=observation();before['model']['workflowDetails']=dict(serial=0)
        after=copy.deepcopy(before);e=dict(before=before,after=after,waitedMs=6000)
        D.exercise_proof('first-details-rate-limit','expired',e,after,before)
        after['server']['channelWorkflows']['details']['requests'].append(dict(serial=1))
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-rate-limit','expired',e,after,before)
    def test_underlying_node_custody(self):
        before=observation();before['model']['workflowDetails']=dict(selection=dict(id=1),serial=1)
        after=copy.deepcopy(before);after['model']['workflowDetails']=None;after['native']['detailsFocus']='button'
        before['native']['details']=dict(opener=dict(id='button'))
        e=dict(action='channel-workflow-details-close',before=before,after=after)
        D.exercise_proof('first-details-success','closed',e,after,before)
        after['native']['detailsCustody']['browse']['nodeId']=99
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-success','closed',e,after,before)
    def test_retry_uses_same_selection(self):
        before=observation();before['model']['workflowDetails']=dict(selection=dict(id=1),serial=1)
        after=copy.deepcopy(before);after['model']['workflowDetails']['serial']=2
        e=dict(action='channel-workflow-details-retry',before=before,after=after)
        D.exercise_proof('first-details-errors','not-found',e,after,before)
        after['model']['workflowDetails']['selection']['id']=2
        with self.assertRaises(RuntimeError):D.exercise_proof('first-details-errors','not-found',e,after,before)

if __name__=='__main__':unittest.main()
