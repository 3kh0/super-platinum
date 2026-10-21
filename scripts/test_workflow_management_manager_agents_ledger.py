"""Closed synthetic wire tests, not executed native acceptance."""
import copy
import unittest
import workflow_management_manager_agents_ledger as A
import workflow_management_managers_ledger as M
from test_workflow_management_managers_ledger import read_args, seed


def sample(case=A.SUCCESS, stage='checked', run_first=False):
    state=A.stage_state(case,stage)
    v=dict(owner=copy.deepcopy(A.OWNER),ownerCurrent=not state['retired'],original=A.original(case),
           current=A.original(case),seed=seed('managers-success'),requests=[],responses=[],settlements=[],
           releases=[],held=state['held_serial'],networkCalls=0,completed=0,cancelled=0)
    n=len(state['initial'])
    sequence=[('read',m,None) for m in state['reads'][:n]]
    if state['queries']:
        sequence += [('discovery',M.DISCOVERY_METHOD,None),('picker','users/search','manager')]
        sequence += [('agent-fact',m,a) for m,a in state['facts']]
        if len(state['queries'])==2:
            sequence += [('discovery',M.DISCOVERY_METHOD,None),('picker','users/search','ordinary')]
    if state['sent']:sequence += [('read',m,None) for m in state['preflight']]
    methods=[A.RUN_METHOD,A.ADD,A.REMOVE] if state['sent'] and run_first else state['methods']
    sequence += [('mutation',m,None) for m in methods]
    if state['follow']:sequence += [('followup',A.RESTRICTION_METHOD,None)]
    sequence += [('read',m,None) for m in state['reads'][n+(len(state['preflight']) if state['sent'] else 0):]]
    access_index=0
    for serial,(phase,method,arg) in enumerate(sequence,1):
        row=dict(serial=serial,phase=phase,method=method,owner=dict(A.OWNER,visit=7,document=8,revision=9),retrySafe=False,applied=False)
        if phase=='discovery':
            row.update(phase='read',fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']])
            row['args']=dict(row['fields']);result=dict(raw=copy.deepcopy(M.DISCOVERY),error=None)
        elif phase=='picker':
            row.update(kind='Members',body=dict(M.QUERY,query=arg),query=dict(M.QUERY,query=arg))
            result=dict(raw=dict(ok=True,results=A.query_rows(arg),next_marker=''),error=None)
        elif phase=='agent-fact':
            row.update(fields=A.fact_fields(method,arg));row['args']=dict(row['fields'])
            result=A.fact_result(method,arg,v['seed'],case==A.RECOVERY and method=='aiApps.list' and access_index==0)
            if method=='aiApps.list':access_index+=1
        else:
            row['fields']=A.wire_fields(case,method) if phase in ('mutation','followup') else list(map(list,read_args(method).items()))
            row['args']=dict(row['fields'])
            if phase in ('mutation','followup'):
                row['applied']=A.apply(v['current'],case,method);result=A.mutation_result(case,method,v['current'])
            else:result=M.read_result(method,v['current'])
        v['requests'].append(row);v['responses'].append(dict(serial=serial,result=result))
    pending=state['held_serial'] if not state['cancelled_receiver'] else None
    v['settlements']=[dict(serial=r['serial'],returned=not(state['cancelled_receiver'] and r['serial']==18))
                      for r in v['requests'] if r['serial']!=pending]
    if state['released']:
        serial=state['released_serial']
        v['releases']=[dict(serial=serial,receiverRetired=state['cancelled_receiver'],delivered=not state['cancelled_receiver'],response=copy.deepcopy(v['responses'][serial-1]))]
    v.update(completed=len(v['settlements']),cancelled=int(state['cancelled_receiver']))
    return v


class LedgerTests(unittest.TestCase):
    def reject(self,v,case=A.SUCCESS,stage='checked'):
        with self.assertRaises((AssertionError,ValueError,RuntimeError)):
            A.server_proof(v,case,stage)

    def test_every_closed_stage(self):
        totals={A.SUCCESS:(57,3),A.RECOVERY:(26,0),A.QUERY_RETIRE:(20,0),A.ACCOUNT_RETIRE:(18,0)}
        for case,stages in A.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):result=A.server_proof(sample(case,stage),case,stage)
            self.assertEqual((result['reads'],result['writes']),totals[case])

    def test_independent_run_group_can_dispatch_first(self):
        A.server_proof(sample(run_first=True),A.SUCCESS,'checked')

    def test_exact_agent_wire_reason(self):
        v=sample();v['requests'][14]['fields'][-1][1]='invented-reason';v['requests'][14]['args']['_x_reason']='invented-reason';self.reject(v)

    def test_exact_profile_application_identity(self):
        v=sample();v['responses'][18]['result']['raw']['app_profile']['id']='AMANAFYES';self.reject(v)

    def test_wrong_bot_access_is_not_repaired_by_oracle(self):
        v=sample();v['responses'][17]['result']['raw']['apps'][2]['bot_user']['id']='UMANAFBAD';self.reject(v)

    def test_disabled_preference_and_default_remain_original(self):
        for key in ('default','disabled'):
            v=sample();p=v['responses'][15]['result']['raw']['team_prefs']['toplevel_prefs']
            if key=='default':p['default_channel_knowledge_agent']['app_id']='AMANAFNO1'
            else:p['ai_apps_settings']['apps'][0]['is_ai_enabled']=True
            self.reject(v)

    def test_original_opaque_fact_response(self):
        v=sample();v['responses'][17]['result']['raw']['opaque']['preserve'].append('changed');self.reject(v)

    def test_no_unrequested_page(self):
        v=sample();v['responses'][17]['result']['raw']['response_metadata']['next_cursor']='next';self.reject(v)

    def test_recovery_preserves_original_unknown_response(self):
        v=sample(A.RECOVERY,'recovered');del v['responses'][17]['result']['raw']['apps'][0]['isUnknown'];self.reject(v,A.RECOVERY,'recovered')

    def test_recovery_does_not_repeat_canonical_query(self):
        v=sample(A.RECOVERY,'recovered');v['requests'][18]['phase']='picker';self.reject(v,A.RECOVERY,'recovered')

    def test_pending_original_receiver_not_completed(self):
        v=sample(A.RECOVERY,'retry-held');v['settlements'].append(dict(serial=22,returned=True));v['completed']+=1;self.reject(v,A.RECOVERY,'retry-held')

    def test_retired_receiver_records_false_return_before_release(self):
        for case,stage in ((A.QUERY_RETIRE,'superseded'),(A.ACCOUNT_RETIRE,'successor')):
            v=sample(case,stage);v['settlements'][17]['returned']=True;self.reject(v,case,stage)

    def test_late_retired_reply_cannot_claim_delivery(self):
        v=sample(A.QUERY_RETIRE,'late-settled');v['releases'][0]['delivered']=True;self.reject(v,A.QUERY_RETIRE,'late-settled')

    def test_release_is_exact_original_response(self):
        v=sample(A.RECOVERY,'recovered');v['releases'][0]['response']['result']['raw']['apps'].reverse();self.reject(v,A.RECOVERY,'recovered')

    def test_release_not_a_second_settlement(self):
        v=sample(A.ACCOUNT_RETIRE,'late-settled');v['settlements'].append(dict(serial=18,returned=True));v['completed']+=1;self.reject(v,A.ACCOUNT_RETIRE,'late-settled')

    def test_add_before_remove_and_exact_manager_union(self):
        v=sample();rows=[r for r in v['requests'] if r['phase']=='mutation'];rows[0]['method'],rows[1]['method']=rows[1]['method'],rows[0]['method'];self.reject(v)
        v=sample();row=next(r for r in v['requests'] if r['method']==A.RUN_METHOD);row['args']['user_ids']='UWORKFLOW,UMANOLD01,UMANNEW01,UMANAFYES,UMANAFDEF';self.reject(v)

    def test_no_unrelated_server_change(self):
        v=sample();v['current']['copy']['permission_type']='everyone';self.reject(v)

    def test_no_retry_or_foreign_owner(self):
        v=sample();v['requests'][14]['retrySafe']=True;self.reject(v)
        v=sample();v['requests'][14]['owner']['account']='other';self.reject(v)

    def test_facts_do_not_grant_missing_query_candidates(self):
        v=sample();v['responses'][13]['result']['raw']['results'][0]['profile']['api_app_id']='AMANAFYES';self.reject(v)


if __name__=='__main__':unittest.main()
