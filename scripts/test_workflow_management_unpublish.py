"""Offline receipt mutations only; synthetic samples are not native acceptance."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

import workflow_management_unpublish as U

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
CAPTURES = Path('/private/unpublish-captures')


def read_args(method, ordinal):
    if method == 'experiments.getByUser': return {'ssb_platform':'windows','ssb_release_channel':'prod'}
    if method == 'client.init': return {'include_relevant_onboarding':'false','_x_reason':'native-workflow-capabilities'}
    if method == 'features.access.policies.list': return {'_x_reason':'native-workflow-capabilities'}
    if method == 'client.extras': return {'extras':'["team_prefs"]','_x_reason':'native-workflow-capabilities'}
    if method == 'workflows.triggers.list': return {'app_id':'A_HOME','_x_reason':'load-workflow-editor'}
    key = 'trigger_id' if method in ('workflows.triggers.info','workflows.triggers.permissions.list') else 'workflow_id'
    return {key: 'TR_REVIEW' if key == 'trigger_id' else 'WF_REVIEW', '_x_reason':'load-workflow-editor'}


def server(case, state, confirmation):
    v = dict(original=dict(workflow=copy.deepcopy(U.WORKFLOW),trigger=copy.deepcopy(U.TRIGGER)),
             current=dict(workflow=copy.deepcopy(U.WORKFLOW),trigger=copy.deepcopy(U.TRIGGER)),
             owner=copy.deepcopy(U.OWNER),ownerCurrent=not state['retired'],requests=[],responses=[],
             settlements=[],releases=[],held=None,networkCalls=0,completed=0,cancelled=0)
    sequence = state['reads'][:23] + list(U.METHODS[:state['writes']]) + state['reads'][23:]
    for serial, method in enumerate(sequence, 1):
        write = method in U.METHODS
        fields = U.expected_form(U.METHODS.index(method)) if write else list(map(list, read_args(method,serial).items()))
        result = U.result_for(case,U.METHODS.index(method)) if write else U.read_result(method,**v['current'])
        applied = write and (result['raw'] is None or result['raw']['ok'] is True)
        if applied:
            key, value = ('workflow', U.UNPUBLISHED) if method == U.METHODS[0] else ('trigger', U.SYNC_TRIGGER)
            v['current'][key] = copy.deepcopy(value)
        v['requests'].append(dict(serial=serial,phase='mutation' if write else 'read',method=method,
            fields=fields,args=dict(fields),retrySafe=not write,applied=applied,
            owner=dict(U.OWNER,**{k:confirmation[k] for k in ('visit','document','revision')})))
        v['responses'].append(dict(serial=serial,result=copy.deepcopy(result)))
        if not state['held'] or serial != len(sequence): v['settlements'].append(dict(serial=serial,returned=True))
    v['completed']=len(v['settlements']);v['held']=len(sequence) if state['held'] else None
    if case in ('unpublish-success','unpublish-retirement') and state['finished']:
        serial = 25 if case == 'unpublish-success' else 24
        v['releases']=[dict(serial=serial,receiverRetired=False,delivered=True,response=copy.deepcopy(v['responses'][serial-1]))]
    return v


def slot(index, phase, case):
    result = U.result_for(case,index) if phase not in ('pending','in-flight') else {'raw':None,'error':None}
    return dict(index=0,method=U.METHODS[index],arguments=copy.deepcopy(U.ARGUMENTS[index]),prerequisite=None,
                outcome=phase,raw=copy.deepcopy(result['raw']),failure='delivery unconfirmed' if phase=='unknown' else None)


def attempt(case,state,c,source):
    p=state['phases'];v=dict(c)
    v.pop('current',None)
    v.update(finished=state['finished'],complete=state['complete'],resolved=state['resolved'],noSend=False,
             current=not state['finished'] and not state['retired'],primarySucceeded=p[0]=='acknowledged',
             slots=[slot(0,p[0],case)],followup=None,readback=None)
    if p[0]=='acknowledged':
        sync=p[1] if len(p)==2 else 'pending'
        v['followup']=dict(phase='Complete' if sync in ('acknowledged','rejected') else 'TriggerInputSynchronization',
            fullySucceeded=sync=='acknowledged',workflow=copy.deepcopy(U.UNPUBLISHED),
            trigger=copy.deepcopy(U.SYNC_TRIGGER if sync=='acknowledged' else U.TRIGGER),
            stages=[dict(phase='TriggerInputSynchronization',slots=[slot(1,sync,case)])])
    if state['checked']:
        v['readback']=dict(copy.deepcopy(source),matches=case in ('unpublish-success','unpublish-sync-unknown'))
    return v


def control(id,text='',disabled=False):
    return dict(id=id,text=text,disabled=disabled,visible=True,hit=True,
                rect=dict(left=10,top=20,right=110,bottom=40,width=100,height=20))


def sample(case='unpublish-success',fixture='workflow-editor'):
    viewport=R['expected_viewport'](fixture,'win32')
    stages=[];confirmation=dict(id=13,visit=10,document=11,revision=12,title=U.TITLE,
                                description=U.DESCRIPTION,workflow='WF_REVIEW',current=True)
    confirmed=dict(confirmation,id=15,revision=14) if case=='unpublish-success' else confirmation
    revision=12
    for name in U.STAGES[case]:
        state=U.stage_state(case,name);busy=state['held'] and not state['retired']
        confirming=name in ('confirmation','reconfirmed');sealed=bool(state['phases']) and not state['resolved']
        route='about' if name in ('about','back') or state['retired'] else 'editor'
        if name=='cancelled':revision=14
        if state['finished'] and name not in ('back','reopened'):revision+=1
        visit=30 if name=='reopened' else 10
        c=confirmed if name=='reconfirmed' or state['phases'] else confirmation
        src=server(case,state,confirmed)
        title,description=(U.WORKFLOW['title'],U.WORKFLOW['description']) if name=='editor' else (U.TITLE,U.DESCRIPTION)
        notice=(U.REOPENED if name=='reopened' else U.VERIFIED if state['resolved'] else U.NOT_OBSERVED if state['checked'] and case=='unpublish-primary-rejected'
                else U.OBSERVED if state['checked'] else U.CONFIRMED if state['complete'] else U.UNCONFIRMED) if state['finished'] else None
        labels={'pending':'Not sent','in-flight':'Waiting for response','acknowledged':'Acknowledged','rejected':'Rejected','unknown':'Unconfirmed'}
        outcomes=(['Unpublish: '+labels[state['phases'][0]]]+(['Trigger synchronization: '+labels[state['phases'][1]]] if len(state['phases'])==2 else [])) if route=='editor' and sealed else []
        model=dict(route=route,visit=visit if route=='editor' else None,busy=busy,reads=len(state['reads']),writes=state['writes'],
            networkCalls=0,retainedDocuments=0 if name=='about' else 1,retainedAttempts=0,
            title=title if route=='editor' else None,description=description if route=='editor' else None,
            dirty=route=='editor' and name!='editor',pending=route=='editor' and sealed,
            observed=route=='editor' and (name=='reopened' or state['checked'] and not state['resolved']),
            notice=notice if route=='editor' else None,outcomes=outcomes if route=='editor' else None)
        document=dict(id=11,revision=revision,title=title,description=description,pending=c['id'] if sealed else None,
            editor=copy.deepcopy(U.UNPUBLISHED if state['resolved'] else U.WORKFLOW),
            trigger=copy.deepcopy(U.SYNC_TRIGGER if state['resolved'] else U.TRIGGER),notice=notice,editable=not(confirming or busy or sealed)) if route=='editor' else None
        product=dict(visit=visit,visible=route=='editor',confirmation=copy.deepcopy(c) if confirming else None,
                     document=document,attempts=[attempt(case,state,c,src['current'])] if state['phases'] else [])
        native=dict(ok=True,viewport=viewport,noOverflow=True,present=route=='editor',
            scope=dict(visit=str(visit),document='11',revision=str(revision)) if route=='editor' else None,
            originalNodes=True if route=='editor' else None,originalDetached=False if route=='editor' else name!='about',
            fields={key:dict(value=value,readonly=confirming or busy or sealed,start=0,end=0)
                    for key,value in [('title',title),('description',description)]} if route=='editor' else None,
            composing=False,leasePresent=confirming or busy,confirmation=None,controls=[],feedback=[])
        if route!='editor':native['controls']=[control('workflow-configuration','Configuration')]
        else:
            for key,disabled in [('save',name=='editor' or confirming or busy or sealed),('check',confirming or busy),('managed',confirming or busy),('delete',confirming or busy or sealed)]:
                native['controls'].append(control('workflow-management-'+key,key,disabled))
            if not state['resolved']:native['controls'].append(control('workflow-management-unpublish','Unpublish workflow',confirming or busy or sealed))
            if confirming:
                actions=[control('workflow-unpublish-cancel','Cancel'),control('workflow-unpublish-confirm','Unpublish')]
                native['controls']+=actions
                native['confirmation']=dict(control('workflow-unpublish-confirmation','Unpublish this workflow? '+U.WARNING),actions=copy.deepcopy(actions))
            native['feedback']=[control('',text) for text in ([notice] if notice else [])+outcomes]
        geometry=dict(copy.deepcopy(native),pngDimensions=viewport)
        row=dict(stage=name,model=model,geometry=geometry,proof=dict(unpublish=product,server=src,native=native,exercise={}))
        stages.append(row)
    by={r['stage']:r for r in stages}
    replacement=None
    for i,row in enumerate(stages):
        name=row['stage'];o=U.observation(row)
        if name in ('primary-ack','primary-held'):e=dict(before=copy.deepcopy(o),after=copy.deepcopy(o),observedMs=500)
        elif name in ('cancelled','checked','checked-again','back'):e=dict(before=U.observation(stages[i-1]),after=o)
        elif name=='confirmed':e=dict(release=o['server']['releases'][-1])
        elif name=='reopened':e=dict(before=U.observation(by['checked']),after=o,remount=dict(ok=True,originalDetached=True,newRoot=True,newTitle=True,newDescription=True))
        elif name=='successor':
            previous=U.observation(stages[i-1]);replacement=dict(before=previous['model'],after=o['model'],previousEpoch=0,currentEpoch=1,foreignDocumentHidden=True)
            e=dict(before=previous,after=o,replacement=replacement)
        elif name=='late-settled':e=dict(before=U.observation(by['successor']),after=o,release=o['server']['releases'][-1],successorUnchanged=True)
        else:e={}
        row['proof']['exercise']=copy.deepcopy(e)
    return dict(ok=True,fixture=fixture,scenario='editor',unpublishCase=case,aboutConfiguration=True,syntheticSession=True,
        noLiveNetwork=True,viewport=viewport,stages=stages,captures={n:str(CAPTURES/f'{fixture}-{n}.png') for n in U.STAGES[case]},
        final=copy.deepcopy(U.observation(stages[-1])),replacement=replacement)


def validate(value):
    R['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False,unpublish_case=value['unpublishCase'])


class DispatchTests(unittest.TestCase):
    def test_old_maps_unchanged(self):
        self.assertEqual(set(R['STAGES']),{'managed','editor','reply-loss'})
        self.assertEqual(len(R['DELETE_STAGES']),6)
        self.assertEqual(len(U.STAGES),6)

    def test_unrequested_or_conflicting_case_refused(self):
        for kwargs,value in [({}, {'unpublishCase':'unpublish-success'}),
                ({'unpublish_case':'invented'},{}),
                ({'unpublish_case':'unpublish-success','delete_case':'delete-success'},{}),
                ({'delete_case':'delete-success'},{'unpublishCase':'unpublish-success'})]:
            with self.subTest(kwargs=kwargs),self.assertRaises(RuntimeError):
                R['validate']('workflow-editor',value,CAPTURES,png=False,**kwargs)

    def test_noneditor_case_refused(self):
        with self.assertRaisesRegex(RuntimeError,'Invalid Unpublish'):
            R['validate']('workflow-managed',{},CAPTURES,png=False,unpublish_case='unpublish-success')


class CompleteReceiptTests(unittest.TestCase):
    def test_all_six_cases_both_widths(self):
        for case in U.STAGES:
            for fixture in ('workflow-editor','workflow-editor-narrow'):
                with self.subTest(case=case,fixture=fixture):validate(sample(case,fixture))

    def reject(self,case,change):
        v=sample(case);change(v)
        with self.assertRaises(RuntimeError):validate(v)

    def test_original_owner_not_fabricated_or_boolean(self):
        for key,value in [('account','fake'),('epoch',False),('epoch',1),('team','OTHER')]:
            self.reject('unpublish-success',lambda v:v['stages'][0]['proof']['server']['owner'].__setitem__(key,value))

    def test_malformed_or_wrong_primary_ack_rejected(self):
        for raw in [{'ok':True},{'ok':1},{'ok':True,'error':'bad'},dict(U.PRIMARY_RAW,decorated_workflow=dict(U.UNPUBLISHED,is_published=True)),
                    dict(U.PRIMARY_RAW,decorated_workflow=dict(U.UNPUBLISHED,id='WRONG'))]:
            self.reject('unpublish-success',lambda v:v['stages'][6]['proof']['server']['responses'][23]['result'].__setitem__('raw',raw))

    def test_original_opaque_mapping_and_removed_key_required(self):
        for inputs in [{},dict(U.SYNC_INPUTS,removed={'value':'remove'}),dict(U.SYNC_INPUTS,keep={'value':'literal source'})]:
            self.reject('unpublish-success',lambda v:v['stages'][6]['proof']['unpublish']['attempts'][0]['followup']['stages'][0]['slots'][0]['arguments'].__setitem__('inputs',inputs))

    def test_wrong_order_reason_or_retry_refused(self):
        for key,value in [('method','functions.workflows.publish'),('retrySafe',True),('fields',[['workflow_id','WF_REVIEW'],['_x_reason','publish-workflow']])]:
            self.reject('unpublish-success',lambda v:v['stages'][6]['proof']['server']['requests'][23].__setitem__(key,value))

    def test_primary_only_ack_cannot_complete(self):
        self.reject('unpublish-success',lambda v:v['stages'][6]['proof']['unpublish']['attempts'][0].__setitem__('complete',True))

    def test_unknown_matching_readback_never_resolves(self):
        self.reject('unpublish-sync-unknown',lambda v:v['stages'][5]['proof']['unpublish']['attempts'][0].__setitem__('resolved',True))
        self.reject('unpublish-sync-unknown',lambda v:v['stages'][5]['proof']['unpublish']['attempts'][0]['followup']['stages'][0]['slots'][0].__setitem__('outcome','acknowledged'))

    def test_rejected_sync_complete_phase_not_success(self):
        self.reject('unpublish-sync-rejected',lambda v:v['stages'][4]['proof']['unpublish']['attempts'][0]['followup'].__setitem__('fullySucceeded',True))

    def test_readback_must_match_desired_inputs_not_old_trigger(self):
        self.reject('unpublish-sync-rejected',lambda v:v['stages'][5]['proof']['unpublish']['attempts'][0]['readback'].__setitem__('matches',True))
        self.reject('unpublish-sync-unknown',lambda v:v['stages'][5]['proof']['unpublish']['attempts'][0]['readback'].__setitem__('matches',False))

    def test_retired_original_response_must_deliver_without_sync(self):
        self.reject('unpublish-retirement',lambda v:v['stages'][-1]['proof']['server']['releases'][0].__setitem__('delivered',False))
        self.reject('unpublish-retirement',lambda v:v['stages'][-1]['proof']['server']['requests'].append(copy.deepcopy(v['stages'][-1]['proof']['server']['requests'][-1])))

    def test_cancel_cannot_write_or_change_native_buffer(self):
        self.reject('unpublish-success',lambda v:v['stages'][4]['proof']['native']['fields']['title'].__setitem__('value','lost'))
        self.reject('unpublish-success',lambda v:v['stages'][4]['model'].__setitem__('writes',1))

    def test_success_needs_explicit_owned_readback_and_new_reopen_nodes(self):
        self.reject('unpublish-success',lambda v:v['stages'][8]['model'].__setitem__('reads',23))
        self.reject('unpublish-success',lambda v:v['stages'][-1]['proof']['exercise']['remount'].__setitem__('newTitle',False))

    def test_reopen_uses_existing_dirty_review_notice_without_rewriting_verified_attempt(self):
        v=sample();checked=v['stages'][8];reopened=v['stages'][-1]
        self.assertEqual(checked['model']['notice'],U.VERIFIED)
        self.assertEqual(reopened['model']['notice'],U.REOPENED)
        self.assertEqual(checked['proof']['unpublish']['attempts'],reopened['proof']['unpublish']['attempts'])
        validate(v)
        for row in (reopened,):
            row['model']['notice']=U.VERIFIED
            row['proof']['unpublish']['document']['notice']=U.VERIFIED
        with self.assertRaisesRegex(RuntimeError,'guidance'):validate(v)

    def test_no_replay_check_and_dirty_draft_preservation(self):
        self.reject('unpublish-primary-unknown',lambda v:v['stages'][-1]['model'].__setitem__('writes',2))
        self.reject('unpublish-success',lambda v:v['stages'][8]['proof']['unpublish']['document'].__setitem__('title',U.WORKFLOW['title']))

    def test_actual_nodes_hit_and_geometry_required(self):
        for key,value in [('originalNodes',False),('composing',True),('leasePresent',False)]:
            self.reject('unpublish-success',lambda v:v['stages'][3]['proof']['native'].__setitem__(key,value))
        self.reject('unpublish-success',lambda v:v['stages'][3]['proof']['native']['controls'][0].__setitem__('hit',False))
        self.reject('unpublish-success',lambda v:v['stages'][3]['geometry']['controls'][0]['rect'].__setitem__('right',99999))

    def test_wrong_case_inventory_and_terminal_refused(self):
        self.reject('unpublish-success',lambda v:v['stages'].reverse())
        self.reject('unpublish-success',lambda v:v.__setitem__('final',{}))
        self.reject('unpublish-success',lambda v:v['captures'].__setitem__('draft','/foreign.png'))

    def test_closed_log_identity_and_case_specific_edges(self):
        value=sample('unpublish-sync-unknown');ready=R['ready_tuple'](value['fixture'],'a'*32,123)
        value['readiness']=ready
        edges=[dict(stage=stage,edge=edge) for stage in U.STAGES[value['unpublishCase']] for edge in ('before','after')]
        with tempfile.TemporaryDirectory() as directory:
            log=Path(directory)/'native.log'
            records={R['WAITING']:[ready],R['EDGE']:edges,R['PREFIX']:[value],'workflow-thread-keyboard: ':[]}
            log.write_text(''.join(prefix+json.dumps(row)+'\n' for prefix,rows in records.items() for row in rows),encoding='utf-8')
            class Native:
                def receipts(self,prefix):return records[prefix]
            native=Native();native.log=log
            self.assertEqual(R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,
                                                  unpublish_case='unpublish-sync-unknown'),value)
            with self.assertRaises(RuntimeError):
                R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,
                                      unpublish_case='unpublish-primary-unknown')
            records[R['EDGE']]=edges[:-1]
            with self.assertRaises(RuntimeError):
                R['validate_log'](native,value['fixture'],CAPTURES,platform='win32',png=False,
                                   unpublish_case='unpublish-sync-unknown')


if __name__=='__main__':unittest.main()
