"""Closed synthetic ordered-page and custody mutations; no native/network launch."""
import copy
import json
from pathlib import Path
import unittest
import workflow_management_share_search as X


def sample(case='person-success',stage='query-ready'):
    plan=X.sequences(case,stage);req=[];res=[]
    for i,(method,query,marker) in enumerate(plan,1):
        discovery=method=='api.getFlannelHttpUrl'
        req.append(dict(serial=i,method=method,query=query,body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else X.body(method,query,marker),endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method,retrySafe=False,owner=copy.deepcopy(X.S.OWNER)))
        raw,keys=(X.F.DISCOVERY_RAW,None) if discovery else X.page(method,query,marker)
        r=dict(serial=i,raw=raw,orderedKeys=keys)
        if query=='offline':r=dict(serial=i,error='offline')
        elif query=='rejected':r=dict(serial=i,raw=dict(ok=False,error='not_allowed',opaque_receipt='original-search-rejected'),orderedKeys=None)
        res.append(r)
    held=[2] if stage in ('query-pending','member-pending') or case=='held-cancel' and stage=='share-cancelled' else [2,4] if stage=='replacement-pending' else [5] if stage=='channel-pending' else []
    released=[] if len(plan)<2 or stage in ('query-pending','member-pending','replacement-pending') or case=='held-cancel' and stage=='share-cancelled' else [2]
    if case=='rapid-query' and stage not in X.A.PREFIX+('share-open','query-pending','replacement-pending'):released=[2,4]
    if case=='prefixes' and stage in ('channel-ready','share-cancelled'):released=[2,5]
    v=dict(case=case,postingSources=[],requests=req,responses=res,held=held,releases=[dict(serial=i,receiverRetired=case in ('rapid-query','held-cancel') and i==2,delivered=not(case in ('rapid-query','held-cancel') and i==2)) for i in released],settlements=[dict(serial=i,returned=not(case in ('rapid-query','held-cancel') and i==2)) for i in range(1,len(plan)+1) if i not in held],networkCalls=0,scheduling='first-search-page-per-wire-query-held-until-native-capture',initialPeople=51,remotePersonInitiallyAbsent=True,remoteChannelInitiallyAbsent=True)
    return dict(confirmationAdd=dict(share=dict(search=v)))

def state(case='person-success',stage='query-ready'):
    v=sample(case,stage)['confirmationAdd']['share']['search']
    query='Dana' if case=='person-success' else 'research' if case=='channel-success' else case
    if case=='prefixes':query='@Dana' if stage=='member-ready' else '#research'
    if case=='rapid-query':query='#research'
    req=list(zip(v['requests'],v['responses']))
    if case=='prefixes' and stage!='member-ready':req=req[3:]
    if case=='rapid-query':req=req[2:]
    reads=[]
    for r,response in req:
        d=r['method']=='api.getFlannelHttpUrl';kind=('channels' if query.startswith('#') else 'members') if d else 'members' if r['method']=='users/search' else 'channels'
        x=dict(kind=kind,method=r['method'],arguments=None if d else r['body'])
        if r['query'] in ('rejected','offline'):x['failure']=dict(kind='offline' if case=='offline' else 'api-or-invalid')
        else:x.update(raw=response['raw'],orderedKeys=response['orderedKeys'])
        reads.append(x)
    failed=case in ('rejected','offline');methods=['channels/search'] if query.startswith('#') else ['users/search'] if query.startswith('@') else ['users/search','channels/search']
    pubs=[] if failed else [X.publication(m,query.lstrip('@#')) for m in methods]
    return dict(search=dict(query=query,revision=10,status='failed' if failed else 'ready',reads=reads,publications=pubs,results=None if failed else [id for p in pubs for id in p['ids']][:24],failures=[dict(kind=k,phase='read',failure=dict(kind='offline' if case=='offline' else 'api-or-invalid')) for k in ('members','channels')] if failed else [],history=[])),v

