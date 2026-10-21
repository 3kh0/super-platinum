"""Independent closed Managers wire/response oracle; no product parser or IO.

Application, original response, receiver settlement and later reads are separate.
The top-level validator owns DOM custody and canonical eligibility observations.
"""
import copy
import json
from workflow_management_delete import (WORKFLOW, TRIGGER, POLICY, EDITOR, INITIAL,
    exact, integer, require, read_fields, read_result as metadata_result)
from workflow_management_unpublish import decoded_form

STAGES = {
 'managers-success': ('about','editor','draft','permissions','unchanged','empty','selected','cancelled','acknowledged','checked','back','reopened'),
 'managers-combined': ('about','editor','draft','named','selected','acknowledged','checked','back','reopened'),
 'managers-add-rejected': ('about','editor','draft','selected','rejected','checked','checked-again'),
 'managers-add-unknown': ('about','editor','draft','selected','unconfirmed','checked','checked-again'),
 'managers-self-success': ('about','editor','draft','selected','confirmation','cancelled-confirmation','departed','settled'),
 'managers-self-rejected-held': ('about','editor','draft','selected','confirmation','departed-held','late-settled'),
 'managers-cold': ('about','editor','draft','missing-offline','retry-held','hydrated','cancelled','back','reopened'),
 'managers-retirement': ('about','editor','draft','selected','held','successor','late-settled'),
 'managers-no-trigger': ('about','editor','draft','permissions','selected','acknowledged','checked','back','reopened'),
}
ADD = 'functions.workflows.collaborators.add'
REMOVE = 'functions.workflows.collaborators.remove'
COPY_METHOD = 'functions.workflows.permissions.copy.set'
RUN_METHOD = 'workflows.triggers.permissions.set'
RESTRICTION_METHOD = 'functions.workflows.getIsRestricted'
DISCOVERY_METHOD = 'api.getFlannelHttpUrl'
OLD, NEW, COLD = 'UMANOLD01', 'UMANNEW01', 'UMANCOLD1'
OWNER = dict(account='synthetic-workflow-managers',epoch=0,team='TWORKFLOW',user='UWORKFLOW')
COPY = dict(ok=True,permission_type='collaborators')
EXTERNAL = {'ok':True,'trippable_by_slack_connected_teams':{
    'hermes_triggers_trippable_by_slack_connected_teams_mcp':'NONE','allowed_teams':['TRUNEXTER'],
    'allowed_teams_with_3p_connectors':[],'restricted_teams':[]}}
RESTRICTION = dict(ok=True,is_restricted=False,restricted_by_connector_steps_only=False)
DISCOVERY = dict(ok=True,url='https://edgeapi.slack.com/cache/TWORKFLOW',ttl_seconds=60)
AVAILABLE = dict(manager_receipt=dict(preserve=[1,'available']))
EXPRESSION = dict(manager_receipt=dict(preserve=[2,'expression']))
QUERY = dict(query='manager',count=24,fuzz=1,enable_workspace_ranking=True,
             filter='NOT deactivated AND NOT guests',include_profile_only_users=True)
INFO_BODY = dict(ids=[COLD],check_interaction=True,include_profile_only_users=True)
TEAM = dict(id='TWORKFLOW',name='Home fixture')
BOT = dict(id='UHOME0001',name='Review requests',real_name='Review requests',is_bot=True,
           profile=dict(display_name='Review requests',real_name='Review requests'))


def person(id, label):
    return dict(id=id,name=label.lower().replace(' ','_'),real_name=label,team_id='TWORKFLOW',updated=101,
        deleted=False,is_bot=False,is_restricted=False,is_ultra_restricted=False,is_stranger=False,
        profile=dict(real_name=label,real_name_normalized=label,display_name=label,display_name_normalized=label))


def query_rows():
    rows=[person(NEW,'New Manager')]
    enterprise=person('UMANENTER','Enterprise Manager')
    enterprise.update(team_id='TOTHER',enterprise_user={'teams':['TWORKFLOW']},is_stranger=True)
    rows.append(enterprise)
    local=person('UMANLOCAL','Local Manager');local['is_stranger']=True;rows.append(local)
    for id,label,key in [('UMANBOT01','Bot Manager','is_bot'),('UMANDEAD1','Deleted Manager','deleted'),
        ('UMANGUEST','Guest Manager','is_restricted'),('UMANPROF1','Profile Manager','is_profile_only_user'),
        ('UMANFLOW1','Workflow Manager','is_workflow_bot')]:
        row=person(id,label);row[key]=True;rows.append(row)
    remote=person('UMANAWAY1','Remote Manager');remote['team_id']='TOTHER';rows.append(remote)
    rows.append(person('USLACKBOT','Slackbot Manager'))
    return rows


