"""Synthetic ledger mutations; these tests are not native acceptance."""
import copy
import unittest
import workflow_management_managers_ledger as M


def read_args(method, gallery=False):
    if method=='experiments.getByUser':return {'ssb_platform':'windows','ssb_release_channel':'prod'}
    if method=='client.init':return {'include_relevant_onboarding':'false','_x_reason':'native-workflow-capabilities'}
    if method=='features.access.policies.list':return {'_x_reason':'native-workflow-capabilities'}
    if method=='client.extras':return {'extras':'["team_prefs"]','_x_reason':'native-workflow-capabilities'}
    if method=='functions.workflows.list':return {'filter_options':'{"name":""}','limit':'50',
        'sort_options':'{"column":"date_updated","direction":"desc"}','workflow_builder_only':'true','_x_reason':'automations_gallery'}
    if method=='workflows.triggers.list':return {'app_ids':'A_HOME','_x_reason':'automations_gallery'} if gallery else {'app_id':'A_HOME','_x_reason':'load-workflow-editor'}
    key='trigger_id' if method in ('workflows.triggers.info','workflows.triggers.permissions.list') else 'workflow_id'
    return {key:'TR_REVIEW' if key=='trigger_id' else 'WF_REVIEW','_x_reason':'load-workflow-editor'}


def seed(case):
    self_user={'id':'UWORKFLOW','team_id':'TWORKFLOW','profile':{}}
    return dict(boot=dict(ok=True,team=copy.deepcopy(M.TEAM),self=self_user,
                        users=[self_user,copy.deepcopy(M.BOT)]+([] if case=='managers-cold' else [M.person(M.OLD,'Original Manager')]),
                        channels=[dict(id='DORIGINAL',user='UHOME0001')],prefs={}),
                init=dict(ok=True,team=copy.deepcopy(M.TEAM),self=copy.deepcopy(self_user),
                          workspaces=[copy.deepcopy(M.TEAM)],onboarding={'user_data':{'onboarding_complete':True}},user_date_created=123),
                canonicalTeams={'TWORKFLOW':copy.deepcopy(M.TEAM)})


