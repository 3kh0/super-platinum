"""Independent source/callback checks for the named local member Save fixture."""
import copy
from slack_list_plain_channel_save import require,exact,R,O
SELF=O['SELF'];MEMBER='ULISTMEM1'
USER=dict(id=SELF,name='fixture-self',team_id='T1',is_admin=True,profile={})
def raw_member(updated=101):
    return dict(id=MEMBER,team_id='T1',name='username-must-not-win',updated=updated,deleted=False,is_bot=False,is_restricted=False,is_ultra_restricted=False,real_name='Harbor Top',profile=dict(display_name='harbor',real_name='Harbor Person',email='harbor@fixture.invalid'),fixture_opaque=dict(keep='original-member',ordered=['source','native']))
def canonical(raw):
    r=copy.deepcopy(raw);p=r['profile']
    for key in ('first_name','last_name'):p.setdefault(key,'')
    if r['id']==SELF:r['is_self']=True
    for key,field in [('_real_name_normalized_lc','real_name'),('_display_name_normalized_lc','display_name'),('_email_normalized_lc','email')]:r[key]=p.get(field,'').lower()
    for key,value in [('_name_lc',r['name']),('_first_name_lc',p['first_name']),('_last_name_lc',p['last_name']),('_real_name_lc',p.get('real_name','')),('_display_name_lc',p.get('display_name',''))]:r[key]=value.lower()
    p.setdefault('real_name_normalized',p.get('real_name',''));p.setdefault('display_name_normalized',p.get('display_name',''))
    return r

def repaired():return canonical(dict(raw_member(102),is_stranger=False))
GROUP=dict(id='SLISTGRP1',team_id='T1',name='Alex group SLISTGRP1',handle='alex-team',date_update=101,date_delete=0,is_hidden=False,users=[],user_count='0',is_external=False,fixture_opaque=dict(preserve='SLISTGRP1',ordered=['raw','effective']))
EFFECTIVE=dict(GROUP,user_count=0,_name_lc='alex group slistgrp1',is_usergroup=True,isNonExistent=False)
GROUPS=[dict(source=GROUP,effective=EFFECTIVE)]
GANTRY=dict(ok=True,self=USER,team={'id':'T1'},workspaces=[{'id':'T1','prefs':{'display_real_names':False}}],default_workspace='T1')
DISCOVERY=dict(ok=True,url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300)

def observations_for(value,owner,storage,base):
    v=copy.deepcopy(value);c=v['completion'];seed=c['seed'];boot=seed['boot'];init=seed['init']
    require(exact(boot['users'],[USER,raw_member()]) and exact(boot.get('workspaces'),[{'id':'T1'}]) and exact(init['workspaces'],[{'id':'T1'}]),'Original member Boot/Init scope changed')
    require(exact(v['canonical'],dict(locale='en-US',membership=[],rows=GROUPS,current=True)),'Actual nonmatching canonical group unavailable/changed')
    require(exact(seed.get('groups'),dict(incoming=[GROUP],rows=GROUPS,membership=[])) and exact(c.get('groups'),dict(rows=GROUPS,membership=[],selfRestricted=False)),'Original group intake/effective source changed')
    requests=c['requests'];responses=c['responses']
    if requests:
        require(exact(requests,[dict(serial=1,method='client.gantryBoot',args={'include_channels':'false','_x_reason':'fetch-client-data'}),dict(serial=2,method='search.autocomplete.model',args={'_x_reason':'AutocompleteModelCache'})]),'Unexpected automatic remote search or name/rank factory')
        require(exact(responses,[dict(serial=1,disposition='response',raw=GANTRY),dict(serial=2,disposition='response',raw={'ok':True,'model':None})]),'Original actual gantry/model reply changed')
    else:require(responses==[],'Unrequested source response')
    # Only validated named source differences are removed in a private copy.
    boot['users']=[USER];boot.pop('workspaces');init['workspaces']=[{'id':'T1'},{'id':'TOTHER001'}];seed.pop('groups')
    v['canonical']['rows']=[];c['requests']=[];c['responses']=[]
    base(v,owner,storage)

def derived(raw):
    r=copy.deepcopy(raw)
    for key,field in [('_real_name_normalized_lc','real_name'),('_display_name_normalized_lc','display_name'),('_email_normalized_lc','email')]:r[key]=raw.get('profile',{}).get(field,'').lower()
    return r