def self_remove(case):
    return case in ('managers-self-success','managers-self-rejected-held')


def copy_changed(case):
    return case in ('managers-combined','managers-add-rejected','managers-add-unknown','managers-self-rejected-held')


def run_changed(case):
    return case in ('managers-combined','managers-self-rejected-held')


def desired_ids(case):
    return [OLD,NEW] if self_remove(case) else ['UWORKFLOW',NEW]


def selected(case):
    return ['UWORKFLOW',COLD] if case=='managers-cold' else desired_ids(case)


def desired_run(case):
    return dict(ok=True,permission_type='named_entities' if case=='managers-combined' else 'everyone',
                user_ids=['UWORKFLOW',NEW,OLD] if case=='managers-combined' else [],
                channel_ids=[],team_ids=[],org_ids=[])


def original(case):
    require(case in STAGES,'Unknown manager case')
    workflow=copy.deepcopy(WORKFLOW)
    workflow['collaborators']=['UWORKFLOW',COLD if case=='managers-cold' else OLD]
    run=(None if case=='managers-no-trigger' else
         dict(ok=True,permission_type='named_entities',user_ids=['UWORKFLOW',OLD],channel_ids=[],team_ids=[],org_ids=[])
         if case=='managers-combined' else dict(ok=True,permission_type='app_collaborators'))
    return dict(workflow=workflow,trigger=None if case=='managers-no-trigger' else copy.deepcopy(TRIGGER),
                copy=copy.deepcopy(COPY),run=run,external=copy.deepcopy(EXTERNAL),restriction=copy.deepcopy(RESTRICTION))


def latest(workflow):
    return dict(ok=True,decorated_workflow=copy.deepcopy(workflow),
                available_data=copy.deepcopy(AVAILABLE),expression_data=copy.deepcopy(EXPRESSION))


def stage_state(case, stage):
    require(case in STAGES and stage in STAGES[case],'Unknown manager stage')
    at=STAGES[case].index(stage)
    sent=case!='managers-cold' and stage in ('acknowledged','checked','checked-again','back','reopened',
          'rejected','unconfirmed','departed','settled','departed-held','late-settled','held','successor')
    checked=case!='managers-cold' and stage in ('checked','checked-again','back','reopened')
    editor=[m for m in EDITOR if case!='managers-no-trigger' or m not in
            ('workflows.triggers.info','workflows.triggers.permissions.list')]
    # A workflow with no trigger cannot be resolved through an invented app
    # trigger. The actual Managed list supplies its original workflow row.
    initial=(POLICY+['functions.workflows.list','workflows.triggers.list']+POLICY+editor
             if case=='managers-no-trigger' else POLICY+['workflows.triggers.list']+editor)
    reads=[] if stage=='about' else list(initial)
    if sent: reads+=POLICY+editor
    if checked: reads+=POLICY+editor
    if stage=='checked-again': reads+=POLICY+editor
    if stage=='reopened': reads+=initial
    if case=='managers-cold':
        kinds=[] if at<3 else ['Discovery','MemberInfo']
        if at>=4:kinds+=['Discovery','MemberInfo']
    else:
        kinds=[] if at<STAGES[case].index('selected') else ['Discovery','Members']
        if case=='managers-success' and sent:kinds+=['Discovery','Members']
    methods=[]
    if sent:
        methods=[ADD]
        if case not in ('managers-add-rejected','managers-add-unknown','managers-self-rejected-held','managers-retirement'):
            methods.append(REMOVE)
        if copy_changed(case):methods.append(COPY_METHOD)
        if run_changed(case):methods.append(RUN_METHOD)
    follow=sent and run_changed(case) and stage!='departed-held'
    held_kind=('picker' if stage=='retry-held' else 'run' if stage=='departed-held' else
               'add' if case=='managers-retirement' and stage in ('held','successor') else None)
    released=(case=='managers-cold' and at>=5 or
              case in ('managers-retirement','managers-self-rejected-held') and stage=='late-settled')
    departed=self_remove(case) and sent
    complete=sent and case in ('managers-success','managers-combined','managers-self-success','managers-no-trigger')
    return dict(reads=reads,initial=initial,preflight=POLICY+editor,kinds=kinds,methods=methods,
                sent=sent,checked=checked,follow=follow,held_kind=held_kind,released=released,
                retired=case=='managers-retirement' and stage in ('successor','late-settled'),
                held=held_kind is not None,departed=departed,finished=sent and held_kind is None,
                complete=complete,resolved=complete and checked,
                copy_changed=copy_changed(case),run_changed=run_changed(case),writes=len(methods))


