"""Independent Delete receipt mutations; no native or Slack execution."""
import copy
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

import workflow_management_delete as D

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
CAPTURES = Path('/private/delete-captures')


class RunnerDispatchTests(unittest.TestCase):
    def test_old_metadata_scenarios_remain_separate(self):
        self.assertEqual(set(R['STAGES']), {'managed', 'editor', 'reply-loss'})
        self.assertEqual(len(D.STAGES), 6)
        self.assertNotIn('delete-first-malformed', D.STAGES)

    def test_unrequested_delete_receipt_refused(self):
        with self.assertRaisesRegex(RuntimeError, 'Unrequested Delete'):
            R['validate']('workflow-editor', {'deleteCase': 'delete-success'}, CAPTURES, png=False)

    def test_invalid_case_or_noneditor_fixture_refused(self):
        for fixture, case in [('workflow-managed', 'delete-success'), ('workflow-editor', 'invented')]:
            with self.subTest(fixture=fixture, case=case), self.assertRaisesRegex(RuntimeError, 'Invalid Delete'):
                R['validate'](fixture, {}, CAPTURES, png=False, delete_case=case)





def server_sample(case='delete-success', writes=2, held=False):
    owner = dict(account=None, epoch=0, team='TWORKFLOW', user='UWORKFLOW')
    v = dict(original=dict(workflow=copy.deepcopy(D.WORKFLOW), trigger=copy.deepcopy(D.TRIGGER)),
             owner=owner, ownerCurrent=True, triggerPresent=True, workflowPresent=True,
             requests=[], responses=[], settlements=[], releases=[], held=None, networkCalls=0, completed=0, cancelled=0)
    for i in range(writes):
        fields = [list(pair) for pair in D.ARGS[i].items()] + [['_x_reason', D.REASONS[i]]]
        applied = case != 'delete-first-rejected'
        v['requests'].append(dict(serial=i+1,phase='mutation',method=D.METHODS[i],args=dict(fields),fields=fields,
                                  retrySafe=False,owner=dict(owner,visit=10,document=11,revision=12),applied=applied))
        v['responses'].append(dict(serial=i+1,result=D.expected_result(case,i)))
        if applied: v['triggerPresent' if i==0 else 'workflowPresent'] = False
        if not held or i != writes-1: v['settlements'].append(dict(serial=i+1,returned=True))
    v['completed']=len(v['settlements'])
    v['held']=writes if held else None
    return v


def attempt_sample(phases, finished=True, terminal=False):
    slots=[]
    for i, phase in enumerate(phases):
        raw={'ok':True} if phase=='acknowledged' else {'ok':False,'error':'not_allowed'} if phase=='rejected' else None
        slots.append(dict(index=i,method=D.METHODS[i],arguments=copy.deepcopy(D.ARGS[i]),prerequisite=None if i==0 else 0,
                          outcome=phase,raw=raw,failure='delivery unconfirmed' if phase=='unknown' else None))
    return dict(id=13,visit=10,document=11,revision=12,title=D.TITLE,description=D.DESCRIPTION,workflow='WF_REVIEW',
                finished=finished,terminal=terminal,noSend=False,primarySucceeded=phases==['acknowledged','acknowledged'],
                current=not finished,slots=slots)