def source_check(s,owner,first,index):
    require(s['account']==owner['account'] and s['epoch']==owner['epoch'] and s['team']=='T1' and s['user']==SELF and R['integer'](s['socket'],1) and s['socket']==first['socket'],'Original member source owner/socket changed')
    stale=index in (2,3)
    require(s['current'] is (not stale) and s['pending'] is False and exact(s['required'],[MEMBER] if stale else []),'Stale full-member source promoted or canonical queue invented')
    pairs=s['retained'];require(isinstance(pairs,list) and len(pairs)==2 and {p['source']['id'] for p in pairs}=={SELF,MEMBER},'Original full member inventory changed')
    for pair in pairs:
        expected=canonical(USER) if pair['source']['id']==SELF else repaired() if index>=4 else canonical(raw_member())
        require(exact(pair['source'],expected),'Original canonical member raw changed')
        require(isinstance(pair['native'],dict) and pair['native'].get('id')==expected['id'],'Original member native observation missing')
    require((s['rows'] is None if stale else exact(s['rows'],pairs)),'Ready source does not retain original full collection')
    require(s['native'].get('id')==MEMBER and s['native'].get('updated')==(102 if index>=2 else 101) and s['native'].get('name')=='username-must-not-win' and s['native'].get('profile',{}).get('display_name')=='harbor','Actual UserChanged native preimage changed')
    require(exact(s['groups'],dict(rows=GROUPS,membership=[],current=True)),'Member recovery replaced independent group source')
    expected=dict(started=int(index>=2),finished=int(index>=4),active=int(index in (2,3)))
    require(exact(s['knownDriver'],expected),'Original known reader settlement changed')

def read_ledger(ledger,owner,index):
    requests=ledger['requests'];held=index in (2,3)
    require(len(requests)==(2 if index>=2 else 0) and ledger['held']==(2 if held else None) and exact(ledger['repair'],{'ok':True} if index>=4 else None),'Original known work held/settled incorrectly')
    if index<2:return
    for serial,(row,kind,method,args) in enumerate(zip(requests,['discovery','member'],['api.getFlannelHttpUrl','users/info'],[{'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'}, {'ids':[MEMBER],'check_interaction':True,'include_profile_only_users':True}]),1):
        require(set(row)=={'serial','kind','lane','method','args','account','epoch','team','user','response','disposition'} and row['serial']==serial and row['kind']==kind and row['lane']=='known' and row['method']==method and exact(row['args'],args) and row['account']==owner['account'] and row['epoch']==owner['epoch'] and row['team']=='T1' and row['user']==SELF,'Original known IDs factory/owner changed')
    require(exact(requests[0]['response'],DISCOVERY) and exact(requests[0]['disposition'],{'kind':'immediate-original'}),'Original discovery source changed')
    require(exact(requests[1]['response'],None if held else dict(ok=True,results=[raw_member(102)],can_interact={MEMBER:True})) and exact(requests[1]['disposition'],None if held else dict(serial=2,receiverRetired=False,delivered=True)),'Original member response or held callback disposition changed')

def renderer(v,query,source,groups):
    require(v['rawQuery']==query and v['formattedQuery']==query and v['locale']=='en-US' and exact(v['query'],dict(normalized=query,matcherNormalized=query)),'Original renderer query/locale changed')
    o=v['options'];require(set(o)=={'ticket','key','search','forced','storage','storageFailed','group','enabled'} and isinstance(o['ticket'],str) and bool(o['ticket']) and o['key']=='ml_fuzzy_match_beyond_word_boundaries_v2' and o['search']=='' and o['forced'] is None and o['storage'] is None and o['storageFailed'] is False and o['group']=='noGroup' and o['enabled'] is False and v['eligibility'] is None,'Actual original session toggle changed')
    require(v['maybeSelf'] is False and exact(v['presentation'],dict(locale='en-US',origin='application-English',me='me',you='you')) and v['demotePassthrough'] is False,'Actual self-alias/demotion observation changed')
    require(exact(v['originalMembers'],source) and exact(v['members'],[derived(r) for r in source]) and exact(v['self'],derived(USER)) and exact(v['groups'],groups),'Renderer borrowed foreign/reordered member or group input')
    require(exact(v['comparisons'],{SELF:'',MEMBER:'harbor'}) and exact(v['groupLower'],{'Alex group SLISTGRP1':'alex group slistgrp1'}),'Original independent comparison/group names changed')
    fields=v['fields'];texts={f['text'] for f in fields}
    expected={'',query,'harbor','Harbor Person','harbor@fixture.invalid','fixture-self','username-must-not-win','Alex group SLISTGRP1','alex group slistgrp1','alex-team','@alex-team'}
    require(len(fields)==len(expected) and texts==expected and all(set(f)=={'text','lowered','localeLowered'} and f['lowered']==f['text'].lower() and f['localeLowered']==f['text'].lower() for f in fields),'Original renderer field coverage or lowercase changed')
    classes=v['classes'];names={'fixture-self','harbor','alex-team','Alex group SLISTGRP1','alex group slistgrp1'}
    require(set(classes)==names and all(type(x) is int and x>=0 for x in classes.values()) and classes['alex group slistgrp1']<classes['Alex group SLISTGRP1']<classes['alex-team']<classes['fixture-self']<classes['harbor'],'Original ICU name classes changed')

def selector_row(raw,self_row=False):
    row=derived(raw);row['is_self']=self_row
    row['facts']=dict(member=True,self=self_row,deactivated=False,active=True,firstName='',lastName='',mpim=False,im=False,privateChannel=False,channel=False,channelType=None,conversation=False,archived=False,deleted=False,external=False,frozen=False,bot=False,slackbot=False,uslack=False,channelMember=False,otherTeam=None,open=False,memberCount=None,team='T1',comparisonName='' if self_row else 'harbor',bestName='' if self_row else 'harbor',mpimName=None,customName=None,customNameAllowed=False)
    return row