def wire_fields(case, method):
    if method in (ADD,REMOVE):
        args=dict(workflow_id='WF_REVIEW',collaborator_ids=NEW if method==ADD else 'UWORKFLOW' if self_remove(case) else OLD)
        reason='add-hermes-workflow-collaborators' if method==ADD else 'remove-hermes-workflow-collaborators'
    elif method==COPY_METHOD:
        args=dict(workflow_id='WF_REVIEW',permission_type='everyone');reason='set-copy-permission'
    elif method==RUN_METHOD:
        args=dict(trigger_id='TR_REVIEW',permission_type='named_entities' if case=='managers-combined' else 'everyone')
        if case=='managers-combined':args['user_ids']=','.join(['UWORKFLOW',NEW,OLD])
        reason='set-trigger-run-permission'
    else:
        require(method==RESTRICTION_METHOD,'Foreign mutation/followup endpoint')
        args=dict(workflow_id='WF_REVIEW');reason='set-trigger-run-permission'
    return [[k,v] for k,v in sorted(args.items())]+[['_x_reason',reason]]


def arguments(case, method):
    args=dict(wire_fields(case,method));args.pop('_x_reason')
    for key in ('collaborator_ids','user_ids'):
        if key in args:args[key]=args[key].split(',')
    return args


def response_for(case, method):
    current=original(case)
    if method==REMOVE:apply(current,case,ADD)
    apply(current,case,method)
    return mutation_result(case,method,current)


def mutation_result(case, method, current):
    if method==ADD and case=='managers-add-unknown':return dict(raw=None,error='transport')
    if method==ADD and case in ('managers-add-rejected','managers-self-rejected-held'):
        return dict(raw=dict(ok=False,error='not_allowed'),error=None)
    if method in (ADD,REMOVE):
        raw=latest(current['workflow']);raw['opaque_receipt']=dict(preserve=[3,'manager'])
    elif method==RUN_METHOD:
        raw=desired_run(case);raw['opaque_receipt']=dict(preserve=[1,'run'])
    elif method==COPY_METHOD:
        raw=dict(ok=True,permission_type='everyone',opaque_receipt=dict(preserve=[1,'copy']))
    else:raw=copy.deepcopy(RESTRICTION)
    return dict(raw=raw,error=None)


def read_result(method, current):
    if method=='functions.workflows.getLatest':return dict(raw=latest(current['workflow']),error=None)
    if method=='functions.workflows.list':
        return dict(raw=dict(ok=True,workflows=[copy.deepcopy(current['workflow'])],
                             response_metadata=dict(next_cursor='')),error=None)
    if method=='workflows.triggers.list':
        return dict(raw=dict(ok=True,triggers=[] if current['trigger'] is None else [copy.deepcopy(current['trigger'])]),error=None)
    field={'functions.workflows.permissions.copy.list':'copy','workflows.triggers.permissions.list':'run',
           'workflows.triggers.prefs.get':'external',RESTRICTION_METHOD:'restriction'}.get(method)
    if field:return dict(raw=copy.deepcopy(current[field]),error=None)
    return metadata_result(method)


def apply(current, case, method):
    applied=method!=RESTRICTION_METHOD and not(method==ADD and case in ('managers-add-rejected','managers-self-rejected-held'))
    if applied:
        if method==ADD:current['workflow']['collaborators'].append(NEW)
        elif method==REMOVE:current['workflow']['collaborators'].remove('UWORKFLOW' if self_remove(case) else OLD)
        elif method==COPY_METHOD:current['copy']['permission_type']='everyone'
        elif method==RUN_METHOD:current['run']=desired_run(case)
    return applied


