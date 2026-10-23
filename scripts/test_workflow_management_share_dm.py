"""Portable existing-DM custody mutations; no native/process/network execution."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import workflow_management_share_dm as D
import test_workflow_management_share as BASE


def sample_send(case='success',stage='share-settled'):
    delivery='rejected' if case=='rejected' else 'success'
    sent=case in ('success','rejected') and stage in ('share-settled','destination')
    server=BASE.sample(delivery,'share-settled' if sent else 'share-open')
    v=server['confirmationAdd']['share'];v['reads']=[]
    if sent:
        v['requests'][0]['request']['fields'][0][1]=D.DEST
        raw=D.S.raw_reply(delivery,BASE.CID,destination=D.DEST)
        v['responses'][0]['raw']=copy.deepcopy(raw)
        v['attempts'][0]['raw']=copy.deepcopy(raw);v['attempts'][0]['responseText']=json.dumps(raw)
        v['applied']=[copy.deepcopy(raw['message'])] if case=='success' else []
    return server


class ModeAndWire(unittest.TestCase):
    def test_exact_modes(self):
        for case in D.CASES:
            D.mode(case,'rejected' if case=='rejected' else 'success',True)
            D.identity(dict(shareDmCheck=True,shareDmCase=case),case)
    def test_mode_conflicts(self):
        for args in [('success','success',False),('success','rejected',True),('other','success',True),
                     ('success','success',True,True),('success','success',True,False,'success'),
                     ('success','success',True,False,None,'success')]:
            with self.subTest(args=args),self.assertRaises(RuntimeError):D.mode(*args)
    def test_unrequested_evidence(self):
        for value in [dict(shareDmCheck=True),dict(shareDmCase='success'),dict(shareDmCheck=1)]:
            with self.assertRaises(RuntimeError):D.identity(value,None)
    def test_existing_channel_remains_strict(self):
        v=BASE.request();D.S.request_proof(v)
        v['fields'][0][1]=D.DEST
        with self.assertRaises(RuntimeError):D.S.request_proof(v)
        D.S.request_proof(v,destination=D.DEST)
    def test_all_cases_all_stages_no_unrequested_send(self):
        for case,stages in D.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):D.send_ledger(case,stage,sample_send(case,stage))
    def reject(self,fn,case='success',stage='share-settled'):
        s=sample_send(case,stage);D.send_ledger(case,stage,s);fn(s['confirmationAdd']['share'])
        with self.assertRaises((RuntimeError,ValueError,TypeError)):D.send_ledger(case,stage,s)
    def test_no_duplicate_user_send(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])))
    def test_wrong_message_destination(self):self.reject(lambda v:v['requests'][0]['request']['fields'][0].__setitem__(1,D.A.DEST))
    def test_wrong_receipt_channel(self):self.reject(lambda v:v['responses'][0]['raw'].update(channel=D.A.DEST))
    def test_cannot_claim_ack_from_queue_completion(self):self.reject(lambda v:v['attempts'][0].update(outcome='acknowledged'),'rejected')
    def test_no_server_application_on_rejection(self):self.reject(lambda v:v['applied'].append({'ts':BASE.TS}),'rejected')
    def test_original_raw_opaque_required(self):self.reject(lambda v:v['responses'][0]['raw'].pop('opaque_receipt'))
    def test_original_owner(self):self.reject(lambda v:v['requests'][0]['owner'].update(epoch=20))
    def test_no_open_in_place_of_post(self):self.reject(lambda v:v['requests'][0]['request'].update(method='conversations.open'))
    def test_no_extra_wire_text(self):self.reject(lambda v:v['requests'][0]['request']['fields'].append(['text','substitute']))
    def test_changed_rich_blocks(self):self.reject(lambda v:v['requests'][0]['request']['fields'][3].__setitem__(1,'[]'))
    def test_no_send_deleted(self):self.reject(lambda v:v['requests'].append({}),'deleted','share-unavailable')
    def test_no_send_mismatch(self):self.reject(lambda v:v.update(started=True),'mismatched-peer','share-cancelled')
    def test_no_send_held_retirement(self):self.reject(lambda v:v['applied'].append({}),'held-cancel','late-settled')
    def test_no_false_mutation_hold(self):self.reject(lambda v:v.update(held=[1]))
    def test_trace_serial_not_boolean(self):self.reject(lambda v:v['requests'][0].update(serial=True))





def acquisition_sample(case='success',stage='share-open',retired=False):
    server=sample_send(case,stage);v=server['confirmationAdd']['share']
    n=0 if stage in D.A.PREFIX else 6 if case in ('success','rejected') and stage in ('share-settled','destination') else 3
    methods=['conversations.info','api.getFlannelHttpUrl','users/info']
    bodies=[[['channel',D.DEST],['include_num_members','false']],
            [['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']],
            dict(ids=[D.PEER],check_interaction=True,include_profile_only_users=True)]
    raw=[dict(ok=True,channel=D.channel(),opaque_receipt='original-share-dm-channel'),D.F.DISCOVERY_RAW,D.peer_reply(case)]
    held=case=='held-cancel' and stage not in D.A.PREFIX and stage!='late-settled'
    req=[];res=[];settle=[]
    for i in range(n):
        slot=i%3;method=methods[slot]
        req.append(dict(serial=i+1,method=method,body=copy.deepcopy(bodies[slot]),
                        endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/users/info' if slot==2 else 'https://example.test/api/'+method,
                        retrySafe=slot==0,owner=copy.deepcopy(D.S.OWNER)))
        res.append(dict(serial=i+1,raw=copy.deepcopy(raw[slot]),keys=None))
        if not (held and i==2) or stage=='share-cancelled' and retired:
            settle.append(dict(serial=i+1,returned=not (case=='held-cancel' and i==2 and retired)))
    release=[dict(serial=3,receiverRetired=retired,delivered=not retired)] if case=='held-cancel' and stage=='late-settled' else []
    v['dm']=dict(case=case,channel=D.DEST,peer=D.PEER,requests=req,responses=res,settlements=settle,
                 releases=release,held=[3] if held else [],networkCalls=0)
    return server


def choice_sample(case='success',stage='share-open'):
    rows=[] if case=='held-cancel' else [dict(id=id,label=D.A.CHANNELS[id]['name'],raw=dict(D.A.CHANNELS[id],fromAnotherTeam=False),peer=None) for id in (D.A.DEST,D.A.PRIVATE)]
    if case in ('success','rejected'):
        rows.append(dict(id=D.DEST,label=D.LABEL,raw=dict(D.channel(),fromAnotherTeam=False),
                         peer=dict(id=D.PEER,raw=D.peer(),effective=D.peer(),source=D.peer_source())))
    return dict(rows=rows,incomplete=True,loading=case=='held-cancel',
                notice=None if case=='held-cancel' or stage!='share-open' and case in ('success','rejected') else
                'Some destinations could not be verified. Refresh to try again.')


class Acquisition(unittest.TestCase):
    def test_all_exact_stage_counters(self):
        for case,stages in D.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):D.acquisition(case,stage,acquisition_sample(case,stage))
    def test_actual_cancel_receiver_retired_alternative(self):
        for stage in ('share-cancelled','late-settled'):
            D.acquisition('held-cancel',stage,acquisition_sample('held-cancel',stage,True))
    def reject(self,fn,case='success',stage='share-open'):
        v=acquisition_sample(case,stage);D.acquisition(case,stage,v);fn(v['confirmationAdd']['share']['dm'])
        with self.assertRaises((RuntimeError,TypeError,ValueError)):D.acquisition(case,stage,v)
    def test_no_open_factory(self):self.reject(lambda v:v['requests'][0].update(method='conversations.open'))
    def test_no_join_factory(self):self.reject(lambda v:v['requests'][0].update(method='conversations.join'))
    def test_order_is_not_a_set(self):self.reject(lambda v:v['requests'].reverse())
    def test_exact_member_flags(self):self.reject(lambda v:v['requests'][2]['body'].pop('check_interaction'))
    def test_wrong_requested_peer(self):self.reject(lambda v:v['requests'][2]['body'].update(ids=['USHARE002']))
    def test_no_conditional_zero_substitution(self):self.reject(lambda v:v['requests'][2]['body'].update(updated_ids={D.PEER:0}))
    def test_no_wrong_endpoint(self):self.reject(lambda v:v['requests'][2].update(endpoint='https://edgeapi.slack.com/cache/TOTHER/users/info'))
    def test_no_false_member_retry_safety(self):self.reject(lambda v:v['requests'][2].update(retrySafe=True))
    def test_discovery_owner_epoch(self):self.reject(lambda v:v['requests'][1]['owner'].update(epoch=123))
    def test_ordered_keys_not_invented_for_array(self):self.reject(lambda v:v['responses'][2].update(keys=[D.PEER]))
    def test_original_raw_deleted_required(self):self.reject(lambda v:v['responses'][2]['raw']['results'][0].update(deleted=False),'deleted')
    def test_original_mismatch_not_normalized(self):self.reject(lambda v:v['responses'][2]['raw']['results'][0].update(id=D.PEER),'mismatched-peer')
    def test_no_omitted_interaction_map(self):self.reject(lambda v:v['responses'][2]['raw'].pop('can_interact'))
    def test_no_fabricated_full_coverage(self):self.reject(lambda v:v['responses'][2]['raw'].update(next_marker='next'))
    def test_duplicate_peer(self):self.reject(lambda v:v['responses'][2]['raw']['results'].append(D.peer()))
    def test_held_not_returned(self):self.reject(lambda v:v['settlements'].append(dict(serial=3,returned=True)),'held-cancel','share-held')
    def test_no_late_release_identity_swap(self):self.reject(lambda v:v['releases'][0].update(serial=2),'held-cancel','late-settled')
    def test_inconsistent_receiver_delivery(self):self.reject(lambda v:v['releases'][0].update(delivered=False),'held-cancel','late-settled')
    def test_no_new_attempt_after_cancel(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][0])),'held-cancel','late-settled')
    def test_no_ledger_erasure_after_listing(self):
        opened={'proof':{'server':acquisition_sample()}}
        final=acquisition_sample('success','destination');D.acquisition('success','destination',final,opened)
        final['confirmationAdd']['share']['dm']['responses'][0]['raw']['opaque_receipt']='rewritten'
        with self.assertRaises(RuntimeError):D.acquisition('success','destination',final,opened)


class Choices(unittest.TestCase):
    def test_all_supported_states(self):
        for case,stages in D.STAGES.items():
            for stage in stages[len(D.A.PREFIX):]:D.choices(case,stage,choice_sample(case,stage))
    def reject(self,fn,case='success'):
        q=choice_sample(case);fn(q)
        with self.assertRaises((RuntimeError,ValueError,TypeError)):D.choices(case,'share-open',q)
    def test_deleted_cannot_select(self):self.reject(lambda q:q['rows'].append(choice_sample()['rows'][-1]),'deleted')
    def test_mismatched_peer_cannot_select(self):self.reject(lambda q:q['rows'].append(choice_sample()['rows'][-1]),'mismatched-peer')
    def test_late_choice_after_cancel_refused(self):
        q=choice_sample('held-cancel');q['rows']=choice_sample()['rows']
        with self.assertRaises(RuntimeError):D.choices('held-cancel','late-settled',q)
    def test_same_id_wrong_peer(self):self.reject(lambda q:q['rows'][-1]['raw'].update(user='USHARE002'))
    def test_no_human_label_substitution(self):self.reject(lambda q:q['rows'][-1].update(label='Another person'))
    def test_returned_peer_not_source(self):self.reject(lambda q:q['rows'][-1]['peer']['source'].update(updated=102))
    def test_raw_boot_is_not_canonical_source(self):self.reject(lambda q:q['rows'][-1]['peer'].update(source=D.peer()))
    def test_derived_source_cannot_replace_fresh_raw(self):self.reject(lambda q:q['rows'][-1]['peer'].update(raw=D.peer_source()))
    def test_missing_derived_source_key(self):self.reject(lambda q:q['rows'][-1]['peer']['source'].pop('_name_lc'))
    def test_changed_derived_source_key(self):self.reject(lambda q:q['rows'][-1]['peer']['source'].update(_display_name_normalized_lc='someone else'))
    def test_source_profile_defaults_required(self):self.reject(lambda q:q['rows'][-1]['peer']['source']['profile'].pop('first_name'))
    def test_source_identity_still_exact(self):self.reject(lambda q:q['rows'][-1]['peer']['source'].update(id='USHARE002'))
    def test_no_arbitrary_canonical_additions(self):self.reject(lambda q:q['rows'][-1]['peer']['source'].update(unproved=True))
    def test_effective_interaction_not_granted(self):self.reject(lambda q:q['rows'][-1]['peer']['effective'].update(is_stranger=True))
    def test_no_unknown_complete_claim(self):self.reject(lambda q:q.update(incomplete=False))
    def test_negative_feedback_required(self):self.reject(lambda q:q.update(notice=None),'deleted')
    def test_selection_clears_stale_notice(self):
        q=choice_sample('success','share-edited');D.choices('success','share-edited',q)
        q['notice']='Some destinations could not be verified. Refresh to try again.'
        with self.assertRaises(RuntimeError):D.choices('success','share-edited',q)


class Runner(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.runner=runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
    def test_exact_edge_stage_inventory(self):
        for case in D.CASES:
            names=('draft-setup',)+D.STAGES[case]+('draft-returned',)
            self.assertEqual(len(names),12 if case in ('deleted','mismatched-peer') else 13)
            self.assertEqual(len(names),len(set(names)))
    def test_runner_rejects_conflict_before_binary_or_process(self):
        with self.assertRaises(RuntimeError):
            self.runner['run_fixture']('workflow-editor',Path('does-not-exist'),Path('does-not-exist'),
                first_publish_case='first-add-success',share_case='success',share_drafts=True,
                share_posting_case='success',share_dm_case='success')
    def test_cli_and_all_shared_custody_signatures(self):
        import inspect
        for name in ('validate_closed','validate','validate_log','run_fixture'):
            self.assertIn('share_dm_case',inspect.signature(self.runner[name]).parameters)
        source=Path(__file__).with_name('check-workflow-management-native.py').read_text()
        self.assertIn("parser.add_argument('--share-dm-case'",source)
        self.assertIn("checks['SUPER_PLATINUM_WORKFLOW_SHARE_DM_CHECK'] = '1'",source)





BOX=dict(left=10,top=10,right=110,bottom=40,width=100,height=30)
VIEW=[1280,780]
def action(kind='click',value=None):
    v=dict(ok=True,kind=kind,nodeId=5,rect=copy.deepcopy(BOX),hitPoint=dict(x=50,y=25),
           event=dict(type='input' if kind=='text' else 'change' if kind=='select' else 'click',isTrusted=False,targetNodeId=5))
    if value is not None:v['value']=value
    if kind=='text':v.update(start=7,end=14)
    return v


def complete_sample(case):
    """New DM loop sample. Previously accepted publication/draft validators are isolated in tests."""
    receipt=dict(ok=True,fixture='workflow-editor',scenario='editor',firstPublishCase='first-add-success',
                 shareCase='rejected' if case=='rejected' else 'success',shareDraftCustody=True,shareDmCheck=True,shareDmCase=case,
                 syntheticSession=True,noLiveNetwork=True,aboutConfiguration=True,viewport=VIEW,stages=[],captures={})
    base_modal=dict(nodeId=100,upload=dict(nodeId=101),fields={k:dict(nodeId=i,value=k,displayed=False) for k,i in [('title',102),('description',103)]})
    for stage in D.STAGES[case]:
        server=acquisition_sample(case,stage);wire=server['confirmationAdd']['share'];sent=bool(wire['requests'])
        post=stage not in D.A.PREFIX;selected=case in ('success','rejected') and stage!='share-open' and post
        active=stage in ('share-open','share-edited','share-unavailable','share-held')
        q=choice_sample(case,stage);q.update(active=active,generation=1,epoch=2,bound=True,text=D.S.MESSAGE if selected else '',
                                           selected=D.DEST if selected else '',operation=None)
        from test_workflow_management_share_person import add_people
        add_people(q,server)
        q['reads']=[dict(raw=r['raw']) for r in wire['dm']['responses'] if not(case=='held-cancel' and r['serial']==3)]
        if sent:
            raw=D.S.raw_reply(receipt['shareCase'],BASE.CID,destination=D.DEST)
            q['operation']=dict(outcome='rejected' if case=='rejected' else 'acknowledged',finished=True,queued=True,
                request=copy.deepcopy(wire['requests'][0]['request']),attempts=copy.deepcopy(wire['attempts']),raw=raw,
                message=dict(channel=D.DEST,clientMsgId=BASE.CID,localTs=BASE.TS,blocks=D.S.blocks(),url=D.S.URL,text=D.S.MESSAGE+'\n'+D.S.URL))
        product=dict(attempts=[dict(closed=not active,confirmationShare=q)])
        message=dict(type='message',user='UWORKFLOW',channel=D.DEST,ts=D.S.SERVER_TS if case=='success' else BASE.TS,
                     client_msg_id=BASE.CID,text=D.S.MESSAGE+'\n'+D.S.URL,blocks=D.S.blocks())
        model=dict(shareCustody=dict(messages=[message] if sent else [],pending=[BASE.TS] if sent and case=='rejected' else [],
                                    delivery={BASE.TS:'Failed'} if sent and case=='rejected' else {}),
                   confirmationDestination=dict(channel=D.DEST,team='TWORKFLOW',tab='Messages'))
        native=dict(modal=copy.deepcopy(base_modal) if active or stage=='checked' else None,share=None)
        if active:
            controls={k:dict(nodeId=i,value=q['text'] if k=='text' else q['selected'] if k=='select' else None,
                            rect=copy.deepcopy(BOX),visible=True,hit=True,disabled=(not selected) if k=='send' else False)
                      for i,k in enumerate(('select','text','refresh','cancel','send'),10)}
            options=[dict(value='',text='Choose a conversation',disabled=False)]+[dict(value=r['id'],text=r['label'],disabled=False) for r in q['options']]
            native['share']=dict(nodeId=200,originalNodes=True,generation='1',epoch='2',selected=q['selected'],controls=controls,options=options,notice=q['notice'])
        if stage=='destination':
            native['destination']=dict(header=dict(id='',nodeId=62,text=D.LABEL,visible=True,rect=copy.deepcopy(BOX)))
            native['shareTimeline']=dict(connected=True,rows=[dict(messageId=message['ts'],text=message['text'],links=[dict(href=D.S.URL)])])
        path='captures/workflow-editor-'+stage+'.png'
        row=dict(stage=stage,model=model,proof=dict(server=server,firstPublish=product,native=native,exercise={}),geometry=dict(pngDimensions=[1942,1256]))
        ex=row['proof']['exercise'];observed=D.F.observation(row)
        ex['after']=copy.deepcopy(observed)
        if stage=='share-edited':ex.update(selectionActivation=action('select',D.DEST),input=action('text',D.S.MESSAGE))
        elif stage=='late-settled':ex['release']=copy.deepcopy(wire['dm']['releases'][0])
        elif stage not in ('share-unavailable','share-held'):ex['activation']=action('sidebar' if stage=='destination' else 'click')
        if stage in ('share-unavailable','share-held','late-settled'):ex['quiet']=dict(observedMs=500,before=copy.deepcopy(observed),after=copy.deepcopy(observed))
        receipt['stages'].append(row);receipt['captures'][stage]=path
    receipt['final']=copy.deepcopy(D.F.observation(receipt['stages'][-1]));receipt['replacement']=None
    return receipt


class IntegrationLoop(unittest.TestCase):
    def verify(self,r):
        with patch.object(D,'common_stage',return_value=None),patch('workflow_management_share_drafts.validate_setup'),patch('workflow_management_share_drafts.validate_return'):
            return D.validate('workflow-editor','first-add-success',r,'captures',VIEW,lambda *_:None,lambda *_:None,png=False,dm_case=r['shareDmCase'],share_case=r['shareCase'])
    def test_all_new_modes_run_full_dm_loop(self):
        for case in D.CASES:
            with self.subTest(case=case):self.verify(complete_sample(case))
    def test_guard_original_query_raw_not_fabricated(self):
        r=complete_sample('success');r['stages'][-1]['proof']['firstPublish']['attempts'][0]['confirmationShare']['reads'].pop()
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_native_peer_label_required(self):
        r=complete_sample('success');r['stages'][-1]['proof']['native']['destination']['header']['text']='#Dana Reviewer'
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_header_compound_visible_requires_original_hit(self):
        r=complete_sample('success');r['stages'][-1]['proof']['native']['destination']['header']['visible']=False
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_header_must_be_actual_im_shape(self):
        for change in ({'id':'channel-details-trigger'},{'nodeId':0},{'nodeId':True},{'hit':True}):
            r=complete_sample('success');r['stages'][-1]['proof']['native']['destination']['header'].update(change)
            with self.subTest(change=change),self.assertRaises(RuntimeError):self.verify(r)
    def test_held_send_must_be_disabled(self):
        r=complete_sample('held-cancel');r['stages'][7]['proof']['native']['share']['controls']['send']['disabled']=False
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_deleted_option_not_smuggled_in_dom(self):
        r=complete_sample('deleted');r['stages'][7]['proof']['native']['share']['options'].append(dict(value=D.DEST,text=D.LABEL,disabled=False))
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_dm_node_replacement_refused(self):
        r=complete_sample('success');r['stages'][8]['proof']['native']['share']['controls']['text']['nodeId']=999
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_capture_directory_escape(self):
        r=complete_sample('success');r['captures']['destination']='elsewhere/workflow-editor-destination.png'
        with self.assertRaises(RuntimeError):self.verify(r)
    def test_final_original_must_match(self):
        r=complete_sample('success');r['final']['model']['confirmationDestination']['channel']=D.A.DEST
        with self.assertRaises(RuntimeError):self.verify(r)


class RetainedNativeRegression(unittest.TestCase):
    def test_original_c_terminals_without_rewriting_failed_receipts(self):
        base=Path(__file__).resolve().parents[1]/'tmp/workflow-share-existing-dm-20261021/integration'
        paths=[base/(case+'-normal-c')/'workflow-editor.log' for case in ('success','rejected')]
        if not all(p.is_file() for p in paths):self.skipTest('Local retained closed native evidence is absent')
        runner=runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
        for case,p in zip(('success','rejected'),paths):
            terms=[json.loads(line.split(': ',1)[1]) for line in p.read_text(encoding='utf-8').splitlines()
                   if line.startswith(runner['PREFIX'])]
            self.assertEqual(len(terms),1)
            if 'people' not in terms[0]['stages'][-1]['proof']['firstPublish']['attempts'][-1]['confirmationShare']:
                self.skipTest('Historical C runtime predates person-option ABI; its immutable revalidation remains separate')
            with self.subTest(case=case):
                runner['validate']('workflow-editor',terms[0],p.parent,platform='win32',png=True,
                    first_publish_case='first-add-success',share_case=case,share_drafts=True,share_dm_case=case)


if __name__=='__main__':unittest.main()
