"""Closed startup acquisition before first Share; no synthetic complete Store."""
from pathlib import Path
import copy
import uuid
import workflow_management_share_search as Q
import workflow_management_share_empty as E
from workflow_management_delete import require, exact, integer
P,S,A,F,D,R=Q.P,Q.S,Q.A,Q.F,Q.D,E.R
CASES=Q.STARTUP_CASES
PRE=('startup-before','startup-offline','startup-retired')
HELD=('success','prefs-retired','socket-retired')


def complete_reads(server):
    return [dict(method='POST',raw=R.observation_reply(m,'cold-mixed',server['seed']['boot']['self'])) for m in ('client.gantryBoot','users.prefs.get','experiments.getByUser')]


def producer(case,stage,row,previous=None):
    server=row['proof']['server'];parent=server['confirmationAdd']['share'];v=parent.get('startup',{})
    require(v.get('case')==case and v.get('networkCalls')==0 and v.get('storeInjection') is False,'Startup original closed producer missing')
    req,res=v.get('requests'),v.get('responses');settled=v.get('settlements')
    require(isinstance(req,list) and isinstance(res,list) and len(req)==len(res)<=3 and isinstance(settled,list),'Startup original read inventory')
    post=stage not in A.PREFIX+('startup-before',)
    n=3 if post and case=='success' else 2
    require(len(req)==n if stage not in A.PREFIX else len(req)<=2,'Startup duplicate/missing complete acquisition')
    methods=['client.gantryBoot','users.prefs.get','experiments.getByUser'][:len(req)]
    gen=parent['connectivity']['generation']
    for i,(r,response,method) in enumerate(zip(req,res,methods),1):
        body=R.READ_BODIES[method] if method!='experiments.getByUser' else [['ssb_platform','windows'],['ssb_release_channel','prod']]
        require(exact(r,dict(serial=i,method=method,body=body,endpoint='https://example.test/api/'+method,retrySafe=False,owner=S.OWNER,rtGeneration=gen,shareMounted=False)),'Startup read factory/owner or pre-Share origin changed')
        raw=R.observation_reply(method,'cold-mixed',server['seed']['boot']['self'])
        if i==2 and case=='missing':raw=dict(ok=True,opaque_receipt='original-startup-missing-prefs')
        expected=dict(serial=i,error='offline') if i==2 and case=='failure' else dict(serial=i,raw=raw)
        require(exact(response,expected),'Startup original raw response changed')
    pending=case in HELD and not post and len(req)==2
    require(v.get('held')==([2] if pending else []),'Startup held identity changed')
    require(v.get('releases')==([dict(serial=2,receiverRetired=False,delivered=True)] if post and case in HELD else []),'Startup original release disposition changed')
    expected=[dict(serial=i,returned=True) for i in range(1,len(req)+1) if not(i==2 and pending)]
    require(exact(settled,expected),'Startup original producer settlement/replay changed')
    if previous is not None:
        for key in ('requests','responses','settlements','releases'):
            require(exact(v[key][:len(previous[key])],previous[key]),'Startup immutable ledger rewritten')
    ranking=parent['search']['ranking']
    try: ns=uuid.UUID(ranking['navigationOwner'])
    except (KeyError,ValueError,TypeError,AttributeError):ns=None
    require(ns is not None and ns.version==4 and str(ns)==ranking['navigationOwner'],'Startup actual navigation storage identity absent')
    for key in ('requests','responses','settlements'):require(ranking.get(key)==[],'Offline first Share dispatched extra observation/cache reads')
    require(ranking.get('networkCalls')==0 and ranking.get('cacheInjection') is False and ranking.get('canonicalWrites')==0,'Startup cache/source injection')
    search=parent['search']
    require(search.get('case')=='startup' and search.get('initialPeople')==75 and search.get('networkCalls')==0,'Startup original candidate source changed')
    for key in ('requests','responses','settlements','releases','held','postingSources'):require(search.get(key)==[],'Offline query performed directory/posting acquisition')
    E.ordinary(stage,server)
    require(parent['reads']==[],'First offline Share performed posting acquisition')
    connection=parent['connectivity']
    require(exact(connection.get('owner'),S.OWNER) and connection.get('carrier')=='owned-in-memory-realtime-channel' and connection.get('receiverClosed') is False and connection.get('networkCalls')==0 and connection.get('httpSuccessClaimed') is False,'Startup fabricated link observation')
    require(exact(connection.get('installed'),dict(routeObserved=True,routable=True,realtimeConnected=True,transportHealth='unknown')),'Startup original connected source changed')
    obs=row['model']['shareCustody'].get('startupObservations')
    require(isinstance(obs,dict) and set(obs)=={'currentComplete','revision','pending','startupAttempted','reads'},'Startup passive Store observation missing')
    integer(obs['revision'],'Startup original revision',0 if stage in A.PREFIX else 1)
    if stage not in A.PREFIX:
        current=post and case=='success'
        require(obs['currentComplete'] is current and obs['pending'] is pending,'Startup incomplete/retired data became current')
        require(exact(obs['reads'],complete_reads(server) if current else None),'Startup complete raw observations fabricated/lost')
        require(obs['startupAttempted'] is (not (post and case in ('prefs-retired','socket-retired'))),'Startup owner reservation did not retire')
    return v


