"""Independent typed-search acquisition, original query and selection custody."""
import json
from pathlib import Path
from collections import Counter
import workflow_management_share_person as P
from workflow_management_delete import exact,require,integer
from workflow_management_managers import boot_member
S,A,F,D=P.S,P.A,P.F,P.D
BASE_CASES=('person-success','channel-success','prefixes','rapid-query','held-cancel','rejected','offline')
RANKING_CASES=('cold-mixed','cached-write','prefixes','stale-cancel','read-failure')
EMPTY_CASES=('initial','clear','prefixes','cold-failure','cold-missing','cold-offline','warm-offline')
STARTUP_CASES=('success','failure','missing','prefs-retired','socket-retired')
CASES=tuple('startup:'+c for c in STARTUP_CASES)+BASE_CASES+tuple('ranking:'+c for c in RANKING_CASES)+tuple('empty:'+c for c in EMPTY_CASES)
CHANNEL='CSEARCH001';LABEL='research-destination'
STAGES={c:A.PREFIX+('share-open',)+(
 ('member-pending','member-ready','channel-pending','channel-ready','share-cancelled') if c=='prefixes' else
 ('query-pending','replacement-pending','query-ready','late-settled','share-cancelled') if c=='rapid-query' else
 ('query-pending','share-cancelled','late-settled') if c=='held-cancel' else
 ('query-pending','query-failed','share-cancelled') if c in ('rejected','offline') else
 ('query-pending','query-ready')+(('person-opening',) if c=='person-success' else ())+('selection-ready','share-settled','destination')) for c in BASE_CASES}
RANKING_TAILS={
 'cold-mixed':('query-pending','query-ready','selection-ready','share-settled','destination'),
 'cached-write':('warm-pending','warm-ready','query-pending','query-ready','selection-ready','share-settled','destination'),
 'prefixes':('member-pending','member-ready','channel-pending','channel-ready','share-cancelled'),
 'stale-cancel':('query-pending','replacement-pending','share-cancelled','late-settled'),
 'read-failure':('query-pending','query-failed','share-cancelled')}
STAGES.update({'ranking:'+c:A.PREFIX+('share-open',)+tail for c,tail in RANKING_TAILS.items()})
EMPTY_TAILS={
 'initial':(), 'cold-failure':(), 'cold-missing':(), 'cold-offline':(),
 'clear':('channel-choices','selection-retained','query-pending','empty-cleared','late-settled','empty-refreshed'),
 'prefixes':('channel-choices','selection-retained','member-empty','channel-empty','whitespace-pending','whitespace-ready','empty-cleared'),
 'warm-offline':('channel-choices','selection-retained','offline-empty','offline-query')}
STAGES.update({'empty:'+c:A.PREFIX+('share-open',)+tail+('share-cancelled',) for c,tail in EMPTY_TAILS.items()})

STAGES.update({'startup:'+c:A.PREFIX+('startup-before','startup-offline','share-open','text-retained','offline-query','share-cancelled') for c in STARTUP_CASES})
STAGES['startup:socket-retired']=A.PREFIX+('startup-before','startup-retired')

FAILURE='Search could not finish. Your previous destinations are still available. Refresh destinations to try again.'

def mode(case,share,drafts,cancel=False,resend=None,posting=None,dm=None,person=None):
    if case is None:return
    require(case in CASES and share=='success' and drafts is True and cancel is False and all(v is None for v in (resend,posting,dm,person)),'Search requires isolated successful Share/drafts mode')

def identity(receipt,case):
    if case is not None and case.startswith('startup:'):
        require(receipt.get('shareSearchCheck') is True and receipt.get('shareSearchCase')=='startup' and receipt.get('shareStartupCase')==case.split(':',1)[1],'Startup mode identity changed')
        return
    if case is not None and case.startswith('empty:'):
        require(receipt.get('shareSearchCheck') is True and receipt.get('shareSearchCase')=='empty' and receipt.get('shareEmptyCase')==case.split(':',1)[1],'Empty mode identity changed')
        return
    if case is not None and case.startswith('ranking:'):
        require(receipt.get('shareSearchCheck') is True and receipt.get('shareSearchCase')=='ranking' and receipt.get('shareRankingCase')==case.split(':',1)[1],'Ranking mode identity changed')
        return
    require(receipt.get('shareSearchCheck',False) is (case is not None) and receipt.get('shareSearchCase')==case,'Search mode identity changed')

