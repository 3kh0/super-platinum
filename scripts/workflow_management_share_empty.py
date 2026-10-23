"""Closed initial/cleared Share suggestions; source ranking is replayed separately."""
import json
import uuid
from pathlib import Path
from collections import Counter
import workflow_management_share_search as Q
import workflow_management_share_ranking as R
from workflow_management_delete import require, exact, integer
P,S,A,F,D=Q.P,Q.S,Q.A,Q.F,Q.D
CASES=Q.EMPTY_CASES
COLD=('cold-failure','cold-missing','cold-offline')


def observation_ledger(case,stage,server,previous=None,*,ordinary_share=False):
    parent=server['confirmationAdd']['share']
    if ordinary_share:
        require(case=='initial' and parent.get('search') is None,'Ordinary Share observation lane mixed with search fixture')
        v=parent.get('ranking')
    else:
        search=parent.get('search')
        v=search.get('ranking') if isinstance(search,dict) else None
    require(isinstance(v,dict),'Original observation lane missing')
    ns=v.get('navigationOwner')
    try: parsed=uuid.UUID(ns)
    except (ValueError,TypeError,AttributeError): parsed=None
    require(parsed is not None and parsed.version==4 and str(parsed)==ns and ns!=S.OWNER['account'],'Empty navigation namespace not original UUID')
    require(v.get('case')==('ordinary-share' if ordinary_share else 'existing-search') and v.get('cacheInjection') is False and v.get('canonicalWrites')==0 and v.get('networkCalls')==0 and v.get('localPeople') is None and v.get('modelMode')=='actual-read-absent-model','Empty shared observation producer changed')
    req,res=v.get('requests'),v.get('responses')
    require(isinstance(req,list) and isinstance(res,list) and len(req)==len(res)<=5,'Empty observation request inventory')
    prefix=stage in A.PREFIX
    methods=[] if prefix or case=='cold-offline' else ['client.gantryBoot','users.prefs.get']+([] if case in COLD else ['experiments.getByUser'])
    require([r['method'] for r in req if r['kind']=='query']==methods,'Fresh name/assignment acquisition or warm reuse changed')
    caches=Counter(r['method'] for r in req if r['kind']=='cache')
    require(caches==(Counter() if prefix or case in COLD else Counter({'search.autocomplete.model':1,'search.autocomplete.offlineFeatures':1})),'Original shared cache producer missing or replayed')
    for n,(r,response) in enumerate(zip(req,res),1):
        method=r.get('method');cache=method in ('search.autocomplete.model','search.autocomplete.offlineFeatures')
        body=R.READ_BODIES.get(method)
        if method=='experiments.getByUser':body=[['ssb_platform','windows'],['ssb_release_channel','prod']]
        require(body is not None and exact(r,dict(serial=n,kind='cache' if cache else 'query',method=method,body=body,endpoint='https://example.test/api/'+method,retrySafe=False,owner=S.OWNER)),'Empty exact original observation request')
        raw=R.observation_reply(method,'cold-mixed',server['seed']['boot']['self'])
        if case=='cold-missing' and method=='users.prefs.get':raw=dict(ok=True,opaque_receipt='original-empty-query-missing-prefs')
        expected=dict(serial=n,error='offline') if case=='cold-failure' and method=='users.prefs.get' else dict(serial=n,raw=raw,orderedKeys=None)
        require(exact(response,expected),'Empty immutable original observation reply')
    require(exact(v.get('settlements'),[dict(serial=n,returned=True) for n in range(1,len(req)+1)]),'Empty observation completion missing')
    if previous is not None:
        require(ns==previous['navigationOwner'],'Empty observation owner changed')
        for key in ('requests','responses','settlements'):require(exact(v[key][:len(previous[key])],previous[key]),'Original observation ledger mutated')
    connection=parent.get('connectivity',{})
    require(exact(connection.get('owner'),S.OWNER) and type(connection.get('generation')) is int and connection.get('carrier')=='owned-in-memory-realtime-channel' and connection.get('receiverClosed') is False and connection.get('networkCalls')==0 and connection.get('httpSuccessClaimed') is False,'Closed actual connectivity carrier absent')
    require(exact(connection.get('installed'),dict(routeObserved=True,routable=case!='cold-offline',realtimeConnected=True,transportHealth='offline' if case=='cold-offline' else 'unknown')),'Fixture fabricated HTTP/route evidence')
    return v


