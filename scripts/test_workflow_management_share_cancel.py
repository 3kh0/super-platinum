"""Closed local Cancel oracle regressions; no application or input launches."""
import copy
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

import workflow_management_share_cancel as C
import test_workflow_management_share as W

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT/'tmp/share-cancel-oracle-synthetic'
FIXTURE = 'workflow-editor'
VIEW = [1280,780]
BOX = dict(left=100,top=100,right=180,bottom=130,width=80,height=30)
S = C.S


def sample(case='rejected'):
    server=W.sample(case,'destination')
    wire=server['confirmationAdd']['share']
    op=dict(outcome='rejected' if case=='rejected' else 'unknown',finished=True,queued=True,
            locallyCancelled=False,request=copy.deepcopy(wire['requests'][0]['request']),
            attempts=copy.deepcopy(wire['attempts']),
            message=dict(channel=S.A.DEST,clientMsgId=W.CID,localTs=W.TS,blocks=S.blocks(),
                         url=S.URL,text=S.MESSAGE+'\n'+S.URL),
            raw=None if case=='lost-response' else S.raw_reply(case,W.CID))
    message=dict(channel=S.A.DEST,client_msg_id=W.CID,ts=W.TS,user='UWORKFLOW',
                 text=S.MESSAGE+'\n'+S.URL,blocks=S.blocks(),opaque='retained original')
    status='Not sent' if case=='rejected' else 'Delivery unconfirmed'
    model=dict(shareCustody=dict(messages=[message],pending=[W.TS],delivery={W.TS:'Failed' if case=='rejected' else 'Unconfirmed'},
                               composer={'text':''},threadComposer={'text':''},drafts=[{'original':'retained'}]),
               shareDrafts=dict(storedMainDraft={'text':'channel \u4e2d'},storedThreadDraft={'text':'thread \U0001f642'}),
               confirmationDestination=dict(channel=S.A.DEST,team='TWORKFLOW',account=S.OWNER['account']),
               reads=0,writes=0,networkCalls=0)
    text='Author '+status+' '+S.MESSAGE+' '+S.URL+' Cancel'
    links=[dict(text=S.URL,href=S.URL)]
    row=dict(nodeId=41,messageId=W.TS,connected=True,text=text,links=links,status=status)
    button=dict(nodeId=42,id='',connected=True,text='Cancel',label='Cancel failed workflow message',
                disabled=False,displayed=True,rect=copy.deepcopy(BOX),hitPoint=dict(x=140,y=115))
    native=dict(ok=True,viewport=VIEW,noOverflow=True,
                shareTimeline=dict(nodeId=40,connected=True,scope='original',rows=[dict(row)]),
                shareCancelRow=dict(row=row,cancel=button,originalNodes=True),
                shareCancelOriginal=dict(rowNodeId=41,cancelNodeId=42,rowConnected=True,cancelConnected=True),
                shareDrafts=dict(windowOriginal=True,channel={'nodeId':55,'value':''},thread=None))
    before=dict(model=model,firstPublish=dict(attempts=[dict(confirmationShare=dict(operation=op),raw={'original':True})]),
                server=server,native=native)
    after=copy.deepcopy(before)
    after['firstPublish']['attempts'][0]['confirmationShare']['operation']['locallyCancelled']=True
    after['model']['shareCustody'].update(messages=[],pending=[],delivery={})
    after['native']['shareTimeline']['rows']=[]
    after['native']['shareCancelRow']=None
    after['native']['shareCancelOriginal'].update(rowConnected=False,cancelConnected=False)
    activation=dict(ok=True,kind='click',provenance='synthetic-dom-click',localTs=W.TS,rowNodeId=41,nodeId=42,
                    rect=copy.deepcopy(BOX),hitPoint=dict(x=140,y=115),
                    event=dict(type='click',isTrusted=False,targetNodeId=42),originalNodes=True)
    path=str(DIRECTORY/(FIXTURE+'-share-cancelled.png'))
    value=dict(before=before,activation=activation,after=after,
               quiet=dict(observedMs=500,before=copy.deepcopy(after),after=copy.deepcopy(after)),
               capture=dict(path=path,pngDimensions=[1942,1256],before=copy.deepcopy(after),after=copy.deepcopy(after)),
               captures={'share-cancelled':path})
    stage=dict(stage='destination',model=copy.deepcopy(model),geometry=dict(pngDimensions=[1942,1256]),
               proof=dict(firstPublish=copy.deepcopy(before['firstPublish']),server=copy.deepcopy(server),native=copy.deepcopy(native)))
    return dict(shareCase=case,shareDraftCustody=True,shareCancelCheck=True,shareCancel=value,stages=[stage])