def transition(case,exercise,row,before):
    server=row['proof']['server'];gen=server['confirmationAdd']['share']['connectivity']['generation']
    expected_transition=dict(kind='closed-fixture-connectivity-transition',owner=S.OWNER,generation=gen,currentGeneration=gen+(1 if case=='socket-retired' else 0),before=dict(routable=True,routeObserved=True,realtimeConnected=True),after=dict(routable=False,routeObserved=True,realtimeConnected=True,transportHealth='offline'),networkCalls=0,eventFabricated=False)
    require(exact(exercise.get('transition'),None if case=='socket-retired' else expected_transition),'Startup actual link loss changed original/successor generation')
    require(exercise.get('release')== (dict(serial=2,receiverRetired=False,delivered=True) if case in HELD else None),'Startup release evidence absent')
    r=exercise.get('retirement')
    if case not in ('prefs-retired','socket-retired'):
        require(r is None,'Unrequested startup owner change');return
    gen=server['confirmationAdd']['share']['connectivity']['generation']
    require(isinstance(r,dict) and r.get('scenario')==case and r.get('generation')==gen and r.get('storeInjection') is False,'Startup original retirement absent')
    require(exact(r.get('before'),before) and r['before']['pending'] is True and r['after']['pending'] is False and r['after']['currentComplete'] is False and r['after']['reads'] is None,'Startup old pending owner not retired')
    expected=dict(type='pref_change',name='display_real_names_override',value=1) if case=='prefs-retired' else dict(kind='closed-fixture-socket-generation-retirement',before=gen,after=gen+1,networkCalls=0)
    require(exact(r.get('frame'),expected),'Startup event/socket source changed')
    require(r['after']['revision']==before['revision']+(1 if case=='prefs-retired' else 0),'Startup invalidation revision changed')
    online=dict(routable=True,routeObserved=True,realtimeConnected=True,transportHealth='unknown')
    require(exact(r.get('connectionBefore'),online) and exact(r.get('connectionAfter'),online),'Retirement was confounded by prior link loss')
    settled='settledOnline' if case=='socket-retired' else 'settledBeforeOffline'
    require(exact(r.get(settled),dict(observations=r['after'],connection=online)),'Original held reply was not rejected while still online')
    if case=='socket-retired':require('settledBeforeOffline' not in r,'Socket case must not claim offline first Share')