def person(n):
    name=f'Fixture Person {n:02}'
    return dict(id=f'USEARCH{n:03}',name=name.lower().replace(' ','_'),real_name=name,team_id='TWORKFLOW',updated=101,deleted=False,is_bot=False,is_restricted=False,is_ultra_restricted=False,is_stranger=False,profile=dict(real_name=name,real_name_normalized=name,display_name=name,display_name_normalized=name),is_app_user=False,is_workflow_bot=False,is_profile_only=False)

def channel():return dict(A.CHANNELS[A.DEST],id=CHANNEL,name=LABEL)

def seed_proof(server):
    if server.get('confirmationAdd',{}).get('share',{}).get('search',{}).get('case')=='ranking':
        from workflow_management_share_ranking import seed_proof as ranking_seed
        return ranking_seed(server)
    A.seed_proof(server)
    users=server['seed']['boot']['users']
    if server.get('confirmationAdd',{}).get('share',{}).get('search',{}).get('case') in ('empty','startup'):
        require(len(users)==77 and exact(users[2:],[person(n) for n in range(75)]),'Empty original75 people seed changed')
        require(exact(server['seed']['boot'].get('workspaces'),[dict(id='TWORKFLOW',name='Home fixture')]),'Empty original Boot membership missing')
        return
    require(len(users)==53 and exact(users[2:],[person(n) for n in range(51)]),'Search original51 people seed changed')
    require(not any(v.get('id') in (D.PEER,CHANNEL) for v in users+server['seed']['canonicalChannels']),'Remote source was preseeded')

def page(method,query,marker=None):
    require(query in ('Dana','research','oldDana','rejected','offline') and marker in (None,'search-page-2'),'Unexpected search query/marker')
    meta=dict(query=query,method=method,page=1 if marker is None else 2)
    if method=='channels/search':
        require(marker is None,'Unexpected channel continuation')
        if query!='research':return dict(ok=True,results=[],next_marker='',opaque_receipt=meta),None
        return dict(ok=True,results={'z99':dict(channel(),is_member=False)},member_channels=[CHANNEL],next_marker='',opaque_receipt=meta),['z99']
    if query=='research':
        require(marker is None,'Unexpected empty member continuation')
        return dict(ok=True,results=[],next_marker='',opaque_receipt=meta),None
    indices=range(13) if marker is None else range(13,25)
    rows=[D.peer() if n==0 and query in ('Dana','oldDana') else person(100+n) for n in indices]
    keys=[f'z{99-n:02}' for n in range(len(rows))]
    return dict(ok=True,results=dict(zip(keys,rows)),next_marker='search-page-2' if marker is None else '',opaque_receipt=meta),keys

def body(method,query,marker=None):
    v=dict(query=query,count=24,fuzz=1)
    if method=='users/search':v.update(enable_workspace_ranking=True,search_email=True,filter='NOT deactivated',include_profile_only_users=True)
    else:v.update(filter='xws',include_record_channels=True,check_membership=True)
    if marker is not None:v['marker']=marker
    return v

def sequences(case,stage):
    if stage in A.PREFIX+('share-open',):return []
    first='Dana' if case in ('person-success','held-cancel') else 'research' if case=='channel-success' else 'oldDana' if case=='rapid-query' else 'Dana' if case=='prefixes' else case
    plan=[('api.getFlannelHttpUrl','',None),('users/search',first,None)]
    if stage in ('query-pending','member-pending') or case=='held-cancel':return plan
    if case=='rapid-query':return plan+[('api.getFlannelHttpUrl','',None),('channels/search','research',None)]
    if case=='prefixes':
        plan.append(('users/search','Dana','search-page-2'))
        return plan if stage=='member-ready' else plan+[('api.getFlannelHttpUrl','',None),('channels/search','research',None)]
    if case=='person-success':plan.append(('users/search','Dana','search-page-2'))
    plan.append(('channels/search',first,None));return plan