def directory_ledger(case,stage,server,previous=None):
    v=server['confirmationAdd']['share']['search']
    require(v.get('case')=='empty' and v.get('emptyCase')==case and v.get('initialPeople')==75 and v.get('networkCalls')==0 and v.get('remotePersonInitiallyAbsent') is True and v.get('remoteChannelInitiallyAbsent') is True and v.get('postingSources')==[],'Empty source seed/mode changed')
    started=(case=='clear' and stage in ('query-pending','empty-cleared','late-settled','empty-refreshed','share-cancelled')) or (case=='prefixes' and stage in ('whitespace-pending','whitespace-ready','empty-cleared','share-cancelled'))
    pending=stage in ('query-pending','whitespace-pending')
    query='review' if case=='clear' else ' '
    methods=[] if not started else ['api.getFlannelHttpUrl','users/search']+(['channels/search'] if case=='prefixes' and not pending else [])
    req,res=v.get('requests'),v.get('responses')
    require(isinstance(req,list) and isinstance(res,list) and len(req)==len(res)==len(methods),'Unexpected empty-directory request/automatic retry')
    for n,(method,r,response) in enumerate(zip(methods,req,res),1):
        discovery=method=='api.getFlannelHttpUrl'
        require(exact(r,dict(serial=n,method=method,query='' if discovery else query,body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if discovery else Q.body(method,query),endpoint='https://example.test/api/'+method if discovery else 'https://edgeapi.slack.com/cache/TWORKFLOW/'+method,retrySafe=False,owner=S.OWNER)),'Empty exact directory factory/query/owner')
        raw=F.DISCOVERY_RAW if discovery else dict(ok=True,results=[],next_marker='',opaque_receipt=dict(fixture='original-empty-query-search',query=query,method=method))
        require(exact(response,dict(serial=n,raw=raw,orderedKeys=None)),'Empty raw directory response changed')
    released=started and not pending and not(case=='clear' and stage=='empty-cleared')
    require(v.get('held')==([2] if started and not released else []),'Original held query identity changed')
    require(v.get('releases')==([dict(serial=2,receiverRetired=case=='clear',delivered=case!='clear')] if released else []),'Retired empty-query release disposition changed')
    expected=[dict(serial=n,returned=not(case=='clear' and n==2)) for n in range(1,len(methods)+1) if not(n==2 and pending)]
    # While clear settles, the dropped old future must have recorded retirement.
    require(sorted(v.get('settlements',[]),key=lambda r:r['serial'])==expected,'Empty actual completion/retirement evidence')
    if previous is not None:
        for key in ('requests','responses'):require(exact(v[key][:len(previous[key])],previous[key]),'Empty query original receipts overwritten')
    return v


def ordinary(stage,server,previous=None):
    v=server['confirmationAdd']['share'];rows=v.get('reads')
    require(isinstance(rows,list) and len(rows)<=24,'Empty ordinary read inventory')
    for n,r in enumerate(rows,1):
        pairs=r.get('body');require(isinstance(pairs,list) and len(pairs)==2,'Empty posting read body')
        id=pairs[0][1];require(id in A.CHANNELS,'Empty unseeded posting destination')
        require(exact(r,dict(serial=n,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,result=dict(raw=dict(ok=True,channel=A.CHANNELS[id],opaque_receipt='original-share-channel')))),'Empty immutable posting source')
    if stage in A.PREFIX:require(rows==[],'Premature Share listing')
    if previous is not None:require(exact(rows[:len(previous)],previous),'Empty ordinary receipts overwritten')
    for key in ('requests','responses','attempts','settlements','releases','held','applied'):require(v.get(key)==[],'Empty search opened/sent/mutated remotely')
    require(v.get('started') is False and v.get('person') is None and v.get('dm') is None,'Empty suggestions silently opened a DM')
    return rows