def retired_state(row,before_row,checked):
    """Exact socket-only absence proof; never turns a stale lease into a grant."""
    n=row['proof']['native'];b=before_row['proof']['native'];c=checked['proof']['native']
    binding=before_row['proof']['exercise'].get('retirementBinding')
    require(isinstance(binding,dict) and set(binding)=={'ok','kind','scope','modalOwner','modalEpoch','nodeIds','windowOriginal'},'Original retirement binding missing')
    require(binding['ok'] is True and binding['kind']=='original-startup-retirement-binding' and binding['windowOriginal'] is True and exact(binding['scope'],c['scope']) and binding['modalOwner']==c['modal']['owner'] and binding['modalEpoch']==c['modal']['epoch'],'Foreign management retirement binding')
    roles=('parent','title','description','modal','modalTitle','modalDescription','upload');ids=binding['nodeIds']
    require(isinstance(ids,dict) and set(ids)==set(roles),'Original seven retirement nodes missing')
    for value in ids.values():integer(value,'Original retirement node identity',1)
    require(len(set(ids.values()))==7,'Original retirement identities aliased')
    expected=dict(title=c['fields']['title']['nodeId'],description=c['fields']['description']['nodeId'],modal=c['modal']['nodeId'],modalTitle=c['modal']['fields']['title']['nodeId'],modalDescription=c['modal']['fields']['description']['nodeId'],upload=c['modal']['upload']['nodeId'])
    require(all(ids[k]==v for k,v in expected.items()) and b.get('originalNodes') is True and b.get('modal',{}).get('originalNodes') is True,'Retirement bound replacement management/modal')
    proof=dict(kind='original-startup-management-retirement',originalObservationOk=False,binding=binding,nodes=[dict(role=k,nodeId=ids[k],connected=False) for k in roles],registryOriginal=True,windowOriginal=True,noReplacement=True,shareEntryAbsent=True,actionDispatched=False)
    require(exact(n.get('startupRetirement'),proof),'Retirement fabricated original absence/ownership evidence')
    require(n.get('ok') is True and n.get('present') is False and n.get('originalDetached') is True and n.get('originalNodes') is None and n.get('leasePresent') is False and n.get('scope') is None and n.get('fields') is None and n.get('modal') is None and n.get('share') is None and n.get('noOverflow') is True and n.get('composing') is False and all(n.get(k)==[] for k in ('controls','feedback','outcomes')),'Retired management retained controls or stale grant')
    p=row['proof']['firstPublish'];previous=before_row['proof']['firstPublish']
    require(previous.get('visible') is True and p.get('visible') is False and p.get('visit')==previous.get('visit'),'Socket original management lease not retired')
    attempts=previous['attempts']
    # finish_up::exact requires !finished. This already-completed publication
    # therefore has current=false before socket retirement; visible/native
    # detachment above proves the distinct management lease retirement.
    require(len(attempts)==1 and attempts[0].get('current') is False and attempts[0].get('finished') is True and attempts[0].get('complete') is True and attempts[0].get('closed') is False and attempts[0].get('confirmationShare') is None,'Unexpected pre-Share completed original attempt')
    require(exact(attempts,checked['proof']['firstPublish']['attempts']) and exact(p.get('attempts'),attempts),'Socket retirement rewrote original attempt or created Share')
    old_server=copy.deepcopy(before_row['proof']['server']);current_server=copy.deepcopy(row['proof']['server'])
    for server in (old_server,current_server):server['confirmationAdd']['share'].pop('startup')
    require(exact(old_server,current_server),'Socket retirement changed independent original server source')
    require('activation' not in row['proof']['exercise'],'Socket retired case attempted a stale action')


