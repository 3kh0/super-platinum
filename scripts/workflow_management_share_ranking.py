"""Independent closed Share ranking custody and retained-function replay.

The fixture response is a source observation, never a posting grant. Original
matcher/ranker replay runs separately in Node against pinned shipped functions.
"""
import json
import subprocess
import uuid
from collections import Counter
from pathlib import Path
import workflow_management_share_search as Q
from workflow_management_delete import exact, require, integer
from workflow_management_managers import boot_member
P,S,A,F,D=Q.P,Q.S,Q.A,Q.F,Q.D
CASES=Q.RANKING_CASES
DEST='CRANKR000'
WEIGHTS=dict(CHANNEL_IS_MEMBER=0,WRITE_CHANNEL_IS_MEMBER=8,READ_CHANNEL_IS_MEMBER=-8,USER_IS_ACTIVE=1)


def person(n,remote=False):
    label=f'Review {"Remote" if remote else "Local"} {n:02}'
    r=Q.person(n)
    r.update(id=f'URANK{"R" if remote else "L"}{n:03}',name=label.lower().replace(' ','_'),real_name=label,
             profile={k:label for k in ('real_name','real_name_normalized','display_name','display_name_normalized')},
             ranking_fixture_opaque=dict(family='member',remote=remote,ordinal=n))
    return r


def channel(n,joined=False):
    return dict(A.CHANNELS[A.DEST],id=f'CRANKR{n:03}',name=f'review-remote-{n:02}',is_member=joined,
                ranking_fixture_opaque=dict(family='channel',ordinal=n))


def canonical_channel(n):
    # Exact ordinary new-row projection: directory_channels/projection.rs
    # merged_before_names/process, after original member_channels publication.
    row=channel(n,True);row.pop('is_shared')
    row.update(fromAnotherTeam=False,_name_lc=row['name'],is_group=False,
               is_org_default=False,is_org_mandatory=False,topic={},purpose={},
               scroll_top=-1,history_is_being_fetched=False,unread_highlights=[],unreads=[],
               has_fetched_history_after_scrollback=False,internalTeamIds=[],
               connectedTeamIds=[],connectedLimitedTeamIds=[])
    return row


def seed_proof(server):
    A.seed_proof(server)
    seed=server['seed'];teams=[dict(id='TWORKFLOW',name='Home fixture')]
    require(exact(seed.get('boot',{}).get('workspaces'),teams) and
            exact(seed.get('init',{}).get('workspaces'),teams) and
            exact(seed['boot'].get('team'),teams[0]),
            'Ranking original Boot workspace membership missing or changed')
    users=server['seed']['boot']['users']
    require(len(users)==44 and exact(users[2:],[person(n) for n in range(42)]),'Ranking original42 Boot people changed')
    require(not any(r.get('id','').startswith(('URANKR','CRANKR')) for r in users+server['seed']['canonicalChannels']),
            'Remote ranking source was preseeded')


def page(method,query,marker=None):
    require(method in ('users/search','channels/search') and query in ('review','Review','oldreview','newreview') and
            marker in (None,'ranking-page-2'),'Unpinned ranking search')
    ns=range(13) if marker is None else range(13,25)
    rows=[person(n,True) if method=='users/search' else channel(n) for n in ns]
    keys=[f'z{99-n:02}' for n in ns]
    raw=dict(ok=True,results=dict(zip(keys,rows)),next_marker='ranking-page-2' if marker is None else '',
             opaque_receipt=dict(query=query,family=method,page=1 if marker is None else 2))
    if method=='channels/search':raw['member_channels']=[r['id'] for r in rows]
    return raw,keys


def publication(method,query):
    a,ka=page(method,query);b,kb=page(method,query,'ranking-page-2')
    rows=[a['results'][k] for k in ka]+[b['results'][k] for k in kb]
    return dict(kind='members' if method=='users/search' else 'channels',ids=[r['id'] for r in rows[:24]],rows=rows[:24],
                membershipIds=a.get('member_channels',[])+b.get('member_channels',[]),pageCount=2,nextMarker=rows[24]['id'],applied=True)


