"""Tampered independent run-access observations, never native acceptance."""
import copy
import unittest
import workflow_management_run_access as U
import workflow_management_run_access_picker as P
from test_workflow_management_permissions import R, control, settings
from test_workflow_management_run_access_ledger import sample


def attempt(case,stage='checked'):
    state=U.stage_state(case,stage);source,_=sample(case,stage)
    changed=U.combined(case);phase=state['phase']
    result=U.response_for(case,U.RUN_METHOD) if phase!='in-flight' else dict(raw=None,error=None)
    group=dict(complete=state['complete'],primary=dict(method=U.RUN_METHOD,arguments=U.arguments(case,U.RUN_METHOD),
               outcome=phase,raw=result['raw'],failure='delivery unconfirmed' if phase=='unknown' else None),followup=None)
    if phase=='acknowledged':
        sent=state['follow'];success=sent and case!='run-followup-failure'
        group['followup']=dict(phase='Complete' if sent else 'Restriction',fullySucceeded=success,
            restriction=copy.deepcopy(U.RESTRICTION) if success else None,slots=[dict(method=U.RESTRICTION_METHOD,
            arguments={'workflow_id':'WF_REVIEW'},outcome='acknowledged' if success else 'not-sent' if sent else 'pending',
            raw=copy.deepcopy(U.RESTRICTION) if success else None,failure='offline before dispatch' if sent and not success else None)])
    value=dict(id=1,visit=2,document=3,revision=4,title=U.TITLE,description=U.DESCRIPTION,workflow='WF_REVIEW',
        originalCopy=copy.deepcopy(U.COPY),copyChanged=changed,desired='everyone' if changed else 'collaborators',
        finished=state['finished'],complete=state['complete'],resolved=state['resolved'],noSend=False,
        primarySucceeded=changed,current=not state['finished'] and not state['retired'],runGroup=group,
        slots=[dict(index=0,prerequisite=None,method=U.COPY_METHOD,
            arguments=dict(workflow_id='WF_REVIEW',permission_type='everyone' if changed else 'collaborators'),
            outcome='acknowledged' if changed else 'pending',failure=None,
            raw=U.response_for(case,U.COPY_METHOD)['raw'] if changed else None)],readback=None)
    if state['checked']:value['readback']=dict({k:copy.deepcopy(v) for k,v in source['current'].items() if k!='restriction'},matches=case!='run-rejected')
    return value,state,source['current']


