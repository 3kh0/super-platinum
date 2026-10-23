"""Portable receipt examples and counterexamples; never launches native or Slack."""
import copy,json,runpy,unittest
from pathlib import Path
import slack_list_plain_member_save as V
import slack_list_plain_member_save_facts as F
import test_list_plain_channel_save as C
D=C.D

def options(query,source):
    names=['alex group slistgrp1','Alex group SLISTGRP1','alex-team','fixture-self','harbor']
    texts={'',query,'harbor','Harbor Person','harbor@fixture.invalid','fixture-self','username-must-not-win','Alex group SLISTGRP1','alex group slistgrp1','alex-team','@alex-team'}
    return dict(rawQuery=query,formattedQuery=query,locale='en-US',query=dict(normalized=query,matcherNormalized=query),
        options=dict(ticket='member-original',key='ml_fuzzy_match_beyond_word_boundaries_v2',search='',forced=None,storage=None,storageFailed=False,group='noGroup',enabled=False),eligibility=None,
        maybeSelf=False,presentation=dict(locale='en-US',origin='application-English',me='me',you='you'),demotePassthrough=False,
        originalMembers=source,members=[F.derived(r) for r in source],self=F.derived(F.USER),groups=F.GROUPS,comparisons={F.SELF:'',F.MEMBER:'harbor'},groupLower={'Alex group SLISTGRP1':'alex group slistgrp1'},
        fields=[dict(text=t,lowered=t.lower(),localeLowered=t.lower()) for t in sorted(texts)],classes={v:i for i,v in enumerate(names)})

def selectors(matched):
    members={F.SELF:F.selector_row(F.USER,True)}
    if matched:members[F.MEMBER]=F.selector_row(F.repaired())
    ids=set(members)
    return dict(selfId=F.SELF,self=members[F.SELF],teamId='T1',enterpriseId=None,enterprise=False,searcherTeams=['T1'],defaultTeam='T1',members=members,ims={},metadata={},navigationHistory={},experiments=dict(autocomplete_affinity_score=None,autocomplete_affinity_ml_model=None),offline={kind:{id:dict(team_ctr_90dd=ctr,user_ctr_30dd=ctr) for id in ids} for kind,ctr in [('channel',0.148),('mpim',0.014)]},entityCtr=dict(member=0.132,channel=0.148,mpim=0.014,ims=0.179,userGroup=0.179),remoteDm={id:None for id in ids},remoteMpim={id:None for id in ids},affinity={id:0.0 for id in ids},channelPriority={id:0.0 for id in ids})

def eligibility(source):
    return dict(registration=dict(client='OrdinaryClientV2',unifiedUser=True,agentEnvironment=True),salesforceBlocked=False,rows=[dict(id=r['id'],source=r,interactionAllowed=True,slackbot=False,systemNotification=False,searchBot=False,channelExpertBot=False,agentEnvironment=True,agent=False,admittedAgent=None,workflow=False,deleted=False,externalHuddleGuest=False,salesforceApp=None,profileOnly=False) for r in sorted(source,key=lambda r:r['id'])])

def transform(v):
    if isinstance(v,(list,tuple)):return [transform(x) for x in v]
    if isinstance(v,dict):
        if v=={'type':'channel','channel_id':'CLISTLOCAL'}:return {'type':'user','user_id':F.MEMBER}
        return {k:transform(x) for k,x in v.items()}
    if isinstance(v,str):
        return v.replace('Known #project-harbor and #not-found','Known @harbor and @not-found').replace('Known #CLISTLOCAL and #not-found','Known @ULISTMEM1 and @not-found').replace(' and #not-found',' and @not-found').replace('own-edit-plain-channel','own-edit-plain-member').replace('source-pending','source-stale').replace('"type":"channel","channel_id":"CLISTLOCAL"','"type":"user","user_id":"ULISTMEM1"')
    return v