def ledger(case,stage,server):
    v=server['confirmationAdd']['share'].get('search',{})
    require(v.get('case')==case and v.get('networkCalls')==0 and v.get('initialPeople')==51 and v.get('remotePersonInitiallyAbsent') is True and v.get('remoteChannelInitiallyAbsent') is True and v.get('scheduling')=='first-search-page-per-wire-query-held-until-native-capture','Closed search source/identity changed')
    plan=sequences(case,stage);req=v.get('requests');res=v.get('responses')
    require(isinstance(req,list) and isinstance(res,list) and len(req)==len(res)==len(plan),'Search issued/response count or replay')
    for serial,((method,query,marker),r,response) in enumerate(zip(plan,req,res),1):
        discovery=method=='api.getFlannelHttpUrl'
        args=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else body(method,query,marker)
        endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method
        require(exact(r,dict(serial=serial,method=method,query=query,body=args,endpoint=endpoint,retrySafe=False,owner=S.OWNER)),'Search exact immutable request/owner changed')
        raw,keys=(F.DISCOVERY_RAW,None) if discovery else page(method,query,marker)
        expected=dict(serial=serial,raw=raw,orderedKeys=keys)
        if query=='offline':expected=dict(serial=serial,error='offline')
        if query=='rejected':expected=dict(serial=serial,raw=dict(ok=False,error='not_allowed',opaque_receipt='original-search-rejected'),orderedKeys=None)
        require(exact(response,expected),'Search original response/ordering lost')
    held=[]
    if stage in ('query-pending','member-pending'):held=[2]
    if stage=='replacement-pending':held=[2,4]
    if stage=='channel-pending':held=[5]
    if case=='held-cancel' and stage=='share-cancelled':held=[2]
    require(exact(v.get('held'),held),'Original search held sender changed')
    releases=v.get('releases');settlements=v.get('settlements')
    require(isinstance(releases,list) and isinstance(settlements,list),'Search settlement evidence absent')
    expected_release=[] if len(plan)<2 or stage in ('query-pending','member-pending','replacement-pending') or case=='held-cancel' and stage=='share-cancelled' else [2]
    if case=='rapid-query' and stage not in A.PREFIX+('share-open','query-pending','replacement-pending'):expected_release=[2,4]
    if case=='prefixes' and stage in ('channel-ready','share-cancelled'):expected_release=[2,5]
    require([r.get('serial') for r in releases]==expected_release,'Search original release ordering changed')
    for r in releases:require(type(r.get('receiverRetired')) is bool and r.get('delivered') is (not r['receiverRetired']),'Search release disposition missing')
    nums=[r.get('serial') for r in settlements]
    require(len(set(nums))==len(nums) and all(type(n) is int and 1<=n<=len(plan) for n in nums),'Search settlement duplicate/foreign')
    ordinary=set(range(1,len(plan)+1))-set(held)
    # A retired held future can settle before its retained sender is released.
    optional={2} if case in ('held-cancel','rapid-query') and stage not in ('query-pending',)+A.PREFIX+('share-open',) else set()
    require(ordinary<=set(nums)<=ordinary|optional,'Search original settlement inventory')
    for r in settlements:
        require(type(r.get('returned')) is bool,'Search actual completion missing')
        if r['serial'] not in optional:require(r['returned'] is True,'Nonretired search did not return original response')
    return v

def publication(method,query):
    raw,keys=page(method,query);rows=list(raw['results']) if isinstance(raw['results'],list) else [raw['results'][k] for k in keys]
    count=1;marker=None
    if raw['next_marker']:
        b,k=page(method,query,raw['next_marker']);rows += [b['results'][x] for x in k];count=2;marker=rows[24]['id'];rows=rows[:24]
    return dict(membershipIds=raw.get('member_channels',[]),kind='members' if method=='users/search' else 'channels',ids=[r['id'] for r in rows],rows=rows,pageCount=count,nextMarker=marker,applied=True)