class RawCustodyTests(unittest.TestCase):
    def test_all_six_server_outcomes(self):
        for case,writes,held in [('delete-success',2,True),('delete-first-rejected',1,False),
                ('delete-first-unknown',1,False),('delete-second-unknown',2,False),
                ('delete-preflight-retirement',0,False),('delete-mutation-retirement',1,True)]:
            with self.subTest(case=case): D.server_proof(server_sample(case,writes,held),case,writes,held=held)

    def test_genuine_unassociated_synthetic_owner_is_exact_and_retained(self):
        v=server_sample('delete-mutation-retirement',1,True)
        self.assertEqual(v['owner'],dict(account=None,epoch=0,team='TWORKFLOW',user='UWORKFLOW'))
        D.server_proof(v,'delete-mutation-retirement',1,held=True)
        v['ownerCurrent']=False
        D.server_proof(v,'delete-mutation-retirement',1,held=True,retired=True)
        for owner in [dict(v['owner'],account='fabricated'),dict(v['owner'],epoch=1),dict(v['owner'],epoch=False)]:
            bad=copy.deepcopy(v);bad['owner']=owner
            with self.subTest(owner=owner),self.assertRaises(RuntimeError):
                D.server_proof(bad,'delete-mutation-retirement',1,held=True,retired=True)

    def test_wrong_order_id_reason_and_retry_refused(self):
        for key,value in [('method',D.METHODS[1]),('retrySafe',True),('fields',[['trigger_id','OTHER']]),
                          ('args',{'trigger_id':'OTHER'})]:
            v=server_sample();v['requests'][0][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError): D.server_proof(v,'delete-success',2)

    def test_malformed_ack_cannot_stand_in_for_success(self):
        for raw in [None,False,{}, {'ok':1},{'ok':True,'error':'bad'},{'ok':False}]:
            v=server_sample();v['responses'][0]['result']['raw']=raw
            with self.subTest(raw=raw),self.assertRaises(RuntimeError): D.server_proof(v,'delete-success',2)

    def test_unknown_applied_is_not_not_sent(self):
        v=server_sample('delete-first-unknown',1)
        v['triggerPresent']=True
        with self.assertRaises(RuntimeError): D.server_proof(v,'delete-first-unknown',1)

    def test_raw_editor_opaque_fields_retained(self):
        v=server_sample();del v['original']['workflow']['retained_unknown']
        with self.assertRaises(RuntimeError): D.server_proof(v,'delete-success',2)

    def test_missing_delivery_and_duplicate_dispatch_refused(self):
        for mutate in [lambda v:v['settlements'].pop(),lambda v:v['requests'].append(copy.deepcopy(v['requests'][1])),
                       lambda v:v.update(networkCalls=1),lambda v:v.update(completed=True)]:
            v=server_sample();mutate(v)
            with self.assertRaises(RuntimeError): D.server_proof(v,'delete-success',2)

    def test_slot_raws_and_terminal_ack_only(self):
        for phases in [['rejected','pending'],['unknown','pending'],['acknowledged','unknown'],['acknowledged','acknowledged']]:
            terminal=phases==['acknowledged','acknowledged']
            v=attempt_sample(phases,terminal=terminal)
            D.attempt_proof(v,phases,finished=True,terminal=terminal)
            v['slots'][0]['raw']={'ok':True,'invented':True}
            with self.assertRaises(RuntimeError):D.attempt_proof(v,phases,finished=True,terminal=terminal)

    def test_lost_ack_cannot_become_confirmed_from_readback(self):
        v=attempt_sample(['acknowledged','unknown']);v['terminal']=True
        with self.assertRaises(RuntimeError):D.attempt_proof(v,['acknowledged','unknown'],finished=True)

    def test_original_failure_never_grants_dependent_slot(self):
        for case in ['delete-first-rejected','delete-first-unknown']:
            v=server_sample(case,2)
            with self.subTest(case=case),self.assertRaises(RuntimeError):D.server_proof(v,case,2)





def fields_for(method, managed=False):
    args = {
        'experiments.getByUser': {'ssb_platform':'windows','ssb_release_channel':'prod'},
        'client.init': {'include_relevant_onboarding':'false','_x_reason':'native-workflow-capabilities'},
        'features.access.policies.list': {'_x_reason':'native-workflow-capabilities'},
        'client.extras': {'extras':'["team_prefs"]','_x_reason':'native-workflow-capabilities'},
        'functions.workflows.list': {'filter_options':'{"name":""}','limit':'50',
            'sort_options':'{"column":"date_updated","direction":"desc"}',
            'workflow_builder_only':'true','_x_reason':'automations_gallery'},
        'workflows.triggers.list': {'app_ids':'','_x_reason':'automations_gallery'} if managed else {'app_id':'A_HOME','_x_reason':'load-workflow-editor'},
    }
    if method in args:return args[method]
    key,value=('trigger_id','TR_REVIEW') if method in ('workflows.triggers.info','workflows.triggers.permissions.list') else ('workflow_id','WF_REVIEW')
    return {key:value,'_x_reason':'load-workflow-editor'}


