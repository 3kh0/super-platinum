"""Synthetic receipt mutations; these tests are not native acceptance."""
import copy
import unittest
import workflow_management_run_access_ledger as U
from test_workflow_management_permissions import read_args


def sample(case='run-success', stage='checked', reverse=False):
    state=U.stage_state(case,stage)
    owner=dict(account='synthetic-workflow-run-access',epoch=0,team='TWORKFLOW',user='UWORKFLOW')
    v=dict(owner=owner,ownerCurrent=not state['retired'],original=copy.deepcopy(U.ORIGINAL),
           current=copy.deepcopy(U.ORIGINAL),requests=[],responses=[],settlements=[],releases=[],
           held=None,networkCalls=0,completed=0,cancelled=0,seed=None)
    groups=([U.RUN_METHOD,U.COPY_METHOD] if reverse else [U.COPY_METHOD,U.RUN_METHOD]) if U.combined(case) else [U.RUN_METHOD]
    methods=state['reads'][:23]+(groups if state['sent'] else [])+([U.RESTRICTION_METHOD] if state['follow'] else [])+state['reads'][23:]
    for serial,method in enumerate(methods,1):
        mutation=method in (U.RUN_METHOD,U.COPY_METHOD)
        follow=state['follow'] and serial==23+len(groups)+1
        fields=U.wire_fields(case,method) if mutation or follow else list(map(list,read_args(method,serial).items()))
        phase='mutation' if mutation else 'followup' if follow else 'read'
        applied=mutation and not(method==U.RUN_METHOD and case=='run-rejected')
        if applied:
            v['current']['run' if method==U.RUN_METHOD else 'copy']=dict(ok=True,**(U.desired(case) if method==U.RUN_METHOD else {'permission_type':'everyone'}))
        result=U.response_for(case,method) if mutation or follow else U.read_result(method,v['current'])
        v['requests'].append(dict(serial=serial,method=method,phase=phase,fields=fields,args=dict(fields),
            retrySafe=False,applied=applied,owner=dict(owner,visit=7,document=8,revision=9)))
        v['responses'].append(dict(serial=serial,result=result))
        if state['held'] and method==U.RUN_METHOD:v['held']=serial
        else:v['settlements'].append(dict(serial=serial,returned=True))
    v['completed']=len(v['settlements'])
    if case=='run-retirement' and state['finished']:
        serial=next(r['serial'] for r in v['requests'] if r['method']==U.RUN_METHOD)
        v['releases']=[dict(serial=serial,receiverRetired=False,delivered=True,response=copy.deepcopy(v['responses'][serial-1]))]
    return v,state


def check(v,case,state):
    def no_picker(rows,seed):
        U.require(rows==[] and seed is None,'Synthetic test has unexpected picker data')
    return U.server_proof(v,case,state,picker_proof=no_picker)


class LedgerTests(unittest.TestCase):
    def test_all_six_case_stage_contracts(self):
        for case,stages in U.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):
                    v,state=sample(case,stage)
                    check(v,case,state)

    def test_changed_groups_have_no_cross_group_order_dependency(self):
        for case in ('run-named-combined','run-rejected'):
            for reverse in (False,True):
                v,state=sample(case,reverse=reverse)
                check(v,case,state)

    def test_lost_reply_is_applied_but_never_fabricated_ack(self):
        v,state=sample('run-unknown')
        request=next(r for r in v['requests'] if r['phase']=='mutation')
        self.assertTrue(request['applied'])
        v['responses'][request['serial']-1]['result']=dict(raw=dict(ok=True,**U.desired('run-unknown')),error=None)
        with self.assertRaises(RuntimeError):check(v,'run-unknown',state)

    def test_exact_wire_and_raw_custody_mutations_are_rejected(self):
        changes=[
            lambda v:v['requests'][23].update(retrySafe=True),
            lambda v:v['requests'][23].update(applied=False),
            lambda v:v['requests'][23]['fields'].append(['org_ids','EFOREIGN1']),
            lambda v:v['requests'][23]['owner'].update(account='replacement'),
            lambda v:v['responses'][23]['result']['raw'].update(team_ids=['TFOREIGN1']),
            lambda v:v['responses'][23]['result']['raw'].pop('org_ids'),
            lambda v:v['responses'][23]['result']['raw'].pop('opaque_receipt'),
            lambda v:v['current']['workflow'].update(title='Saved metadata accidentally'),
            lambda v:v['current']['external']['trippable_by_slack_connected_teams'].update(allowed_teams=[]),
            lambda v:v.update(cancelled=1),
            lambda v:v.update(completed=True),
            lambda v:v['settlements'].pop(),
            lambda v:v['requests'][0].update(serial=True),
            lambda v:v.update(networkCalls=1),
        ]
        for mutate in changes:
            v,state=sample()
            mutate(v)
            with self.subTest(change=changes.index(mutate)),self.assertRaises(RuntimeError):check(v,'run-success',state)

    def test_noop_and_cancel_refuse_any_dispatched_group(self):
        sent,_=sample(stage='acknowledged')
        for stage in ('permissions','unchanged','selected','cancelled'):
            with self.subTest(stage=stage),self.assertRaises(RuntimeError):
                check(sent,'run-success',U.stage_state('run-success',stage))

    def test_followup_cannot_be_forged_from_later_readback(self):
        v,state=sample('run-followup-failure')
        follow=next(r for r in v['requests'] if r['phase']=='followup')
        v['responses'][follow['serial']-1]['result']=dict(raw=copy.deepcopy(U.RESTRICTION),error=None)
        with self.assertRaises(RuntimeError):check(v,'run-followup-failure',state)

    def test_original_retirement_release_keeps_original_response(self):
        v,state=sample('run-retirement','late-settled')
        for key,value in [('receiverRetired',True),('delivered',False),('serial',25)]:
            changed=copy.deepcopy(v);changed['releases'][0][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):check(changed,'run-retirement',state)


if __name__=='__main__':unittest.main()
