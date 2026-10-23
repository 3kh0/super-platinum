"""Source-bound agent access observations; no fallback predicate or invented catalog."""
import copy,runpy
from pathlib import Path
F=runpy.run_path(str(Path(__file__).with_name('slack_list_plain_member_save_facts.py')))
require,exact=F['require'],F['exact'];SELF=F['SELF'];MEMBER=F['MEMBER'];ID='UAGENT001';APP='AAGENT001'
# This isolated fixture has no UserChanged/known repair: source stays updated101.
F['selectors'].__globals__['repaired']=lambda:F['canonical'](F['raw_member']())
AGENT=dict(id=ID,team_id='T1',name='agent-raw-username',updated=101,is_agentforce_bot=True,is_bot=True,deleted=False,is_restricted=False,is_ultra_restricted=False,profile=dict(display_name='restricted-agent',real_name='Restricted Agent',api_app_id=APP),fixture_opaque=dict(keep='original-agent',ordered=['profile','access']))
INIT=dict(ok=True,self=F['USER'],team={'id':'T1'})
EXTRAS=dict(ok=True,team_prefs=dict(toplevel_prefs=dict(ai_apps_settings=dict(apps=[dict(app_id=APP,is_ai_enabled=False)]))))
CATALOG=dict(ok=True,apps=[dict(id=APP,is_agent_app=True,agentforce={},bot_user={'id':ID})],response_metadata={'next_cursor':''})
REASON='native-calendar-attendee-agents'
READS=[('client.init',{'include_relevant_onboarding':'false','_x_reason':REASON},INIT),('client.extras',{'extras':'["team_prefs"]','_x_reason':REASON},EXTRAS),('aiApps.list',{'limit':'20','_x_reason':REASON},CATALOG)]

def observations_for(v,owner,storage,base):
    private=copy.deepcopy(v);users=private['completion']['seed']['boot']['users']
    require(exact(users,[F['USER'],F['raw_member'](),AGENT]),'Original mixed human/Agentforce Boot changed')
    users.pop();F['observations_for'](private,owner,storage,base)

def source_check(s,owner,original):
    require(s['account']==owner['account'] and s['epoch']==owner['epoch'] and s['team']=='T1' and s['user']==SELF and s['socket']==original['socket'] and F['R']['integer'](s['socket'],1),'Original agent source owner/socket changed')
    require(s['current'] is True and s['pending'] is False and s['required']==[] and exact(s['knownDriver'],dict(started=0,finished=0,active=0)),'Agent catalog read mutated/repaired canonical member inventory')
    rows=s['rows'];require(isinstance(rows,list) and len(rows)==3 and [r['source']['id'] for r in rows]==[SELF,MEMBER,ID],'Full original ready source or encounter order changed')
    # Ready retains saved.records order; passive source_ids walks a BTreeMap.
    retained=s['retained'];require(isinstance(retained,list) and [r['source']['id'] for r in retained]==[ID,SELF,MEMBER] and exact(retained,sorted(rows,key=lambda r:r['source']['id'])),'Retained source ID order or exact source/native pairs changed')
    expected={r['id']:F['canonical'](r) for r in [F['USER'],F['raw_member'](),AGENT]}
    require({r['source']['id'] for r in rows}==set(expected),'Mixed canonical source inventory changed')
    for row in rows:
        raw=row['source'];require(exact(raw,expected[raw['id']]) and row['native'].get('id')==raw['id'],'Original effective/native source changed')
    require(s['native'].get('id')==MEMBER and s['native'].get('updated')==101 and s['native'].get('profile',{}).get('display_name')=='harbor','Original human native row changed')
    require(exact(s['groups'],dict(rows=F['GROUPS'],membership=[],current=True)),'Independent group inventory changed')
    require(exact(s,original),'Catalog lookup replaced original source/currentness')

def catalog(v,owner,socket,index):
    require(set(v)=={'enabled','requests','noLiveNetwork'} and v['enabled'] is True and v['noLiveNetwork'] is True,'Unnamed or live agent catalog')
    rows=v['requests'];require(len(rows)==(0 if index<2 else 3 if index==2 else 6),'Original failed/successful catalog acquisition count changed')
    for i,row in enumerate(rows):
        method,args,raw=READS[i%3];failed=i==2
        expected=dict(serial=i+1,method=method,args=args,owner=dict(account=owner['account'],epoch=owner['epoch'],team='T1',user=SELF),socket=socket,response={'error':'offline'} if failed else raw,disposition='offline' if failed else 'response',currentAtReturn=True)
        require(exact(row,expected),'Original catalog factory/raw/current owner or failure changed')

