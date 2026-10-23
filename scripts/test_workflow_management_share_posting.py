"""Portable posting-refresh custody mutations; no process/network/device access."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

import workflow_management_share_posting as P
import test_workflow_management_share_resend as R
import test_workflow_management_first_publish as FIRST

ROOT=Path(__file__).resolve().parents[1]
C=R.C


def sample(case='success'):
    base=R.sample('rejected' if case=='retry-restricted' else 'suppressed')
    resend=base.pop('shareResend'); before=resend['before']
    base.pop('shareResendCase'); base['shareResendCheck']=False
    base.update(sharePostingCheck=True,sharePostingCase=case)
    team=dict(id='TWORKFLOW',name='Home fixture')
    if case!='missing-policy':team['plan']='plus'
    for item in (before,resend.get('held'),resend.get('after')):
        if item is None:continue
        seed=item['server'].setdefault('seed',dict(boot={},init={}))
        seed['boot']['team']=copy.deepcopy(team);seed['init']['team']=copy.deepcopy(team)
        seed['init']['workspaces']=[copy.deepcopy(team)];seed['canonicalTeams']={'TWORKFLOW':copy.deepcopy(team)}
    wire=before['server']['confirmationAdd']['share']
    if case!='retry-restricted': wire.pop('resend')
    else: wire['resend']['case']='posting-restricted'
    raw=copy.deepcopy(P.S.A.CHANNELS[P.S.A.DEST])
    canonical=dict(raw,fromAnotherTeam=False)
    policy=dict(authorityCurrent=True,rawCurrent=True,raw=raw,originalRaw=raw,originalCurrent=True,effectiveFailure=None,
                effective=dict(readOnly=False,threadOnly=False,permitsRootPost=True),canonicalCurrent=True,
                canonical=canonical,typed=dict(canonical,unread_count=None,has_unreads=False))
    before['model']['sharePosting']=copy.deepcopy(policy)
    P.C.operation(before)['postingRefresh']=[]
    wire['posting']=P.empty(case)
    generation=19 if case=='retry-restricted' else 0
    read=dict(serial=1,generation=generation,method='conversations.info',body=[['channel',P.S.A.DEST],['_x_reason',P.REASON]],
              endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=P.S.OWNER)
    posting=P.empty(case)
    posting.update(requests=[read],responses=[dict(serial=1,error='offline') if case=='read-failure' else dict(serial=1,raw=P.response())],held=[1])
    effect=dict(generation=generation,request=dict(method='conversations.info',channel=P.S.A.DEST,_x_reason=P.REASON),
                raw=None,outcome='reading',finished=False,applied=False)
    retry=None
    if case=='retry-restricted':
        held=resend['held']; pending=resend['after']
        for item in (held,pending):
            item['model']['sharePosting']=copy.deepcopy(policy)
            item['server']['confirmationAdd']['share']['posting']=P.empty(case)
            item['server']['confirmationAdd']['share']['resend']['case']='posting-restricted'
            P.C.operation(item)['postingRefresh']=[]
            raw= item['server']['confirmationAdd']['share']['resend']['responses'][0]['raw']
            raw.update(error='restricted_action_read_only_channel',opaque_receipt='original-share-resend-restricted')
            if item is pending:
                attempt=item['server']['confirmationAdd']['share']['resend']['attempts'][0]
                attempt['raw']=copy.deepcopy(raw); attempt['responseText']=json.dumps(raw)
                op=P.C.operation(item)['resends'][0]
                op['raw']=copy.deepcopy(raw);op['attempts']=copy.deepcopy(item['server']['confirmationAdd']['share']['resend']['attempts'])
                row=item['native']['shareResendRow'];row['resend']=None
                item['native']['shareTimeline']['rows']=[{k:copy.deepcopy(v) for k,v in row.items() if k not in ('resend','cancel')}]
        retry=dict(before=copy.deepcopy(before),activation=resend['activation'],held=held,
                   quiet=quiet(held),release=resend['release'],after=None)
    else: pending=before
    pending['server']['confirmationAdd']['share']['posting']=copy.deepcopy(posting)
    P.C.operation(pending)['postingRefresh']=[copy.deepcopy(effect)]
    if retry: retry['after']=copy.deepcopy(pending)
    after=copy.deepcopy(pending)
    def publish(value,newer):
        p=value['model']['sharePosting']; raw=P.response(newer)['channel']
        p.update(raw=raw,originalCurrent=False,effective=dict(readOnly=not newer,threadOnly=newer,permitsRootPost=False))
        if case=='missing-policy':p.update(effective=None,effectiveFailure='Policy(FeatureAccess)')
        for key in ('canonical','typed'):
            for field in ('is_read_only','is_thread_only','properties'):p[key][field]=copy.deepcopy(raw[field])
    newer=None
    observations={'posting-pending':pending}
    if case=='held-newer':
        changed=copy.deepcopy(pending); publish(changed,True)
        ingress=dict(kind='synthetic-source-response-ingress',requestDispatched=False,raw=P.response(True),
                     before=copy.deepcopy(policy),after=copy.deepcopy(changed['model']['sharePosting']))
        changed['server']['confirmationAdd']['share']['posting']['newer']=[ingress]
        newer=dict(before=copy.deepcopy(pending),ingress=ingress,after=changed,quiet=quiet(changed))
        observations['posting-newer']=changed
        after=copy.deepcopy(changed)
    elif case!='read-failure':publish(after,False)
    d=after['server']['confirmationAdd']['share']['posting']
    release=dict(serial=1,receiverRetired=False,delivered=True)
    d.update(held=[],settlements=[dict(serial=1,returned=True)],releases=[release])
    outcome={'read-failure':'failed','held-newer':'retired'}.get(case,'applied')
    P.C.operation(after)['postingRefresh'][0].update(raw=None if case=='read-failure' else P.response(),
                                                   outcome=outcome,finished=True,applied=outcome=='applied')
    observations['posting-settled']=after
    if retry:observations['posting-retry-pending']=retry['held']
    captures={};evidence={}
    for stage,obs in observations.items():
        path=str(C.DIRECTORY/(C.FIXTURE+'-'+stage+'.png'));captures[stage]=path
        evidence[stage]=dict(path=path,pngDimensions=[1942,1256],before=copy.deepcopy(obs),after=copy.deepcopy(obs))
    base['stages'][0].update(model=copy.deepcopy(before['model']),proof={k:copy.deepcopy(before[k]) for k in ('firstPublish','server','native')})
    base['sharePosting']=dict(before=before,retry=retry,pending=pending,pendingQuiet=quiet(pending),newer=newer,
                             release=release,after=after,quiet=quiet(after),captures=captures,captureEvidence=evidence)
    return base


def quiet(value):return dict(observedMs=500,before=copy.deepcopy(value),after=copy.deepcopy(value))
def validate(value, png=False, callback=lambda *a:None):
    return P.validate(value,C.FIXTURE,C.DIRECTORY,C.VIEW,lambda *a:None,callback,png=png)


class Posting(unittest.TestCase):
    def refused(self, change, case='success'):
        v=sample(case);validate(v);change(v)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):validate(v)

    def test_four_distinct_original_outcomes(self):
        for case in P.CASES:
            result=validate(sample(case))
            self.assertEqual(P.C.operation(result)['postingRefresh'][0]['outcome'],
                             {'read-failure':'failed','held-newer':'retired'}.get(case,'applied'))

    def test_each_physical_capture_matches_original_window(self):
        for case in P.CASES:
            seen=[];validate(sample(case),True,lambda p,d:seen.append(d))
            self.assertEqual(seen,[[1942,1256]]*len(P.STAGES[case]))

    def test_foreign_read_owner(self):
        self.refused(lambda v:P.data(v['sharePosting']['after'])['requests'][0]['owner'].update(account='other'))

    def test_request_has_reason_not_member_count(self):
        self.refused(lambda v:P.data(v['sharePosting']['after'])['requests'][0]['body'].append(['include_num_members','false']))

    def test_request_generation_not_successor(self):
        self.refused(lambda v:P.data(v['sharePosting']['after'])['requests'][0].update(generation=99))

    def test_no_duplicate_background_read(self):
        self.refused(lambda v:P.data(v['sharePosting']['after'])['requests'].__imul__(2))

    def test_immutable_original_send_receipt(self):
        self.refused(lambda v:v['sharePosting']['after']['server']['confirmationAdd']['share']['responses'][0].update(raw={'ok':True}))

    def test_failed_refresh_is_not_grant(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting']['effective'].update(readOnly=True),'read-failure')

    def test_stale_response_cannot_overwrite_newer_raw(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(raw=P.response()['channel']),'held-newer')

    def test_stale_response_cannot_claim_applied(self):
        self.refused(lambda v:P.C.operation(v['sharePosting']['after'])['postingRefresh'][0].update(applied=True),'held-newer')

    def test_newer_response_is_explicit_synthetic_ingress_not_http(self):
        self.refused(lambda v:v['sharePosting']['newer']['ingress'].update(requestDispatched=True),'held-newer')

    def test_raw_policy_no_synthetic_defaults(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting']['raw'].update(fromAnotherTeam=False))

    def test_effective_companion_required(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting']['effective'].update(permitsRootPost=True))

    def test_success_cannot_hide_unknown_policy(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(effective=None,effectiveFailure='Policy(FeatureAccess)'))

    def test_missing_policy_cannot_invent_raw_flag_permission(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(
            effective=dict(readOnly=True,threadOnly=False,permitsRootPost=False),effectiveFailure=None),'missing-policy')

    def test_missing_policy_cannot_grant_root_post(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(
            effective=dict(readOnly=False,threadOnly=False,permitsRootPost=True)),'missing-policy')

    def test_missing_policy_error_is_exact_dependency(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(effectiveFailure='Stale'),'missing-policy')

    def test_paid_plan_must_be_original_acquired_seed(self):
        self.refused(lambda v:v['stages'][0]['proof']['server']['seed']['boot']['team'].pop('plan'))

    def test_missing_policy_retains_original_free_shape(self):
        self.refused(lambda v:v['stages'][0]['proof']['server']['seed']['boot']['team'].update(plan='plus'),'missing-policy')

    def test_init_and_boot_plan_custody(self):
        for key in ('team','workspaces'):
            self.refused(lambda v:v['stages'][0]['proof']['server']['seed']['init'].__setitem__(key,None))

    def test_pending_false_flags_need_no_feature_grant(self):
        self.refused(lambda v:v['sharePosting']['pending']['model']['sharePosting'].update(effectiveFailure='Policy(FeatureAccess)'),'missing-policy')

    def test_paid_boot_requires_explicit_opt_in_base_validator(self):
        server=FIRST.server('first-success','about')
        P.F.server_proof('first-success','about',server)
        team=dict(id='TWORKFLOW',name='Home fixture',plan='plus')
        seed=server['seed'];seed['boot']['team']=team;seed['init'].update(team=team,workspaces=[team])
        seed['canonicalTeams']={'TWORKFLOW':team}
        P.F.server_proof('first-success','about',server,posting_case='success')
        with self.assertRaises(RuntimeError):P.F.server_proof('first-success','about',server)
        with self.assertRaises(RuntimeError):P.F.server_proof('first-success','about',server,posting_case='missing-policy')

    def test_canonical_and_typed_both_updated(self):
        for field in ('canonical','typed'):
            self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'][field].pop('is_read_only'))

    def test_unrelated_channel_metadata_unchanged(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting']['canonical'].update(name='foreign'))

    def test_properties_replace_not_merge(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting']['typed']['properties'].update(old=True))

    def test_old_source_must_retire(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['sharePosting'].update(originalCurrent=True))

    def test_failed_row_is_not_removed_by_refresh(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['shareCustody'].update(messages=[]))

    def test_separate_drafts_unchanged(self):
        self.refused(lambda v:v['sharePosting']['after']['model']['shareDrafts'].update(storedMainDraft={'text':''}))

    def test_cancel_only_after_restricted_retry(self):
        self.refused(lambda v:v['sharePosting']['after']['native']['shareResendRow'].update(resend={'text':'Try again'}),'retry-restricted')

    def test_retry_is_explicit_same_rich_identity(self):
        self.refused(lambda v:v['sharePosting']['retry']['held']['server']['confirmationAdd']['share']['resend']['requests'][0]['request']['fields'][3].__setitem__(1,'[]'),'retry-restricted')

    def test_retry_retains_original_unknown(self):
        self.refused(lambda v:P.C.operation(v['sharePosting']['after']).update(outcome='rejected'),'retry-restricted')

    def test_original_error_not_old_public_resend_error(self):
        self.refused(lambda v:v['sharePosting']['after']['server']['confirmationAdd']['share']['resend']['responses'][0]['raw'].update(error='not_allowed'),'retry-restricted')

    def test_pending_controls_absent(self):
        self.refused(lambda v:v['sharePosting']['retry']['held']['native']['shareResendRow'].update(cancel={'text':'Cancel'}),'retry-restricted')

    def test_actual_click_not_fabricated(self):
        self.refused(lambda v:v['sharePosting']['retry']['activation']['event'].update(isTrusted=True),'retry-restricted')

    def test_no_automatic_work_during_quiet(self):
        self.refused(lambda v:v['sharePosting']['quiet']['after']['server'].update(extraRequest=True))

    def test_capture_before_after_exact(self):
        self.refused(lambda v:v['sharePosting']['captureEvidence']['posting-settled']['after']['model'].update(extra=True))

    def test_capture_not_css_dimensions(self):
        self.refused(lambda v:v['sharePosting']['captureEvidence']['posting-settled'].update(pngDimensions=C.VIEW))

    def test_owned_capture_path(self):
        self.refused(lambda v:v['sharePosting']['captureEvidence']['posting-settled'].update(path='elsewhere.png'))

    def test_mode_scope(self):
        for args in [('foreign','rejected',True,False,None),('success','lost-response',True,False,None),
                     ('success','rejected',False,False,None),('success','rejected',True,True,None),
                     ('success','rejected',True,False,'rejected')]:
            with self.assertRaises(RuntimeError):P.mode(*args)

    def test_mode_refuses_before_filesystem(self):
        runner=runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        with patch.object(Path,'mkdir',side_effect=AssertionError('filesystem reached')):
            with self.assertRaises(RuntimeError):
                runner['run_fixture']('workflow-editor',ROOT/'missing.exe',ROOT/'tmp/unused',
                    first_publish_case='first-add-success',share_case='lost-response',share_drafts=True,share_posting_case='success')

    def test_legacy_unrequested_evidence_refused(self):
        with self.assertRaises(RuntimeError):P.identity({'sharePosting':{}},None)

    def test_exact_closed_capture_order_for_each_mode(self):
        runner=runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        fn=runner['validate_log'];receipt=dict(ok=True)
        class Log:
            def __init__(self,edges):self.edges=edges
            def receipts(self,prefix):return [receipt] if prefix==runner['PREFIX'] else self.edges
        with patch.dict(fn.__globals__,validate=lambda *a,**k:None):
            for case in P.CASES:
                initial='lost-response' if case=='retry-restricted' else 'rejected'
                stages=('draft-setup',)+tuple(runner['SHARE_STAGES'][initial])+P.STAGES[case]+('draft-returned',)
                edges=[dict(stage=s,edge=e) for s in stages for e in ('before','after')]
                kwargs=dict(png=False,share_case=initial,share_drafts=True,share_posting_case=case)
                self.assertIs(fn(Log(edges),C.FIXTURE,C.DIRECTORY,**kwargs),receipt)
                for wrong in (edges[:-2],edges+edges[-4:-2],list(reversed(edges))):
                    with self.assertRaises(RuntimeError):fn(Log(wrong),C.FIXTURE,C.DIRECTORY,**kwargs)


if __name__=='__main__':unittest.main()