def sample(narrow=False):
    fixture,r,closed=transform(C.sample(narrow));rows=r['stages'];initial=rows[0]['model']['comments'];event=None;release=None
    owner={k:initial[k] for k in ('account','epoch')};owner.update(team='T1',user=F.SELF)
    for i,row in enumerate(rows):
        p=row['plain'];p.pop('channels');p.pop('channelSource')
        p['canonical']=dict(locale='en-US',membership=[],rows=copy.deepcopy(F.GROUPS),current=True)
        c=p['completion'];seed=c['seed'];seed['boot']['channels'][0]['name']='project-local'
        seed['boot']['users']=[copy.deepcopy(F.USER),F.raw_member()];seed['boot']['workspaces']=[{'id':'T1'}];seed['init']['workspaces']=[{'id':'T1'}]
        seed['groups']=dict(incoming=[copy.deepcopy(F.GROUP)],rows=copy.deepcopy(F.GROUPS),membership=[])
        c['groups']=dict(rows=copy.deepcopy(F.GROUPS),membership=[],selfRestricted=False)
        c['requests']=[];c['responses']=[]
        if i>=5:
            c['requests']=[dict(serial=1,method='client.gantryBoot',args={'include_channels':'false','_x_reason':'fetch-client-data'}),dict(serial=2,method='search.autocomplete.model',args={'_x_reason':'AutocompleteModelCache'})]
            c['responses']=[dict(serial=1,disposition='response',raw=copy.deepcopy(F.GANTRY)),dict(serial=2,disposition='response',raw={'ok':True,'model':None})]
        raws=[F.canonical(F.USER),F.repaired() if i>=4 else F.canonical(F.raw_member())]
        pairs=[dict(source=raw,native=copy.deepcopy(raw)) for raw in raws]
        source=dict(owner,socket=8,current=i not in (2,3),pending=False,required=[F.MEMBER] if i in (2,3) else [],rows=None if i in (2,3) else copy.deepcopy(pairs),retained=pairs,native=F.raw_member(102 if i>=2 else 101),groups=dict(rows=copy.deepcopy(F.GROUPS),membership=[],current=True),knownDriver=dict(started=int(i>=2),finished=int(i>=4),active=int(i in (2,3))))
        p['memberSource']=source
        requests=[]
        if i>=2:
            for serial,kind,method,args in [(1,'discovery','api.getFlannelHttpUrl',{'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'}),(2,'member','users/info',{'ids':[F.MEMBER],'check_interaction':True,'include_profile_only_users':True})]:
                requests.append(dict(owner,serial=serial,kind=kind,lane='known',method=method,args=args,response=F.DISCOVERY if serial==1 else None if i<4 else dict(ok=True,results=[F.raw_member(102)],can_interact={F.MEMBER:True}),disposition={'kind':'immediate-original'} if serial==1 else None if i<4 else dict(serial=2,receiverRetired=False,delivered=True)))
        lookups=[]
        if i>=5:
            for serial,q in enumerate(('harbor','not-found'),1):
                lookups.append(dict(serial=serial,observation=dict(lookup=dict(kind='member',query=q),requested=dict(query=q,options=dict(entities=dict(userGroups=dict(includeFromAnotherTeam=True),members=dict(includeSelf=True,includeSlackbot=False,excludeSalesforceApp=False)),limit=25,sort={'source':'texty-autocomplete'})),source=raws,groups=F.GROUPS,options=options(q,raws),eligibility=eligibility(raws),namePolicy=dict(gantry=F.GANTRY,enterprise=None,defaultWorkspace='T1'),selectors=selectors(serial==1),results=[F.repaired()] if serial==1 else [],sourceCurrent=True,groupsCurrent=True,rankCurrent=True)))
        p['members']=dict(enabled=True,lookups=lookups,requests=requests,held=2 if i in (2,3) else None,repair={'ok':True} if i>=4 else None)
        proof=row['proof'];proof.pop('channelEvent',None);proof.pop('channelRelease',None)
        if i==2:
            post=copy.deepcopy(source);post['knownDriver']=dict(started=0,finished=0,active=0)
            event=dict(raw=dict(type='user_change',user=F.raw_member(102)),before=copy.deepcopy(rows[1]['plain']['memberSource']),originalCurrent=False,source=post)
        if i>=2:proof['memberEvent']=copy.deepcopy(event)
        if i==4:release=dict(source=copy.deepcopy(source),originalCurrent=False,contextCurrent=True)
        if i>=4:proof.update(memberRelease=copy.deepcopy(release),memberCallback=dict(serial=2,receiverRetired=False,delivered=True))
    r['proof']={**copy.deepcopy(rows[-1]['proof']),'closedDisk':r['proof']['closedDisk']};r['final']=copy.deepcopy(rows[-1]['model'])
    return copy.deepcopy((fixture,r,closed))

class MemberTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V.validate(f,r,D,png=False)
    def test_full_both_widths_and_closed_database(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V.validate(f,r,D,png=False);V.validate_closed(c,r)
    def test_original_future_bytes(self):
        p=Path(__file__).parents[1]/'src/desktop/slack_lists/comments/fixture/plain/member-future.json'
        self.assertEqual(p.read_bytes().decode('utf-8'),V.FUTURE_BYTES)
    def test_old_handle_stays_retired(self):self.reject(lambda r:r['stages'][4]['proof']['memberRelease'].update(originalCurrent=True))
    def test_pending_is_not_ready(self):self.reject(lambda r:r['stages'][3]['plain']['memberSource'].update(current=True))
    def test_no_queue_invented(self):self.reject(lambda r:r['stages'][2]['plain']['memberSource'].update(pending=True))
    def test_same_socket_required(self):self.reject(lambda r:r['stages'][4]['plain']['memberSource'].update(socket=9))
    def test_no_boot_reseed(self):self.reject(lambda r:r['stages'][4]['plain']['completion']['seed']['boot']['users'][1].update(updated=102))
    def test_actual_native_event(self):self.reject(lambda r:r['stages'][2]['plain']['memberSource']['native'].update(updated=101))
    def test_original_raw_not_event_replacement(self):self.reject(lambda r:r['stages'][2]['plain']['memberSource']['retained'][1]['source'].update(updated=102))
    def test_known_factory_not_absent(self):self.reject(lambda r:r['stages'][2]['plain']['members']['requests'][1]['args'].update(updated_ids={F.MEMBER:0}))
    def test_known_original_owner(self):self.reject(lambda r:r['stages'][2]['plain']['members']['requests'][1].update(epoch=999))
    def test_held_until_actual_release(self):self.reject(lambda r:r['stages'][3]['plain']['members'].update(held=None))
    def test_closed_callback_not_lost(self):self.reject(lambda r:r['stages'][4]['plain']['members']['requests'][1]['disposition'].update(delivered=False))
    def test_unavailable_cannot_lookup(self):self.reject(lambda r:r['stages'][3]['plain']['members']['lookups'].append({}))
    def test_unavailable_cannot_read_names(self):self.reject(lambda r:r['stages'][3]['plain']['completion']['requests'].append({}))
    def test_nonmatch_group_preserved(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(groups=[]))
    def test_group_current_required(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(groupsCurrent=False))
    def test_source_current_required(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(sourceCurrent=False))
    def test_rank_current_required(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation'].update(rankCurrent=False))
    def test_category_source_options(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['requested']['options']['entities']['userGroups'].update(includeFromAnotherTeam=False))
    def test_no_username_postfilter(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['results'][0].update(name='harbor'))
    def test_observed_no_match(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][1]['observation'].update(results=[F.repaired()]))
    def test_original_renderer_inputs(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['options']['originalMembers'].reverse())
    def test_no_fabricated_toggle(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['options']['options'].update(group='on'))
    def test_actual_name_policy(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['namePolicy']['gantry']['workspaces'].append({'id':'TFOREIGN'}))
    def test_predicate_interaction(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['eligibility']['rows'][1].update(interactionAllowed=False))
    def test_predicate_no_unknown_agent_grant(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['eligibility']['rows'][1].update(admittedAgent=True))
    def test_no_remote_search(self):self.reject(lambda r:r['stages'][5]['plain']['completion']['requests'][0].update(method='users/search'))
    def test_rank_selector_source(self):self.reject(lambda r:r['stages'][5]['plain']['members']['lookups'][0]['observation']['selectors']['members'][F.MEMBER]['facts'].update(comparisonName='username-must-not-win'))
    def test_entity_ctr_requires_actual_ims_key(self):
        def change(r):
            ctr=r['stages'][5]['plain']['members']['lookups'][0]['observation']['selectors']['entityCtr'];ctr['im']=ctr.pop('ims')
        self.reject(change)
    def test_navigation_not_guessed_empty(self):
        f,r,_=sample()
        for row in r['stages'][5:]:
            for item in row['plain']['members']['lookups']:item['observation']['selectors']['navigationHistory']={'C12345678':{'ts':12.5,'position':2}}
        V.validate(f,r,D,png=False)
    def test_failed_save_no_attempt(self):self.reject(lambda r:r['stages'][3]['model']['edit']['attempts'].append({}))
    def test_failure_native_same_node(self):self.reject(lambda r:r['stages'][3]['geometry']['edit'].update(sameNode=False))
    def test_independent_reply(self):self.reject(lambda r:r['stages'][3]['journal'].update(blocks=[]))
    def test_preserved_delta(self):self.reject(lambda r:r['stages'][3]['editInventory']['plain_delta'].update(ops=[]))
    def test_exact_member_wire(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'][0]['elements'][0]['elements'][1].update(user_id='UFOREIGN1'))
    def test_context_suffix(self):self.reject(lambda r:r['stages'][5]['model']['edit']['attempts'][0]['blocks'].pop())
    def test_closed_original_history(self):
        _,r,c=sample();c['editHistory']=[]
        with self.assertRaises(RuntimeError):V.validate_closed(c,r)
    def test_original_closed_runner(self):
        h=runpy.run_path(str(Path(__file__).with_name('check-list-plain-member-save-native.py')))['harness']()
        self.assertEqual(h['case_contract'](V.CASES[0])[0],V.STAGES[V.CASES[0]])

if __name__=='__main__':unittest.main()