def renderer(v,query,source):
    agent=F['canonical'](AGENT);require(exact(v['originalMembers'],source) and exact(v['members'],[F['derived'](r) for r in source]),'Original renderer mixed inventory/order changed')
    require(v['comparisons'].get(ID)=='restricted-agent','Actual original agent comparison name missing')
    names={'agent-raw-username','restricted-agent','Restricted Agent'}
    extra=[f for f in v['fields'] if f['text'] in names]
    require(len(extra)==3 and {f['text'] for f in extra}==names and all(f==dict(text=f['text'],lowered=f['text'].lower(),localeLowered=f['text'].lower()) for f in extra),'Agent renderer name facts changed')
    require(v['classes'].get('restricted-agent')==5,'Actual agent name collation missing')
    private=copy.deepcopy(v)
    for key in ('originalMembers','members'):private[key]=[r for r in private[key] if r['id']!=ID]
    private['comparisons'].pop(ID);private['fields']=[f for f in private['fields'] if f['text'] not in names];private['classes'].pop('restricted-agent')
    F['renderer'](private,query,[r for r in source if r['id']!=ID],F['GROUPS'])

def eligibility(v,source,policy,ledger):
    private=copy.deepcopy(v);access=private.pop('agentAccess')
    require(exact(access,dict(mode='fetch',complete=True,assignments=policy,reads=[dict(method='POST',raw=q['response']) for q in ledger['requests'][3:]])),'Agent access borrowed failed/foreign/partial raw reads')
    rows=private['rows'];agents=[r for r in rows if r['id']==ID]
    expected=dict(id=ID,source=F['canonical'](AGENT),interactionAllowed=True,slackbot=False,systemNotification=False,searchBot=False,channelExpertBot=False,agentEnvironment=True,agent=True,admittedAgent=False,workflow=False,deleted=False,externalHuddleGuest=False,salesforceApp=None,profileOnly=False)
    require(exact(agents,[expected]),'Disabled agent admission or original predicate changed')
    private['rows']=[r for r in rows if r['id']!=ID]
    F['eligibility'](private,[r for r in source if r['id']!=ID])

def member_stage(row,owner,index,rows):
    p=row['plain'];source=p['memberSource'];ledger=p['members'];original=rows[0]['plain']['memberSource']
    source_check(source,owner,original);catalog(p['agentCatalog'],owner,source['socket'],index)
    require(ledger['enabled'] is True and ledger['requests']==[] and ledger['held'] is None and ledger['repair'] is None,'Agent access borrowed known-member recovery lane')
    for key in ('memberEvent','memberRelease','memberCallback'):require(key not in row['proof'],'Invented member repair proof')
    lookups=ledger['lookups'];require(len(lookups)==(2 if index>=3 else 0),'Failed access became lookup or duplicate acquisition')
    if index:
        before=rows[index-1]['plain'];require(exact(p['agentCatalog']['requests'][:len(before['agentCatalog']['requests'])],before['agentCatalog']['requests']) and exact(lookups[:len(before['members']['lookups'])],before['members']['lookups']),'Original observation history changed')
    if index<3:
        require(p['completion']['requests']==[],'Failed agent access reached unrelated name/rank reads');return
    assignments=row['model']['fixture']['policy'][-2]['response']
    # Actual complete experiments source, not a missing=>off substitute.
    require(assignments.get('ok') is True and isinstance(assignments.get('assignments'),dict),'Original Save assignments absent')
    for i,(entry,query) in enumerate(zip(lookups,['harbor','not-found']),1):
        require(set(entry)=={'serial','observation'} and entry['serial']==i,'Lookup history serial changed')
        o=entry['observation'];require(exact(o['lookup'],dict(kind='member',query=query)) and all(o[k] is True for k in ['sourceCurrent','groupsCurrent','rankCurrent']),'Atomic mixed source/rank authority absent')
        require(exact(o['source'],[r['source'] for r in source['rows']]) and exact(o['groups'],F['GROUPS']),'Original mixed source/order changed')
        require(exact(o['requested'],dict(query=query,options=dict(entities=dict(userGroups=dict(includeFromAnotherTeam=True),members=dict(includeSelf=True,includeSlackbot=False,excludeSalesforceApp=False)),limit=25,sort={'source':'texty-autocomplete'}))),'Original requested categories changed')
        require(exact(o['namePolicy'],dict(gantry=F['GANTRY'],enterprise=None,defaultWorkspace='T1')),'Original naming source changed')
        renderer(o['options'],query,o['source']);eligibility(o['eligibility'],o['source'],assignments,p['agentCatalog']);F['selectors'](o['selectors'],i==1)
        require(exact(o['results'],[F['canonical'](F['raw_member']())] if i==1 else []),'Agent excluded/human match or no-match changed')
    require(exact(lookups[0]['observation']['eligibility'],lookups[1]['observation']['eligibility']) and exact(lookups[0]['observation']['selectors']['navigationHistory'],lookups[1]['observation']['selectors']['navigationHistory']),'Queries borrowed different access/navigation observations')
