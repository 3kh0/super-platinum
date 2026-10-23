"""Portable source/selection custody checks; no process, network or native launch."""
import copy
import json
import inspect
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import workflow_management_share_person as P
import test_workflow_management_share as B


def add_people(q,server):
    users=[P.F.person('UWORKFLOW','You'),dict(id='UHOME0001',is_bot=True),P.D.peer()]
    server.setdefault('seed',{}).setdefault('boot',{})['users']=users
    people=[dict(id=r['id'],source=P.boot_member(r),interaction=True) for r in users if not r.get('is_bot')]
    people.sort(key=lambda r:r['id'])
    if q.get('loading') and q.get('opening') is None:people=[]
    q['people']=people
    mapped={r['raw'].get('user') for r in q['rows']}
    labels={'UWORKFLOW':'You',P.PEER:P.LABEL}
    q['options']=[dict(id=r['id'],label=r['label']) for r in q['rows']]+[
        dict(id=r['id'],label=labels[r['id']]) for r in people if r['id'] not in mapped]


def sample(case='success',stage='person-ready'):
    started,pending,settled,n=P.phase(case,stage)
    reads=[];replies=[]
    methods=['api.getFlannelHttpUrl','users/info','conversations.info','conversations.info','api.getFlannelHttpUrl','users/info'][:n]
    for i,m in enumerate(methods,1):
        body=([['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if m=='api.getFlannelHttpUrl' else
              dict(ids=[P.PEER],check_interaction=True,include_profile_only_users=True) if m=='users/info' else
              [['channel',P.DEST],['include_num_members','false']])
        reads.append(dict(serial=i,method=m,body=body,endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/users/info' if m=='users/info' else 'https://example.test/api/'+m,retrySafe=m=='conversations.info',owner=copy.deepcopy(P.S.OWNER)))
        replies.append(dict(serial=i,error='offline') if case=='posting-failure' and i==3 else dict(serial=i,raw=P.read_raw(m),keys=None))
    wire=dict(method='conversations.open',url=B.request()['url'].replace('chat.postMessage','conversations.open'),tokenPresent=True,
              fields=[['quiet','1'],['return_im','1'],['users',P.PEER],['_x_reason','startMpimByMemberIds']])
    raw=P.open_raw(case)
    outcome='retired' if case=='held-cancel' else 'rejected' if case=='rejected' else 'unknown' if case in ('missing','mismatched-peer') else 'acknowledged'
    v=dict(case=case,peer=P.PEER,channel=P.DEST,networkCalls=0,scheduling='closed-fixture-hold-until-original-selection-capture',reads=reads,replies=replies,
        requests=[dict(serial=1,request=wire,owner=copy.deepcopy(P.S.OWNER))] if started else [],
        responses=[dict(serial=1,raw=raw,mayHaveOpened=case!='rejected')] if started else [],held=[1] if pending else [],
        attempts=[dict(number=1,status=200,outcome=outcome,raw=raw,responseText=json.dumps(raw),retryAfterMillis=None,mayHaveApplied=case!='rejected')] if settled else [],
        settlements=[dict(serial=1,returned=True)] if settled else [],releases=[dict(serial=1,receiverRetired=False,delivered=True)] if settled else [])
    return dict(confirmationAdd=dict(share=dict(person=v,reads=[])))


def source(case='success',stage='person-ready'):
    adopted=P.phase(case,stage)[2] and case in ('success','posting-failure')
    return dict(canonical=dict(P.D.channel(),fromAnotherTeam=False) if adopted else None,
        raw=P.D.channel() if adopted and case=='success' else None,rawCurrent=adopted and case=='success',
        effective=dict(readOnly=False,threadOnly=False,permitsRootPost=True) if adopted and case=='success' else None,
        latest=P.LATEST if adopted else None,lastRead=P.LAST_READ if adopted else None,readRevision=1 if adopted else None)


class PersonLedger(unittest.TestCase):
    def test_all_cases_stages(self):
        for case,stages in P.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):P.ledger(case,stage,sample(case,stage));P.source(case,stage,source(case,stage))
    def reject(self,change,case='success',stage='person-ready'):
        v=sample(case,stage);P.ledger(case,stage,v);change(v['confirmationAdd']['share']['person'])
        with self.assertRaises((RuntimeError,ValueError,TypeError)):P.ledger(case,stage,v)
    def test_no_open_before_selection(self):self.reject(lambda v:v['requests'].append({}),stage='share-open')
    def test_no_replay(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])))
    def test_no_join_substitute(self):self.reject(lambda v:v['requests'][0]['request'].update(method='conversations.join'))
    def test_original_peer(self):self.reject(lambda v:v['requests'][0]['request']['fields'][2].__setitem__(1,'USHARE002'))
    def test_numeric_quiet(self):self.reject(lambda v:v['requests'][0]['request']['fields'][0].__setitem__(1,'true'))
    def test_return_im_required(self):self.reject(lambda v:v['requests'][0]['request']['fields'].pop(1))
    def test_exact_reason(self):self.reject(lambda v:v['requests'][0]['request']['fields'][-1].__setitem__(1,'other'))
    def test_no_fabricated_trace(self):self.reject(lambda v:v['requests'][0]['request'].update(url='https://example.test/api/conversations.open'))
    def test_original_owner(self):self.reject(lambda v:v['requests'][0]['owner'].update(epoch=99))
    def test_read_order(self):self.reject(lambda v:v['reads'].reverse())
    def test_ordered_original_member_ids(self):self.reject(lambda v:v['reads'][1]['body'].update(ids=['USHARE002']))
    def test_missing_cannot_be_ack(self):self.reject(lambda v:v['attempts'][0].update(outcome='acknowledged'),'missing','person-failed')
    def test_mismatch_not_normalized(self):self.reject(lambda v:v['responses'][0]['raw']['channel'].update(user=P.PEER),'mismatched-peer','person-failed')
    def test_cancel_cannot_adopt(self):self.reject(lambda v:v['attempts'][0].update(outcome='acknowledged'),'held-cancel','late-settled')
    def test_pending_cannot_claim_settlement(self):self.reject(lambda v:v['settlements'].append(dict(serial=1,returned=True)),stage='person-opening')
    def test_raw_opaque_retained(self):self.reject(lambda v:v['responses'][0]['raw'].pop('opaque_receipt'))
    def test_no_permission_fabrication(self):self.reject(lambda v:v['replies'][2].update(raw=P.read_raw('conversations.info')),'posting-failure','person-failed')
    def test_no_six_reads_before_send(self):self.reject(lambda v:v['reads'].extend(copy.deepcopy(v['reads'])))


