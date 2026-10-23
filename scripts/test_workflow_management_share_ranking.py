"""Portable synthetic custody mutations plus independent retained-source replay."""
import copy
import json
from pathlib import Path
import subprocess
import unittest
import workflow_management_share_ranking as X


def sample(case='cold-mixed',stage='query-ready'):
    plan=X.sequences(case,stage);requests=[];responses=[]
    for n,(method,query,marker) in enumerate(plan,1):
        discovery=method=='api.getFlannelHttpUrl'
        requests.append(dict(serial=n,method=method,query=query,body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else X.Q.body(method,query,marker),endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method,retrySafe=False,owner=copy.deepcopy(X.S.OWNER)))
        raw,keys=(X.F.DISCOVERY_RAW,None) if discovery else X.page(method,query,marker)
        responses.append(dict(serial=n,raw=raw,orderedKeys=keys))
    query,pending=X.query_phase(case,stage)
    held=[len(plan)] if plan and pending else []
    if case=='stale-cancel':held=[] if stage=='late-settled' else [2,4] if len(plan)==4 else [2] if plan else []
    releases=([2,4] if stage=='late-settled' else []) if case=='stale-cancel' else [i+1 for i,r in enumerate(requests,1) if r['method']=='api.getFlannelHttpUrl' and i+1 not in held]
    retired={2,4} if case=='stale-cancel' and stage in ('share-cancelled','late-settled') else {2} if case=='stale-cancel' and stage=='replacement-pending' else set()
    settlements=[dict(serial=n,returned=n not in retired) for n in range(1,len(plan)+1) if n not in held or n in retired]
    v=dict(case='ranking',initialPeople=42,networkCalls=0,remotePersonInitiallyAbsent=True,remoteChannelInitiallyAbsent=True,scheduling='first-search-page-per-wire-query-held-until-native-capture',requests=requests,responses=responses,held=held,releases=[dict(serial=i,receiverRetired=case=='stale-cancel',delivered=case!='stale-cancel') for i in releases],settlements=settlements,postingSources=[])
    return dict(confirmationAdd=dict(share=dict(search=v,reads=[])))


def observations(case='cold-mixed'):
    server=sample(case);server['seed']=dict(boot=dict(self=dict(id='UWORKFLOW',team_id='TWORKFLOW')))
    methods=['search.autocomplete.model','search.autocomplete.offlineFeatures','client.gantryBoot','users.prefs.get']+([] if case=='read-failure' else ['experiments.getByUser'])
    v=dict(case=case,navigationOwner='12345678-1234-4234-8234-123456789abc',networkCalls=0,cacheInjection=False,canonicalWrites=0,localPeople=[X.person(n) for n in range(42)],modelMode='actual-read-write-coefficients' if case=='cached-write' else 'actual-read-absent-model',requests=[],responses=[],settlements=[])
    for i,m in enumerate(methods,1):
        v['requests'].append(dict(serial=i,kind='cache' if m.startswith('search.') else 'query',method=m,body=X.READ_BODIES.get(m,[['ssb_platform','windows'],['ssb_release_channel','prod']]),endpoint='https://example.test/api/'+m,retrySafe=False,owner=copy.deepcopy(X.S.OWNER)))
        v['responses'].append(dict(serial=i,error='offline') if case=='read-failure' and m=='users.prefs.get' else dict(serial=i,raw=X.observation_reply(m,case,server['seed']['boot']['self']),orderedKeys=None))
        v['settlements'].append(dict(serial=i,returned=True))
    server['confirmationAdd']['share']['search']['ranking']=v
    return server