def state_proof(case,stage,q,v):
    s=q.get('search');require(isinstance(s,dict),'Typed search observation absent')
    integer(s.get('revision'),'Search revision',1)
    prefix=stage in A.PREFIX+('share-open',)
    if prefix:require(s.get('query')=='' and s.get('status')=='idle' and s.get('results') is None and all(s.get(k)==[] for k in ('reads','failures','publications','history')),'Search ran before original input');return
    query='Dana' if case in ('person-success','held-cancel') else 'research' if case=='channel-success' else case
    if case=='prefixes':query='@Dana' if stage in ('member-pending','member-ready') else '#research'
    if case=='rapid-query':query='oldDana' if stage=='query-pending' else '#research'
    pending=stage in ('query-pending','member-pending','replacement-pending','channel-pending') or case=='held-cancel'
    require(s.get('query')==query and s.get('status')==('searching' if pending else 'failed' if case in ('rejected','offline') else 'ready'),'Search adopted stale query/status')
    if pending:require(s.get('reads')==[] and s.get('publications')==[] and s.get('failures')==[],'Partial held page published');return
    current=[(r,response) for r,response in zip(v['requests'],v['responses'])]
    if case=='prefixes' and query.startswith('#'):current=current[3:]
    if case=='rapid-query':current=current[2:]
    first_kind='channels' if query.startswith('#') else 'members'
    reads=[]
    for r,response in current:
        discovery=r['method']=='api.getFlannelHttpUrl';kind=first_kind if discovery else 'members' if r['method']=='users/search' else 'channels'
        x=dict(kind=kind,method=r['method'],arguments=None if discovery else r['body'])
        if 'error' in response:x['failure']=dict(kind='offline')
        elif r['query']=='rejected':x['failure']=dict(kind='api-or-invalid')
        else:x.update(raw=response['raw'],orderedKeys=response['orderedKeys'])
        reads.append(x)
    require(exact(s.get('reads'),reads),'Typed search discarded/fabricated original ordered read')
    if case in ('rejected','offline'):
        failure='offline' if case=='offline' else 'api-or-invalid'
        require(exact(s.get('failures'),[dict(kind=k,phase='read',failure=dict(kind=failure)) for k in ('members','channels')]) and s.get('publications')==[] and s.get('results') is None,'Failed search granted candidates')
        return
    wire=query.lstrip('@#');methods=['users/search'] if query.startswith('@') else ['channels/search'] if query.startswith('#') else ['users/search','channels/search']
    pubs=[publication(m,wire) for m in methods]
    require(exact(s.get('publications'),pubs) and s.get('failures')==[],'Search partial/unordered publication')
    ids=[i for p in pubs for i in p['ids']][:24]
    require(exact(s.get('results'),ids),'Search final count24/order changed')