def query_phase(case,stage):
    if stage in A.PREFIX+('share-open',):return '',False
    if stage.startswith('warm-'):return 'oldreview',stage=='warm-pending'
    if case=='prefixes':return ('@review',stage=='member-pending') if stage.startswith('member-') else ('#Review',stage=='channel-pending')
    if case=='stale-cancel':return ('oldreview' if stage=='query-pending' else 'review'),True
    return 'review',stage=='query-pending'


def sequences(case,stage):
    query,pending=query_phase(case,stage)
    if not query:return []
    def one(query,held):
        methods=['users/search'] if query.startswith('@') else ['channels/search'] if query.startswith('#') else ['users/search','channels/search']
        wire=query.lstrip('@#');out=[('api.getFlannelHttpUrl','',None),(methods[0],wire,None)]
        if not held:
            out.append((methods[0],wire,'ranking-page-2'))
            if len(methods)==2:out.extend([(methods[1],wire,None),(methods[1],wire,'ranking-page-2')])
        return out
    prior=[]
    if case=='cached-write' and not stage.startswith('warm-'):prior=one('oldreview',False)
    if case=='prefixes' and query.startswith('#'):prior=one('@review',False)
    if case=='stale-cancel' and query=='review':prior=one('oldreview',True)
    return prior+one(query,pending)


def ledger(case,stage,server):
    v=server['confirmationAdd']['share']['search'];plan=sequences(case,stage)
    require(v.get('case')=='ranking' and v.get('initialPeople')==42 and v.get('networkCalls')==0 and
            v.get('remotePersonInitiallyAbsent') is True and v.get('remoteChannelInitiallyAbsent') is True and
            v.get('scheduling')=='first-search-page-per-wire-query-held-until-native-capture','Ranking closed acquisition gate')
    require(len(v.get('requests',[]))==len(v.get('responses',[]))==len(plan),'Ranking request count/replay')
    for serial,((method,query,marker),r,response) in enumerate(zip(plan,v['requests'],v['responses']),1):
        discovery=method=='api.getFlannelHttpUrl'
        expected=dict(serial=serial,method=method,query=query,body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else Q.body(method,query,marker),
                      endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method,retrySafe=False,owner=S.OWNER)
        require(exact(r,expected),'Ranking ordered original search request/owner')
        raw,keys=(F.DISCOVERY_RAW,None) if discovery else page(method,query,marker)
        require(exact(response,dict(serial=serial,raw=raw,orderedKeys=keys)),'Ranking immutable ordered search response')
    query,pending=query_phase(case,stage)
    held=[len(plan)] if pending and plan else []
    releases=[]
    if case=='stale-cancel':
        held=[] if stage=='late-settled' else [2,4] if len(plan)==4 else [2] if plan else []
        releases=[2,4] if stage=='late-settled' else []
    elif plan:
        starts=[i for i,r in enumerate(v['requests'],1) if r['method']=='api.getFlannelHttpUrl']
        releases=[i+1 for i in starts if i+1 not in held]
    require(v.get('held')==held and [r.get('serial') for r in v.get('releases',[])]==releases,'Ranking original held/release identity')
    for r in v['releases']:
        require(set(r)=={'serial','receiverRetired','delivered'} and type(r['receiverRetired']) is bool and r['delivered'] is (not r['receiverRetired']),'Ranking release delivery proof')
        if case!='stale-cancel':require(r['delivered'] is True,'Unexpected original receiver retirement')
    settled={r.get('serial'):r for r in v.get('settlements',[])}
    require(len(settled)==len(v['settlements']) and set(settled)<=set(range(1,len(plan)+1)),'Ranking foreign/duplicate settlement')
    required=set(range(1,len(plan)+1))-set(held)
    optional=set(held) if case=='stale-cancel' and stage!='query-pending' else set()
    require(required<=set(settled)<=required|optional,'Ranking original read completion inventory')
    for i,r in settled.items():
        require(type(r.get('returned')) is bool and set(r)=={'serial','returned'},'Ranking completion evidence')
        if case=='stale-cancel' and i in (2,4):require(r['returned'] is False,'Retired query returned/published late raw')
        else:require(r['returned'] is True,'Original read unexpectedly cancelled')
    return v