def selectors(v,matched):
    members={SELF:selector_row(USER,True)}
    if matched:members[MEMBER]=selector_row(repaired())
    ids=set(members);expected=dict(selfId=SELF,self=members[SELF],teamId='T1',enterpriseId=None,enterprise=False,searcherTeams=['T1'],defaultTeam='T1',members=members,ims={},metadata={},experiments=dict(autocomplete_affinity_score=None,autocomplete_affinity_ml_model=None),offline={kind:{id:dict(team_ctr_90dd=ctr,user_ctr_30dd=ctr) for id in ids} for kind,ctr in [('channel',0.148),('mpim',0.014)]},entityCtr=dict(member=0.132,channel=0.148,mpim=0.014,ims=0.179,userGroup=0.179),remoteDm={id:None for id in ids},remoteMpim={id:None for id in ids},affinity={id:0.0 for id in ids},channelPriority={id:0.0 for id in ids})
    require(exact({k:x for k,x in v.items() if k!='navigationHistory'},expected),'Original member rank selectors/cache/name authority changed')
    nav=v.get('navigationHistory');require(isinstance(nav,dict),'Missing actual Navigation projection')
    for id,row in nav.items():require(isinstance(id,str) and len(id)>=9 and id[0] in 'CGDZ' and all(c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789' for c in id[1:]) and set(row)=={'ts','position'} and R['integer'](row['position'],1) and (row['ts'] is None or type(row['ts']) in (int,float)),'Invalid original navigation projection')

def member_stage(row,owner,index,rows):
    source=row['plain']['memberSource'];ledger=row['plain']['members'];p=row['proof']
    source_check(source,owner,rows[0]['plain']['memberSource'],index);require(ledger['enabled'] is True,'Unnamed member ledger')
    read_ledger(ledger,owner,index)
    if index>=2:
        e=p['memberEvent'];require(exact(e['raw'],dict(type='user_change',user=raw_member(102))) and exact(e['before'],rows[1]['plain']['memberSource']) and e['originalCurrent'] is False,'Original same-socket event or retired source changed')
        ev=copy.deepcopy(e['source']);ev['knownDriver']=dict(started=1,finished=0,active=1)
        require(exact(ev,rows[2]['plain']['memberSource']),'Actual stale source differs from event before original known repair')
    if index>=4:require(exact(p['memberCallback'],dict(serial=2,receiverRetired=False,delivered=True)) and exact(p['memberRelease'],dict(source=rows[4]['plain']['memberSource'],originalCurrent=False,contextCurrent=True)),'Original callback or repaired source witness changed')
    lookups=ledger['lookups'];require(len(lookups)==(2 if index>=5 else 0),'Premature/missing atomic member lookups')
    if index>0:
        old=rows[index-1]['plain']['members']['lookups'];require(exact(lookups[:len(old)],old),'Original lookup history rewritten')
    if index<5:require(row['plain']['completion']['requests']==[],'Unavailable source reached naming/rank reads');return
    for serial,(entry,query) in enumerate(zip(lookups,['harbor','not-found']),1):
        require(set(entry)=={'serial','observation'} and entry['serial']==serial,'Original lookup serial changed')
        o=entry['observation'];require(exact(o['lookup'],dict(kind='member',query=query)) and all(o[k] is True for k in ['sourceCurrent','groupsCurrent','rankCurrent']),'Atomic original member/group/rank authority absent')
        require(exact(o['source'],[r['source'] for r in rows[4]['plain']['memberSource']['rows']]) and exact(o['groups'],GROUPS),'Original mixed source raw/order changed')
        require(exact(o['requested'],dict(query=query,options=dict(entities=dict(userGroups=dict(includeFromAnotherTeam=True),members=dict(includeSelf=True,includeSlackbot=False,excludeSalesforceApp=False)),limit=25,sort={'source':'texty-autocomplete'}))),'Original member/group category request changed')
        require(exact(o['namePolicy'],dict(gantry=GANTRY,enterprise=None,defaultWorkspace='T1')),'Original name-policy membership source changed')
        eligibility(o['eligibility'],o['source']);renderer(o['options'],query,o['source'],o['groups']);selectors(o['selectors'],serial==1)
        require(exact(o['results'],[repaired()] if serial==1 else []),'Actual known result/observed local no-match changed')
    require(exact(lookups[0]['observation']['selectors']['navigationHistory'],lookups[1]['observation']['selectors']['navigationHistory']),'Original navigation changed between lookup categories')


def eligibility(v,source):
    expected=[]
    for raw in sorted(source,key=lambda r:r['id']):
        expected.append(dict(id=raw['id'],source=raw,interactionAllowed=True,slackbot=False,systemNotification=False,searchBot=False,channelExpertBot=False,agentEnvironment=True,agent=False,admittedAgent=None,workflow=False,deleted=False,externalHuddleGuest=False,salesforceApp=None,profileOnly=False))
    require(exact(v,dict(registration=dict(client='OrdinaryClientV2',unifiedUser=True,agentEnvironment=True),salesforceBlocked=False,rows=expected)), 'Original ordinary member predicate/registration facts changed')