def ordinary(stage,server,opened=None):
    rows=server['confirmationAdd']['share']['reads'];initial=[] if opened is None else opened['proof']['server']['confirmationAdd']['share']['reads']
    require(isinstance(rows,list) and len(rows)<=8,'Search ordinary read bound')
    for n,r in enumerate(rows,1):
        pairs=r.get('body');require(isinstance(pairs,list) and len(pairs)==2,'Search posting read body')
        id=pairs[0][1];require(id in A.CHANNELS or id==CHANNEL,'Search unexpected posting destination')
        raw=channel() if id==CHANNEL else A.CHANNELS[id]
        receipt='original-search-channel-posting' if id==CHANNEL else 'original-share-channel'
        require(exact(r,dict(serial=n,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,result=dict(raw=dict(ok=True,channel=raw,opaque_receipt=receipt)))),'Search posting source/request changed')
    if stage in A.PREFIX:require(rows==[],'Premature Share listing')
    elif opened is None:require(len(rows)<=5 and len({r['body'][0][1] for r in rows})==len(rows),'Repeated initial ordinary reads')
    else:require(exact(rows[:len(initial)],initial) and all(r['body'][0][1]==CHANNEL for r in rows[len(initial):]),'Search changed initial ordinary source')
    sources=server['confirmationAdd']['share']['search'].get('postingSources')
    info=[r for r in rows if r['body'][0][1]==CHANNEL]
    require(isinstance(sources,list) and len(sources)==len(info),'Search pre-info source witness missing or duplicated')
    for r,source in zip(info,sources):
        require(set(source)=={'readSerial','canonical','current'} and source['readSerial']==r['serial'] and source['current'] is True,'Search pre-info source ownership/serial changed')
        canonical=source.get('canonical')
        require(isinstance(canonical,dict) and canonical.get('id')==CHANNEL and canonical.get('name')==LABEL and canonical.get('is_member') is True and canonical.get('fromAnotherTeam') is False,'Original membership companion did not produce joined canonical before info')
        require(r['result']['raw']['channel']['is_member'] is True,'Fresh posting info membership changed')
    return rows


def options(q,server):
    known={r['id']:r for r in server['seed']['boot']['users'] if not r.get('is_bot')}
    known.update({person(n)['id']:person(n) for n in range(100,125)});known[D.PEER]=D.peer()
    people=q.get('people');require(isinstance(people,list) and len({p['id'] for p in people})==len(people),'Search duplicate/absent canonical people')
    for p in people:
        require(p.get('id') in known and exact(p,dict(id=p['id'],source=boot_member(known[p['id']]),interaction=True)),'Search raw result used as unsupported person grant')
    expected=[];mapped=set();selected=q['selected'];results=q['search']['results']
    for r in q['rows']:
        id=r['id'];raw=D.channel() if id==D.DEST else channel() if id==CHANNEL else A.CHANNELS.get(id)
        require(raw is not None and exact(r['raw'],dict(raw,fromAnotherTeam=False)),'Search channel source/foreign ownership changed')
        if id==D.DEST:
            require(exact(r['peer'],dict(id=D.PEER,raw=D.peer(),effective=D.peer(),source=D.peer_source())),'Searched person fresh peer validation missing')
            if 'recipients' not in q or results is None or id==selected or id in results:mapped.add(D.PEER)
        else:require(r.get('peer') is None,'Channel fabricated peer')
        expected.append(dict(id=id,label=D.LABEL if id==D.DEST else raw['name']))
    expected += [dict(id=p['id'],label=known[p['id']]['profile']['display_name']) for p in people if p['id'] not in mapped]
    if selected and not any(r['id']==selected for r in expected):
        require(selected==A.DEST,'Unproved pinned selection');expected.append(dict(id=A.DEST,label=A.CHANNELS[A.DEST]['name']))
    if results is not None:
        expected=[r for r in expected if r['id']==selected or r['id'] in results]
        expected.sort(key=lambda r:results.index(r['id']) if r['id'] in results else 10**6)
    require(exact(q.get('options'),expected),'Typed options changed original results/selected pin')
    return [dict(value='',text='Choose a conversation',disabled=False)]+[dict(value=r['id'],text=r['label'],disabled=False) for r in expected]


