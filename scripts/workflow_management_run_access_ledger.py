"""Independent run-access wire/application oracle for closed native fixtures.

The original response, application, settlement and readback are separate facts.
This module never imports a product parser or contacts a service.
"""
import copy
from workflow_management_delete import (WORKFLOW, TRIGGER, POLICY, EDITOR, INITIAL,
    exact, integer, require, read_fields, read_result as metadata_result)
from workflow_management_unpublish import decoded_form

STAGES = {
    'run-success': ('about','editor','draft','permissions','unchanged','selected','cancelled',
                    'acknowledged','checked','back','reopened'),
    'run-named-combined': ('about','editor','draft','named','targets','acknowledged','checked','back','reopened'),
    'run-rejected': ('about','editor','draft','selected','rejected','checked','checked-again'),
    'run-unknown': ('about','editor','draft','selected','unconfirmed','checked','checked-again'),
    'run-followup-failure': ('about','editor','draft','selected','followup-failed','checked','checked-again'),
    'run-retirement': ('about','editor','draft','selected','held','successor','late-settled'),
}
COPY_METHOD = 'functions.workflows.permissions.copy.set'
RUN_METHOD = 'workflows.triggers.permissions.set'
RESTRICTION_METHOD = 'functions.workflows.getIsRestricted'
COPY = {'ok':True,'permission_type':'collaborators'}
RUN = {'ok':True,'permission_type':'app_collaborators'}
EXTERNAL = {'ok':True,'trippable_by_slack_connected_teams':{
    'hermes_triggers_trippable_by_slack_connected_teams_mcp':'NONE',
    'allowed_teams':['TRUNEXTER'],'allowed_teams_with_3p_connectors':[],'restricted_teams':[]}}
RESTRICTION = {'ok':True,'is_restricted':False,'restricted_by_connector_steps_only':False}
ORIGINAL = dict(workflow=WORKFLOW,trigger=TRIGGER,copy=COPY,run=RUN,external=EXTERNAL,restriction=RESTRICTION)
DISCOVERY = {'ok':True,'url':'https://edgeapi.slack.com/cache/TWORKFLOW','ttl_seconds':60}


def combined(case):
    return case in ('run-named-combined','run-rejected')


def desired(case):
    return dict(permission_type='named_entities' if case=='run-named-combined' else 'everyone',
                user_ids=['UWORKFLOW','URUNADA01'] if case=='run-named-combined' else [],
                channel_ids=['CRUNTEAM1'] if case=='run-named-combined' else [],
                team_ids=['TRUNINNER','TRUNEXTER'] if case=='run-named-combined' else [],org_ids=[])


def arguments(case, method):
    if method==COPY_METHOD:
        return {'workflow_id':'WF_REVIEW','permission_type':'everyone'}
    if method==RESTRICTION_METHOD:
        return {'workflow_id':'WF_REVIEW'}
    require(method==RUN_METHOD,'Unknown mutation endpoint')
    return dict(trigger_id='TR_REVIEW',**{k:v for k,v in desired(case).items() if v!=[]})


def wire_fields(case, method):
    fields = [[k,','.join(v) if isinstance(v,list) else v] for k,v in sorted(arguments(case,method).items())]
    fields.append(['_x_reason','set-copy-permission' if method==COPY_METHOD else 'set-trigger-run-permission'])
    return fields


def response_for(case, method):
    if method==RUN_METHOD and case=='run-unknown':
        return {'raw':None,'error':'transport'}
    if method==RUN_METHOD and case=='run-rejected':
        return {'raw':{'ok':False,'error':'not_allowed'},'error':None}
    if method==RESTRICTION_METHOD:
        return {'raw':None,'error':'offline'} if case=='run-followup-failure' else {'raw':copy.deepcopy(RESTRICTION),'error':None}
    raw = dict(ok=True,**(desired(case) if method==RUN_METHOD else {'permission_type':'everyone'}),
               opaque_receipt={'preserve':[1,'run' if method==RUN_METHOD else 'copy']})
    return {'raw':raw,'error':None}


def stage_state(case, stage):
    require(case in STAGES and stage in STAGES[case],'Unknown run-access stage')
    sent = stage not in ('about','editor','draft','permissions','unchanged','selected','cancelled','named','targets')
    held = stage in ('held','successor')
    retired = stage in ('successor','late-settled')
    checked = stage in ('checked','checked-again','back','reopened')
    reads = [] if stage=='about' else list(INITIAL)
    if sent: reads += POLICY + EDITOR
    if checked: reads += POLICY + EDITOR
    if stage=='checked-again': reads += POLICY + EDITOR
    if stage=='reopened': reads += INITIAL
    follow = sent and case not in ('run-rejected','run-unknown','run-retirement')
    success = case in ('run-success','run-named-combined')
    phase = ('in-flight' if held else 'rejected' if case=='run-rejected' else
             'unknown' if case=='run-unknown' else 'acknowledged') if sent else None
    return dict(sent=sent,held=held,retired=retired,checked=checked,reads=reads,follow=follow,
                phase=phase,finished=sent and not held,complete=sent and success,
                resolved=checked and success,writes=(1+combined(case)) if sent else 0)


