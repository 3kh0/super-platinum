"""Focused closed Resend proof mutations; no native process or input launches."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

import workflow_management_share_resend as R
import test_workflow_management_share as W
import test_workflow_management_share_cancel as C

ROOT = Path(__file__).resolve().parents[1]


def requests():
    original = W.sample('lost-response', 'destination')['confirmationAdd']['share']['requests'][0]['request']
    retry = copy.deepcopy(original)
    retry['fields'] += [['user', 'UWORKFLOW'], ['showResend', 'true'], ['showCancel', 'true'],
                        ['is_pending', 'true'], ['isEKMError', 'false'],
                        ['isDLPUserWarningError', 'false'], ['skip_dlp_user_warning', 'false']]
    return original, retry


def ledgers(case='success', phase='settled'):
    server = W.sample('rejected' if case == 'suppressed' else 'lost-response', 'destination')
    empty = dict(case=case, requests=[], responses=[], attempts=[], settlements=[], releases=[], held=[],
                 networkCalls=0, scheduling='closed-fixture-hold-until-original-pending-capture')
    server['confirmationAdd']['share']['resend'] = empty
    current = copy.deepcopy(server)
    if phase == 'before' or case == 'suppressed': return server, current
    data = current['confirmationAdd']['share']['resend']; _, wire = requests()
    raw = R.raw_reply(case, W.CID)
    data.update(requests=[dict(intent=1,serial=1,request=wire,owner=R.S.OWNER)],
                responses=[dict(intent=1,serial=1,raw=raw,applied=case!='rejected',deduplicated=True)])
    if phase == 'pending': data['held'] = [1]
    else:
        data.update(attempts=[dict(number=1,status=200,outcome='rejected' if case=='rejected' else 'acknowledged',
                                  raw=raw,responseText=json.dumps(raw),retryAfterMillis=None,mayHaveApplied=case!='rejected')],
                    settlements=[dict(intent=1,serial=1,returned=True)],
                    releases=[dict(intent=1,serial=1,receiverRetired=False,delivered=True)])
    return server, current


def sample(case='success'):
    base=C.sample('rejected' if case=='suppressed' else 'lost-response')
    before=base['shareCancel']['before']; base.pop('shareCancel'); base['shareCancelCheck']=False
    base.update(shareResendCase=case,shareResendCheck=True)
    initial_server,_=ledgers(case,'before'); before['server']=initial_server
    op=R.C.operation(before); op.update(resends=[],retryBlocked=False)
    n=before['native']; cancelled=n.pop('shareCancelRow'); n.pop('shareCancelOriginal')
    row=cancelled['row']; row['cancel']=cancelled['cancel']; row['resend']=None
    if case!='suppressed':
        row['resend']=copy.deepcopy(row['cancel'])
        row['resend'].update(nodeId=43,text='Try again',label='Retry failed workflow message')
    n['shareResendRow']=row
    n['shareResendOriginal']=dict(rowNodeId=41,resendNodeId=None if case=='suppressed' else 43,
                                  cancelNodeId=42,rowConnected=True,resendConnected=case!='suppressed',cancelConnected=True)
    def observation(phase):
        result=copy.deepcopy(before); _,server=ledgers(case,phase); result['server']=server
        if case=='suppressed': return result
        d=server['confirmationAdd']['share']['resend']; pending=phase=='pending'
        R.C.operation(result)['resends']=[dict(sequence=1,generation=19,request=d['requests'][0]['request'],
            attempts=d['attempts'],raw=None if pending else d['responses'][0]['raw'],
            outcome='sending' if pending else 'rejected' if case=='rejected' else 'acknowledged',
            finished=not pending,applied=not pending)]
        c=result['model']['shareCustody'];c['delivery']={} if pending or case!='rejected' else {W.TS:'Failed'}
        if not pending and case!='rejected':
            c['messages'][0].update(R.raw_reply(case,W.CID)['message']);c['pending']=[]
        native=result['native']; r=native['shareResendRow']; r['status']='Sending…' if pending else 'Not sent' if case=='rejected' else None
        if pending or case!='rejected':r.update(resend=None,cancel=None)
        else:r['resend']['nodeId']=45;r['cancel']['nodeId']=46
        if not pending and case!='rejected':r.update(nodeId=50,messageId=R.S.SERVER_TS)
        r['text']=R.S.MESSAGE+' '+R.S.URL+' '+str(r['status'])
        native['shareTimeline']['rows']=[{k:copy.deepcopy(v) for k,v in r.items() if k not in ('resend','cancel')}]
        native['shareResendOriginal'].update(rowConnected=r['nodeId']==41,resendConnected=False,cancelConnected=False)
        return result
    held=None if case=='suppressed' else observation('pending');after=observation('settled')
    action=None if held is None else dict(ok=True,kind='click',provenance='synthetic-dom-click',
        localTs=W.TS,rowNodeId=41,nodeId=43,rect=copy.deepcopy(C.BOX),hitPoint=dict(x=140,y=115),
        event=dict(type='click',isTrusted=False,targetNodeId=43),originalNodes=True)
    quiet=lambda value:dict(observedMs=500,before=copy.deepcopy(value),after=copy.deepcopy(value))
    blocked=None if held is None else dict(kind='passive-original-control-absence',resendAbsent=True,
                cancelAbsent=True,actionDispatched=False,quiet=quiet(held))
    stages={'resend-suppressed':after} if held is None else {'resend-pending':held,'resend-settled':after}
    captures={};evidence={}
    for stage,value in stages.items():
        path=str(C.DIRECTORY/(C.FIXTURE+'-'+stage+'.png'));captures[stage]=path
        evidence[stage]=dict(path=path,pngDimensions=[1942,1256],before=copy.deepcopy(value),after=copy.deepcopy(value))
    base['stages'][0].update(model=copy.deepcopy(before['model']),proof=dict(firstPublish=copy.deepcopy(before['firstPublish']),
        server=copy.deepcopy(before['server']),native=copy.deepcopy(before['native'])))
    base['shareResend']=dict(before=before,activation=action,held=held,blocked=blocked,
        release=None if held is None else dict(intent=1,serial=1,receiverRetired=False,delivered=True),
        after=after,quiet=quiet(after),captureEvidence=evidence,captures=captures)
    return base


def validate(value):
    return R.validate(value,C.FIXTURE,C.DIRECTORY,C.VIEW,lambda *args:None,lambda *args:None,png=False)


class RetryWire(unittest.TestCase):
    def test_source_retry_retains_identity_with_explicit_failed_row_flags(self):
        initial, retry = requests()
        self.assertEqual(R.request_proof(retry, initial), (W.CID, W.TS))

    def test_initial_wire_is_not_a_retry(self):
        initial, _ = requests()
        with self.assertRaises(RuntimeError): R.request_proof(initial, initial)

    def test_metadata_and_flags_exact(self):
        for index in range(8, 15):
            initial, retry = requests(); retry['fields'][index][1] = 'wrong'
            with self.assertRaises(RuntimeError): R.request_proof(retry, initial)

    def test_rich_and_identity_cannot_be_reconstructed_from_current_composer(self):
        for index in (0, 1, 3, 4, 5):
            initial, retry = requests(); retry['fields'][index][1] = 'foreign'
            with self.assertRaises((RuntimeError, ValueError)): R.request_proof(retry, initial)

    def test_no_fabricated_text_thread_or_unfurl_boolean(self):
        for field in ('text', 'thread_ts', 'unfurl_links'):
            initial, retry = requests(); retry['fields'].append([field, 'true'])
            with self.assertRaises(RuntimeError): R.request_proof(retry, initial)

    def test_explicit_mode_guard_precedes_filesystem(self):
        runner = runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        for retry, initial, drafts, cancel in [('success','rejected',True,False),
                ('suppressed','lost-response',True,False), ('held','lost-response',False,False),
                ('success','lost-response',True,True), ('foreign','lost-response',True,False)]:
            with patch.object(Path, 'mkdir', side_effect=AssertionError('filesystem reached')):
                with self.assertRaises(RuntimeError):
                    runner['run_fixture']('workflow-editor', ROOT/'absent.exe', ROOT/'tmp/unused',
                        first_publish_case='first-add-success', share_case=initial,
                        share_drafts=drafts, share_cancel=cancel, share_resend_case=retry)

    def test_closed_capture_edges_are_exact_for_each_retry_case(self):
        runner=runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        fn=runner['validate_log'];receipt=dict(ok=True)
        class Log:
            def __init__(self,edges):self.edges=edges
            def receipts(self,prefix):return [receipt] if prefix==runner['PREFIX'] else self.edges
        with patch.dict(fn.__globals__,validate=lambda *a,**k:None):
            for case in R.CASES:
                initial='rejected' if case=='suppressed' else 'lost-response'
                stages=('draft-setup',)+tuple(runner['SHARE_STAGES'][initial])+R.STAGES[case]+('draft-returned',)
                edges=[dict(stage=s,edge=e) for s in stages for e in ('before','after')]
                kwargs=dict(png=False,share_case=initial,share_drafts=True,share_resend_case=case)
                self.assertIs(fn(Log(edges),C.FIXTURE,C.DIRECTORY,**kwargs),receipt)
                for wrong in (edges[:-2],edges+edges[-4:-2],list(reversed(edges))):
                    with self.assertRaises(RuntimeError):fn(Log(wrong),C.FIXTURE,C.DIRECTORY,**kwargs)


class RetryLedger(unittest.TestCase):
    def refused(self, mutation, case='success', phase='settled'):
        original, current = ledgers(case, phase)
        R.ledger(case,phase,current,original); mutation(current)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):
            R.ledger(case,phase,current,original)

    def test_each_exact_phase_and_outcome(self):
        for case in R.CASES:
            for phase in ('before','pending','settled'):
                a,b=ledgers(case,phase)
                self.assertEqual(R.ledger(case,phase,b,a),(W.CID,W.TS))

    def test_no_duplicate_user_retry(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['requests'].append({}))

    def test_original_three_transport_attempts_retained(self):
        self.refused(lambda s:s['confirmationAdd']['share']['attempts'].pop())

    def test_no_second_server_identity(self):
        self.refused(lambda s:s['confirmationAdd']['share']['applied'].append({}))

    def test_no_new_destination_read_from_retry(self):
        self.refused(lambda s:s['confirmationAdd']['share']['reads'].append({}))

    def test_retry_owner_and_sequence_exact(self):
        for key,val in [('owner',{}),('intent',2),('serial',True)]:
            self.refused(lambda s:s['confirmationAdd']['share']['resend']['requests'][0].update({key:val}))

    def test_original_lost_receipt_not_replaced_by_retry_ack(self):
        self.refused(lambda s:s['confirmationAdd']['share']['responses'][0].update(raw={}))

    def test_pending_cannot_claim_delivery(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['attempts'].append({}),phase='pending')

    def test_rejected_does_not_claim_success(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['attempts'][0].update(outcome='acknowledged'),case='rejected')

    def test_original_raw_and_response_text_exact(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['attempts'][0].update(responseText='{}'))

    def test_suppressed_cannot_dispatch(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['requests'].append({}),case='suppressed')

    def test_live_network_counter_and_fake_release_refused(self):
        self.refused(lambda s:s['confirmationAdd']['share']['resend'].update(networkCalls=1))
        self.refused(lambda s:s['confirmationAdd']['share']['resend']['releases'][0].update(receiverRetired=True))


class MountedRetry(unittest.TestCase):
    def refused(self,mutation,case='success'):
        value=sample(case);validate(value);mutation(value)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):validate(value)

    def test_all_four_complete_closed_shapes(self):
        for case in R.CASES:validate(sample(case))

    def test_original_action_not_controller_call(self):self.refused(lambda v:v['shareResend']['activation'].update(provenance='controller'))
    def test_original_button_not_successor(self):self.refused(lambda v:v['shareResend']['activation'].update(nodeId=99))
    def test_original_row_not_foreign(self):self.refused(lambda v:v['shareResend']['activation'].update(localTs='foreign'))
    def test_exposed_action_required(self):self.refused(lambda v:v['shareResend']['activation'].update(hitPoint=None))
    def test_cannot_fabricate_trusted_input(self):self.refused(lambda v:v['shareResend']['activation']['event'].update(isTrusted=True))
    def test_original_outcome_cannot_become_retry_ack(self):self.refused(lambda v:R.C.operation(v['shareResend']['after']).update(outcome='acknowledged'))
    def test_retired_retry_not_applied(self):self.refused(lambda v:R.C.operation(v['shareResend']['after'])['resends'][0].update(applied=False))
    def test_no_second_user_retry(self):self.refused(lambda v:R.C.operation(v['shareResend']['after'])['resends'].append({}))
    def test_same_native_drafts_and_selection(self):self.refused(lambda v:v['shareResend']['after']['native']['shareDrafts']['channel'].update(nodeId=77))
    def test_same_stored_separate_drafts(self):self.refused(lambda v:v['shareResend']['after']['model']['shareDrafts'].update(storedThreadDraft=None))
    def test_pending_no_cancel_or_retry(self):self.refused(lambda v:v['shareResend']['held']['native']['shareResendRow'].update(cancel={}))
    def test_rejected_retry_keeps_cancel(self):self.refused(lambda v:v['shareResend']['after']['native']['shareResendRow'].update(cancel=None),case='rejected')
    def test_ack_no_failed_action(self):self.refused(lambda v:v['shareResend']['after']['native']['shareResendRow'].update(resend={}))
    def test_suppressed_no_try_again(self):self.refused(lambda v:v['shareResend']['after']['native']['shareResendRow'].update(resend={}),case='suppressed')
    def test_duplicate_proof_is_passive_not_hidden_action(self):self.refused(lambda v:v['shareResend']['blocked'].update(actionDispatched=True))
    def test_pending_quiet_no_automatic_retry(self):self.refused(lambda v:v['shareResend']['blocked']['quiet']['after']['server']['confirmationAdd']['share']['resend']['requests'].append({}))
    def test_original_sender_release(self):self.refused(lambda v:v['shareResend']['release'].update(receiverRetired=True))
    def test_settlement_generation_custody(self):self.refused(lambda v:R.C.operation(v['shareResend']['after'])['resends'][0].update(generation=20))
    def test_original_pending_request_unchanged(self):self.refused(lambda v:v['shareResend']['held']['server']['confirmationAdd']['share']['resend']['requests'][0]['request']['fields'][0].__setitem__(1,'foreign'))
    def test_original_physical_capture(self):self.refused(lambda v:v['shareResend']['captureEvidence']['resend-settled'].update(pngDimensions=C.VIEW))
    def test_capture_exact_observation(self):self.refused(lambda v:v['shareResend']['captureEvidence']['resend-pending']['after']['model'].update(writes=3))
    def test_lost_server_identity_retained(self):self.refused(lambda v:v['shareResend']['after']['server']['confirmationAdd']['share'].update(applied=[]))
    def test_unknown_original_response_is_not_fabricated(self):self.refused(lambda v:R.C.operation(v['shareResend']['after']).update(raw={'ok':True}))


if __name__ == '__main__': unittest.main()
