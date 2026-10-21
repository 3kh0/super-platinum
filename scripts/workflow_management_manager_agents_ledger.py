"""Independent manager-agent wire/response oracle; no Rust or fixture imports.

Reuses only the accepted ordinary metadata wire literals. App facts, candidate
rows, acquisition order and original receiver outcomes are independently pinned.
"""
import copy
import workflow_management_managers_ledger as M
from workflow_management_delete import exact, integer, require, read_fields
from workflow_management_unpublish import decoded_form

STAGES = {
 'managers-agents': ('about','editor','draft','permissions','eligible','selected','acknowledged','checked','back','reopened'),
 'managers-agents-recovery': ('about','editor','draft','permissions','facts-failed','retry-held','recovered','cancelled'),
 'managers-agents-query-retirement': ('about','editor','draft','permissions','facts-held','superseded','late-settled','cancelled'),
 'managers-agents-account-retirement': ('about','editor','draft','permissions','facts-held','successor','late-settled'),
}
SUCCESS='managers-agents'
RECOVERY='managers-agents-recovery'
QUERY_RETIRE='managers-agents-query-retirement'
ACCOUNT_RETIRE='managers-agents-account-retirement'
ADD,REMOVE,RUN_METHOD,RESTRICTION_METHOD=M.ADD,M.REMOVE,M.RUN_METHOD,M.RESTRICTION_METHOD
OWNER=M.OWNER
ADDED=['UMANNEW01','UMANAFYES','UMANAFDEF']
MATRIX=[('UMANNEW01','Agent Manager','AMANAGENT',False),
        ('UMANBOT01','Ordinary Bot Manager','AMANBOT',False),
        ('UMANAFYES','Accessible Agent Manager','AMANAFYES',True),
        ('UMANAFNO1','Denied Agent Manager','AMANAFNO1',True),
        ('UMANAFDEF','Default Agent Manager','AMANAFDEF',True),
        ('UMANAFDIS','Disabled Agent Manager','AMANAFDIS',True),
        ('UMANAFBAD','Wrong Bot Agent Manager','AMANAFBAD',True)]
FACTS=[('client.init',None),('client.extras',None),('experiments.getByUser',None),
       ('aiApps.list',None)]+[('apps.profile.get',a) for a in ['AMANAFDEF','AMANAFNO1','AMANAGENT','AMANBOT']]


def original(case):
    require(case in STAGES,'Unknown manager-agent case')
    return M.original('managers-combined' if case==SUCCESS else 'managers-success')


def selected(case):
    require(case in STAGES,'Unknown manager-agent case')
    return ['UWORKFLOW']+ADDED if case==SUCCESS else ['UWORKFLOW',M.OLD]


def desired_run(case):
    require(case in STAGES,'Unknown manager-agent case')
    return dict(ok=True,permission_type='named_entities',user_ids=['UWORKFLOW']+ADDED+[M.OLD],
                channel_ids=[],team_ids=[],org_ids=[]) if case==SUCCESS else original(case)['run']


def query_rows(query='manager'):
    if query=='ordinary':return [M.person('UMANLOCAL','Ordinary Manager')]
    require(query=='manager','Foreign agent query')
    rows=[]
    for id,label,app,agentforce in MATRIX:
        row=M.person(id,label);row['is_bot']=True;row['profile']['api_app_id']=app
        if agentforce:row['is_agentforce_bot']=True
        rows.append(row)
    return rows


def profile(app):
    match=next((r for r in MATRIX if r[2]==app),None)
    require(match is not None,'Foreign app profile')
    raw=dict(id=app,is_agent_app=app!='AMANBOT',bot_user=dict(id='UWRONGBOT' if app=='AMANAFBAD' else match[0]),
             opaque=dict(preserve=[app,'original-profile']))
    if match[3]:raw['agentforce']={}
    return raw


def access(failed=False):
    rows=[profile(a) for a in ['AMANAFYES','AMANAFDIS','AMANAFBAD']]
    if failed:rows[0]['isUnknown']=True
    return dict(ok=True,apps=rows,response_metadata=dict(next_cursor=''),opaque=dict(preserve=['access','original']))