class RankingCustody(unittest.TestCase):
    def seed(self):
        teams=[dict(id='TWORKFLOW',name='Home fixture')]
        channels=X.A.channels()
        return dict(seed=dict(unifiedUserClientEnabled={'TWORKFLOW':False},
            boot=dict(team=copy.deepcopy(teams[0]),workspaces=copy.deepcopy(teams),
                channels=[dict(id=X.A.ORIGINAL)]+copy.deepcopy(channels),
                users=[dict(id='UWORKFLOW'),dict(id='UHOME0001')]+[X.person(n) for n in range(42)]),
            init=dict(workspaces=copy.deepcopy(teams)),
            canonicalChannels=[dict(r,fromAnotherTeam=False) for r in channels]))
    def test_original_boot_workspace_membership(self):X.seed_proof(self.seed())
    def test_boot_membership_cannot_be_replaced_by_init(self):
        for value in [None,[],{},'TWORKFLOW']:
            s=self.seed();s['seed']['boot']['workspaces']=value
            with self.assertRaisesRegex(RuntimeError,'Boot workspace membership'):X.seed_proof(s)
        s=self.seed();del s['seed']['boot']['workspaces']
        with self.assertRaisesRegex(RuntimeError,'Boot workspace membership'):X.seed_proof(s)
    def test_boot_workspace_identity_and_original_shape(self):
        for rows in [[dict(id='TOTHER',name='Home fixture')],
                     [dict(id='TWORKFLOW',name='Home fixture')]*2,
                     [dict(id='TWORKFLOW',name='Home fixture',allow_external_users=True)]]:
            s=self.seed();s['seed']['boot']['workspaces']=rows
            with self.assertRaisesRegex(RuntimeError,'Boot workspace membership'):X.seed_proof(s)
    def test_boot_init_membership_and_team_must_agree(self):
        for field in ['init','boot']:
            s=self.seed()
            if field=='init':s['seed']['init']['workspaces']=[]
            else:s['seed']['boot']['team']['id']='TOTHER'
            with self.assertRaisesRegex(RuntimeError,'Boot workspace membership'):X.seed_proof(s)
    def test_every_pinned_acquisition_phase(self):
        for case in X.CASES:
            for stage in X.Q.STAGES['ranking:'+case]:
                with self.subTest(case=case,stage=stage):X.ledger(case,stage,sample(case,stage))
    def reject(self,mutate):
        s=sample();mutate(s['confirmationAdd']['share']['search'])
        with self.assertRaises(RuntimeError):X.ledger('cold-mixed','query-ready',s)
    def test_wrong_owner(self):self.reject(lambda v:v['requests'][1]['owner'].update(team='OTHER'))
    def test_wrong_count(self):self.reject(lambda v:v['requests'][1]['body'].update(count=25))
    def test_order_keys(self):self.reject(lambda v:v['responses'][1]['orderedKeys'].reverse())
    def test_nonoriginal_membership(self):self.reject(lambda v:v['responses'][3]['raw']['results']['z99'].update(is_member=True))
    def test_missing_membership(self):self.reject(lambda v:v['responses'][3]['raw'].update(member_channels=[]))
    def test_extra_remote_result(self):self.reject(lambda v:v['responses'][1]['raw']['results'].update(fake=X.person(99,True)))
    def test_wire_replay(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][-1])))
    def test_wrong_release(self):self.reject(lambda v:v['releases'][0].update(serial=4))
    def test_false_release_delivery(self):self.reject(lambda v:v['releases'][0].update(delivered=False))
    def test_late_adoption(self):
        s=sample('stale-cancel','late-settled');s['confirmationAdd']['share']['search']['settlements'][1]['returned']=True
        with self.assertRaises(RuntimeError):X.ledger('stale-cancel','late-settled',s)
    def test_prefix_does_not_call_other_family(self):
        for stage,method in [('member-ready','users/search'),('channel-ready','channels/search')]:
            p=X.sequences('prefixes',stage)
            current=p if stage=='member-ready' else p[3:]
            self.assertEqual([x[0] for x in current],['api.getFlannelHttpUrl',method,method])
    def test_full25_is_retained_only24_published(self):
        for method in ('users/search','channels/search'):
            p=X.publication(method,'review');self.assertEqual(len(p['rows']),24);self.assertTrue(p['nextMarker'].endswith('024'));self.assertEqual(p['pageCount'],2)
            if method=='channels/search':self.assertEqual(len(p['membershipIds']),25);self.assertTrue(all(not r['is_member'] for r in p['rows']))
    def test_original_observation_families(self):
        for case in ('cold-mixed','cached-write','read-failure'):
            X.observations(case,'query-failed' if case=='read-failure' else 'query-ready',observations(case))
    def test_failed_preferences_has_exact_acquisition_classification(self):
        server=sample('read-failure','query-failed');v=server['confirmationAdd']['share']['search']
        reads=[dict(kind='channels' if r['method']=='channels/search' else 'members',
                    method=r['method'],arguments=None if r['method']=='api.getFlannelHttpUrl' else r['body'],
                    raw=a['raw'],orderedKeys=a['orderedKeys']) for r,a in zip(v['requests'],v['responses'])]
        search=dict(revision=1,query='review',status='failed',results=None,ranking=None,reads=reads,
                    publications=[X.publication(m,'review') for m in ['users/search','channels/search']],
                    failures=[dict(stage='ranking',failure='search observations unavailable',
                                   diagnostic='share_ranking_stage:observations.names.read.POST'),
                              dict(stage='adoption',failure='source retired')])
        X.state_proof('read-failure','query-failed',dict(search=search),v,server)
        for wrong in [None,'share_ranking_stage:observations.names.membership','offline']:
            broken=copy.deepcopy(search)
            if wrong is None:del broken['failures'][0]['diagnostic']
            else:broken['failures'][0]['diagnostic']=wrong
            with self.assertRaisesRegex(RuntimeError,'Missing observations'):
                X.state_proof('read-failure','query-failed',dict(search=broken),v,server)
    def reject_observation(self,mutate):
        s=observations();mutate(s['confirmationAdd']['share']['search']['ranking'])
        with self.assertRaises(RuntimeError):X.observations('cold-mixed','query-ready',s)
    def test_model_injection(self):self.reject_observation(lambda v:v.update(cacheInjection=True))
    def test_absent_priority_map_cannot_claim_original_key_order(self):
        for keys in [[],['CADDDEST01']]:
            self.reject_observation(lambda v:v['responses'][1].update(orderedKeys=keys))
    def test_navigation_cannot_reuse_synthetic_account(self):self.reject_observation(lambda v:v.update(navigationOwner=X.S.OWNER['account']))
    def test_navigation_missing_namespace(self):self.reject_observation(lambda v:v.pop('navigationOwner'))
    def test_navigation_non_v4_or_noncanonical_namespace(self):
        for value in ['12345678123442348234123456789abc','12345678-1234-1234-8234-123456789abc','12345678-1234-4234-0234-123456789abc']:
            self.reject_observation(lambda v:v.update(navigationOwner=value))
    def test_navigation_namespace_stable_with_original_permission_owner(self):
        s=observations();old=copy.deepcopy(s['confirmationAdd']['share']['search']['ranking'])
        X.observations('cold-mixed','query-ready',s,old)
        s['confirmationAdd']['share']['search']['ranking']['navigationOwner']='22345678-1234-4234-8234-123456789abc'
        with self.assertRaises(RuntimeError):X.observations('cold-mixed','query-ready',s,old)
        s=observations();s['confirmationAdd']['share']['search']['ranking']['requests'][0]['owner']['account']=old['navigationOwner']
        with self.assertRaises(RuntimeError):X.observations('cold-mixed','query-ready',s)
    def test_wrong_read_supplement(self):self.reject_observation(lambda v:v['responses'][0]['raw'].update(model=X.WEIGHTS))
    def test_missing_complete_preferences(self):self.reject_observation(lambda v:v['responses'][3]['raw'].pop('prefs'))
    def test_fabricated_boundary_experiment(self):self.reject_observation(lambda v:v['responses'][4]['raw']['assignments'].update(can_match_beyond_word_boundaries=dict(group='on')))
    def test_unobserved_cache_callback(self):self.reject_observation(lambda v:v['settlements'].pop())
    def test_query_before_acquisition(self):
        with self.assertRaises(RuntimeError):X.observations('cold-mixed','query-pending',observations())
    def test_membership_precedes_independent_info(self):
        s=sample();p=s['confirmationAdd']['share'];raw=X.channel(23,True)
        p['reads']=[dict(serial=1,method='conversations.info',body=[['channel',raw['id']],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=X.S.OWNER,result=dict(raw=dict(ok=True,channel=raw,opaque_receipt='original-ranking-destination-posting')))]
        p['search']['postingSources']=[dict(readSerial=1,current=True,canonical=X.canonical_channel(23))]
        X.ordinary('query-ready',s,[])
        for key,value in [('current',False),('readSerial',2)]:
            b=copy.deepcopy(s);b['confirmationAdd']['share']['search']['postingSources'][0][key]=value
            with self.assertRaises(RuntimeError):X.ordinary('query-ready',b,[])
        for key,value in [('is_member',False),('_name_lc','wrong'),('is_shared',False),
                          ('ranking_fixture_opaque',{}),('scroll_top',0)]:
            b=copy.deepcopy(s);b['confirmationAdd']['share']['search']['postingSources'][0]['canonical'][key]=value
            with self.assertRaises(RuntimeError):X.ordinary('query-ready',b,[])
        b=copy.deepcopy(s);b['confirmationAdd']['share']['search']['postingSources'][0]['canonical']=dict(raw,fromAnotherTeam=False)
        with self.assertRaises(RuntimeError):X.ordinary('query-ready',b,[])
    def test_mode_remains_opt_in(self):
        for case in X.CASES:X.Q.mode('ranking:'+case,'success',True)
        with self.assertRaises(RuntimeError):X.Q.mode('ranking:cold-mixed','success',False)
        with self.assertRaises(RuntimeError):X.Q.identity(dict(shareSearchCheck=True,shareSearchCase='ranking',shareRankingCase='cached-write'),'ranking:cold-mixed')


class RetainedReplay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Reuse frozen source-produced vectors; ask actual retained methods for
        # full feature rows, then mutate independently at the verifier boundary.
        script="""import fs from 'node:fs';import {reset,setState,run} from './tmp/workflow-picker-ranking-v3-prep/loader.mjs';
const v=JSON.parse(fs.readFileSync('tmp/workflow-share-search-ranking-20261021/fixture/source-vectors.json')).variants;
const out=v.map(x=>{reset();setState(x.state);let groups=structuredClone(x.groups),r=run('review',groups,{source:'share_dialog_channel_picker',limit:24,model:'autocompleteFallback',avoidExactMatchForMembers:true});let p={source:'share_dialog_channel_picker',query:'review',passthrough:false,nowMillis:1760000000000,selectors:x.state,inputCount:92,memberItems:groups.members.map(r=>r.item),channelItems:groups.channels.map(r=>r.item),groups,matches:[],results:[]};for(let kind of ['members','channels'])groups[kind].forEach((r,i)=>p.matches.push({id:r.item.id,index:i,kind}));p.results=r.result.map(r=>{let kind=r.type==='member'?'members':'channels';return{id:r.id,index:groups[kind].findIndex(x=>x.item.id===r.id),kind,features:r.features,modelScore:r.modelScore}});return{proof:p,model:x.state.modelWeights??null,remote:Object.fromEntries(['members','channels'].map(k=>[k,groups[k].filter(x=>x.source==='remote').map(x=>x.item.id)]))}});console.log(JSON.stringify(out));"""
        run=subprocess.run(['node','--input-type=module','-e',script],capture_output=True,text=True,encoding='utf-8',timeout=120)
        if run.returncode:raise AssertionError(run.stderr)
        cls.vectors=json.loads(run.stdout)
    def test_actual_retained_cold_and_write(self):X.replay(self.vectors)
    def test_wrong_mixed_order(self):
        v=copy.deepcopy(self.vectors[:1]);v[0]['proof']['results'][0:2]=reversed(v[0]['proof']['results'][0:2])
        with self.assertRaises(RuntimeError):X.replay(v)
    def test_wrong_feature(self):
        v=copy.deepcopy(self.vectors[:1]);v[0]['proof']['results'][0]['features'][0][1]+=1
        with self.assertRaises(RuntimeError):X.replay(v)
    def test_read_supplement_not_write(self):
        v=copy.deepcopy(self.vectors[1:]);v[0]['model']['WRITE_CHANNEL_IS_MEMBER']=-8
        with self.assertRaises(RuntimeError):X.replay(v)


class NativeDShape(unittest.TestCase):
    def test_original_http_observations_and_exact_factory_ledger(self):
        path=Path('tmp/workflow-share-search-ranking-20261021/integration/cold-mixed-normal-d/terminal.json')
        r=json.loads(path.read_text(encoding='utf-8'))
        row=next(v for v in r['stages'] if v['stage']=='query-ready');p=row['proof']
        q=p['firstPublish']['attempts'][-1]['confirmationShare'];server=p['server']
        v=server['confirmationAdd']['share']['search']
        # Historical D supplied [] ordering for an absent priority map. Preserve
        # the original artifact and explicitly refuse it under corrected custody.
        with self.assertRaisesRegex(RuntimeError,'immutable observation response'):
            X.observations('cold-mixed','query-ready',server)
        X.state_proof('cold-mixed','query-ready',q,v,server)
        for wrong in ['client.gantryBoot','GET']:
            changed=copy.deepcopy(q);changed['search']['ranking']['reads'][0]['method']=wrong
            with self.assertRaisesRegex(RuntimeError,'Ranker lacked original'):
                X.state_proof('cold-mixed','query-ready',changed,v,server)
        changed=observations()
        reads=changed['confirmationAdd']['share']['search']['ranking']['requests']
        next(v for v in reads if v['method']=='users.prefs.get')['endpoint']='https://example.test/api/client.gantryBoot'
        with self.assertRaisesRegex(RuntimeError,'factory/owner'):X.observations('cold-mixed','query-ready',changed)


if __name__=='__main__':unittest.main()