class CompanionSource(unittest.TestCase):
    def reject(self,change,case='success',stage='person-ready'):
        v=source(case,stage);P.source(case,stage,v);change(v)
        with self.assertRaises(RuntimeError):P.source(case,stage,v)
    def test_latest_distinct_from_last_read(self):self.reject(lambda v:v.update(latest=P.LAST_READ))
    def test_last_read_not_default(self):self.reject(lambda v:v.update(lastRead=None))
    def test_read_revision_exact(self):self.reject(lambda v:v.update(readRevision=0))
    def test_actual_raw_not_canonical(self):self.reject(lambda v:v['raw'].update(fromAnotherTeam=False))
    def test_posting_failure_no_grant(self):self.reject(lambda v:v.update(effective=dict(readOnly=False,threadOnly=False,permitsRootPost=True)),'posting-failure','person-failed')
    def test_posting_failure_no_raw(self):self.reject(lambda v:v.update(raw=P.D.channel(),rawCurrent=True),'posting-failure','person-failed')
    def test_retired_no_source(self):self.reject(lambda v:v.update(canonical=P.D.channel()),'held-cancel','late-settled')
    def test_mismatch_no_companions(self):self.reject(lambda v:v.update(latest=P.LATEST),'mismatched-peer','person-failed')


class ActualOptions(unittest.TestCase):
    def sample(self,mapped=False):
        q=dict(rows=[dict(id=P.DEST,label=P.LABEL,raw=P.D.channel())] if mapped else [],loading=False);s={};add_people(q,s);return q,s
    def test_self_and_human_actual_source(self):
        q,s=self.sample();self.assertEqual([r['value'] for r in P.options(q,s)],['',P.PEER,'UWORKFLOW'])
    def test_offered_mapping_suppresses_duplicate_only(self):
        q,s=self.sample(True);self.assertEqual([r['value'] for r in P.options(q,s)],['',P.DEST,'UWORKFLOW']);self.assertEqual(len(q['people']),2)
    def test_source_not_fresh_wire(self):
        q,s=self.sample();q['people'][0]['source']=P.D.peer()
        with self.assertRaises(RuntimeError):P.options(q,s)
    def test_missing_self_not_fixture_gate(self):
        q,s=self.sample();q['people'].pop()
        with self.assertRaises(RuntimeError):P.options(q,s)
    def test_bot_not_person(self):
        q,s=self.sample();q['people'].append(dict(id='UHOME0001'))
        with self.assertRaises(RuntimeError):P.options(q,s)
    def test_pending_listing_does_not_invent_candidates(self):
        q,s=self.sample();q.update(loading=True,opening=None);add_people(q,s);self.assertEqual(len(P.options(q,s)),1)
    def test_duplicate_option_refused(self):
        q,s=self.sample(True);q['options'].append(dict(id=P.PEER,label=P.LABEL))
        with self.assertRaises(RuntimeError):P.options(q,s)