READ_BODIES={
 'client.gantryBoot':[['include_channels','false'],['_x_reason','fetch-client-data']],
 'users.prefs.get':[], 'team.prefs.get':[['target_team','TWORKFLOW']],
 'search.autocomplete.model':[['_x_reason','AutocompleteModelCache']],
 'search.autocomplete.offlineFeatures':[['_x_reason','search-offline-features']]}


def observation_reply(method,case,self_raw):
    if method=='client.gantryBoot':return dict(ok=True,self=self_raw,team=dict(id='TWORKFLOW'),workspaces=[dict(id='TWORKFLOW',allow_external_users=False,prefs=dict(display_real_names=True))],opaque_receipt='original-ranking-grid')
    if method=='users.prefs.get':return dict(ok=True,prefs=dict(display_real_names_override=1),opaque_receipt='original-ranking-user-prefs')
    if method=='team.prefs.get':return dict(ok=True,prefs=dict(display_real_names=True),opaque_receipt='original-ranking-team-prefs')
    if method=='experiments.getByUser':return dict(ok=True,assignments={k:dict(group=v) for k,v in [('autocomplete_affinity_score','off'),('autocomplete_affinity_ml_model','off'),('autocomplete_flannel_fuzzy_match_passthrough','control')]},opaque_receipt='original-ranking-experiments')
    if method=='search.autocomplete.model':return dict(ok=True,model=WEIGHTS if case=='cached-write' else None,opaque_receipt='original-ranking-model')
    if method=='search.autocomplete.offlineFeatures':return dict(ok=True,top_channels={},top_users={},users_has_dmed={},users_has_mpdmed={},opaque_receipt='original-ranking-offline')
    raise RuntimeError('Unpinned ranking observation method')