def fact_fields(method,app=None):
    reason='native-calendar-attendee-agents'
    if method=='client.init':return [['include_relevant_onboarding','false'],['_x_reason',reason]]
    if method=='client.extras':return [['extras','["team_prefs"]'],['_x_reason',reason]]
    if method=='experiments.getByUser':return [['ssb_platform','windows'],['ssb_release_channel','prod']]
    if method=='aiApps.list':return [['limit','20'],['_x_reason',reason]]
    require(method=='apps.profile.get' and app in [r[2] for r in MATRIX],'Foreign app fact endpoint')
    return [['app',app],['_x_reason','fetch-all-apps-by-id']]


def fact_result(method,app,seed,failed=False):
    if method=='client.init':raw=dict(ok=True,self=copy.deepcopy(seed['boot']['self']),team=copy.deepcopy(M.TEAM))
    elif method=='client.extras':raw=dict(ok=True,team_prefs=dict(toplevel_prefs={
        'default_channel_knowledge_agent':{'app_id':'AMANAFDEF'},
        'ai_apps_settings':{'apps':[{'app_id':'AMANAFDIS','is_ai_enabled':False}]}}))
    elif method=='experiments.getByUser':raw=dict(ok=True,assignments={})
    elif method=='aiApps.list':raw=access(failed)
    else:
        require(method=='apps.profile.get','Foreign fact result')
        raw=dict(ok=True,app_profile=profile(app))
    return dict(raw=raw,error=None)


def stage_state(case,stage):
    require(case in STAGES and stage in STAGES[case],'Unknown manager-agent stage')
    at=STAGES[case].index(stage)
    initial=M.POLICY+['workflows.triggers.list']+M.EDITOR
    preflight=M.POLICY+M.EDITOR
    sent=case==SUCCESS and at>=6
    checked=case==SUCCESS and at>=7
    reads=[] if at==0 else list(initial)
    if sent:reads+=preflight
    if checked:reads+=preflight
    if stage=='reopened':reads+=initial
    facts=[];queries=[];cancelled_receiver=False;held_serial=None;released=False
    if at>=4:
        queries=['manager']
        facts=list(FACTS if case==SUCCESS else FACTS[:4])
        if case==RECOVERY and at>=5:facts+=FACTS[:4] if at==5 else FACTS
        if case==RECOVERY:
            held_serial=22 if at==5 else None
            released=at>=6
        elif case in (QUERY_RETIRE,ACCOUNT_RETIRE):
            held_serial=18 if at in (4,5) else None
            cancelled_receiver=at>=5
            released=at>=6
            if case==QUERY_RETIRE and at>=5:queries+=['ordinary']
    methods=[ADD,REMOVE,RUN_METHOD] if sent else []
    return dict(reads=reads,initial=initial,preflight=preflight,kinds=['Discovery','Members']*len(queries),
        methods=methods,sent=sent,checked=checked,follow=sent,held_kind='agent-fact' if held_serial else None,
        released=released,retired=case==ACCOUNT_RETIRE and at>=5,held=held_serial is not None,
        departed=False,finished=sent,complete=sent,resolved=checked,copy_changed=False,
        run_changed=case==SUCCESS,writes=len(methods),facts=facts,queries=queries,
        held_serial=held_serial,cancelled_receiver=cancelled_receiver,
        released_serial=(22 if case==RECOVERY else 18) if released else None)


def wire_fields(case,method):
    require(case==SUCCESS,'Unexpected manager-agent write')
    if method in (ADD,REMOVE):
        args=dict(workflow_id='WF_REVIEW',collaborator_ids=','.join(ADDED) if method==ADD else M.OLD)
        reason='add-hermes-workflow-collaborators' if method==ADD else 'remove-hermes-workflow-collaborators'
    elif method==RUN_METHOD:
        args=dict(trigger_id='TR_REVIEW',permission_type='named_entities',user_ids=','.join(desired_run(case)['user_ids']))
        reason='set-trigger-run-permission'
    else:
        require(method==RESTRICTION_METHOD,'Foreign mutation/followup')
        args=dict(workflow_id='WF_REVIEW');reason='set-trigger-run-permission'
    return [[k,v] for k,v in sorted(args.items())]+[['_x_reason',reason]]


def arguments(case,method):
    args=dict(wire_fields(case,method));args.pop('_x_reason')
    for k in ('collaborator_ids','user_ids'):
        if k in args:args[k]=args[k].split(',')
    return args