def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,case):
    import workflow_management_share_drafts as drafts
    mode='startup:'+case;Q.mode(mode,receipt.get('shareCase'),receipt.get('shareDraftCustody'));Q.identity(receipt,mode)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Startup terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==Q.STAGES[mode] and set(receipt['captures'])==set(Q.STAGES[mode]),'Startup exact stage/capture inventory')
    checked=parent_ids=share_ids=query_node=prev=before=before_row=None;proofs=[];offline=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];ex=p['exercise']
        parent_ids=P.common(receipt,row,checked,parent_ids,viewport,rectangle,None,search_case=mode)
        prev=producer(case,stage,row,prev)
        if stage=='startup-before':
            before_row=row
            before=row['model']['shareCustody']['startupObservations']
            require(ex.get('scheduleCalls')==(2 if case in HELD else 0),'Repeated actual startup hooks not exercised')
        if stage=='startup-offline':
            transition(case,ex,row,before);offline=prev
        if stage=='startup-retired':
            require(case=='socket-retired','Unrequested retired management predicate')
            transition(case,ex,row,before);retired_state(row,before_row,checked)
        if stage in PRE:
            require(native.get('share') is None,'Share mounted before startup acquisition')
            q=p['firstPublish']['attempts'][-1]['confirmationShare']
            require(q is None or q.get('active') is False,'Startup acquired through a Share modal')
        if stage not in A.PREFIX:
            require(exact(ex.get('after'),F.observation(row)),'Startup action/capture changed')
            if 'quiet' in ex:
                quiet=ex['quiet'];require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet['after'],F.observation(row)),'Startup unexpected side effect while quiet')
        if stage not in A.PREFIX+PRE:
            require(exact(prev,offline),'Startup producer read after original link loss')
            q=p['firstPublish']['attempts'][-1]['confirmationShare'];s=q['search'];active=stage!='share-cancelled'
            query='Fixture' if stage in ('offline-query','share-cancelled') else ''
            require(q.get('bound') is True and q.get('active') is active and q.get('loading') is False and q.get('selected')=='' and q.get('text')==('' if stage=='share-open' else S.MESSAGE),'First offline modal consumed text/selection/custody')
            require(q.get('operation') is None and q.get('opening') is None and q.get('openingHistory')==[],'Startup suggestions opened/sent')
            require(s.get('query')==query and s.get('reads')==[] and s.get('publications')==[],'Offline query dispatched remote reads')
            route=dict(route='local',online=False,hostPolicy='OrdinaryClientV2',wire=query,routeObserved=True,routable=False,transportHealth='offline',realtimeConnected=True)
            require(exact(s.get('route'),route),'Offline routing fabricated connectivity')
            if case=='success':
                proofs.append(E.ranking_proof(q,server,fresh=False));E.successful_notice(q,native)
                require(s.get('status')=='ready' and len(s.get('results',[]))==24,'First offline complete rank failed')
            else:
                expected=[dict(stage='ranking',failure='search observations unavailable',diagnostic='share_ranking_stage:observations.account-unavailable'),dict(stage='adoption',failure='source retired')]
                require(s.get('ranking') is None and s.get('results') is None and s.get('status')=='failed' and exact(s.get('failures'),expected),'Missing/retired startup invented complete data')
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle);v=native['share'];controls=v['controls'];node=controls['query']['nodeId']
                query_node=query_node or node
                require(node==query_node and controls['query']['value']==query and v['queryRevision']==str(s['revision']) and exact(v['options'],Q.options(q,server)),'Startup original query/options changed')
                require(controls['send']['disabled'] is True and all(controls[k]['disabled'] is False for k in ('select','refresh','cancel')) and controls['text']['readOnly'] is False and controls['query']['readOnly'] is False,'Startup offline controls changed')
                status=v.get('searchStatus');require(status is None if case=='success' else isinstance(status,dict) and status.get('text')==Q.FAILURE and status.get('visible') is True,'Startup missing failure guidance')
            else:require(native.get('share') is None and native.get('modal') is None,'Startup Cancel left modal mounted')
            a=ex.get('activation')
            if stage=='offline-query':
                require(a.get('kind')=='query' and a.get('originalNode') is True and a.get('nodeId')==query_node and a.get('value')=='Fixture' and a.get('start')==a.get('end')==7 and exact(a.get('event'),dict(type='input',isTrusted=False,targetNodeId=query_node)),'Startup actual query input/caret absent');rectangle(a['rect'],viewport)
            else:S.activation(a,'text' if stage=='text-retained' else 'click',viewport,rectangle)
        if stage=='checked':checked=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Startup original PNG path custody')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Startup final original observation changed')
    R.replay(proofs)
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