def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,search_case):
    if search_case.startswith('startup:'):
        from workflow_management_share_startup import validate as startup_validate
        return startup_validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,png=png,case=search_case.split(':',1)[1])
    if search_case.startswith('empty:'):
        from workflow_management_share_empty import validate as empty_validate
        return empty_validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,png=png,case=search_case.split(':',1)[1])
    if search_case.startswith('ranking:'):
        from workflow_management_share_ranking import validate as ranking_validate
        return ranking_validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,png=png,case=search_case.split(':',1)[1])
    import workflow_management_share_drafts as drafts
    case=search_case;mode(case,receipt.get('shareCase'),receipt.get('shareDraftCustody'));identity(receipt,case)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Search original terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==STAGES[case] and set(receipt['captures'])==set(STAGES[case]),'Search exact stage inventory')
    from workflow_management_share_search_current import current, state as current_state, ordinary as current_ordinary
    import workflow_management_share_empty as empty
    current_abi=current(rows[-1]['proof']['server']);proofs=[];previous_observations=None
    checked=opened=None;parent_ids=share_ids=query_node=None;old_revision=None;previous_ledger=None
    success=case in ('person-success','channel-success');destination=D.DEST if case=='person-success' else CHANNEL
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];model=row['model'];ex=p['exercise']
        parent_ids=P.common(receipt,row,checked,parent_ids,viewport,rectangle,None,search_case=case)
        require(current(server) is current_abi,'Mixed historical/current search receipt schema')
        v=ledger(case,stage,server);ordinary_reads=(current_ordinary if current_abi else ordinary)(stage,server,opened)
        if current_abi:
            previous_observations=empty.observation_ledger('initial',stage,server,previous_observations)
            require(exact(server['seed']['boot'].get('workspaces'),[dict(id='TWORKFLOW',name='Home fixture')]),'Current typed original Boot membership missing')
        if previous_ledger is not None:
            for key in ('requests','responses'):require(exact(v[key][:len(previous_ledger[key])],previous_ledger[key]),'Search original ledger mutated')
        previous_ledger=v
        cid,ts=D.send_ledger('success',stage if success else 'share-open',server,destination=destination)
        if case=='person-success':
            mapped='share-open' if stage in A.PREFIX+('share-open','query-pending','query-ready') else 'person-ready' if stage=='selection-ready' else stage
            person_ledger=P.ledger('success',mapped,server);P.source('success',mapped,model['sharePersonSource'])
        else:require(server['confirmationAdd']['share'].get('person') is None,'Search unexpectedly opened a DM')
        if stage not in A.PREFIX:
            q=p['firstPublish']['attempts'][-1]['confirmationShare']
            if current_abi:
                empty.successful_notice(q,native)
                replay=current_state(case,stage,q,v,server)
                if replay is not None:proofs.append(replay)
            else:state_proof(case,stage,q,v)
            S.operation_proof('success',stage,q,server,destination=destination);S.message_proof('success',stage,row,cid,ts,destination=destination)
            active=stage not in ('share-cancelled','share-settled','destination') and not (case=='held-cancel' and stage=='late-settled')
            require(q.get('bound') is True and q.get('active') is active and p['firstPublish']['attempts'][-1]['closed'] is (not active),'Search native modal lifecycle')
            require(q.get('text')==('' if stage=='share-open' else S.MESSAGE),'Search consumed optional message')
            selected='' if stage=='share-open' or success and stage in ('query-pending','query-ready') else D.PEER if stage=='person-opening' else destination if success else A.DEST
            require(q.get('selected')==selected,'Search selected/opened before actual choice or lost original choice')
            expected_options=options(q,server)
            if stage=='share-open':
                require(D.PEER not in [x['id'] for x in q['people']],'Absent remote peer was preseeded')
                if not current_abi:require(len(q['people'])==50,'Historical initial50 cap changed')
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle);controls=native['share']['controls'];query=controls.get('query',{})
                integer(query.get('nodeId'),'Original search node',1)
                query_node=query_node or query['nodeId'];require(query['nodeId']==query_node and query.get('value')==q['search']['query'] and native['share'].get('queryRevision')==str(q['search']['revision']),'Search original node/value/revision mismatch')
                require(exact(native['share']['options'],expected_options),'Search rendered result order changed')
                pending=q['search']['status']=='searching';opening=stage=='person-opening'
                require(controls['select']['disabled'] is (pending or opening) and controls['refresh']['disabled'] is (pending or opening) and controls['text']['readOnly'] is (pending or opening) and controls['cancel']['disabled'] is False and query['readOnly'] is opening,'Search pending original input/Cancel gates')
                require(controls['send']['disabled'] is (pending or opening or not selected),'Search premature Send')
                status=native['share'].get('searchStatus')
                if pending:require(isinstance(status,dict) and status['text']=='Searching conversations\u2026' and status['visible'] is True,'Actual searching guidance missing')
                elif case in ('rejected','offline') and stage=='query-failed':require(isinstance(status,dict) and status['text']==FAILURE and status['visible'] is True,'Actual failed search guidance missing')
                elif current_abi and q['search']['status']=='partial':require(isinstance(status,dict) and status.get('text')=='Some results could not be verified. Refresh destinations to try again.' and status.get('visible') is True,'Current posting-admission partial guidance missing')
                else:require(status is None,'Unexpected search guidance')
            else:require(native.get('share') is None and native.get('modal') is None,'Search modal survived Cancel/send')
            require(exact(ex.get('after'),F.observation(row)),'Search action/capture changed')
            if stage in ('query-pending','member-pending'):
                setup=ex.get('before',{});S.activation(setup.get('text'),'text',viewport,rectangle)
                require(exact(setup.get('observation',{}).get('model',{}).get('shareCustody'),model.get('shareCustody')),'Typing search consumed separate draft')
                if success:require(setup.get('selection') is None,'Success query preselected a destination')
                else:
                    S.activation(setup.get('selection'),'select',viewport,rectangle)
                    require(setup['selection']['value']==A.DEST,'Negative search did not retain actual original selection')
            activation=ex.get('activation')
            if isinstance(activation,dict) and activation.get('kind')=='query':
                require(activation.get('originalNode') is True and activation.get('nodeId')==query_node and activation.get('id')=='workflow-share-query' and activation.get('value')==q['search']['query'],'Search did not input into original query')
                require(exact(activation.get('event'),dict(type='input',isTrusted=False,targetNodeId=query_node)),'Search input provenance')
                require(activation.get('start')==activation.get('end')==len(activation['value'].encode('utf-16-le'))//2,'Search UTF16 caret')
                rectangle(activation['rect'],viewport)
            elif activation is not None:S.activation(activation,'select' if stage in ('person-opening','selection-ready') else 'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if 'quiet' in ex:require(ex['quiet'].get('observedMs')==500 and exact(ex['quiet'].get('before'),ex['quiet'].get('after')) and exact(ex['quiet']['after'],F.observation(row)),'Search automatic side effect during quiet')
            # All separate query acquisition reads live in search; ordinary fresh
            # posting/member validation remains in the existing immutable ledger.
            expected_reads=[dict(raw=r['result']['raw']) for r in ordinary_reads]
            if case=='person-success':expected_reads += [dict(raw=r['raw']) for r in person_ledger['replies']]
            encode=lambda x:json.dumps(x,sort_keys=True)
            require(Counter(map(encode,q['reads']))==Counter(map(encode,expected_reads)),'Search ordinary read/source ledger changed')
            if stage=='destination':
                require(model['confirmationDestination']['channel']==destination,'Search navigated wrong destination')
                rendered=native['shareTimeline']['rows'];require(len(rendered)==1 and rendered[0]['messageId']==S.SERVER_TS and all(t in rendered[0]['text'] for t in S.MESSAGE.split('\n')) and any(l['href']==S.URL for l in rendered[0]['links']),'Search rich message not rendered')
            revision=q['search']['revision']
            if old_revision is not None:require(revision>=old_revision,'Search revision regressed')
            old_revision=revision
        if stage=='checked':checked=row
        if opened is not None:
            extra=len(ordinary_reads)-len(opened['proof']['server']['confirmationAdd']['share']['reads'])
            expected_extra=(2 if stage in ('share-settled','destination') else 1 if stage in ('query-ready','selection-ready') else 0) if case=='channel-success' else (1 if stage in ('query-ready','late-settled','share-cancelled') else 0) if case=='rapid-query' else (1 if stage in ('channel-ready','share-cancelled') else 0) if case=='prefixes' else 0
            require(extra==expected_extra,'Search repeated/omitted fresh posting acquisition')
        if stage=='share-open':opened=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Search PNG custody')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Search final original observation')
    if current_abi:empty.R.replay(proofs)
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