def full_server(case,stage):
    writes,phases,finished,terminal,reads=D.stage_expectation(case,stage)
    v=server_sample(case,0);v['ownerCurrent']=stage not in ('successor','late-refused','late-settled')
    sequence=[('read',m) for m in reads[:23]]+[('mutation',D.METHODS[i]) for i in range(writes)]+[('read',m) for m in reads[23:]]
    mutation=0
    for serial,(phase,method) in enumerate(sequence,1):
        args=(dict(D.ARGS[mutation],_x_reason=D.REASONS[mutation]) if phase=='mutation' else fields_for(method,serial>25))
        applied=phase=='mutation' and case!='delete-first-rejected'
        result=D.expected_result(case,mutation) if phase=='mutation' else D.read_result(method,v['workflowPresent'],v['triggerPresent'])
        if phase=='mutation':
            if applied:v['triggerPresent' if mutation==0 else 'workflowPresent']=False
            mutation+=1
        v['requests'].append(dict(serial=serial,phase=phase,method=method,args=args,fields=[list(p) for p in args.items()],
                                  retrySafe=phase=='read',owner=dict(v['owner'],visit=10,document=11,revision=16 if case=='delete-success' and phase=='mutation' else 14),applied=applied))
        v['responses'].append(dict(serial=serial,result=result))
    held=stage in ('trigger-ack','preflight-held','mutation-held','successor')
    v['held']=len(sequence) if held else None
    v['settlements']=[dict(serial=i,returned=True) for i in range(1,len(sequence)+1) if i!=v['held']]
    v['completed']=len(v['settlements'])
    if stage in ('deleted','managed-empty','late-refused','late-settled'):
        serial=25 if case=='delete-success' else 18 if case=='delete-preflight-retirement' else 24
        v['releases']=[dict(serial=serial,receiverRetired=False,delivered=True,response=v['responses'][serial-1])]
    return v