class SearchLedger(unittest.TestCase):
    def test_all_exact_case_stage_ledgers(self):
        for case in X.BASE_CASES:
            stages=X.STAGES[case]
            for stage in stages:
                with self.subTest(case=case,stage=stage):X.ledger(case,stage,sample(case,stage))
    def test_valid_completed_originals(self):
        for case,stage in [('person-success','query-ready'),('channel-success','query-ready'),('prefixes','member-ready'),('prefixes','channel-ready'),('rapid-query','query-ready'),('offline','query-failed'),('rejected','query-failed')]:
            with self.subTest(case=case,stage=stage):q,v=state(case,stage);X.state_proof(case,stage,q,v)
    def reject_ledger(self,mutate,case='person-success',stage='query-ready'):
        s=sample(case,stage);mutate(s['confirmationAdd']['share']['search'])
        with self.assertRaises(RuntimeError):X.ledger(case,stage,s)
    def test_wrong_owner(self):self.reject_ledger(lambda v:v['requests'][1]['owner'].update(team='OTHER'))
    def test_wrong_count(self):self.reject_ledger(lambda v:v['requests'][1]['body'].update(count=25))
    def test_extra_remote_family(self):self.reject_ledger(lambda v:v['requests'].append(copy.deepcopy(v['requests'][-1])),case='prefixes',stage='member-ready')
    def test_wrong_marker(self):self.reject_ledger(lambda v:v['requests'][2]['body'].update(marker='guessed'))
    def test_lost_order(self):self.reject_ledger(lambda v:v['responses'][1]['orderedKeys'].reverse())
    def test_changed_original_raw(self):self.reject_ledger(lambda v:v['responses'][1]['raw']['results']['z99'].update(id='UOTHER001'))
    def test_replay(self):self.reject_ledger(lambda v:v['requests'].append(copy.deepcopy(v['requests'][1])))
    def test_fake_settlement(self):self.reject_ledger(lambda v:v['settlements'].append(dict(serial=99,returned=True)))
    def test_fake_release(self):self.reject_ledger(lambda v:v['releases'][0].update(delivered=False))
    def test_failed_raw_not_empty_success(self):self.reject_ledger(lambda v:v['responses'][1].update(raw=dict(ok=True,results=[])),case='rejected',stage='query-failed')
    def reject_state(self,mutate,case='person-success',stage='query-ready'):
        q,v=state(case,stage);mutate(q['search'])
        with self.assertRaises(RuntimeError):X.state_proof(case,stage,q,v)
    def test_no_25th_result(self):self.reject_state(lambda s:s['results'].append('USEARCH124'))
    def test_no_partial_page_publication(self):self.reject_state(lambda s:s['publications'][0].update(pageCount=1))
    def test_final_order(self):self.reject_state(lambda s:s['results'].reverse())
    def test_overflow_marker(self):self.reject_state(lambda s:s['publications'][0].update(nextMarker=None))
    def test_raw_read_required(self):self.reject_state(lambda s:s['reads'].pop())
    def test_failed_family_no_grant(self):self.reject_state(lambda s:s.update(publications=[X.publication('channels/search','research')]),'offline','query-failed')
    def test_no_stale_query_adoption(self):self.reject_state(lambda s:s.update(query='oldDana'),'rapid-query','query-ready')
    def test_initial50_does_not_seed_remote(self):
        self.assertEqual(len({X.person(n)['id'] for n in range(51)}),51)
        self.assertNotIn(X.D.PEER,[X.person(n)['id'] for n in range(51)])
    def test_raw_false_is_independent_of_published_membership(self):
        raw,keys=X.page('channels/search','research')
        self.assertFalse(raw['results'][keys[0]]['is_member'])
        self.assertEqual(raw['member_channels'],[X.CHANNEL])
        self.assertTrue(X.channel()['is_member'])
        self.assertFalse(X.publication('channels/search','research')['rows'][0]['is_member'])
        self.assertEqual(X.publication('channels/search','research')['membershipIds'],[X.CHANNEL])
    def test_missing_membership_companion_rejected(self):
        self.reject_state(lambda s:s['publications'][-1].update(membershipIds=[]),'channel-success','query-ready')
    def test_preapplied_raw_membership_rejected(self):
        self.reject_ledger(lambda v:v['responses'][-1]['raw']['results']['z99'].update(is_member=True),'channel-success','query-ready')
    def test_pre_info_source_transform(self):
        s=sample('channel-success','query-ready');parent=s['confirmationAdd']['share']
        parent['reads']=[dict(serial=1,method='conversations.info',body=[['channel',X.CHANNEL],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=X.S.OWNER,result=dict(raw=dict(ok=True,channel=X.channel(),opaque_receipt='original-search-channel-posting')))]
        parent['search']['postingSources']=[dict(readSerial=1,current=True,canonical=dict(X.channel(),fromAnotherTeam=False))]
        opened={'proof':{'server':{'confirmationAdd':{'share':{'reads':[]}}}}}
        X.ordinary('query-ready',s,opened)
        for change in (lambda v:v.update(current=False),lambda v:v.update(readSerial=2),lambda v:v['canonical'].update(is_member=False)):
            bad=copy.deepcopy(s);change(bad['confirmationAdd']['share']['search']['postingSources'][0])
            with self.assertRaises(RuntimeError):X.ordinary('query-ready',bad,opened)
    def test_mode_isolation(self):
        for case in X.CASES:X.mode(case,'success',True)
        for args in [('person-success','rejected',True),('person-success','success',False),('person-success','success',True,True)]:
            with self.subTest(args=args),self.assertRaises(RuntimeError):X.mode(*args)
    def test_unrequested_identity(self):
        X.identity({},None)
        with self.assertRaises(RuntimeError):X.identity(dict(shareSearchCheck=True,shareSearchCase='person-success'),None)

if __name__=='__main__':unittest.main()