def ranking_proof(q,server,*,fresh,publications=None,require_cutoff=True):
    """Current source facts, then exact retained matcher/model replay by caller."""
    s=q['search'];p=s.get('ranking');require(isinstance(p,dict),'Empty rank source missing')
    wire=s['query'][1:] if s['query'].startswith(('@','#')) else s['query']
    require(p.get('source')=='share_dialog_channel_picker' and p.get('query')==wire and p.get('passthrough') is False,'Empty rank source/query gate')
    original=[dict(method='POST',raw=R.observation_reply(m,'cold-mixed',server['seed']['boot']['self'])) for m in ('client.gantryBoot','users.prefs.get','experiments.getByUser')]
    require(p.get('observationSource')==('fresh' if fresh else 'reused') and exact(p.get('observationReads'),original) and exact(p.get('reads'),original if fresh else []),'Empty complete observation reuse/fresh receipts')
    selectors=p.get('selectors');require(isinstance(selectors,dict) and selectors.get('selfId')=='UWORKFLOW' and selectors.get('teamId')=='TWORKFLOW' and selectors.get('enterpriseId') is None and selectors.get('enterprise') is False,'Empty rank owner changed')
    require(selectors.get('experiments')==dict(autocomplete_affinity_score='off',autocomplete_affinity_ml_model='off'),'Empty guessed experiment values')
    publications=[] if publications is None else publications
    source={r['id']:r for r in server['seed']['boot']['users']}
    for publication in publications:
        if publication['kind']=='members':source.update({r['id']:r for r in publication['rows']})
    member_ids=[r.get('id') for r in p.get('memberItems',[])]
    expected_members=set() if s['query'].startswith('#') else {id for id,raw in source.items() if not raw.get('is_bot')}
    require(len(member_ids)==len(set(member_ids)) and set(member_ids)==expected_members,'Original local/acquired member collection or prefix suppression changed')
    for item in p.get('memberItems',[]):
        raw=source.get(item.get('id'));require(raw is not None and not raw.get('is_bot'),'Empty unobserved member ranked')
        require(exact(selectors['members'].get(item['id']),item),'Empty member facts not original selectors')
        for key in ('id','name','real_name','team_id','deleted','is_bot','is_restricted','is_ultra_restricted'):
            require(item.get(key)==raw.get(key),'Empty member raw changed '+key)
        facts=item.get('facts',{});label=raw['profile'].get('display_name') or raw['real_name']
        require(facts.get('member') is True and facts.get('active') is True and facts.get('team')=='TWORKFLOW' and facts.get('self') is (raw['id']=='UWORKFLOW') and facts.get('bot') is False and facts.get('external') is False and facts.get('comparisonName')==label and facts.get('bestName')==label,'Empty invented member facts')
    canonical={r['id']:r for r in server['seed']['canonicalChannels']}
    for publication in publications:
        if publication['kind']=='channels':
            for raw in publication['rows']:
                # Original member_channels is independently verified in Q.ledger.
                canonical[raw['id']]=dict(raw,is_member=raw['id'] in publication['membershipIds']) if publication['membershipIds'] else raw
    if s['query'].startswith('@'):require(p.get('channelItems')==[],'Member prefix ranked channel family')
    for item in p.get('channelItems',[]):
        raw=canonical.get(item.get('id'));require(raw is not None and item.get('facts',{}).get('channelMember') is True,'Empty unobserved channel ranked')
        for key in ('id','name','is_member','is_im','is_mpim','is_archived','is_frozen'):require(item.get(key)==raw.get(key),'Empty channel raw changed '+key)
    posting_subset(s,server)
    if require_cutoff and s['query'] in ('','@'):
        require(len(p.get('matches',[]))>72 and len(p['results'])==24,'Empty actual local72/24 cutoff not exercised')
    return dict(proof=p,remote={kind:[id for publication in publications if publication['kind']==kind for id in publication['ids']] for kind in ('members','channels')},model=None)


def posting_subset(search,server):
    """Full source ranking is separate from the existing posting admission."""
    ranked=[r['id'] for r in search.get('ranking',{}).get('results',[])]
    require(len(ranked)<=24 and len(ranked)==len(set(ranked)),'Empty source ranking cutoff/duplicates')
    denied='CADDREAD01' in ranked
    if denied:
        original=next((r for r in server['seed']['boot']['channels'] if r['id']=='CADDREAD01'),None)
        require(exact(original,A.CHANNELS['CADDREAD01']) and original['is_read_only'] is True and original['is_general'] is True,'Empty rejected channel lacks exact source readonly policy')
        require(any(r.get('result',{}).get('raw',{}).get('channel')==original for r in server['confirmationAdd']['share']['reads']),'Empty posting refusal lacks original fresh metadata receipt')
    failures=[dict(channel='CADDREAD01',stage='posting',failure='destination unavailable')] if denied else []
    require(search.get('failures')==failures and search.get('status')==('partial' if denied else 'ready'),'Empty unexpected postranking failure or false complete result')
    require(search.get('results')==[id for id in ranked if id!='CADDREAD01'],'Empty admitted subset changed original rank order/permission')



