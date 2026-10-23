"""Portable protocol mutations; these synthetic vectors are not native acceptance."""
import copy
import json
import runpy
import unittest
from unittest.mock import patch
from pathlib import Path
import workflow_management_share_multiple as M
import workflow_management_share_empty as E
import test_workflow_management_share as B

def request():
    return dict(method='conversations.open',url=B.request()['url'].replace('chat.postMessage','conversations.open'),tokenPresent=True,
                fields=[['quiet','1'],['return_im','1'],['users',','.join(M.MEMBERS)],['_x_reason','startMpimByMemberIds']])

def sample(case='success',stage='group-ready'):
    pre=stage in M.A.PREFIX+('share-open','first-recipient')+M.TOKEN_STAGES
    opened=not pre and case!='reuse';held=opened and (stage=='group-opening' or case=='held-change-cancel' and stage in ('recipient-changed','share-cancelled'))
    settled=opened and not held;raw=M.open_raw(case)
    outcome='retired' if case=='held-change-cancel' else 'rejected' if case=='rejected' else 'unknown' if case in ('missing-members','mismatched-members') else 'acknowledged'
    rows=[];replies=[]
    if stage not in M.A.PREFIX:
        for id in M.PEERS:
            i=len(rows)+1
            rows.append(dict(serial=i,method='api.getFlannelHttpUrl',body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']],endpoint='https://example.test/api/api.getFlannelHttpUrl',retrySafe=False,owner=copy.deepcopy(M.S.OWNER)))
            replies.append(dict(serial=i,raw=M.F.DISCOVERY_RAW,keys=None));i+=1
            rows.append(dict(serial=i,method='users/info',body=dict(ids=[id],check_interaction=True,include_profile_only_users=True),endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/users/info',retrySafe=False,owner=copy.deepcopy(M.S.OWNER)))
            replies.append(dict(serial=i,raw=dict(ok=True,results=[M.peer(M.PEERS.index(id))],can_interact={id:True},next_marker='',opaque_receipt='original-multiple-peers'),keys=None))
    def info():
        n=len(rows)+1
        rows.append(dict(serial=n,method='conversations.info',body=[['channel',M.DEST],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=copy.deepcopy(M.S.OWNER)))
        replies.append(dict(serial=n,raw=dict(ok=True,channel=M.channel(),opaque_receipt='original-multiple-posting'),keys=None))
    if case in ('success','reuse') and stage in ('group-ready','share-settled','destination'):
        info()
        if stage in ('share-settled','destination'):
            for a,b in list(zip(rows[:4],replies[:4])):
                a=copy.deepcopy(a);b=copy.deepcopy(b);a['serial']=b['serial']=len(rows)+1;rows.append(a);replies.append(b)
            info()
    v=dict(case=case,peers=list(M.PEERS),channel=M.DEST,reads=rows,replies=replies,
        requests=[dict(serial=1,request=request(),owner=copy.deepcopy(M.S.OWNER))] if opened else [],
        responses=[dict(serial=1,raw=raw,mayHaveOpened=case!='rejected')] if opened else [],
        attempts=[dict(number=1,status=200,outcome=outcome,raw=raw,responseText=json.dumps(raw),retryAfterMillis=None,mayHaveApplied=case!='rejected')] if settled else [],
        settlements=[dict(serial=1,returned=True)] if settled else [],releases=[dict(serial=1,receiverRetired=False,delivered=True)] if settled else [],held=[1] if held else [],networkCalls=0,scheduling='closed-fixture-hold-until-original-group-capture')
    return dict(confirmationAdd=dict(share=dict(multiple=v)))

def source(case='success',stage='group-ready'):
    success=case in ('success','reuse') and stage in ('group-ready','share-settled','destination')
    return dict(canonical=dict(M.channel(),fromAnotherTeam=False) if success or case=='reuse' else None,canonicalCurrent=True,
        raw=M.channel() if success else None,rawCurrent=success,effective=dict(readOnly=False,threadOnly=False,permitsRootPost=True) if success else None,
        latest='1781524809.000001' if success and case=='success' else None,lastRead='1781524803.000001' if success and case=='success' else None,readRevision=1 if success and case=='success' else None)

class MultipleProtocol(unittest.TestCase):
    def test_every_case_stage_original_inventory(self):
        for case,stages in M.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):M.acquisition(case,stage,sample(case,stage));M.source(case,stage,source(case,stage))

    def mutate(self,path,value):
        server=sample();v=server['confirmationAdd']['share']['multiple'];target=v
        for key in path[:-1]:target=target[key]
        target[path[-1]]=value
        with self.assertRaises(RuntimeError):M.acquisition('success','group-ready',server)

    def test_no_fabricated_membership_request(self):self.mutate(('requests',0,'request','fields',2,1),'USHARE001,USHARE002')
    def test_no_duplicate_open(self):self.mutate(('requests',),sample()['confirmationAdd']['share']['multiple']['requests']*2)
    def test_no_foreign_owner(self):self.mutate(('requests',0,'owner','account'),'foreign')
    def test_raw_members_immutable(self):self.mutate(('responses',0,'raw','channel','members'),['UWORKFLOW','USHARE001'])
    def test_actual_reply_text_must_match(self):self.mutate(('attempts',0,'responseText'),'{}')
    def test_receipt_not_ack_from_sparse(self):
        s=sample('missing-members','group-failed');s['confirmationAdd']['share']['multiple']['attempts'][0]['outcome']='acknowledged'
        with self.assertRaises(RuntimeError):M.acquisition('missing-members','group-failed',s)
    def test_retired_reply_not_success(self):
        s=sample('held-change-cancel','late-settled');s['confirmationAdd']['share']['multiple']['attempts'][0]['outcome']='acknowledged'
        with self.assertRaises(RuntimeError):M.acquisition('held-change-cancel','late-settled',s)
    def test_ordered_member_keys_not_fabricated(self):self.mutate(('replies',1,'keys'),[])
    def test_member_interaction_false_not_grant(self):self.mutate(('replies',1,'raw','can_interact','USHARE001'),False)
    def test_member_request_flags_strict(self):self.mutate(('reads',1,'body','check_interaction'),False)
    def test_original_prefix_history_immutable(self):
        v=copy.deepcopy(sample('success','group-opening')['confirmationAdd']['share']['multiple']);v['replies'][0]['raw']={}
        with self.assertRaises(RuntimeError):M.acquisition('success','group-ready',sample(),v)
    def test_failed_group_cannot_publish(self):
        v=source('rejected','group-failed');v['canonical']=M.channel()
        with self.assertRaises(RuntimeError):M.source('rejected','group-failed',v)
    def test_open_is_not_posting_permission(self):
        v=source();v['rawCurrent']=False
        with self.assertRaises(RuntimeError):M.source('success','group-ready',v)
    def test_exact_reuse_members_required(self):
        v=source('reuse');v['canonical']['members'].append('UOTHER001')
        with self.assertRaises(RuntimeError):M.source('reuse','group-ready',v)
    def test_original_open_companions_preserved(self):
        v=source();v['lastRead']=None
        with self.assertRaises(RuntimeError):M.source('success','group-ready',v)
    def test_request_schemes_and_unknown_fields(self):
        for modify in (lambda v:v.update(url=v['url'].replace('example.test','other.test')),lambda v:v['fields'].append(['channel',M.DEST]),lambda v:v.update(tokenPresent=False)):
            v=request();modify(v)
            with self.assertRaises(RuntimeError):M.open_request(v)
    def test_modes_and_caption_scope(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
        self.assertEqual(runner['pointer_options'](None),{})
        for case in M.CASES:
            M.mode(case,'success',True);self.assertEqual(runner['pointer_options'](None,case),{'caption_timeout_ms':2000})
        for args in [('success','success',False),('success','rejected',True),('unknown','success',True),('success','success',True,True)]:
            with self.assertRaises(RuntimeError):M.mode(*args)
        with self.assertRaises(RuntimeError):M.mode('success','success',True,search='person-success')
    def test_stage_inventory_one_rich_send_only(self):
        for case,stages in M.STAGES.items():
            self.assertEqual(len(stages),len(set(stages)))
            self.assertEqual('share-settled' in stages,case in ('success','reuse'))
            self.assertEqual('group-opening' in stages,case!='reuse')
        self.assertEqual(M.STAGES['success'][9:13],M.TOKEN_STAGES)

class TokenCustody(unittest.TestCase):
    def setUp(self):
        self.q=dict(recipients=[dict(id=M.PEERS[0],label=M.LABELS[0],evidence=dict(id=M.PEERS[0],source=M.P.boot_member(M.peer(0)),interaction=True))],recipientRevision=3,selectedRecipient=None,search=dict(query=''))
        self.native=dict(multiple=dict(original=True,recipientRevision='3',query='',recipients=[dict(id=M.PEERS[0],nodeId=4,connected=True,selected='false',original=True,remove=dict(nodeId=5,connected=True,disabled=False,label='Remove '+M.LABELS[0],rect=dict(left=0,top=0,right=20,bottom=20,width=20,height=20),hitPoint=dict(x=10,y=10)))]))
    def check(self,previous=None):return M.tokens(self.q,self.native,[M.PEERS[0]],[100,100],lambda r,v:None,previous or {})
    def test_original_token_positive(self):self.assertEqual(self.check(),{M.PEERS[0]:(4,5)})
    def test_replacement_token_refused(self):
        with self.assertRaises(RuntimeError):self.check({M.PEERS[0]:(40,50)})
    def test_foreign_source_refused(self):
        self.q['recipients'][0]['evidence']['interaction']=False
        with self.assertRaises(RuntimeError):self.check()
    def test_invisible_remove_refused(self):
        self.native['multiple']['recipients'][0]['remove']['hitPoint']=dict(x=100,y=100)
        with self.assertRaises(RuntimeError):self.check()
    def test_selected_state_refused(self):
        self.native['multiple']['recipients'][0]['selected']='true'
        with self.assertRaises(RuntimeError):self.check()
    def test_actual_event_provenance(self):
        v=dict(ok=True,kind='Backspace',peer=M.PEERS[0],nodeId=2,defaultPrevented=True,queryFocused=True,rect={},event=dict(type='keydown',key='Backspace',button=None,isTrusted=False,targetNodeId=2),before=dict(query=''))
        M.token_action(v,'Backspace',M.PEERS[0],[],lambda r,v:None)
        for key,value in [('defaultPrevented',False),('queryFocused',False)]:
            bad=copy.deepcopy(v);bad[key]=value
            with self.assertRaises(RuntimeError):M.token_action(bad,'Backspace',M.PEERS[0],[],lambda r,v:None)
        v['event']['isTrusted']=True
        with self.assertRaises(RuntimeError):M.token_action(v,'Backspace',M.PEERS[0],[],lambda r,v:None)

class SinglePersonQueryReset(unittest.TestCase):
    def sample(self):
        import test_workflow_management_share_search as T
        original=T.state()[0]['search'];server=T.sample();v=server['confirmationAdd']['share']['search']
        q=dict(recipients=[dict(id=M.PEERS[0])],search=dict(query='',revision=4,status='recipient',results=[],reads=[],publications=[],failures=[],ranking=None,route=None,history=[original]))
        return q,v,server
    def test_actual_first_add_clear_keeps_original_search(self):
        from workflow_management_share_search_current import state
        q,v,server=self.sample();self.assertIsNone(state('person-success','person-opening',q,v,server))
    def test_no_old_query_or_fabricated_receipt_after_clear(self):
        from workflow_management_share_search_current import state
        for mutate in (lambda q:q['search'].update(query='Dana'),lambda q:q['recipients'][0].update(id='UOTHER001'),lambda q:q['search']['history'][0]['reads'].pop(),lambda q:q['search'].update(reads=[{}])):
            q,v,server=self.sample();mutate(q)
            with self.assertRaises(RuntimeError):state('person-success','person-opening',q,v,server)
    def test_resolved_token_reranks_locally_with_original_acquired_sources(self):
        import workflow_management_share_search_current as C
        q,v,server=self.sample();q['search'].update(status='ready',route=dict(route='local',online=True,hostPolicy='OrdinaryClientV2',wire='',routeObserved=True,routable=True,transportHealth='unknown',realtimeConnected=True))
        with patch.object(C.E,'ranking_proof',return_value='verified') as rank:
            self.assertEqual(C.state('person-success','selection-ready',q,v,server),'verified')
        self.assertFalse(rank.call_args.kwargs['fresh']);self.assertEqual(len(rank.call_args.kwargs['publications']),2)

class GroupChoice(unittest.TestCase):
    def sample(self):
        src=source();src['canonical'].update(name=M.DISPLAY,_internal_name=M.DISPLAY,is_member=True);server=dict(seed=dict(boot=dict(users=[M.peer(0),M.peer(1)])))
        group=dict(membership=copy.deepcopy(src['canonical']),selfId='UWORKFLOW',peers=[dict(id=id,raw=M.peer(i),effective=M.peer(i),source=M.P.boot_member(M.peer(i))) for i,id in enumerate(M.PEERS)])
        q=dict(people=[],rows=[dict(id=M.DEST,raw=dict(M.channel(),fromAnotherTeam=False),label=', '.join(M.LABELS),group=group)],selected=M.DEST,search=dict(results=[]),options=[dict(id=M.DEST,label=', '.join(M.LABELS))])
        return q,server,src
    def test_pinned_group_has_actual_members_and_fresh_peers(self):M.options(*self.sample())
    def test_canonical_projection_cannot_replace_original_posting_raw(self):
        q,s,v=self.sample();q['rows'][0]['raw']=copy.deepcopy(v['canonical'])
        with self.assertRaises(RuntimeError):M.options(q,s,v)
    def test_foreign_canonical_cannot_grant_original_posting_row(self):
        q,s,v=self.sample();v['canonical']['fromAnotherTeam']=True
        with self.assertRaises(RuntimeError):M.options(q,s,v)
    def test_group_not_granted_by_label_or_id(self):
        for mutate in (lambda q:q['rows'][0]['group'].update(membership={}),lambda q:q['rows'][0]['group']['peers'][1]['raw'].update(id=M.PEERS[0]),lambda q:q['rows'][0]['raw'].update(members=[]),lambda q:q['options'].append(dict(id='GOTHER001',label='unobserved'))):
            q,s,v=self.sample();mutate(q)
            with self.assertRaises(RuntimeError):M.options(q,s,v)
    def test_posting_required_separately_on_send(self):
        server=sample('success','share-settled');v=server['confirmationAdd']['share']['multiple'];v['reads'].pop();v['replies'].pop()
        with self.assertRaises(RuntimeError):M.acquisition('success','share-settled',server)
    def test_missing_second_peer_cannot_open(self):
        server=sample('success','group-opening');v=server['confirmationAdd']['share']['multiple'];v['reads']=v['reads'][:2];v['replies']=v['replies'][:2]
        with self.assertRaises(RuntimeError):M.acquisition('success','group-opening',server)

class RankedPostingWaves(unittest.TestCase):
    def sample(self):
        ids=['CADDPRIV01','CADDREAD01','CADDDEST01'];rows=[]
        for id in ids*2:
            rows.append(dict(serial=len(rows)+1,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=copy.deepcopy(M.S.OWNER),result=dict(raw=dict(ok=True,channel=copy.deepcopy(M.A.CHANNELS[id]),opaque_receipt='original-share-channel'))))
        rank=lambda rev:dict(revision=rev,status='partial',query='',ranking=dict(source='share_dialog_channel_picker',results=[dict(id=id) for id in [M.PEERS[0],*ids,M.PEERS[1]]]))
        search=rank(52);search['history']=[rank(39)]
        return dict(confirmationAdd=dict(share=dict(reads=rows))),search
    def test_second_actual_rank_revalidates_all_original_channel_sources(self):
        server,s=self.sample();M.ordinary_reads('first-recipient',server,copy.deepcopy(server['confirmationAdd']['share']['reads'][:3]),search=s)
    def test_missing_extra_reordered_or_unproved_wave_refused(self):
        for mode in ('missing','extra','reordered','no-history','duplicate-revision'):
            server,s=self.sample();rows=server['confirmationAdd']['share']['reads']
            if mode=='missing':rows.pop()
            elif mode=='extra':r=copy.deepcopy(rows[-1]);r['serial']=len(rows)+1;rows.append(r)
            elif mode=='reordered':rows[3],rows[4]=rows[4],rows[3];rows[3]['serial']=4;rows[4]['serial']=5
            elif mode=='no-history':s['history']=[]
            else:s['revision']=39
            with self.subTest(mode=mode),self.assertRaises(RuntimeError):M.ordinary_reads('first-recipient',server,search=s)
    def test_generated_group_label_uses_original_member_names(self):
        self.assertEqual(M.DISPLAY,'@'+','.join(M.peer(i)['name'] for i in range(2)))
        self.assertNotEqual(M.DISPLAY,', '.join(M.LABELS))

class DisplayedMappedRecipient(unittest.TestCase):
    def sample(self,selected='',ranked=None):
        ranked=list(M.PEERS) if ranked is None else ranked
        people=[dict(id=id,source=M.P.boot_member(M.peer(i)),interaction=True) for i,id in enumerate(M.PEERS)]
        row=dict(id=M.D.DEST,label=M.LABELS[0],raw=dict(M.D.channel(),fromAnotherTeam=False),peer=dict(id=M.PEERS[0],raw=M.peer(0),effective=M.peer(0),source=M.P.boot_member(M.peer(0))))
        q=dict(people=people,rows=[row],recipients=[],selected=selected,search=dict(results=ranked))
        visible=selected==M.D.DEST or M.D.DEST in ranked
        ids=[id for id in ranked if not(visible and id==M.PEERS[0])]
        if selected==M.D.DEST and selected not in ids:ids.append(selected)
        q['options']=[dict(id=id,label=M.LABELS[0] if id==M.D.DEST else M.LABELS[M.PEERS.index(id)]) for id in ids]
        return q,dict(seed=dict(boot=dict(users=[M.peer(0),M.peer(1)]))),{}
    def test_ranked_person_survives_unranked_dm(self):M.options(*self.sample())
    def test_selected_dm_suppresses_duplicate_and_stays_pinned(self):M.options(*self.sample(selected=M.D.DEST))
    def test_ranked_dm_suppresses_duplicate_in_rank_order(self):M.options(*self.sample(ranked=[M.PEERS[1],M.D.DEST,M.PEERS[0]]))
    def test_dropped_person_or_duplicate_visible_dm_refused(self):
        for selected in ('',M.D.DEST):
            q,s,v=self.sample(selected=selected)
            if selected:q['options'].insert(0,dict(id=M.PEERS[0],label=M.LABELS[0]))
            else:q['options'].pop(0)
            with self.assertRaises(RuntimeError):M.options(q,s,v)
    def search_sample(self):
        import workflow_management_share_search as Q
        p=Q.D.peer();q=dict(people=[dict(id=Q.D.PEER,source=Q.D.peer_source(),interaction=True)],rows=[dict(id=Q.D.DEST,raw=dict(Q.D.channel(),fromAnotherTeam=False),peer=dict(id=Q.D.PEER,raw=p,effective=p,source=Q.D.peer_source()))],recipients=[],selected='',search=dict(results=[Q.D.PEER]),options=[dict(id=Q.D.PEER,label=Q.D.LABEL)])
        return Q,q,dict(seed=dict(boot=dict(users=[p])))
    def test_current_typed_search_retains_ranked_person(self):
        Q,q,s=self.search_sample();Q.options(q,s)
        q['options']=[]
        with self.assertRaises(RuntimeError):Q.options(q,s)
    def test_historical_typed_schema_retains_original_projection(self):
        Q,q,s=self.search_sample();del q['recipients'];q['options']=[];Q.options(q,s)

class OrdinaryObservationLane(unittest.TestCase):
    def sample(self,stage='share-open'):
        from test_workflow_management_share_empty import server
        s=server(stage=stage);p=s['confirmationAdd']['share']
        p['ranking']=p.pop('search')['ranking'];p['ranking']['case']='ordinary-share'
        return s
    def check(self,s,stage='share-open',previous=None):
        return E.observation_ledger('initial',stage,s,previous,ordinary_share=True)
    def test_original_five_settled_reads_and_reuse(self):
        s=self.sample();v=self.check(s)
        self.assertEqual(len(v['requests']),5)
        self.check(s,'first-recipient',copy.deepcopy(v))
    def test_original_prefix_has_no_reads(self):self.check(self.sample('checked'),'checked')
    def test_missing_or_misplaced_ledger_refused(self):
        for mode in ('missing','moved','duplicated','wrong-case'):
            s=self.sample();p=s['confirmationAdd']['share']
            if mode=='missing':del p['ranking']
            elif mode=='moved':p['search']={'ranking':p.pop('ranking')}
            elif mode=='duplicated':p['search']={'ranking':copy.deepcopy(p['ranking'])}
            else:p['ranking']['case']='existing-search'
            with self.subTest(mode=mode),self.assertRaises(RuntimeError):self.check(s)
    def test_default_search_lane_does_not_accept_ordinary(self):
        with self.assertRaises(RuntimeError):E.observation_ledger('initial','share-open',self.sample())
    def test_original_request_receipt_and_settlement_remain_strict(self):
        mutations=(lambda v:v['requests'][0].update(endpoint='https://wrong.test/api/client.gantryBoot'),
                   lambda v:v['requests'][0]['owner'].update(account='foreign'),
                   lambda v:v['responses'][1].update(raw={'ok':True}),
                   lambda v:v['settlements'].pop(),
                   lambda v:v['settlements'][0].update(returned=False))
        for mutate in mutations:
            s=self.sample();mutate(s['confirmationAdd']['share']['ranking'])
            with self.assertRaises(RuntimeError):self.check(s)
    def test_immutable_original_history_not_replaced(self):
        s=self.sample();old=copy.deepcopy(self.check(s));old['responses'][0]['raw']['opaque_receipt']='replaced'
        with self.assertRaises(RuntimeError):self.check(s,previous=old)

if __name__=='__main__':unittest.main()