def read_result(method, current):
    field = {'functions.workflows.permissions.copy.list':'copy','workflows.triggers.permissions.list':'run',
             'workflows.triggers.prefs.get':'external','functions.workflows.getIsRestricted':'restriction'}.get(method)
    if field: return {'raw':copy.deepcopy(current[field]),'error':None}
    if method=='api.getFlannelHttpUrl': return {'raw':copy.deepcopy(DISCOVERY),'error':None}
    return metadata_result(method)


def server_proof(server, case, state, *, picker_proof):
    require(isinstance(server,dict),'Missing original closed run-access server')
    owner=server.get('owner',{})
    require(owner.get('account')=='synthetic-workflow-run-access' and owner.get('team')=='TWORKFLOW'
            and owner.get('user')=='UWORKFLOW' and set(owner)=={'account','epoch','team','user'},'Foreign original owner')
    integer(owner.get('epoch'),'Invalid original account epoch')
    require(exact(server.get('original'),ORIGINAL),'Original acquired settings changed')
    require(server.get('ownerCurrent') is (not state['retired']),'Wrong original owner currentness')
    require(type(server.get('networkCalls')) is int and server['networkCalls']==0,'Unexpected network authority')
    requests,responses,settlements,releases=(server.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(v,list) for v in (requests,responses,settlements,releases)),'Missing original dispatch ledgers')
    serials=list(range(1,len(requests)+1))
    require(exact([r.get('serial') for r in requests],serials)
            and exact([r.get('serial') for r in responses],serials),'Missing/reordered original responses')
    mutations=[r for r in requests if r.get('phase')=='mutation']
    expected_methods=([RUN_METHOD,COPY_METHOD] if combined(case) else [RUN_METHOD]) if state['sent'] else []
    require(sorted(r.get('method','') for r in mutations)==sorted(expected_methods),'Changed group missing, replayed or unsolicited')
    follows=[r for r in requests if r.get('phase')=='followup']
    require(len(follows)==int(state['follow']),'Missing/replayed restriction follow-up or follow-up after retirement')
    if follows:
        require(follows[0].get('method')==RESTRICTION_METHOD,'Wrong follow-up endpoint')
        primary=next(r for r in mutations if r['method']==RUN_METHOD)
        require(follows[0]['serial']>primary['serial'],'Restriction read preceded original run ACK')
    regular=[r for r in requests if r.get('phase')=='read' and r.get('method')!='api.getFlannelHttpUrl']
    require([r.get('method') for r in regular]==state['reads'],'Fresh read/preflight order or count changed')
    if mutations:
        first=min(r['serial'] for r in mutations)
        last=max(r['serial'] for r in mutations+follows)
        require([r['method'] for r in regular if r['serial']<first]==INITIAL+POLICY+EDITOR,
                'Mutation escaped common preflight')
        require(not any(first<r['serial']<last for r in regular),'Readback overlapped unsettled groups')
    current=copy.deepcopy(ORIGINAL)
    picker=[]
    for request,response in zip(requests,responses):
        witness=request.get('owner',{})
        require(exact({k:witness.get(k) for k in owner},owner),'Dispatch escaped original account/session')
        integer(witness.get('visit'),'Missing original visit',1)
        require(type(request.get('applied')) is bool and type(request.get('retrySafe')) is bool,'Missing dispatch facts')
        require(request['retrySafe'] is False,'Unexpected automatic retry authority')
        method,phase=request.get('method'),request.get('phase')
        if phase=='picker' or method=='api.getFlannelHttpUrl':
            require(case=='run-named-combined' and not request['applied'],'Unrequested picker acquisition')
            picker.append((request,response))
            continue
        require(exact(request.get('args'),decoded_form(request.get('fields'))),'Actual wire differs from decoded arguments')
        if phase in ('mutation','followup'):
            require(exact(request['fields'],wire_fields(case,method)),'Wrong original settings wire')
            result=response_for(case,method)
            applied=phase=='mutation' and not (method==RUN_METHOD and case=='run-rejected')
            require(request['applied'] is applied,'Server application confused with raw ACK')
            if applied: current['copy' if method==COPY_METHOD else 'run']=dict(ok=True,**({'permission_type':'everyone'} if method==COPY_METHOD else desired(case)))
        else:
            require(phase=='read' and not request['applied'],'Unrequested mutation phase')
            read_fields(method,request['args'])
            result=read_result(method,current)
        require(exact(response,{'serial':request['serial'],'result':result}),'Raw original response changed')
    picker_proof(picker,server.get('seed'))
    require(exact(server.get('current'),current),'Unrelated setting changed or applied result fabricated')
    held=next((r['serial'] for r in mutations if r['method']==RUN_METHOD),None) if state['held'] else None
    require(exact(server.get('held'),held),'Wrong original held response')
    settled=[{'serial':n,'returned':True} for n in serials if n!=held]
    require(exact(settlements,settled),'Missing/cancelled original receiver settlement')
    require(type(server.get('completed')) is int and server['completed']==len(settled)
            and type(server.get('cancelled')) is int and server['cancelled']==0,'Wrong lifetime counts')
    expected=[]
    if case=='run-retirement' and state['finished']:
        serial=next(r['serial'] for r in mutations if r['method']==RUN_METHOD)
        expected=[{'serial':serial,'receiverRetired':False,'delivered':True,'response':responses[serial-1]}]
    require(exact(releases,expected),'Original late release changed or repeated')
    return dict(owner=owner,reads=len(requests)-len(mutations),writes=len(mutations))