def sample(case='managers-success',stage='checked',reverse=False):
    state=M.stage_state(case,stage)
    v=dict(owner=copy.deepcopy(M.OWNER),ownerCurrent=not state['retired'],original=M.original(case),
           current=M.original(case),seed=seed(case),requests=[],responses=[],settlements=[],releases=[],
           held=None,networkCalls=0,completed=0,cancelled=0)
    n=len(state['initial'])
    sequence=[('read',m) for m in state['reads'][:n]]
    sequence += [('picker',k) for k in state['kinds']]
    if state['sent']:sequence += [('read',m) for m in state['preflight']]
    methods=list(state['methods'])
    if reverse:methods=sorted(methods,key=lambda m:0 if m==M.RUN_METHOD else 1 if m==M.COPY_METHOD else 2 if m==M.ADD else 3)
    sequence += [('mutation',m) for m in methods]
    if state['follow']:sequence += [('followup',M.RESTRICTION_METHOD)]
    sequence += [('read',m) for m in state['reads'][n+(len(state['preflight']) if state['sent'] else 0):]]
    picker_index=0
    for serial,(phase,method) in enumerate(sequence,1):
        witness=dict(M.OWNER,visit=7,document=8,revision=9)
        row=dict(serial=serial,phase=phase,method=method,owner=witness,retrySafe=False,applied=False)
        if phase=='picker':
            kind=method
            if kind=='Discovery':
                row.update(phase='read',method=M.DISCOVERY_METHOD,
                           fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']])
                row['args']=dict(row['fields']);result=dict(raw=copy.deepcopy(M.DISCOVERY),error=None)
            else:
                cold=kind=='MemberInfo'
                row.update(kind=kind,method='users/info' if cold else 'users/search',
                           body=copy.deepcopy(M.INFO_BODY if cold else M.QUERY),query=None if cold else copy.deepcopy(M.QUERY))
                raw=dict(ok=True,results=[M.person(M.COLD,'Cold Manager')],can_interact={M.COLD:True},next_marker='') if cold else dict(ok=True,results=M.query_rows(),next_marker='')
                result=dict(raw=None,error='offline') if case=='managers-cold' and picker_index==1 else dict(raw=raw,error=None)
            picker_index+=1
        else:
            gallery=serial>1 and sequence[serial-2][1]=='functions.workflows.list'
            row['fields']=M.wire_fields(case,method) if phase in ('mutation','followup') else list(map(list,read_args(method,gallery).items()))
            row['args']=dict(row['fields'])
            if phase in ('mutation','followup'):
                row['applied']=M.apply(v['current'],case,method)
                result=M.mutation_result(case,method,v['current'])
            else:result=M.read_result(method,v['current'])
        v['requests'].append(row);v['responses'].append(dict(serial=serial,result=result))
    held_method={'add':M.ADD,'run':M.RUN_METHOD,'picker':'users/info'}.get(state['held_kind'])
    if held_method:
        v['held']=[r['serial'] for r in v['requests'] if r['method']==held_method][-1]
    release=None
    if state['released']:
        method='users/info' if case=='managers-cold' else M.ADD if case=='managers-retirement' else M.RUN_METHOD
        release=[r['serial'] for r in v['requests'] if r['method']==method][-1]
    order=[r['serial'] for r in v['requests'] if r['serial'] not in (v['held'],release)]
    if release:
        after=next((r['serial'] for r in v['requests'] if r['serial']>release and (r['phase']=='followup' or r['phase']=='read')),len(sequence)+1)
        index=next((i for i,n in enumerate(order) if n>=after),len(order));order.insert(index,release)
        v['releases']=[dict(serial=release,receiverRetired=False,delivered=True,response=copy.deepcopy(v['responses'][release-1]))]
    v['settlements']=[dict(serial=n,returned=True) for n in order];v['completed']=len(order)
    return v


class ManagersLedgerTests(unittest.TestCase):
    def check(self,v,case='managers-success',stage='checked'):
        return M.server_proof(v,case,stage)

    def reject(self,v,case='managers-success',stage='checked'):
        with self.assertRaises(RuntimeError):self.check(v,case,stage)

    def test_every_authored_case_stage_and_exact_final_totals(self):
        totals={'managers-success':(50,2),'managers-combined':(49,4),'managers-add-rejected':(47,2),
                'managers-add-unknown':(47,2),'managers-self-success':(25,2),'managers-self-rejected-held':(26,3),
                'managers-cold':(28,0),'managers-retirement':(25,1),'managers-no-trigger':(50,2)}
        for case,stages in M.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):
                    proof=self.check(sample(case,stage),case,stage)
            self.assertEqual((proof['reads'],proof['writes']),totals[case])

    def test_independent_groups_need_no_fixed_cross_group_order(self):
        for case,stage in [('managers-combined','checked'),('managers-add-rejected','checked'),
                           ('managers-self-rejected-held','departed-held'),('managers-self-rejected-held','late-settled')]:
            self.check(sample(case,stage,reverse=True),case,stage)

    def test_exact_wire_identity_and_unknown_fields_are_preserved(self):
        mutations=[
            lambda r:r['fields'].append(['unexpected','true']),
            lambda r:r['fields'].__setitem__(0,['collaborator_ids','UWRONG001']),
            lambda r:r['fields'].__setitem__(-1,['_x_reason','other']),
            lambda r:r['owner'].update(account='replacement'),
            lambda r:r.update(retrySafe=True),lambda r:r.update(applied=False),
        ]
        for change in mutations:
            v=sample();r=next(r for r in v['requests'] if r['method']==M.ADD);change(r);self.reject(v)

    def test_opaque_ack_and_full_workflow_cannot_be_replaced_by_ok_boolean(self):
        for change in [lambda r:r.pop('opaque_receipt'),lambda r:r.pop('available_data'),
                       lambda r:r['expression_data'].update(extra=True),
                       lambda r:r['decorated_workflow'].update(retained_unknown=0),
                       lambda r:r['decorated_workflow'].update(collaborators=['UWORKFLOW',M.NEW])]:
            v=sample();r=next(r for r in v['requests'] if r['method']==M.ADD)
            change(v['responses'][r['serial']-1]['result']['raw']);self.reject(v)

    def test_add_before_remove_and_no_replay(self):
        v=sample();adds=next(r for r in v['requests'] if r['method']==M.ADD);remove=next(r for r in v['requests'] if r['method']==M.REMOVE)
        adds['method'],remove['method']=remove['method'],adds['method'];self.reject(v)
        v=sample();v['requests'].append(copy.deepcopy(v['requests'][-1]));self.reject(v)

    def test_rejected_add_does_not_apply_or_authorize_remove(self):
        case='managers-add-rejected';v=sample(case);r=next(r for r in v['requests'] if r['method']==M.ADD)
        self.assertFalse(r['applied']);self.assertNotIn(M.REMOVE,[r['method'] for r in v['requests']])
        r['applied']=True;self.reject(v,case)

    def test_lost_applied_add_keeps_raw_unknown_not_later_readback_ack(self):
        case='managers-add-unknown';v=sample(case);r=next(r for r in v['requests'] if r['method']==M.ADD)
        self.assertTrue(r['applied']);self.assertEqual(v['current']['workflow']['collaborators'],['UWORKFLOW',M.OLD,M.NEW])
        v['responses'][r['serial']-1]['result']=M.response_for('managers-success',M.ADD);self.reject(v,case)

    def test_original_settings_and_application_cannot_drift(self):
        for key in ('workflow','trigger','copy','run','external','restriction'):
            v=sample();v['current'][key]['extra']='unrequested';self.reject(v)
        v=sample();v['original']['workflow']['collaborators'].append(M.NEW);self.reject(v)

    def test_noop_cancel_and_speed_bump_dispatch_nothing(self):
        for stage in ('permissions','unchanged','empty','selected','cancelled'):
            self.reject(sample(),stage=stage)
        self.reject(sample('managers-self-success','settled'),'managers-self-success','confirmation')

    def test_readback_cannot_be_reused_as_fresh_preflight(self):
        v=sample();r=next(r for r in v['requests'] if r['method']==M.ADD);v['requests'][r['serial']-2]['method']='client.init';self.reject(v)
        v=sample();v['responses'][0]['result']['raw']['assignments']={};self.reject(v)

    def test_no_trigger_never_creates_fake_trigger_or_run_read(self):
        case='managers-no-trigger';v=sample(case);self.assertIsNone(v['original']['trigger']);self.assertIsNone(v['current']['run'])
        self.assertNotIn('workflows.triggers.info',[r['method'] for r in v['requests']])
        v['current']['trigger']=copy.deepcopy(M.TRIGGER);self.reject(v,case)

    def test_no_trigger_uses_actual_managed_row_and_empty_trigger_join(self):
        case='managers-no-trigger';v=sample(case,'reopened')
        joins=[r for r in v['requests'] if r['method']=='workflows.triggers.list' and r['args']['_x_reason']=='automations_gallery']
        self.assertEqual(len(joins),2)
        for r in joins:
            self.assertEqual(v['requests'][r['serial']-2]['method'],'functions.workflows.list')
            self.assertEqual(v['responses'][r['serial']-1]['result']['raw'],dict(ok=True,triggers=[]))
        wrong=copy.deepcopy(v);r=next(r for r in wrong['requests'] if r['method']=='functions.workflows.list')
        wrong['responses'][r['serial']-1]['result']['raw']['workflows'][0]['collaborators']=['UWORKFLOW']
        self.reject(wrong,case,'reopened')
        for key,value in [('app_ids',''),('_x_reason','load-workflow-editor')]:
            wrong=copy.deepcopy(v);r=wrong['requests'][joins[0]['serial']-1]
            r['args'][key]=value;r['fields']=[[k,r['args'][k]] for k,_ in r['fields']]
            self.reject(wrong,case,'reopened')
        wrong=copy.deepcopy(v);wrong['responses'][joins[0]['serial']-1]['result']['raw']['triggers']=[copy.deepcopy(M.TRIGGER)]
        self.reject(wrong,case,'reopened')

    def test_cold_reads_are_original_identity_not_query_search(self):
        case='managers-cold';v=sample(case,'hydrated');self.check(v,case,'hydrated')
        rows=[r for r in v['requests'] if r['phase']=='picker'];self.assertEqual(len(rows),2)
        for key,value in [('kind','Members'),('query',M.INFO_BODY),('body',{'ids':['UOTHER001']})]:
            wrong=copy.deepcopy(v);r=next(r for r in wrong['requests'] if r['phase']=='picker');r[key]=value;self.reject(wrong,case,'hydrated')

    def test_cold_missing_input_is_not_preseeded_or_offline_response_fabricated(self):
        case='managers-cold';v=sample(case,'missing-offline');v['seed']['boot']['users'].append(M.person(M.COLD,'Cold Manager'));self.reject(v,case,'missing-offline')
        v=sample(case,'missing-offline');v['responses'][-1]['result']=dict(raw=dict(ok=True,results=[]),error=None);self.reject(v,case,'missing-offline')

    def test_picker_full_original_ineligible_rows_and_scope_preserved(self):
        for change in [lambda r:r['results'].pop(),lambda r:r['results'][1].update(team_id='TWORKFLOW'),
                       lambda r:r['results'][3].update(is_bot=False)]:
            v=sample();r=next(r for r in v['requests'] if r.get('kind')=='Members');change(v['responses'][r['serial']-1]['result']['raw']);self.reject(v)
        v=sample();r=next(r for r in v['requests'] if r.get('kind')=='Members');r['body']['count']=100;self.reject(v)

    def test_self_departure_preserves_pending_run_and_late_followup(self):
        case='managers-self-rejected-held';v=sample(case,'departed-held',reverse=True)
        held=v['held'];self.assertEqual(v['requests'][held-1]['method'],M.RUN_METHOD)
        self.assertNotIn(M.REMOVE,[r['method'] for r in v['requests']]);self.assertFalse(any(r['phase']=='followup' for r in v['requests']))
        late=sample(case,'late-settled',reverse=True);self.check(late,case,'late-settled')
        late['releases'][0]['response']['result']['raw']['opaque_receipt']['preserve'][1]='copy';self.reject(late,case,'late-settled')

    def test_retirement_cannot_cancel_original_receiver_or_dispatch_remove(self):
        case='managers-retirement';v=sample(case,'late-settled')
        self.assertFalse(v['ownerCurrent']);self.assertEqual([r['method'] for r in v['requests'] if r['phase']=='mutation'],[M.ADD])
        for key,value in [('delivered',False),('receiverRetired',True),('serial',1)]:
            wrong=copy.deepcopy(v);wrong['releases'][0][key]=value;self.reject(wrong,case,'late-settled')

    def test_lifetime_counts_types_and_settlement_order_are_strict(self):
        for change in [lambda v:v.update(networkCalls=1),lambda v:v.update(cancelled=1),
                       lambda v:v.update(completed=True),lambda v:v['settlements'].reverse(),
                       lambda v:v['settlements'][0].update(returned=False),lambda v:v['requests'][0].update(serial=True)]:
            v=sample();change(v);self.reject(v)

    def test_public_helper_desired_sets_and_source_arguments(self):
        self.assertEqual(M.arguments('managers-combined',M.RUN_METHOD),dict(trigger_id='TR_REVIEW',permission_type='named_entities',user_ids=['UWORKFLOW',M.NEW,M.OLD]))
        self.assertEqual(M.arguments('managers-self-success',M.REMOVE),dict(workflow_id='WF_REVIEW',collaborator_ids=['UWORKFLOW']))
        self.assertEqual(M.response_for('managers-success',M.ADD)['raw']['decorated_workflow']['collaborators'],['UWORKFLOW',M.OLD,M.NEW])
        self.assertEqual(M.response_for('managers-success',M.REMOVE)['raw']['decorated_workflow']['collaborators'],['UWORKFLOW',M.NEW])


if __name__=='__main__':unittest.main()