def validate(value,png=False,check=lambda *args:None):
    return C.validate(value,FIXTURE,DIRECTORY,VIEW,lambda *args:None,check,png=png)


class LocalCancel(unittest.TestCase):
    def refused(self,change,case='rejected'):
        v=sample(case);validate(v);change(v)
        with self.assertRaises((RuntimeError,ValueError,KeyError,TypeError)):
            validate(v)

    def test_rejected_and_unknown_only_local_removal(self):
        for case in C.CASES:
            v=sample(case);after=validate(v)
            self.assertEqual(after['server'],v['shareCancel']['before']['server'])
            self.assertEqual(len(after['server']['confirmationAdd']['share']['applied']),int(case=='lost-response'))

    def test_physical_capture_bound_to_base_window(self):
        seen=[];validate(sample(),True,lambda p,d:seen.append(d))
        self.assertEqual(seen,[[1942,1256]])
        self.refused(lambda v:v['shareCancel']['capture'].update(pngDimensions=VIEW))

    def test_no_unrequested_or_successful_cancel(self):
        for change in (lambda v:v.update(shareCancelCheck=False),lambda v:v.update(shareDraftCustody=False),
                       lambda v:v.update(shareCase='success'),lambda v:v.update(shareCase='held')):
            self.refused(change)

    def test_original_destination_required(self):self.refused(lambda v:v['stages'][0].update(stage='share-settled'))
    def test_original_before_cannot_be_rewritten(self):self.refused(lambda v:v['shareCancel']['before']['model'].update(reads=1))
    def test_no_extra_remote_read(self):self.refused(lambda v:v['shareCancel']['after']['server']['confirmationAdd']['share']['reads'].append({}))
    def test_no_extra_send(self):self.refused(lambda v:v['shareCancel']['after']['server']['confirmationAdd']['share']['requests'].append({}))
    def test_no_remote_delete(self):self.refused(lambda v:v['shareCancel']['after']['server'].update(delete={'method':'chat.delete'}))
    def test_cancel_cannot_undo_lost_server_application(self):self.refused(lambda v:v['shareCancel']['after']['server']['confirmationAdd']['share'].update(applied=[]),'lost-response')
    def test_original_ack_error_immutable(self):self.refused(lambda v:v['shareCancel']['after']['firstPublish']['attempts'][0]['confirmationShare']['operation']['raw'].update(ok=True))
    def test_original_uuid_immutable(self):self.refused(lambda v:v['shareCancel']['after']['firstPublish']['attempts'][0]['confirmationShare']['operation']['message'].update(clientMsgId='foreign'))
    def test_cancel_flag_required(self):self.refused(lambda v:v['shareCancel']['after']['firstPublish']['attempts'][0]['confirmationShare']['operation'].update(locallyCancelled=False))
    def test_no_pending_status_retained(self):self.refused(lambda v:v['shareCancel']['after']['model']['shareCustody']['delivery'].update({W.TS:'Failed'}))
    def test_local_message_actually_removed(self):self.refused(lambda v:v['shareCancel']['after']['model']['shareCustody']['messages'].append({}))
    def test_both_unrelated_drafts_exact(self):self.refused(lambda v:v['shareCancel']['after']['model']['shareDrafts'].update(storedThreadDraft=None))
    def test_unrelated_core_buffer_not_consumed(self):self.refused(lambda v:v['shareCancel']['after']['model']['shareCustody']['composer'].update(text='changed'))
    def test_original_node_target(self):self.refused(lambda v:v['shareCancel']['activation'].update(nodeId=999))
    def test_original_row_target(self):self.refused(lambda v:v['shareCancel']['activation'].update(localTs='foreign'))
    def test_actual_dom_event_required(self):self.refused(lambda v:v['shareCancel']['activation'].update(event=None))
    def test_synthetic_click_not_claimed_trusted(self):self.refused(lambda v:v['shareCancel']['activation']['event'].update(isTrusted=True))
    def test_hidden_action_rejected(self):self.refused(lambda v:v['shareCancel']['activation'].update(hitPoint=None))
    def test_original_nodes_detached(self):self.refused(lambda v:v['shareCancel']['after']['native']['shareCancelOriginal'].update(rowConnected=True))
    def test_local_row_absent_in_dom(self):self.refused(lambda v:v['shareCancel']['after']['native']['shareTimeline']['rows'].append({}))
    def test_no_unrelated_native_remount(self):self.refused(lambda v:v['shareCancel']['after']['native']['shareDrafts']['channel'].update(nodeId=56))
    def test_quiet_after_cancel(self):self.refused(lambda v:v['shareCancel']['quiet'].update(observedMs=0))
    def test_no_late_replay(self):self.refused(lambda v:v['shareCancel']['quiet']['after']['server']['confirmationAdd']['share']['requests'].append({}))
    def test_capture_stable(self):self.refused(lambda v:v['shareCancel']['capture']['after']['model'].update(writes=1))
    def test_capture_path_owned(self):self.refused(lambda v:v['shareCancel']['capture'].update(path=str(DIRECTORY/'other.png')))
    def test_capture_map_exact(self):self.refused(lambda v:v['shareCancel']['captures'].update(extra='foreign'))

    def test_pending_ack_unrelated_controls_refused_even_coherent_before(self):
        for key,val in [('outcome','sending'),('outcome','acknowledged'),('finished',False),('locallyCancelled',True)]:
            v=sample()
            for op in (v['shareCancel']['before']['firstPublish']['attempts'][0]['confirmationShare']['operation'],
                       v['stages'][0]['proof']['firstPublish']['attempts'][0]['confirmationShare']['operation']):op[key]=val
            with self.assertRaises(RuntimeError):validate(v)

    def test_original_rich_blocks_and_status_required(self):
        for field,val in [('blocks',[]),('client_msg_id','foreign')]:
            v=sample()
            for model in (v['shareCancel']['before']['model'],v['stages'][0]['model']):model['shareCustody']['messages'][0][field]=val
            with self.assertRaises(RuntimeError):validate(v)
        v=sample()
        for native in (v['shareCancel']['before']['native'],v['stages'][0]['proof']['native']):native['shareCancelRow']['row']['status']='Sending'
        with self.assertRaises(RuntimeError):validate(v)

    def test_cli_guard_precedes_filesystem_or_process(self):
        runner=runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        for case,drafts in [('success',True),('held',True),('rejected',False),(None,False)]:
            with patch.object(Path,'mkdir',side_effect=AssertionError('must not reach filesystem')):
                with self.assertRaises(RuntimeError):runner['run_fixture'](FIXTURE,ROOT/'absent.exe',DIRECTORY,
                    first_publish_case='first-add-success',share_case=case,share_drafts=drafts,share_cancel=True)

    def test_closed_capture_order_includes_cancel_before_draft_return(self):
        runner=runpy.run_path(str(ROOT/'scripts/check-workflow-management-native.py'))
        fn=runner['validate_log']; receipt=dict(ok=True)
        stages=('draft-setup',)+tuple(runner['SHARE_STAGES']['rejected'])+('share-cancelled','draft-returned')
        edges=[dict(stage=s,edge=e) for s in stages for e in ('before','after')]
        class Log:
            def __init__(self,items):self.items=items
            def receipts(self,prefix):return [receipt] if prefix==runner['PREFIX'] else self.items
        with patch.dict(fn.__globals__,validate=lambda *a,**k:None):
            self.assertIs(fn(Log(edges),FIXTURE,DIRECTORY,png=False,share_case='rejected',
                             share_drafts=True,share_cancel=True),receipt)
            missing=[e for e in edges if e['stage']!='share-cancelled']
            reordered=edges[:-4]+edges[-2:]+edges[-4:-2]
            for wrong in (missing,reordered,edges+edges[-4:-2]):
                with self.assertRaises(RuntimeError):
                    fn(Log(wrong),FIXTURE,DIRECTORY,png=False,share_case='rejected',
                       share_drafts=True,share_cancel=True)
            with self.assertRaises(RuntimeError):
                fn(Log(edges),FIXTURE,DIRECTORY,png=False,share_case='rejected',share_drafts=True)


if __name__=='__main__':unittest.main()