def apply(current,case,method):
    require(case==SUCCESS,'Mutation outside success case')
    if method==ADD:current['workflow']['collaborators']+=ADDED
    elif method==REMOVE:current['workflow']['collaborators'].remove(M.OLD)
    elif method==RUN_METHOD:current['run']=desired_run(case)
    else:require(method==RESTRICTION_METHOD,'Foreign operation')
    return method!=RESTRICTION_METHOD


def mutation_result(case,method,current):
    if method in (ADD,REMOVE):raw=M.latest(current['workflow']);raw['opaque_receipt']=dict(preserve=[3,'manager'])
    elif method==RUN_METHOD:raw=desired_run(case);raw['opaque_receipt']=dict(preserve=[1,'run'])
    else:raw=copy.deepcopy(M.RESTRICTION)
    return dict(raw=raw,error=None)


def response_for(case,method):
    current=original(case)
    if method==REMOVE:apply(current,case,ADD)
    apply(current,case,method)
    return mutation_result(case,method,current)


def server_proof(server,case,stage):
    state=stage_state(case,stage)
    require(isinstance(server,dict) and exact(server.get('original'),original(case))
            and exact(server.get('owner'),OWNER),'Foreign original agent fixture/owner')
    require(server.get('ownerCurrent') is (not state['retired']),'Agent owner retirement changed')
    require(type(server.get('networkCalls')) is int and server['networkCalls']==0,'Unexpected live authority')
    M.seed_proof(server.get('seed'),'managers-success')
    requests,responses,settlements,releases=(server.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(v,list) for v in (requests,responses,settlements,releases)),'Missing agent ledgers')
    serials=list(range(1,len(requests)+1))
    require(exact([r.get('serial') for r in requests],serials) and exact([r.get('serial') for r in responses],serials),'Dispatch/original response serial changed')
    regular=[r for r in requests if r.get('phase')=='read' and r.get('method')!=M.DISCOVERY_METHOD]
    mutations=[r for r in requests if r.get('phase')=='mutation']
    follows=[r for r in requests if r.get('phase')=='followup']
    picker=[r for r in requests if r.get('phase')=='picker' or r.get('method')==M.DISCOVERY_METHOD]
    facts=[r for r in requests if r.get('phase')=='agent-fact']
    require([r.get('method') for r in regular]==state['reads'],'Editor/preflight/readback count/order changed')
    require(sorted(r.get('method','') for r in mutations)==sorted(state['methods']),'Missing/replayed/unsolicited manager write')
    require([r.get('method') for r in follows]==([RESTRICTION_METHOD] if state['follow'] else []),'Restriction followup changed')
    require([r.get('kind') if r.get('phase')=='picker' else 'Discovery' for r in picker]==state['kinds'],'Canonical picker sequence changed')
    require([(r.get('method'),r.get('args',{}).get('app')) for r in facts]==state['facts'],'App fact order/retry changed')
    # Explicit source transactions: no extra canonical query on fact Retry.
    acquisition=sorted(picker+facts,key=lambda r:r['serial'])
    expected=[]
    if state['queries']:
        expected=[('picker','Discovery'),('picker','Members')]+[('fact',v) for v in state['facts']]
        if len(state['queries'])==2:expected += [('picker','Discovery'),('picker','Members')]
    actual=[('fact',(r['method'],r['args'].get('app'))) if r.get('phase')=='agent-fact' else
            ('picker',r.get('kind') if r.get('phase')=='picker' else 'Discovery') for r in acquisition]
    require(actual==expected and [r['serial'] for r in acquisition]==list(range(13,13+len(expected))),
            'App facts/query escaped original admission or repeated')
    if mutations:
        first=min(r['serial'] for r in mutations);last=max(r['serial'] for r in mutations+follows)
        require([r['method'] for r in regular if r['serial']<first]==state['initial']+state['preflight'], 'Write escaped common fresh preflight')
        require(not any(first<r['serial']<last for r in regular),'Readback preceded group completion')
        by={r['method']:r for r in mutations}
        require(by[ADD]['serial']<by[REMOVE]['serial'],'Remove preceded original add ACK')
        require(follows[0]['serial']>by[RUN_METHOD]['serial'],'Restriction preceded run ACK')
        require(all(exact(r.get('owner'),mutations[0]['owner']) for r in mutations+follows),'Independent groups lost original owner')
    current=original(case);query_index=0;access_index=0
    for req,resp in zip(requests,responses):
        owner=req.get('owner',{})
        require(set(owner)==set(OWNER)|{'visit','document','revision'} and exact({k:owner.get(k) for k in OWNER},OWNER),'Foreign request owner')
        integer(owner.get('visit'),'Missing original visit',1)
        require(req.get('retrySafe') is False and type(req.get('applied')) is bool,'Unexpected retry/application authority')
        method,phase=req.get('method'),req.get('phase')
        if method==M.DISCOVERY_METHOD:
            fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
            require(phase=='read' and exact(req.get('fields'),fields) and exact(req.get('args'),dict(fields)) and req['applied'] is False,'Wrong agent query discovery')
            result=dict(raw=copy.deepcopy(M.DISCOVERY),error=None)
        elif phase=='picker':
            query=state['queries'][query_index];query_index+=1
            body=dict(M.QUERY,query=query)
            require(method=='users/search' and req.get('kind')=='Members' and exact(req.get('body'),body)
                    and exact(req.get('query'),body) and req['applied'] is False,'Wrong agent picker wire')
            result=dict(raw=dict(ok=True,results=query_rows(query),next_marker=''),error=None)
        else:
            require(exact(req.get('args'),decoded_form(req.get('fields'))),'Actual wire/args mismatch')
            if phase=='agent-fact':
                app=req['args'].get('app')
                require(exact(req['fields'],fact_fields(method,app)) and req['applied'] is False,'Wrong exact app fact wire')
                failed=method=='aiApps.list' and case==RECOVERY and access_index==0
                result=fact_result(method,app,server['seed'],failed)
                if method=='aiApps.list':access_index+=1
            elif phase in ('mutation','followup'):
                integer(owner.get('document'),'Missing mutation document',1);integer(owner.get('revision'),'Missing mutation revision',1)
                require(exact(req['fields'],wire_fields(case,method)),'Exact agent-manager mutation wire changed')
                require(req['applied'] is apply(current,case,method),'Application confused with ACK')
                result=mutation_result(case,method,current)
            else:
                require(phase=='read' and req['applied'] is False,'Foreign request phase')
                read_fields(method,req['args']);result=M.read_result(method,current)
        require(exact(resp,dict(serial=req['serial'],result=result)),'Original opaque app/manager response changed')
    require(exact(server.get('current'),current),'Unrequested server change')
    require(exact(server.get('held'),state['held_serial']),'Wrong original held fact')
    cancelled=18 if state['cancelled_receiver'] else None
    pending=state['held_serial'] if cancelled is None else None
    expected_serials=[n for n in serials if n!=pending]
    require(len(settlements)==len(expected_serials) and sorted(r.get('serial',-1) for r in settlements)==expected_serials,'Missing/replayed receiver settlement')
    require(all(exact(r,dict(serial=r['serial'],returned=r['serial']!=cancelled)) for r in settlements),'False delivery or lost cancellation')
    # Query cancellation and the new query may settle in either task order;
    # all synchronous returns retain dispatch order and cancellation follows
    # the original first 17 completed reads. No manufactured release return.
    require([r['serial'] for r in settlements if r['serial']!=cancelled]==[n for n in expected_serials if n!=cancelled],'Return ordering changed')
    if cancelled is not None:require(next(i for i,r in enumerate(settlements) if r['serial']==cancelled)>=17,'Cancellation predates original receiver')
    require(type(server.get('completed')) is int and server['completed']==len(settlements)
            and type(server.get('cancelled')) is int and server['cancelled']==int(cancelled is not None),'Completion/cancellation count changed')
    released=state['released_serial']
    expected_release=[] if released is None else [dict(serial=released,receiverRetired=cancelled is not None,
        delivered=cancelled is None,response=responses[released-1])]
    require(exact(releases,expected_release),'Original held fact release changed/replayed')
    return dict(owner=copy.deepcopy(OWNER),reads=len(requests)-len(mutations),writes=len(mutations),
                mutations=mutations,followups=follows,picker=picker,facts=facts,state=state)
