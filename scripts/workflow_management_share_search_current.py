"""Current empty-on-mount ABI for the two typed-search regression smokes.

Historical receipts keep the pre-mount-ranking validator. This adapter is selected
only by the original closed connectivity producer added with the new ABI.
"""
import workflow_management_share_search as Q
import workflow_management_share_empty as E
from workflow_management_delete import require, exact, integer


def current(server):
    return 'connectivity' in server.get('confirmationAdd',{}).get('share',{})


def state(case,stage,q,v,server):
    require(case in ('person-success','channel-success'),'Current mount-ranking smoke case not specified')
    s=q.get('search');require(isinstance(s,dict),'Current typed search absent')
    integer(s.get('revision'),'Current search revision',1)
    token_reset=case=='person-success' and 'recipients' in q and stage in ('person-opening','selection-ready','share-settled','destination')
    query='' if stage=='share-open' or token_reset else 'Dana' if case=='person-success' else 'research'
    if token_reset:
        require(s.get('query')=='', 'Person addition did not clear original query')
        require([r.get('id') for r in q['recipients']]==[Q.D.PEER], 'Current person token identity changed')
        history=s.get('history');require(isinstance(history,list) and all(isinstance(h,dict) for h in history),'Person reset history malformed')
        prior=[h for h in history if h.get('query')=='Dana' and h.get('status')=='ready']
        require(len(prior)==1, 'Person reset discarded original completed search')
        expected_pubs=[Q.publication(method,'Dana') for method in ('users/search','channels/search')]
        expected_reads=[]
        for r,response in zip(v['requests'],v['responses']):
            discovery=r['method']=='api.getFlannelHttpUrl'
            expected_reads.append(dict(kind='members' if discovery or r['method']=='users/search' else 'channels',method=r['method'],arguments=None if discovery else r['body'],raw=response['raw'],orderedKeys=response['orderedKeys']))
        require(exact(prior[0]['reads'],expected_reads) and exact(prior[0]['publications'],expected_pubs), 'Person reset changed original ordered acquisition')
        require(s.get('reads')==[] and s.get('publications')==[], 'Person addition reacquired remote query')
        if stage=='person-opening':
            require(s.get('status')=='recipient' and s.get('results')==[] and s.get('ranking') is None and s.get('route') is None and s.get('failures')==[], 'Person opening did not clear original query')
            return None
        require(exact(s.get('route'),dict(route='local',online=True,hostPolicy='OrdinaryClientV2',wire='',routeObserved=True,routable=True,transportHealth='unknown',realtimeConnected=True)), 'Person reset local route changed')
        return E.ranking_proof(q,server,fresh=False,publications=expected_pubs,require_cutoff=False)
    require(s.get('query')==query,'Current typed original query changed')
    if stage=='query-pending':
        require(s.get('status')=='searching' and s.get('ranking') is None and all(s.get(k)==[] for k in ('reads','failures','publications')),'Current pending search adopted partial source')
        return None
    if stage=='share-open':
        require(s.get('reads')==[] and s.get('publications')==[],'Empty mount performed remote directory search')
        pubs=[]
    else:
        reads=[]
        for r,response in zip(v['requests'],v['responses']):
            discovery=r['method']=='api.getFlannelHttpUrl'
            reads.append(dict(kind='members' if discovery or r['method']=='users/search' else 'channels',method=r['method'],arguments=None if discovery else r['body'],raw=response['raw'],orderedKeys=response['orderedKeys']))
        pubs=[Q.publication(method,query) for method in ('users/search','channels/search')]
        require(exact(s.get('reads'),reads) and exact(s.get('publications'),pubs),'Current typed original ordered receipt/publication changed')
    require(exact(s.get('route'),dict(route='local' if stage=='share-open' else 'remote',online=True,hostPolicy='OrdinaryClientV2',wire=query,routeObserved=True,routable=True,transportHealth='unknown',realtimeConnected=True)),'Current typed route/owner observation changed')
    return E.ranking_proof(q,server,fresh=stage=='share-open',publications=pubs,require_cutoff=False)


def ordinary(stage,server,opened=None):
    rows=server['confirmationAdd']['share']['reads']
    require(isinstance(rows,list) and len(rows)<=10,'Current typed ordinary read bound')
    for n,r in enumerate(rows,1):
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Current typed posting request shape')
        id=body[0][1];require(id in Q.A.CHANNELS or id==Q.CHANNEL,'Current typed unobserved destination')
        raw=Q.channel() if id==Q.CHANNEL else Q.A.CHANNELS[id]
        opaque='original-search-channel-posting' if id==Q.CHANNEL else 'original-share-channel'
        require(exact(r,dict(serial=n,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=Q.S.OWNER,result=dict(raw=dict(ok=True,channel=raw,opaque_receipt=opaque)))),'Current typed exact posting read/raw changed')
    if stage in Q.A.PREFIX:require(rows==[],'Premature current Share reads')
    elif opened is None:
        require(len(rows)<=8 and all(r['body'][0][1] in Q.A.CHANNELS for r in rows),'Current mount source/read bound changed')
        for id in Q.A.CHANNELS:
            count=sum(r['body'][0][1]==id for r in rows)
            require(count<=(1 if id in ('CADDARCH01','CADDLEFT01') else 2),'Current mount repeated posting reads')
    else:
        previous=opened['proof']['server']['confirmationAdd']['share']['reads']
        require(exact(rows[:len(previous)],previous) and all(r['body'][0][1]==Q.CHANNEL for r in rows[len(previous):]),'Current typed original mount reads changed')
    info=[r for r in rows if r['body'][0][1]==Q.CHANNEL]
    sources=server['confirmationAdd']['share']['search'].get('postingSources')
    require(isinstance(sources,list) and len(sources)==len(info),'Current typed pre-info witness absent')
    for r,source in zip(info,sources):
        require(set(source)=={'readSerial','canonical','current'} and source['readSerial']==r['serial'] and source['current'] is True,'Current typed source ownership/serial changed')
        raw=source.get('canonical');require(isinstance(raw,dict) and raw.get('id')==Q.CHANNEL and raw.get('name')==Q.LABEL and raw.get('is_member') is True and raw.get('fromAnotherTeam') is False,'Original membership companion did not publish before info')
    return rows