class Runner(unittest.TestCase):
    def test_exact_modes(self):
        for c in P.CASES:P.mode(c,'success',True);P.identity(dict(sharePersonCheck=True,sharePersonCase=c),c)
    def test_conflicts(self):
        for args in [('success','rejected',True),('success','success',False),('unknown','success',True),('success','success',True,True),('success','success',True,False,None,None,'success')]:
            with self.subTest(args=args),self.assertRaises(RuntimeError):P.mode(*args)
    def test_runner_signatures_and_early_admission(self):
        r=runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
        for name in ('validate_closed','validate','validate_log','run_fixture'):self.assertIn('share_person_case',inspect.signature(r[name]).parameters)
        with self.assertRaises(RuntimeError):r['run_fixture']('workflow-editor',Path('missing'),Path('missing'),share_case='rejected',share_drafts=True,share_person_case='success')


def complete_sample(case):
    import test_workflow_management_share_dm as T
    base=T.complete_sample('success')
    r={k:copy.deepcopy(v) for k,v in base.items() if k not in ('stages','captures','final','shareDmCheck','shareDmCase')}
    r.update(sharePersonCheck=True,sharePersonCase=case,stages=[],captures={})
    for stage in P.STAGES[case]:
        original=next(x for x in base['stages'] if x['stage']==(stage if stage in P.A.PREFIX else 'destination' if stage=='destination' else 'share-open'))
        row=copy.deepcopy(original);row['stage']=stage
        started,pending,settled,_=P.phase(case,stage)
        sent=case=='success' and stage in ('share-settled','destination')
        server=T.sample_send('success','share-settled' if sent else 'share-open')
        v=sample(case,stage)['confirmationAdd']['share']['person'];server['confirmationAdd']['share']['person']=v
        row['proof']['server']=server;row['model']['sharePersonSource']=source(case,stage)
        q=row['proof']['firstPublish']['attempts'][0]['confirmationShare']
        rows=[dict(id=id,label=P.A.CHANNELS[id]['name'],raw=dict(P.A.CHANNELS[id],fromAnotherTeam=False),peer=None) for id in (P.A.DEST,P.A.PRIVATE)]
        if case=='success' and settled:rows.append(dict(id=P.DEST,label=P.LABEL,raw=dict(P.D.channel(),fromAnotherTeam=False),peer=dict(id=P.PEER,raw=P.D.peer(),effective=P.D.peer(),source=P.D.peer_source())))
        active=stage in ('share-open','person-opening','person-ready','person-failed')
        selected='' if stage=='share-open' or case not in ('success','held-cancel') and settled else P.PEER if pending or case=='held-cancel' else P.DEST
        status='opening' if pending else 'retired' if case=='held-cancel' else 'ready' if case=='success' else 'failed'
        q.update(rows=rows,loading=pending or case=='held-cancel' and started,active=active,bound=True,
            selected=selected,text='' if stage=='share-open' else P.S.MESSAGE,openingHistory=[],
            opening=dict(peerId=P.PEER,status=status,request=v['requests'][0]['request'],attempts=v['attempts'],raw=P.open_raw(case) if settled else None,
                channelId=P.DEST if settled and case in ('success','posting-failure') else None,error='destination_unavailable' if status=='failed' else None,reused=False) if started else None,
            notice=P.FAILURE if case!='success' and settled and case!='held-cancel' else None,
            reads=[dict(raw=x['raw']) if 'raw' in x else dict(failure='read failed') for x in v['replies']],operation=None)
        add_people(q,server)
        if sent:
            wire=server['confirmationAdd']['share'];raw=P.S.raw_reply('success',B.CID,destination=P.DEST)
            q['operation']=dict(outcome='acknowledged',finished=True,queued=True,request=wire['requests'][0]['request'],attempts=wire['attempts'],raw=raw,
                message=dict(channel=P.DEST,clientMsgId=B.CID,localTs=B.TS,blocks=P.S.blocks(),url=P.S.URL,text=P.S.MESSAGE+'\n'+P.S.URL))
            row['model']['shareCustody']=copy.deepcopy(base['stages'][-1]['model']['shareCustody'])
        else:row['model']['shareCustody']=dict(messages=[],pending=[],delivery={})
        row['proof']['firstPublish']['attempts'][0]['closed']=not active
        native=row['proof']['native']
        if active:
            native['share']['selected']=selected;native['share']['notice']=q['notice'];native['share']['options']=P.options(q,server)
            for k,c in native['share']['controls'].items():
                c['value']=q['text'] if k=='text' else selected if k=='select' else None
                c['disabled']=stage!='person-ready' if k=='send' else False
            native['share']['controls']['text']['readOnly']=pending
        elif stage not in P.A.PREFIX:native.update(share=None,modal=None)
        path='captures/workflow-editor-'+stage+'.png';r['captures'][stage]=path
        observed=P.F.observation(row);ex=dict(after=copy.deepcopy(observed));row['proof']['exercise']=ex
        if stage=='person-opening':ex.update(input=T.action('text',P.S.MESSAGE),selectionActivation=T.action('select',P.PEER))
        elif stage in ('person-ready','person-failed','late-settled'):ex['release']=copy.deepcopy(v['releases'][0])
        else:ex['activation']=T.action('sidebar' if stage=='destination' else 'click')
        if stage in ('person-opening','person-ready','person-failed','late-settled'):ex['quiet']=dict(observedMs=500,before=copy.deepcopy(observed),after=copy.deepcopy(observed))
        r['stages'].append(row)
    r['final']=copy.deepcopy(P.F.observation(r['stages'][-1]));return r