def seed_proof(seed, case):
    require(isinstance(seed,dict) and set(seed)=={'boot','init','canonicalTeams'},'Missing genuine original seed')
    boot,init,teams=(seed.get(k,{}) for k in ('boot','init','canonicalTeams'))
    require(boot.get('ok') is True and init.get('ok') is True and exact(boot.get('team'),TEAM)
            and exact(init.get('team'),TEAM) and exact(init.get('workspaces'),[TEAM])
            and exact(teams,{'TWORKFLOW':TEAM}),'Original team producer changed')
    require(exact(boot.get('self'),init.get('self')) and boot['self'].get('id')=='UWORKFLOW'
            and boot['self'].get('team_id')=='TWORKFLOW','Original self producer changed')
    expected=[boot['self'],BOT]+([] if case=='managers-cold' else [person(OLD,'Original Manager')])
    require(exact(boot.get('users'),expected),'Seed injected missing/search member or changed original row')
    require(exact(boot.get('prefs'),{}) and exact(init.get('onboarding'),{'user_data':{'onboarding_complete':True}})
            and init.get('user_date_created')==123,'Original initialization changed')
    channels=boot.get('channels')
    require(isinstance(channels,list) and len(channels)==1 and channels[0].get('user')=='UHOME0001',
            'Original app channel seed changed')
    require(COLD not in json.dumps(seed) and NEW not in json.dumps(seed),'Missing manager was preseeded')


def picker_pair(request, response, kind, ordinal, case):
    if kind=='Discovery':
        fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
        require(request.get('phase')=='read' and request.get('method')==DISCOVERY_METHOD
                and exact(request.get('fields'),fields) and exact(request.get('args'),dict(fields)),
                'Wrong manager discovery request')
        raw,error=DISCOVERY,None
    else:
        cold=kind=='MemberInfo'
        require(request.get('phase')=='picker' and request.get('kind')==kind
                and request.get('method')==('users/info' if cold else 'users/search')
                and exact(request.get('body'),INFO_BODY if cold else QUERY)
                and exact(request.get('query'),None if cold else QUERY),'Wrong canonical manager acquisition')
        raw=(dict(ok=True,results=[person(COLD,'Cold Manager')],can_interact={COLD:True},next_marker='')
             if cold else dict(ok=True,results=query_rows(),next_marker=''))
        error=None
        if case=='managers-cold' and ordinal==1:raw,error=None,'offline'
    require(exact(response,dict(serial=request['serial'],result=dict(raw=raw,error=error))),
            'Original canonical manager response changed')