def successful_notice(q,native):
    if q.get('search',{}).get('status') in ('ready','partial'):
        require(q.get('notice') is None,'Successful ranking retained stale listing notice')
        if native.get('share') is not None:
            require('notice' in native['share'] and native['share']['notice'] is None,'Successful ranked DOM retained stale listing notice')


def offline_transition(value,server):
    connection=server['confirmationAdd']['share']['connectivity']
    require(exact(value,dict(kind='closed-fixture-connectivity-transition',owner=S.OWNER,generation=connection['generation'],before=dict(routable=True,routeObserved=True,realtimeConnected=True),after=dict(routable=False,routeObserved=True,realtimeConnected=True,transportHealth='offline'),networkCalls=0,eventFabricated=False)),'Warm offline lacks original owned source transition')


def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,case):
    import workflow_management_share_drafts as drafts
    Q.mode('empty:'+case,receipt.get('shareCase'),receipt.get('shareDraftCustody'));Q.identity(receipt,'empty:'+case)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Empty terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==Q.STAGES['empty:'+case] and set(receipt['captures'])==set(Q.STAGES['empty:'+case]),'Empty exact stage inventory')
    checked=parent_ids=share_ids=query_node=prev_obs=prev_dir=prev_ordinary=None;proofs=[];initial_q=None;offline_reads=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];ex=p['exercise'];prefix=stage in A.PREFIX
        parent_ids=P.common(receipt,row,checked,parent_ids,viewport,rectangle,None,search_case='empty:'+case)
        prev_obs=observation_ledger(case,stage,server,prev_obs);prev_dir=directory_ledger(case,stage,server,prev_dir);prev_ordinary=ordinary(stage,server,prev_ordinary)
        if case=='cold-offline':require(prev_ordinary==[],'Cold offline performed posting IO')
        if not prefix:
            q=p['firstPublish']['attempts'][-1]['confirmationShare'];s=q['search'];active=stage!='share-cancelled';pending=stage in ('query-pending','whitespace-pending')
            require(q.get('bound') is True and q.get('active') is active and q.get('loading') is False and p['firstPublish']['attempts'][-1]['closed'] is (not active),'Empty original modal lifecycle')
            edited=case not in COLD+('initial',) and stage not in ('share-open','channel-choices')
            require(q.get('text')==(S.MESSAGE if edited else '') and q.get('selected')==(A.DEST if edited else ''),'Clear/refresh consumed original selection or optional text')
            require(q.get('operation') is None and q.get('opening') is None and q.get('openingHistory')==[],'Empty suggestions authorized open/send')
            query=('review' if stage in ('query-pending','offline-query') else '@' if stage=='member-empty' else '#' if stage in ('channel-choices','selection-retained','channel-empty') else ' ' if stage in ('whitespace-pending','whitespace-ready') else '')
            if stage=='share-cancelled':query='review' if case=='warm-offline' else ''
            require(s.get('query')==query,'Original empty/prefix/whitespace query changed')
            denied=not pending and case not in COLD and any(r.get('id')=='CADDREAD01' for r in (s.get('ranking') or {}).get('results',[]))
            status='searching' if pending else 'failed' if case in COLD else 'partial' if denied else 'ready'
            require(s.get('status')==status,'Empty missing observation or acquisition outcome changed')
            offline=case=='cold-offline' or stage in ('offline-empty','offline-query') or case=='warm-offline' and stage=='share-cancelled'
            if not pending:
                route=dict(route='remote' if query==' ' and not offline else 'local',online=not offline,hostPolicy='OrdinaryClientV2',wire=query[1:] if query.startswith(('@','#')) else query,routeObserved=True,routable=not offline,transportHealth='offline' if offline else 'unknown',realtimeConnected=True)
                require(exact(s.get('route'),route),'Empty source host/actual connection route changed')
            if case in COLD:
                diagnostic={'cold-failure':'observations.names.read.POST','cold-missing':'observations.names.user-prefs','cold-offline':'observations.account-unavailable'}[case]
                require(s.get('ranking') is None and s.get('results') is None and s.get('reads')==[] and s.get('publications')==[] and s.get('failures')==[dict(stage='ranking',failure='search observations unavailable',diagnostic='share_ranking_stage:'+diagnostic),dict(stage='adoption',failure='source retired')],'Cold missing observations fabricated suggestions')
            elif not pending:
                # Selection/text edits retain the completed prefix query and reused observations.
                fresh=stage=='share-open' or case=='initial'
                proofs.append(ranking_proof(q,server,fresh=fresh))
                if query!=' ':
                    require(s.get('reads')==[] and s.get('publications')==[],'Empty/local query dispatched directory search')
                else:
                    req=prev_dir['requests'];res=prev_dir['responses'];expected=[]
                    for r,a in zip(req,res):
                        method=r['method'];expected.append(dict(kind='channels' if method=='channels/search' else 'members',method=method,arguments=None if method=='api.getFlannelHttpUrl' else r['body'],raw=a['raw'],orderedKeys=None))
                    require(exact(s.get('reads'),expected),'Whitespace original remote receipts lost')
                    require(exact(s.get('publications'),[dict(kind=k,ids=[],rows=[],membershipIds=[],pageCount=1,nextMarker=None,applied=True) for k in ('members','channels')]),'Whitespace original empty aggregate publication')
            else:require(s.get('ranking') is None and s.get('reads')==[] and s.get('publications')==[],'Pending source prematurely adopted')
            successful_notice(q,native)
            expected_options=Q.options(q,server)
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle);controls=native['share']['controls'];query_control=controls['query'];node=query_control.get('nodeId');integer(node,'Original empty query node',1)
                query_node=query_node or node;require(node==query_node and query_control.get('value')==query and native['share']['queryRevision']==str(s['revision']),'Empty query node/value/revision replaced')
                require(exact(native['share']['options'],expected_options),'Empty rendered order/selection differs from source')
                require(controls['select']['disabled'] is pending and controls['refresh']['disabled'] is pending and controls['text']['readOnly'] is pending and controls['cancel']['disabled'] is False and query_control['readOnly'] is False and controls['send']['disabled'] is (pending or not q['selected']),'Empty actual readonly/pending/Cancel controls')
                text='Searching conversations\u2026' if pending else Q.FAILURE if case in COLD else 'Some results could not be verified. Refresh destinations to try again.' if denied else None
                status_node=native['share'].get('searchStatus')
                require(status_node is None if text is None else isinstance(status_node,dict) and status_node.get('text')==text and status_node.get('visible') is True,'Empty visible guidance changed')
            else:require(native.get('share') is None and native.get('modal') is None,'Empty Cancel left original modal visible')
            require(exact(ex.get('after'),F.observation(row)),'Empty action/capture changed')
            activation=ex.get('activation')
            if isinstance(activation,dict) and activation.get('kind')=='query':
                require(activation.get('originalNode') is True and activation.get('nodeId')==query_node and activation.get('value')==query and activation.get('start')==activation.get('end')==len(query.encode('utf-16-le'))//2 and exact(activation.get('event'),dict(type='input',isTrusted=False,targetNodeId=query_node)),'Empty actual query input provenance/caret')
                rectangle(activation['rect'],viewport)
            elif activation is not None:S.activation(activation,'select' if stage=='selection-retained' else 'click',viewport,rectangle)
            if stage=='selection-retained':S.activation(ex['extra']['text'],'text',viewport,rectangle)
            if stage=='offline-empty':offline_transition(ex.get('extra',{}).get('transition'),server)
            if 'quiet' in ex:require(ex['quiet'].get('observedMs')==500 and exact(ex['quiet'].get('before'),ex['quiet'].get('after')) and exact(ex['quiet']['after'],F.observation(row)),'Empty automatic side effect after Cancel')
            require(Counter(json.dumps(x,sort_keys=True) for x in q['reads'])==Counter(json.dumps(dict(raw=r['result']['raw']),sort_keys=True) for r in prev_ordinary),'Empty ordinary read receipts discarded/fabricated')
            if stage=='selection-retained':offline_reads=list(prev_ordinary)
            if offline and case=='warm-offline':require(exact(prev_ordinary,offline_reads),'Warm offline query performed posting IO')
            if stage=='share-open':initial_q=q
            if stage=='empty-cleared' and case=='clear':require(any(h.get('query')=='review' for h in s.get('history',[])),'Cleared original query custody missing')
        if stage=='checked':checked=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Empty original PNG custody')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Empty final original observation')
    R.replay(proofs)
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