class FullLoop(unittest.TestCase):
    def verify(self,r):
        with patch.object(P,'common',return_value=None),patch('workflow_management_share_drafts.validate_setup'),patch('workflow_management_share_drafts.validate_return'):
            return P.validate('workflow-editor','first-add-success',r,'captures',[1280,780],lambda *_:None,lambda *_:None,png=False,person_case=r['sharePersonCase'])
    def test_all_actual_stage_loops(self):
        for case in P.CASES:
            with self.subTest(case=case):self.verify(complete_sample(case))
    def reject(self,change,case='success',stage='person-ready'):
        r=complete_sample(case);self.verify(r);change(next(x for x in r['stages'] if x['stage']==stage))
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_open_is_not_message_send(self):self.reject(lambda row:row['proof']['server']['confirmationAdd']['share'].update(started=True),stage='person-opening')
    def test_pending_text_not_editable(self):self.reject(lambda row:row['proof']['native']['share']['controls']['text'].update(readOnly=False),stage='person-opening')
    def test_pending_cancel_enabled(self):self.reject(lambda row:row['proof']['native']['share']['controls']['cancel'].update(disabled=True),stage='person-opening')
    def test_pending_send_disabled(self):self.reject(lambda row:row['proof']['native']['share']['controls']['send'].update(disabled=False),stage='person-opening')
    def test_failed_status_not_ready(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['opening'].update(status='ready'),'posting-failure','person-failed')
    def test_no_foreign_selected_channel(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare'].update(selected='DSHARE002'))
    def test_original_open_response_not_post_ack(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['opening'].update(raw=P.S.raw_reply('success',B.CID)))
    def test_same_original_dom_nodes(self):self.reject(lambda row:row['proof']['native']['share']['controls']['text'].update(nodeId=800))
    def test_retired_source_cannot_reappear(self):self.reject(lambda row:row['model']['sharePersonSource'].update(raw=P.D.channel()),'held-cancel','late-settled')
    def test_cannot_drop_read_provenance(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].pop())
    def test_posting_failure_must_retain_query_failure(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].pop(),'posting-failure','person-failed')
    def test_posting_failure_cannot_fabricate_raw(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].__setitem__(-1,dict(raw=P.read_raw('conversations.info'))),'posting-failure','person-failed')
    def test_query_failure_exact_source_shape(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].__setitem__(-1,dict(error='offline')),'posting-failure','person-failed')
    def test_query_success_cannot_be_failure(self):self.reject(lambda row:row['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].__setitem__(-1,dict(failure='read failed')))


if __name__=='__main__':unittest.main()