def sample(case, narrow=False):
    fixture='workflow-editor'+('-narrow' if narrow else '')
    viewport=[668,380] if narrow else [1280,780]
    box=dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
    def control(id,text=None,disabled=False):return dict(id=id,text=text or id,disabled=disabled,rect=copy.deepcopy(box),visible=True,hit=True)
    rows=[];replacement=None
    for stage in D.STAGES[case]:
        writes,phases,finished,terminal,reads=D.stage_expectation(case,stage)
        retired=stage in ('successor','late-refused','late-settled');busy=stage in ('trigger-ack','preflight-held','mutation-held')
        confirming=stage in ('confirmation','reconfirmed');sealed=bool(phases) and case!='delete-preflight-retirement'
        route='about' if stage=='about' or retired else 'managed' if stage=='managed-empty' else 'editor'
        title,description=(D.WORKFLOW['title'],D.WORKFLOW['description']) if stage=='editor' else (D.TITLE,D.DESCRIPTION)
        revision=12 if stage=='editor' else 16 if stage in ('cancelled','reconfirmed','trigger-ack','deleted','managed-empty') and case=='delete-success' else 14
        notice=(D.DELETED if terminal else D.UNCONFIRMED) if phases and finished and route=='editor' else None
        labels={'pending':'Not sent','in-flight':'Waiting for response','acknowledged':'Acknowledged','rejected':'Rejected','unknown':'Unconfirmed'}
        outcomes=[f'{kind}: {labels[phase]}' for kind,phase in zip(('Trigger','Workflow'),phases)] if route=='editor' else None if route=='about' else []
        model=dict(route=route,visit=None if route=='about' else 10,busy=busy,title=title if route=='editor' else None,
                   description=description if route=='editor' else None,dirty=route=='editor' and stage!='editor',pending=route=='editor' and bool(phases),
                   observed=False,notice=notice,outcomes=outcomes,writes=writes,reads=len(reads),networkCalls=0,
                   retainedDocuments=0 if stage=='about' else 1,retainedAttempts=0)
        document=dict(id=11,revision=revision,title=title,description=description,pending=13 if phases else None,
                      editor=copy.deepcopy(D.WORKFLOW),trigger=copy.deepcopy(D.TRIGGER),notice=notice) if route=='editor' else None
        attempt=attempt_sample(phases,finished,terminal) if phases else None
        if attempt:
            attempt.update(current=not finished and not retired,noSend=case=='delete-preflight-retirement' and finished,
                           revision=16 if case=='delete-success' else 14)
        product=dict(visit=10,visible=not retired and stage!='about',confirmation=dict(id=13 if stage=='reconfirmed' or case!='delete-success' else 9,visit=10,document=11,revision=revision,
                       title=title,description=description,workflow='WF_REVIEW',current=True) if confirming else None,
                     document=document,terminal=terminal if route=='editor' else False,attempts=[attempt] if attempt else [])
        controls=([control('workflow-configuration')] if route=='about' else [control('workflow-management-search')] if route=='managed' else
                  [control('workflow-management-save',disabled=stage=='editor' or confirming or busy or sealed),
                   control('workflow-management-check',disabled=confirming or busy or sealed),
                   control('workflow-management-managed',disabled=confirming or busy)])
        if route=='editor' and stage!='deleted':controls.append(control('workflow-management-delete',disabled=confirming or busy or sealed))
        confirmation=None
        if confirming:
            actions=[control('workflow-delete-cancel','Cancel'),control('workflow-delete-confirm','Delete')];controls+=actions
            confirmation=dict(control('workflow-delete-confirmation','Permanently delete this workflow? '+D.WARNING),actions=copy.deepcopy(actions))
        feedback=[control('status',text) for text in ([notice] if notice else [])+(outcomes or [])]
        fields={k:dict(value=v,readonly=confirming or busy or sealed,start=len(v.encode('utf-16-le'))//2,end=len(v.encode('utf-16-le'))//2)
                for k,v in [('title',title),('description',description)]} if route=='editor' else None
        native=dict(ok=True,viewport=viewport,noOverflow=True,present=route!='about',scope=dict(visit='10',document='11',revision=str(revision)) if route=='editor' else None,
                    originalNodes=True if route=='editor' else None,originalDetached=retired,fields=fields,composing=False,
                    leasePresent=confirming or busy,confirmation=confirmation,controls=controls,feedback=feedback)
        row=dict(stage=stage,model=model,geometry=dict(copy.deepcopy(native),pngDimensions=[1280,800]),
                 proof=dict(delete=product,server=full_server(case,stage),native=native,exercise={}))
        obs=D.observation(row)
        if stage=='cancelled':row['proof']['exercise']=dict(before=copy.deepcopy(D.observation(rows[-1])),after=copy.deepcopy(obs))
        elif stage in ('trigger-ack','preflight-held','mutation-held','checked','checked-again'):
            row['proof']['exercise']=dict(before=copy.deepcopy(obs),after=copy.deepcopy(obs),observedMs=500)
            if stage.startswith('checked'):row['proof']['exercise']['activation']=dict(ok=True,id='workflow-management-check',disabled=True,sameNode=True)
        elif stage=='deleted':row['proof']['exercise']=dict(release=copy.deepcopy(row['proof']['server']['releases'][-1]))
        elif stage=='successor':
            replacement=dict(before=copy.deepcopy(rows[-1]['model']),after=copy.deepcopy(model),previousEpoch=0,currentEpoch=1,foreignDocumentHidden=True,retainedPending=1)
            row['proof']['exercise']=dict(replacement=copy.deepcopy(replacement),before=copy.deepcopy(D.observation(rows[-1])),after=copy.deepcopy(obs))
        elif stage in ('late-refused','late-settled'):
            row['proof']['exercise']=dict(before=copy.deepcopy(D.observation(rows[-1])),after=copy.deepcopy(obs),
                   release=copy.deepcopy(row['proof']['server']['releases'][-1]),successorUnchanged=True)
        rows.append(row)
    return dict(ok=True,fixture=fixture,scenario='editor',deleteCase=case,aboutConfiguration=True,syntheticSession=True,noLiveNetwork=True,
                viewport=viewport,stages=rows,captures={r['stage']:str(CAPTURES/f"{fixture}-{r['stage']}.png") for r in rows},
                final=copy.deepcopy(D.observation(rows[-1])),replacement=replacement)


class CompleteReceiptTests(unittest.TestCase):
    def validate(self,value):
        R['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False,delete_case=value['deleteCase'])

    def test_six_cases_both_widths(self):
        for case in D.STAGES:
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):self.validate(sample(case,narrow))





class FullMutationTests(unittest.TestCase):
    validate = CompleteReceiptTests.validate
    def reject(self,case,stage,mutate):
        value=sample(case)
        row=next(r for r in value['stages'] if r['stage']==stage)
        mutate(row,value)
        with self.assertRaises((RuntimeError,KeyError,ValueError)):self.validate(value)

    def test_cancel_does_not_create_attempt(self):
        self.reject('delete-success','cancelled',lambda r,v:r['proof']['delete'].update(attempts=[attempt_sample(['pending','pending'],False)]))

    def test_cancel_original_native_selection_preserved(self):
        self.reject('delete-success','cancelled',lambda r,v:r['proof']['exercise']['after']['native']['fields']['title'].update(start=0))

    def test_unknown_not_acknowledged_by_readback(self):
        self.reject('delete-second-unknown','checked',lambda r,v:r['proof']['delete']['attempts'][0]['slots'][1].update(outcome='acknowledged',raw={'ok':True},failure=None))

    def test_malformed_raw_cannot_masquerade_as_rejected(self):
        self.reject('delete-first-rejected','rejected',lambda r,v:r['proof']['delete']['attempts'][0]['slots'][0].update(raw={'error':'not_allowed'}))

    def test_foreign_original_request_owner(self):
        self.reject('delete-success','trigger-ack',lambda r,v:r['proof']['server']['requests'][-1]['owner'].update(account='foreign'))

    def test_held_reply_cannot_be_fabricated_returned(self):
        self.reject('delete-success','trigger-ack',lambda r,v:r['proof']['server']['settlements'].append({'serial':25,'returned':True}))

    def test_retired_owner_cannot_remain_current(self):
        self.reject('delete-mutation-retirement','successor',lambda r,v:r['proof']['delete']['attempts'][0].update(current=True))

    def test_late_preflight_cannot_dispatch(self):
        self.reject('delete-preflight-retirement','late-refused',lambda r,v:r['model'].update(writes=1))

    def test_late_successor_native_change_refused(self):
        self.reject('delete-mutation-retirement','late-settled',lambda r,v:r['proof']['exercise']['after']['native'].update(originalDetached=False))

    def test_no_replay_requires_actual_original_disabled_check(self):
        self.reject('delete-first-unknown','checked',lambda r,v:r['proof']['exercise']['activation'].update(sameNode=False))

    def test_warning_and_both_actions_must_remain_visible(self):
        for mutate in [lambda r,v:r['proof']['native']['confirmation'].update(visible=False),
                       lambda r,v:r['proof']['native']['confirmation']['actions'][0].update(disabled=True),
                       lambda r,v:r['proof']['native']['confirmation'].update(text='Delete?')]:
            self.reject('delete-success','confirmation',mutate)

    def test_original_node_buffer_and_scope_strict(self):
        for mutate in [lambda r,v:r['proof']['native'].update(originalNodes=False),
                       lambda r,v:r['proof']['native']['fields']['description'].update(value='lost'),
                       lambda r,v:r['proof']['native']['scope'].update(document='other')]:
            self.reject('delete-success','draft',mutate)

    def test_exact_read_sequence_and_body(self):
        for mutate in [lambda r,v:r['proof']['server']['requests'][0].update(method='client.init'),
                       lambda r,v:r['proof']['server']['requests'][1]['args'].update(include_relevant_onboarding='true')]:
            self.reject('delete-success','editor',mutate)

    def test_boolean_counts_and_missing_capture_refused(self):
        self.reject('delete-success','draft',lambda r,v:r['model'].update(writes=False))
        self.reject('delete-success','draft',lambda r,v:v['captures'].pop('draft'))

    def test_wrong_case_unsolicited_confirmation_or_changed_final(self):
        for mutate in [lambda r,v:v.update(deleteCase='delete-first-unknown'),
                       lambda r,v:r['proof']['delete'].update(confirmation={'id':1}),
                       lambda r,v:v['final']['model'].update(writes=9)]:
            self.reject('delete-success','managed-empty',mutate)

    def test_hidden_clipped_action_geometry_refused(self):
        for mutate in [lambda r,v:r['proof']['native']['controls'][0].update(hit=False),
                       lambda r,v:r['proof']['native']['controls'][0]['rect'].update(bottom=900,height=890),
                       lambda r,v:r['proof']['native']['controls'][0]['rect'].update(width=float('nan'))]:
            self.reject('delete-success','confirmation',mutate)

    def test_closed_log_case_and_edges_remain_owned(self):
        import json
        import tempfile
        value=sample('delete-success');ready=R['ready_tuple'](value['fixture'],'a'*32,123)
        value['readiness']=ready
        edges=[dict(stage=stage,edge=edge) for stage in D.STAGES['delete-success'] for edge in ('before','after')]
        with tempfile.TemporaryDirectory() as directory:
            log=Path(directory)/'native.log'
            records={R['WAITING']:[ready],R['EDGE']:edges,R['PREFIX']:[value]}
            log.write_text(''.join(prefix+json.dumps(row)+'\n' for prefix,rows in records.items() for row in rows))
            class Native:
                def receipts(self,prefix):return records[prefix]
            native=Native();native.log=log
            self.assertEqual(R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,delete_case='delete-success'),value)
            with self.assertRaises(RuntimeError):R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,delete_case='delete-first-unknown')
            records[R['EDGE']]=edges[:-1]
            with self.assertRaises(RuntimeError):R['validate_log'](native,value['fixture'],CAPTURES,platform='win32',png=False,delete_case='delete-success')


if __name__ == '__main__':
    unittest.main()
