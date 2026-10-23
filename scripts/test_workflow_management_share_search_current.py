"""Current-schema observations retain original producers; historical ABI is explicit."""
import copy
import unittest
from unittest.mock import patch
import workflow_management_share_search_current as C
import workflow_management_share_empty as E
from test_workflow_management_share_empty import server


def ranked(fresh=True):
    s=server();raw=E.Q.person(0);s['seed']['boot']['users']=[raw];s['seed']['canonicalChannels']=[]
    item=copy.deepcopy(raw);item['facts']=dict(member=True,active=True,team='TWORKFLOW',self=False,bot=False,external=False,comparisonName=raw['real_name'],bestName=raw['real_name'])
    original=[dict(method='POST',raw=E.R.observation_reply(m,'cold-mixed',s['seed']['boot']['self'])) for m in ('client.gantryBoot','users.prefs.get','experiments.getByUser')]
    p=dict(source='share_dialog_channel_picker',query='',passthrough=False,observationSource='fresh' if fresh else 'reused',observationReads=copy.deepcopy(original),reads=copy.deepcopy(original) if fresh else [],memberItems=[item],channelItems=[],selectors=dict(selfId='UWORKFLOW',teamId='TWORKFLOW',enterpriseId=None,enterprise=False,experiments=dict(autocomplete_affinity_score='off',autocomplete_affinity_ml_model='off'),members={raw['id']:item}),results=[dict(id=raw['id'])])
    q=dict(search=dict(query='',ranking=p,results=[raw['id']],status='ready',failures=[]))
    return s,q


class CurrentObservationProof(unittest.TestCase):
    def test_fresh_and_reused_exact_original_receipts(self):
        for fresh in (True,False):
            s,q=ranked(fresh);E.ranking_proof(q,s,fresh=fresh,require_cutoff=False)
    def reject(self,mutate,fresh=False):
        s,q=ranked(fresh);mutate(q['search']['ranking'])
        with self.assertRaises(RuntimeError):E.ranking_proof(q,s,fresh=fresh,require_cutoff=False)
    def test_reuse_does_not_fabricate_current_requests(self):self.reject(lambda p:p.update(reads=p['observationReads']))
    def test_reuse_retains_complete_original_acquisition(self):self.reject(lambda p:p['observationReads'].pop())
    def test_fresh_cannot_claim_reused(self):self.reject(lambda p:p.update(observationSource='reused'),True)
    def test_original_preference_body_immutable(self):self.reject(lambda p:p['observationReads'][1]['raw'].update(prefs={}))
    def test_unobserved_member_refused(self):self.reject(lambda p:p['memberItems'][0].update(id='UFOREIGN'))
    def test_original_family_publication_required(self):
        s,q=ranked(False);remote=E.Q.person(101);q['search']['ranking']['memberItems'][0]['id']=remote['id']
        with self.assertRaises(RuntimeError):E.ranking_proof(q,s,fresh=False,require_cutoff=False)
    def test_complete_original_local_collection_required(self):
        s,q=ranked();q['search']['ranking']['memberItems']=[]
        with self.assertRaises(RuntimeError):E.ranking_proof(q,s,fresh=True,require_cutoff=False)
    def test_channel_prefix_cannot_include_member_family(self):
        s,q=ranked();q['search']['query']='#'
        with self.assertRaises(RuntimeError):E.ranking_proof(q,s,fresh=True,require_cutoff=False)
    def test_historical_abi_is_explicit(self):
        self.assertFalse(C.current(dict(confirmationAdd=dict(share={}))))
        self.assertTrue(C.current(server()))
    def test_initial_requires_no_remote_search(self):
        s,q=ranked();q['search'].update(revision=1,reads=[{'raw':{}}],publications=[])
        with self.assertRaises(RuntimeError):C.state('person-success','share-open',q,{},s)
    def test_pending_keeps_original_acquisition_unpublished(self):
        s=server();q=dict(search=dict(revision=2,query='Dana',status='searching',ranking=None,reads=[],failures=[],publications=[]))
        self.assertIsNone(C.state('person-success','query-pending',q,{},s))
        q['search']['publications']=[{}]
        with self.assertRaises(RuntimeError):C.state('person-success','query-pending',q,{},s)
    def test_typed_reuse_exact_ordered_source(self):
        s=server();v=dict(requests=[],responses=[])
        for n,(method,query,marker) in enumerate(E.Q.sequences('person-success','query-ready'),1):
            discovery=method=='api.getFlannelHttpUrl';raw,keys=(E.F.DISCOVERY_RAW,None) if discovery else E.Q.page(method,query,marker)
            v['requests'].append(dict(method=method,body=None if discovery else E.Q.body(method,query,marker)))
            v['responses'].append(dict(raw=raw,orderedKeys=keys))
        reads=[dict(kind='channels' if r['method']=='channels/search' else 'members',method=r['method'],arguments=r['body'],raw=a['raw'],orderedKeys=a['orderedKeys']) for r,a in zip(v['requests'],v['responses'])]
        pubs=[E.Q.publication(m,'Dana') for m in ('users/search','channels/search')]
        q=dict(search=dict(query='Dana',revision=2,reads=reads,publications=pubs,route=dict(route='remote',online=True,hostPolicy='OrdinaryClientV2',wire='Dana',routeObserved=True,routable=True,transportHealth='unknown',realtimeConnected=True)))
        with patch.object(E,'ranking_proof',return_value={'proof':'checked'}) as check:
            self.assertEqual(C.state('person-success','query-ready',q,v,s),{'proof':'checked'})
            check.assert_called_once_with(q,s,fresh=False,publications=pubs,require_cutoff=False)
        q['search']['reads'][1]['orderedKeys']=[]
        with self.assertRaises(RuntimeError):C.state('person-success','query-ready',q,v,s)

if __name__=='__main__':unittest.main()