class RunAccessTests(unittest.TestCase):
    def test_picker_requires_original_boot_canonical_rows_and_exact_wire_pages(self):
        self_user=dict(id='UWORKFLOW',team_id='TWORKFLOW')
        seed=dict(boot=dict(ok=True,team=P.TEAMS[0],self=self_user,users=[P.PERSON],channels=[P.CHANNEL]),
                  init=dict(ok=True,team=P.TEAMS[0],self=self_user,workspaces=P.TEAMS,
                            onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123),
                  canonicalTeams={t['id']:t for t in P.TEAMS})
        rows=[]
        for serial,kind in enumerate(('Members','Channels','InternalTeams','AvailableExternal','ExternalTeams'),1):
            query=copy.deepcopy(P.QUERIES[kind]);body=copy.deepcopy(query)
            if kind=='InternalTeams':body=[[k,str(v)] for k,v in sorted(query.items())]+[['_x_reason','fetchAndUpsertTeamsWithQuery']]
            rows.append((dict(serial=serial,phase='picker',kind=kind,method=P.METHODS[kind],query=query,body=body),
                         dict(serial=serial,result=dict(raw=copy.deepcopy(P.PAGES[kind]),error=None))))
        kinds=[r['kind'] for r,_ in rows]
        P.picker_proof(rows,seed,kinds)
        changes=[lambda r,s:r[0][0]['body'].update(filter='NOT bots'),
                 lambda r,s:r[1][1]['result']['raw'].update(member_channels=[]),
                 lambda r,s:r[2][0].update(method='search.all'),
                 lambda r,s:r[3][0]['query'].update(count=24),
                 lambda r,s:r[4][1]['result']['raw'].update(next_marker='unread-page'),
                 lambda r,s:s['canonicalTeams'].pop('TRUNEXTER')]
        for change in changes:
            raw,original=copy.deepcopy(rows),copy.deepcopy(seed);change(raw,original)
            with self.subTest(change=changes.index(change)),self.assertRaises(RuntimeError):P.picker_proof(raw,original,kinds)

    def test_original_independent_outcomes_all_terminal_cases(self):
        for case in U.STAGES:
            stages=('held','successor','late-settled') if case=='run-retirement' else ('checked','checked-again') if case in ('run-rejected','run-unknown','run-followup-failure') else ('acknowledged','checked','reopened')
            for stage in stages:
                with self.subTest(case=case,stage=stage):
                    value,state,current=attempt(case,stage)
                    U.attempt_proof(value,case,state,current)

    def test_unknown_and_failed_followup_do_not_unlock_matching_readback(self):
        for case in ('run-unknown','run-followup-failure'):
            for field in ('complete','resolved'):
                value,state,current=attempt(case);value[field]=True
                with self.subTest(case=case,field=field),self.assertRaises(RuntimeError):U.attempt_proof(value,case,state,current)

    def test_primary_ack_cannot_hide_missing_target_or_independent_copy_failure(self):
        changes=[lambda v:v['runGroup']['primary']['raw'].update(team_ids=[]),
                 lambda v:v['runGroup']['primary']['raw'].pop('org_ids'),
                 lambda v:v['runGroup']['followup'].update(restriction=None),
                 lambda v:v['slots'][0].update(outcome='unknown',raw=None),
                 lambda v:v['runGroup'].update(complete=False),
                 lambda v:v['readback']['run'].update(user_ids=[])]
        for change in changes:
            value,state,current=attempt('run-named-combined');change(value)
            with self.subTest(change=changes.index(change)),self.assertRaises(RuntimeError):U.attempt_proof(value,'run-named-combined',state,current)

    def test_raw_late_ack_does_not_authorize_successor_followup(self):
        value,state,current=attempt('run-retirement','late-settled')
        value['runGroup']['followup']['slots'][0]['outcome']='acknowledged'
        with self.assertRaises(RuntimeError):U.attempt_proof(value,'run-retirement',state,current)

    def test_run_control_options_and_original_epoch(self):
        c=dict(selected='collaborators',runDraft=dict(epoch=5,pending=False,notice=None,mode='app_collaborators',
            ids=[],results=[],externalResults=[],externalEnabled=False,query='',externalQuery='',
            scope=dict(currentTeam='TWORKFLOW',topLevel='ERUNORG01',unified=False,collaborators=['UWORKFLOW'])))
        s=settings('collaborators');s.update(epoch='5',originalControls=True,external=None,query=None,externalQuery=None,selected=[],results=[],
            runLabel=control('','Who can find and use this workflow?'),run=dict(control('workflow-permissions-run'),value='app_collaborators',options=[
                dict(value='everyone',text='Everyone internal in Home fixture',disabled=False),
                dict(value='named_entities',text='Workflow managers, plus specific people, channels, and workspaces',disabled=False),
                dict(value='app_collaborators',text='Workflow managers only',disabled=False)]))
        n=dict(settings=s);controls={v['id']:v for v in s['actions']}
        U.settings_native(n,controls,c,[1360,900],R['rectangle'])
        for key,value in [('epoch','6'),('originalControls',False),('selected',[dict(target='UFOREIGN')])]:
            bad=copy.deepcopy(n);bad['settings'][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):U.settings_native(bad,controls,c,[1360,900],R['rectangle'])

    def test_runner_rejects_conflicts_and_foreign_case_receipts(self):
        for flags in [dict(run_access_case='run-success',permissions_case='permissions-success'),
                      dict(run_access_case='run-success',delete_case='delete-success')]:
            with self.subTest(flags=flags),self.assertRaises(RuntimeError):R['validate']('workflow-editor',{},None,png=False,**flags)
        for receipt,flags in [(dict(runAccessCase='run-success'),{}),
                              (dict(permissionsCase='permissions-success'),dict(run_access_case='run-success'))]:
            with self.subTest(receipt=receipt),self.assertRaises(RuntimeError):R['validate']('workflow-editor',receipt,None,png=False,**flags)


if __name__=='__main__':unittest.main()