def observations(case,stage,server,previous=None):
    v=server['confirmationAdd']['share']['search']['ranking'];requests=v.get('requests');responses=v.get('responses')
    navigation_owner=v.get('navigationOwner')
    try: parsed_owner=uuid.UUID(navigation_owner) if isinstance(navigation_owner,str) else None
    except (ValueError,AttributeError): parsed_owner=None
    require(parsed_owner is not None and parsed_owner.version==4 and parsed_owner.variant==uuid.RFC_4122 and
            str(parsed_owner)==navigation_owner and navigation_owner!=S.OWNER['account'],
            'Ranking navigation storage owner is not its distinct original UUIDv4 namespace')
    require(v.get('case')==case and v.get('networkCalls')==0 and v.get('cacheInjection') is False and v.get('canonicalWrites')==0 and
            exact(v.get('localPeople'),[person(n) for n in range(42)]) and v.get('modelMode')==('actual-read-write-coefficients' if case=='cached-write' else 'actual-read-absent-model'),'Ranking original observation producer')
    require(isinstance(requests,list) and isinstance(responses,list) and len(requests)==len(responses)<=256,'Ranking observation count')
    for n,(r,response) in enumerate(zip(requests,responses),1):
        method=r.get('method');cache=method in ('search.autocomplete.model','search.autocomplete.offlineFeatures')
        body=READ_BODIES.get(method)
        if method=='experiments.getByUser':
            body=[['ssb_platform','windows'],['ssb_release_channel','prod']]
        require(body is not None and exact(r,dict(serial=n,kind='cache' if cache else 'query',method=method,body=body,endpoint='https://example.test/api/'+method,retrySafe=False,owner=S.OWNER)),'Ranking exact observed factory/owner')
        expected=dict(serial=n,error='offline') if case=='read-failure' and method=='users.prefs.get' else dict(serial=n,raw=observation_reply(method,case,server['seed']['boot']['self']),orderedKeys=None)
        require(exact(response,expected),'Ranking immutable observation response')
    require(exact(v.get('settlements'),[dict(serial=n,returned=True) for n in range(1,len(requests)+1)]),'Ranking actual completion callbacks')
    query_methods=[r['method'] for r in requests if r['kind']=='query']
    unit=['client.gantryBoot','users.prefs.get']+([] if case=='read-failure' else ['experiments.getByUser'])
    require(len(query_methods)%len(unit)==0 and query_methods==unit*(len(query_methods)//len(unit)),'Ranking observation acquisition order')
    counts=Counter(r['method'] for r in requests if r['kind']=='cache')
    require(all(n==1 for n in counts.values()),'Ranking injected/replayed background cache')
    if stage in A.PREFIX+('share-open','member-pending','warm-pending') or stage=='query-pending' and case!='cached-write':
        require(requests==[],'Ranking observations before completed original page')
    if stage in ('query-ready','warm-ready','member-ready','channel-ready','query-failed'):
        require(counts==Counter({'search.autocomplete.model':1,'search.autocomplete.offlineFeatures':1}),'Original cache callbacks not observed')
    if previous is not None:
        require(navigation_owner==previous.get('navigationOwner'),'Ranking navigation storage namespace changed during original run')
        for key in ('requests','responses','settlements'):require(exact(v[key][:len(previous[key])],previous[key]),'Original observation ledger changed')
    return v


def ordinary(stage,server,initial):
    rows=server['confirmationAdd']['share']['reads'];sources=server['confirmationAdd']['share']['search']['postingSources'];infos=[]
    require(isinstance(rows,list) and len(rows)<=128,'Ranking posting ledger bound')
    for n,r in enumerate(rows,1):
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Ranking fresh posting body')
        id=body[0][1];remote=id.startswith('CRANKR')
        if remote:
            require(id in {f'CRANKR{n:03}' for n in range(24)},'Overflow/foreign ranking destination')
            raw=channel(int(id[6:]),True);tag='original-ranking-destination-posting';infos.append(r)
        else:require(id in A.CHANNELS,'Unpinned ranking destination');raw=A.CHANNELS[id];tag='original-share-channel'
        require(exact(r,dict(serial=n,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,result=dict(raw=dict(ok=True,channel=raw,opaque_receipt=tag)))),'Ranking fresh posting request/response')
    require(len(sources)==len(infos),'Ranking pre-info canonical witness count')
    for r,w in zip(infos,sources):
        raw=r['result']['raw']['channel']
        require(set(w)=={'readSerial','canonical','current'} and w['readSerial']==r['serial'] and w['current'] is True and exact(w['canonical'],canonical_channel(int(raw['id'][6:]))),'Original false membership was not independently published before fresh info')
    if stage in A.PREFIX:require(rows==[],'Premature ranking listing')
    if initial is None:require(len(rows)<=5 and len({r['body'][0][1] for r in rows})==len(rows),'Initial listing duplicated')
    else:require(exact(rows[:len(initial)],initial),'Ranking replaced initial posting receipts')
    return rows


def state_proof(case,stage,q,v,server):
    s=q.get('search');require(isinstance(s,dict),'Ranking search proof absent');integer(s.get('revision'),'Ranking revision',1)
    query,pending=query_phase(case,stage)
    if not query:
        require(s.get('query')=='' and s.get('status')=='idle' and s.get('results') is None and s.get('ranking') is None and all(s.get(k)==[] for k in ('reads','failures','publications','history')),'Premature ranking');return None
    require(s.get('query')==query,'Ranking wrong original query')
    if pending:
        require(s.get('status')=='searching' and s.get('ranking') is None and all(s.get(k)==[] for k in ('reads','failures','publications')),'Pending/retired source adopted ranking');return None
    current=[(r,a) for r,a in zip(v['requests'],v['responses']) if r['query']==query.lstrip('@#') or r['method']=='api.getFlannelHttpUrl']
    # Each query has exactly one discovery; keep the last discovery before its pages.
    start=max(i for i,(r,_) in enumerate(current) if r['method']=='api.getFlannelHttpUrl');current=current[start:]
    methods=['users/search'] if query.startswith('@') else ['channels/search'] if query.startswith('#') else ['users/search','channels/search']
    reads=[dict(kind=('members' if methods[0]=='users/search' else 'channels') if r['method']=='api.getFlannelHttpUrl' else 'members' if r['method']=='users/search' else 'channels',method=r['method'],arguments=None if r['method']=='api.getFlannelHttpUrl' else r['body'],raw=a['raw'],orderedKeys=a['orderedKeys']) for r,a in current]
    pubs=[publication(m,query.lstrip('@#')) for m in methods]
    require(exact(s.get('reads'),reads) and exact(s.get('publications'),pubs),'Ranking lost original ordered acquisition/publication')
    if case=='read-failure':
        require(s.get('status')=='failed' and s.get('results') is None and s.get('ranking') is None and s.get('failures')==[dict(stage='ranking',failure='search observations unavailable',diagnostic='share_ranking_stage:observations.names.read.POST'),dict(stage='adoption',failure='source retired')],'Missing observations granted ranking');return None
    require(s.get('status')=='ready' and s.get('failures')==[],'Ranking source/posting failure')
    proof=s.get('ranking');require(isinstance(proof,dict) and proof.get('source')=='share_dialog_channel_picker' and proof.get('query')==query.lstrip('@#') and proof.get('passthrough') is False,'Wrong source/query/passthrough')
    require([r['id'] for r in proof.get('results',[])]==s.get('results') and len(s['results'])<=24,'Mixed result identity/cutoff')
    # Product observation records PreparedRequest.method (HTTP verb). The
    # independent fixture ledger above binds each exact Slack factory/endpoint.
    expected_reads=[dict(method='POST',raw=observation_reply(m,case,server['seed']['boot']['self'])) for m in ('client.gantryBoot','users.prefs.get','experiments.getByUser')]
    require(exact(proof.get('reads'),expected_reads),'Ranker lacked original name/experiment observations')
    selectors=proof.get('selectors');require(isinstance(selectors,dict),'Actual selectors missing')
    require(selectors.get('selfId')=='UWORKFLOW' and selectors.get('teamId')=='TWORKFLOW' and selectors.get('enterpriseId') is None and selectors.get('enterprise') is False and selectors.get('self',{}).get('facts',{}).get('team')=='TWORKFLOW','Ranking owner/self scope changed')
    required={'selfId','self','teamId','enterpriseId','enterprise','searcherTeams','defaultTeam','members','ims','metadata','navigationHistory','experiments','offline','entityCtr','remoteDm','remoteMpim','affinity','channelPriority'}
    require(set(selectors)==required and selectors['experiments']==dict(autocomplete_affinity_score='off',autocomplete_affinity_ml_model='off'),'Missing/guessed selectors or experiments')
    for item in proof.get('memberItems',[]):
        require(exact(item,selectors['members'].get(item.get('id'))),'Ranked person not original observed selector')
        raw=next((r for r in server['seed']['boot']['users'] if r['id']==item.get('id')),None)
        if raw is None:raw=next((person(n,True) for n in range(24) if person(n,True)['id']==item.get('id')),None)
        require(raw is not None,'Ranked member not acquired from original Boot/search')
        for key in ('id','name','real_name','team_id','deleted','is_bot','is_restricted','is_ultra_restricted'):
            require(item.get(key)==raw.get(key),'Ranked member changed source '+key)
        facts=item.get('facts',{});label=raw['profile'].get('display_name') or raw['real_name']
        require(facts.get('member') is True and facts.get('active') is True and facts.get('team')==raw['team_id'] and
                facts.get('self') is (raw['id']=='UWORKFLOW') and facts.get('bot') is False and facts.get('external') is False and
                facts.get('comparisonName')==label and facts.get('bestName')==label,'Unobserved member ranking facts')
    for item in proof.get('channelItems',[]):require(item.get('facts',{}).get('channelMember') is True,'Unjoined channel ranked as granted')
    if case in ('cold-mixed','cached-write') and stage!='warm-ready':require(len(proof.get('matches',[]))>72 and len(proof['results'])==24,'Combined72/24 cutoff not exercised')
    model=WEIGHTS if case=='cached-write' and query!='oldreview' else None
    return dict(proof=proof,remote={kind:next((p['ids'] for p in pubs if p['kind']==kind),[]) for kind in ('members','channels')},model=model)


def options(q,server):
    known={r['id']:r for r in server['seed']['boot']['users'] if not r.get('is_bot')}
    known.update({person(n,True)['id']:person(n,True) for n in range(24)})
    people=q.get('people');require(isinstance(people,list) and len({r['id'] for r in people})==len(people),'Ranking duplicate people')
    for p in people:require(p.get('id') in known and exact(p,dict(id=p['id'],source=boot_member(known[p['id']]),interaction=True)),'Ranking person lacked canonical source')
    expected=[]
    for r in q['rows']:
        id=r['id'];raw=channel(int(id[6:]),True) if id.startswith('CRANKR') and id in {f'CRANKR{n:03}' for n in range(24)} else A.CHANNELS.get(id)
        require(raw is not None and r.get('peer') is None and exact(r.get('raw'),dict(raw,fromAnotherTeam=False)),'Ranking channel fresh posting source')
        expected.append(dict(id=id,label=raw['name']))
    expected += [dict(id=p['id'],label=known[p['id']]['profile']['display_name']) for p in people]
    ids=q['search']['results'];selected=q['selected']
    if selected and selected not in {r['id'] for r in expected}:
        require(selected==A.DEST,'Unproved ranking pinned destination');expected.append(dict(id=A.DEST,label=A.CHANNELS[A.DEST]['name']))
    if ids is not None:
        expected=[r for r in expected if r['id'] in ids or r['id']==selected]
        expected.sort(key=lambda r:ids.index(r['id']) if r['id'] in ids else 10**6)
    require(exact(q.get('options'),expected),'Mixed ranked identityâ†’option order changed')
    return [dict(value='',text='Choose a conversation',disabled=False)]+[dict(value=r['id'],text=r['label'],disabled=False) for r in expected]


def replay(inputs):
    unique={json.dumps(x,sort_keys=True):x for x in inputs}
    if not unique:return
    run=subprocess.run(['node','scripts/workflow-share-ranking-oracle.mjs'],input=json.dumps(list(unique.values())),capture_output=True,text=True,encoding='utf-8',cwd=Path(__file__).resolve().parents[1],timeout=120)
    require(run.returncode==0,'Retained original ranking replay failed: '+run.stderr[-5000:])
    result=json.loads(run.stdout);require(result.get('ok') is True and len(result.get('results',[]))==len(unique),'Retained ranking replay incomplete')


def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,case):
    import workflow_management_share_drafts as drafts
    Q.mode('ranking:'+case,receipt.get('shareCase'),receipt.get('shareDraftCustody'));Q.identity(receipt,'ranking:'+case)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Ranking original terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==Q.STAGES['ranking:'+case] and set(receipt['captures'])==set(Q.STAGES['ranking:'+case]),'Ranking exact stage/PNG inventory')
    checked=initial=parent_ids=share_ids=query_node=previous=prev_obs=None;proofs=[]
    success=case in ('cold-mixed','cached-write')
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];model=row['model'];ex=p['exercise']
        parent_ids=P.common(receipt,row,checked,parent_ids,viewport,rectangle,None,search_case='ranking:'+case)
        v=ledger(case,stage,server);prev_obs=observations(case,stage,server,prev_obs);ordinary_reads=ordinary(stage,server,initial)
        if previous is not None:
            for key in ('requests','responses'):require(exact(v[key][:len(previous[key])],previous[key]),'Ranking original acquisition ledger mutated')
        previous=v
        cid,ts=D.send_ledger('success',stage if success else 'share-open',server,destination=DEST)
        require(server['confirmationAdd']['share'].get('person') is None,'Ranking query silently opened a person')
        if stage not in A.PREFIX:
            q=p['firstPublish']['attempts'][-1]['confirmationShare'];rp=state_proof(case,stage,q,v,server)
            if rp:proofs.append(rp)
            S.operation_proof('success',stage,q,server,destination=DEST);S.message_proof('success',stage,row,cid,ts,destination=DEST)
            active=stage not in ('share-cancelled','late-settled','share-settled','destination')
            require(q.get('bound') is True and q.get('active') is active and p['firstPublish']['attempts'][-1]['closed'] is (not active),'Ranking original modal lifecycle')
            require(q.get('text')==('' if stage=='share-open' else S.MESSAGE),'Ranking consumed optional text')
            selected='' if stage=='share-open' or success and stage not in ('selection-ready','share-settled','destination') else DEST if success else A.DEST
            require(q.get('selected')==selected,'Ranking selected before explicit user choice/lost pin')
            expected=options(q,server)
            if stage=='share-open':require(len(q['people'])==43,'Original self+42 local candidates missing')
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle);controls=native['share']['controls'];query=controls.get('query',{})
                integer(query.get('nodeId'),'Original ranking query node',1);query_node=query_node or query['nodeId']
                require(query['nodeId']==query_node and query.get('value')==q['search']['query'] and native['share'].get('queryRevision')==str(q['search']['revision']),'Ranking original query node/value/revision')
                require(exact(native['share']['options'],expected),'Rendered mixed option order')
                pending=q['search']['status']=='searching'
                require(controls['select']['disabled'] is pending and controls['refresh']['disabled'] is pending and controls['text']['readOnly'] is pending and controls['cancel']['disabled'] is False and query['readOnly'] is False and controls['send']['disabled'] is (pending or not selected),'Ranking pending controls/Cancel/Send gates')
                status=native['share'].get('searchStatus')
                expected_status='Searching conversations\u2026' if pending else Q.FAILURE if q['search']['status']=='failed' else 'No matching conversations found.' if q['search']['results']==[] else None
                require(status is None if expected_status is None else isinstance(status,dict) and status.get('text')==expected_status and status.get('visible') is True,'Ranking visible status')
            else:require(native.get('share') is None and native.get('modal') is None,'Ranking modal survived explicit close')
            require(exact(ex.get('after'),F.observation(row)),'Ranking action/capture changed')
            if stage in ('warm-pending','query-pending','member-pending') and isinstance(ex.get('before'),dict) and 'text' in ex['before']:
                setup=ex['before'];S.activation(setup.get('text'),'text',viewport,rectangle)
                require(exact(setup['observation']['model']['shareCustody'],model['shareCustody']),'Query consumed independent drafts')
                if success:require(setup.get('selection') is None,'Ranking success preselected destination')
                else:S.activation(setup.get('selection'),'select',viewport,rectangle);require(setup['selection']['value']==A.DEST,'Ranking negative lost actual selection')
            action=ex.get('activation')
            if isinstance(action,dict) and action.get('kind')=='query':
                require(action.get('originalNode') is True and action.get('nodeId')==query_node and action.get('id')=='workflow-share-query' and action.get('value')==q['search']['query'] and exact(action.get('event'),dict(type='input',isTrusted=False,targetNodeId=query_node)),'Ranking original input provenance')
                require(action.get('start')==action.get('end')==len(action['value'].encode('utf-16-le'))//2,'Ranking Unicode caret');rectangle(action['rect'],viewport)
            elif action is not None:S.activation(action,'select' if stage=='selection-ready' else 'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if 'quiet' in ex:require(ex['quiet'].get('observedMs')==500 and exact(ex['quiet']['before'],ex['quiet']['after']) and exact(ex['quiet']['after'],F.observation(row)),'Ranking late automatic effect')
            encode=lambda x:json.dumps(x,sort_keys=True)
            require(Counter(map(encode,q['reads']))==Counter(encode(dict(raw=r['result']['raw'])) for r in ordinary_reads),'Ranking discarded/fabricated fresh posting receipts')
            completed=[h['ranking'] for h in q['search']['history'] if isinstance(h.get('ranking'),dict)]
            if isinstance(q['search'].get('ranking'),dict):completed.append(q['search']['ranking'])
            expected_info=[r['id'] for ranked in completed for r in ranked['results'] if r['kind']=='channels']
            if stage in ('share-settled','destination'):expected_info.append(DEST)
            baseline=initial if initial is not None else ordinary_reads
            require([r['body'][0][1] for r in ordinary_reads[len(baseline):]]==expected_info,'Ranking repeated/omitted independent per-result posting check or Send check')
            if stage=='destination':
                require(model['confirmationDestination']['channel']==DEST,'Ranked selection navigated wrong identity')
                rendered=native['shareTimeline']['rows'];require(len(rendered)==1 and rendered[0]['messageId']==S.SERVER_TS and all(t in rendered[0]['text'] for t in S.MESSAGE.split('\n')) and any(l['href']==S.URL for l in rendered[0]['links']),'Ranked destination rich message absent')
        if stage=='checked':checked=row
        if stage=='share-open':initial=ordinary_reads
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Ranking original PNG custody')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    replay(proofs)
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Ranking final original observation')
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