def server_proof(server, case, stage):
    state=stage_state(case,stage)
    require(isinstance(server,dict),'Missing closed Managers server')
    require(exact(server.get('owner'),OWNER) and exact(server.get('original'),original(case)),
            'Wrong original owner/settings')
    require(server.get('ownerCurrent') is (not state['retired']),'Original owner currentness changed')
    require(type(server.get('networkCalls')) is int and server['networkCalls']==0,'Unexpected live authority')
    seed_proof(server.get('seed'),case)
    requests,responses,settlements,releases=(server.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(x,list) for x in (requests,responses,settlements,releases)),'Missing raw ledgers')
    serials=list(range(1,len(requests)+1))
    require(exact([r.get('serial') for r in requests],serials)
            and exact([r.get('serial') for r in responses],serials),'Original dispatch/response serials changed')
    regular=[r for r in requests if r.get('phase')=='read' and r.get('method')!=DISCOVERY_METHOD]
    mutations=[r for r in requests if r.get('phase')=='mutation']
    follows=[r for r in requests if r.get('phase')=='followup']
    picker=[r for r in requests if r.get('phase')=='picker' or r.get('method')==DISCOVERY_METHOD]
    require([r.get('method') for r in regular]==state['reads'],'Editor/preflight/readback count or order changed')
    require(sorted(r.get('method','') for r in mutations)==sorted(state['methods']),'Missing/replayed/unsolicited manager group')
    require([r.get('method') for r in follows]==([RESTRICTION_METHOD] if state['follow'] else []),'Wrong restriction followup')
    require([r.get('kind') if r.get('phase')=='picker' else 'Discovery' for r in picker]==state['kinds'],
            'Missing/repeated or wrong-kind canonical acquisition')
    initial_count=len(state['initial'])
    if picker:
        require(picker[0]['serial']==initial_count+1 and [r['serial'] for r in picker]==list(range(initial_count+1,initial_count+len(picker)+1)),
                'Canonical acquisition escaped original editor or overlapped mutation')
    if mutations:
        first=min(r['serial'] for r in mutations);last=max(r['serial'] for r in mutations+follows)
        require([r['method'] for r in regular if r['serial']<first]==state['initial']+state['preflight'],
                'Mutation escaped fresh common preflight')
        require(not any(first<r['serial']<last for r in regular),'Readback overlapped unsettled groups')
        remove=next((r for r in mutations if r['method']==REMOVE),None)
        add=next(r for r in mutations if r['method']==ADD)
        require(remove is None or remove['serial']>add['serial'],'Remove preceded original add acknowledgement')
        if follows:
            run=next(r for r in mutations if r['method']==RUN_METHOD)
            require(follows[0]['serial']>run['serial'],'Restriction preceded run ACK')
        authority=mutations[0]['owner']
        require(all(exact(r.get('owner'),authority) for r in mutations+follows),'Independent groups lost original attempt owner')
    current=original(case);picker_index=0
    for request,response in zip(requests,responses):
        witness=request.get('owner',{})
        require(set(witness)==set(OWNER)|{'visit','document','revision'}
                and exact({k:witness.get(k) for k in OWNER},OWNER),'Foreign original dispatch owner')
        integer(witness.get('visit'),'Missing original visit',1)
        require(request.get('retrySafe') is False and type(request.get('applied')) is bool,'Unexpected retry/application authority')
        method,phase=request.get('method'),request.get('phase')
        if phase=='picker' or method==DISCOVERY_METHOD:
            require(request['applied'] is False,'Picker acquired mutation authority')
            picker_pair(request,response,state['kinds'][picker_index],picker_index,case);picker_index+=1
            continue
        require(exact(request.get('args'),decoded_form(request.get('fields'))),'Actual wire/args mismatch')
        if phase in ('mutation','followup'):
            integer(witness.get('document'),'Missing original mutation document',1)
            integer(witness.get('revision'),'Missing original mutation revision',1)
            require(exact(request['fields'],wire_fields(case,method)),'Exact collaborator/settings wire changed')
            require(request['applied'] is apply(current,case,method),'Application confused with original response')
            result=mutation_result(case,method,current)
        else:
            require(phase=='read' and request['applied'] is False,'Unrequested operation')
            if method=='workflows.triggers.list' and request['args'].get('_x_reason')=='automations_gallery':
                require(case=='managers-no-trigger' and exact(request['fields'],
                    [['app_ids','A_HOME'],['_x_reason','automations_gallery']]),'Wrong Managed trigger join')
                require(request['serial']>1 and requests[request['serial']-2]['method']=='functions.workflows.list',
                        'Managed trigger join lost original workflow page')
            else:read_fields(method,request['args'])
            result=read_result(method,current)
        require(exact(response,dict(serial=request['serial'],result=result)),'Original opaque response changed')
    require(exact(server.get('current'),current),'Server applied unrelated changes or lost original fields')
    held_request=None
    if state['held_kind']=='picker':held_request=picker[-1]
    elif state['held_kind'] in ('run','add'):
        held_request=next(r for r in mutations if r['method']==(RUN_METHOD if state['held_kind']=='run' else ADD))
    held=held_request['serial'] if held_request else None
    require(exact(server.get('held'),held),'Wrong original held response')
    released_request=None
    if state['released']:
        if case=='managers-cold':released_request=picker[-1]
        else:released_request=next(r for r in mutations if r['method']==(ADD if case=='managers-retirement' else RUN_METHOD))
    released=released_request['serial'] if released_request else None
    order=[n for n in serials if n!=held and n!=released]
    if released is not None:
        # The original held receive completes after the independent already
        # settled groups, before its own followup and any later editor reopen.
        boundary=(follows[0]['serial'] if follows else
                  next((r['serial'] for r in regular if r['serial']>released),len(requests)+1))
        index=next((i for i,n in enumerate(order) if n>=boundary),len(order));order.insert(index,released)
    require(exact(settlements,[dict(serial=n,returned=True) for n in order]),'Original receiver settlement order changed')
    require(type(server.get('completed')) is int and server['completed']==len(order)
            and type(server.get('cancelled')) is int and server['cancelled']==0,'Wrong actual completion/cancellation counts')
    expected_release=[] if released is None else [dict(serial=released,receiverRetired=False,delivered=True,response=responses[released-1])]
    require(exact(releases,expected_release),'Original held response release changed/replayed')
    return dict(owner=copy.deepcopy(OWNER),reads=len(requests)-len(mutations),writes=len(mutations),
                mutations=mutations,followups=follows,picker=picker,state=state)
