"""Independent original-byte/dispatch oracle for target prerequisites; no app IO."""
import copy
import json
from workflow_management_delete import exact, integer, require, INITIAL, POLICY, EDITOR, read_fields
from workflow_management_unpublish import decoded_form
from workflow_management_run_access_ledger import (ORIGINAL, COPY_METHOD, RUN_METHOD, RESTRICTION_METHOD,
    RESTRICTION, DISCOVERY, read_result)
from workflow_management_run_access_picker import seed_proof, TEAMS

STAGES = {
 'run-cold-targets': ('about','editor','draft','missing-offline','retry-held','hydrated','selected','acknowledged','checked','back','reopened'),
 'run-everyone-external': ('about','editor','draft','permissions','available-held','selected','acknowledged','checked','back','reopened'),
}
COLD_IDS = ['UMISSING01','CMISSING01','TMISSING01']
PERSON = dict(id='UMISSING01',name='cold_runner',real_name='Cold Runner',team_id='TWORKFLOW',updated=101,
    deleted=False,is_bot=False,is_restricted=False,is_ultra_restricted=False,is_stranger=False,
    profile=dict(real_name='Cold Runner',real_name_normalized='Cold Runner',display_name='Cold Runner',display_name_normalized='Cold Runner'))
CHANNEL = dict(id='CMISSING01',name='cold-run-team',is_channel=True,is_private=False,is_archived=False,
    is_member=True,is_im=False,is_mpim=False)
TEAM = dict(id='TMISSING01',name='Cold internal',enterprise_id='ERUNORG01')
RAW_ROWS = [PERSON,CHANNEL,TEAM]
RUN = {
 'run-cold-targets':dict(ok=True,permission_type='named_entities',user_ids=['UMISSING01'],channel_ids=['CMISSING01'],team_ids=['TMISSING01'],org_ids=[]),
 'run-everyone-external':dict(ok=True,permission_type='named_entities',user_ids=['UWORKFLOW'],channel_ids=[],team_ids=['TRUNEXTER'],org_ids=[]),
}
BODY = {
 'MemberInfo':dict(ids=['UMISSING01'],check_interaction=True,include_profile_only_users=True),
 'ChannelInfo':dict(ids=['CMISSING01'],check_membership=True),
 'TeamsById':[['include_unlisted_if_admin','false'],['teams','TMISSING01']],
 'AvailableExternal':dict(count=100,filter='available'),
}
METHOD = dict(MemberInfo='users/info',ChannelInfo='channels/info',TeamsById='team.info',AvailableExternal='externalTeams/list')
PAGES = dict(MemberInfo=dict(ok=True,results=[PERSON],can_interact={'UMISSING01':True},next_marker=''),
             ChannelInfo=dict(ok=True,results=[CHANNEL],member_channels=['CMISSING01'],next_marker=''),
             TeamsById=dict(ok=True,teams=[TEAM]),AvailableExternal=dict(ok=True,results=[TEAMS[2]],next_marker=''))


def original(case):
    value=copy.deepcopy(ORIGINAL);value['run']=copy.deepcopy(RUN[case]);return value


def desired(case):
    return dict(permission_type='everyone' if case=='run-cold-targets' else 'named_entities',
                user_ids=[],channel_ids=[],team_ids=[] if case=='run-cold-targets' else ['TWORKFLOW','TRUNEXTER'],org_ids=[])


def arguments(case):
    return dict(trigger_id='TR_REVIEW',**{k:v for k,v in desired(case).items() if v!=[]})


def ack(case):
    return dict(ok=True,**desired(case),opaque_receipt=dict(preserve=[1,'run']))


def stage_state(case,stage):
    require(case in STAGES and stage in STAGES[case],'Unknown prerequisite stage')
    sent=stage in ('acknowledged','checked','back','reopened');checked=stage in ('checked','back','reopened')
    reads=[] if stage=='about' else list(INITIAL)
    if sent:reads+=POLICY+EDITOR
    if checked:reads+=POLICY+EDITOR
    if stage=='reopened':reads+=INITIAL
    if case=='run-cold-targets':
        kinds=[] if stage in ('about','editor','draft') else ['Discovery','MemberInfo']
        if stage not in ('about','editor','draft','missing-offline'):kinds+=['Discovery','MemberInfo']
        if stage not in ('about','editor','draft','missing-offline','retry-held'):kinds+=['ChannelInfo','TeamsById']
    else:kinds=[] if stage in ('about','editor','draft','permissions') else ['Discovery','AvailableExternal']
    held=stage in ('retry-held','available-held')
    released=len(kinds)==6 if case=='run-cold-targets' else bool(kinds) and not held
    return dict(sent=sent,checked=checked,reads=reads,finished=sent,complete=sent,resolved=checked,
                retired=False,held=False,follow=sent,phase='acknowledged' if sent else None,
                writes=int(sent),kinds=kinds,pickerHeld=held,released=released)


def picker_pair(request,response,kind,index,case):
    require(request.get('applied') is False,'Prerequisite read acquired mutation authority')
    if kind=='Discovery':
        fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
        require(request.get('phase')=='read' and request.get('method')=='api.getFlannelHttpUrl'
                and exact(request.get('fields'),fields) and exact(request.get('args'),dict(fields))
                and request.get('retrySafe') is False,'Wrong actual discovery request')
        raw,error=DISCOVERY,None
    else:
        require(request.get('phase')=='picker' and request.get('kind')==kind and request.get('method')==METHOD[kind]
                and exact(request.get('body'),BODY[kind]) and exact(request.get('query'),BODY[kind])
                and request.get('retrySafe') is (kind=='TeamsById'),'Wrong original identity/available read')
        raw,error=(None,'offline') if case=='run-cold-targets' and index==1 else (PAGES[kind],None)
    require(exact(response,dict(serial=request['serial'],result=dict(raw=raw,error=error))),'Original prerequisite response changed')


def server_proof(server,case,state):
    require(isinstance(server,dict),'Missing closed source server')
    owner=server.get('owner',{})
    require(set(owner)=={'account','epoch','team','user'} and owner.get('account')=='synthetic-workflow-run-access'
            and owner.get('team')=='TWORKFLOW' and owner.get('user')=='UWORKFLOW','Wrong source owner')
    integer(owner.get('epoch'),'Invalid source epoch')
    require(server.get('ownerCurrent') is True and exact(server.get('original'),original(case)),'Original source settings changed')
    require(type(server.get('networkCalls')) is int and server['networkCalls']==0,'Live request authority')
    seed_proof(server.get('seed'))
    encoded=json.dumps(server['seed'],ensure_ascii=False)
    require(all('"'+id+'"' not in encoded for id in COLD_IDS),'Cold grants were preseeded')
    requests,responses,settlements,releases=(server.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(x,list) for x in (requests,responses,settlements,releases)),'Missing raw ledgers')
    serials=list(range(1,len(requests)+1))
    require(exact([r.get('serial') for r in requests],serials) and exact([r.get('serial') for r in responses],serials),'Original serials changed')
    regular=[r for r in requests if r.get('phase')=='read' and r.get('method')!='api.getFlannelHttpUrl']
    require([r.get('method') for r in regular]==state['reads'],'Wrong editor/preflight/readback sequence')
    expected=[('read',m) for m in state['reads'][:12]]
    expected += [('read' if k=='Discovery' else 'picker','api.getFlannelHttpUrl' if k=='Discovery' else METHOD[k]) for k in state['kinds']]
    if state['sent']:
        expected += [('read',m) for m in state['reads'][12:23]]+ [('mutation',RUN_METHOD),('followup',RESTRICTION_METHOD)]
        expected += [('read',m) for m in state['reads'][23:]]
    require([(r.get('phase'),r.get('method')) for r in requests]==expected,'Unrequested, replayed or reordered work')
    current=original(case);pickers=[]
    for request,response in zip(requests,responses):
        witness=request.get('owner',{})
        require(exact({k:witness.get(k) for k in owner},owner),'Request escaped original account')
        integer(witness.get('visit'),'Missing request visit',1)
        method,phase=request['method'],request['phase']
        if phase=='picker' or method=='api.getFlannelHttpUrl':
            index=len(pickers);picker_pair(request,response,state['kinds'][index],index,case);pickers.append(request);continue
        require(request.get('retrySafe') is False and exact(request.get('args'),decoded_form(request.get('fields'))),'Wrong original write/read wire')
        if phase=='mutation':
            fields=[[k,','.join(v) if isinstance(v,list) else v] for k,v in sorted(arguments(case).items())]+[['_x_reason','set-trigger-run-permission']]
            require(exact(request['fields'],fields) and request.get('applied') is True,'Wrong once-only run mutation')
            result=dict(raw=ack(case),error=None);current['run']=dict(ok=True,**desired(case))
        elif phase=='followup':
            require(exact(request['fields'],[['workflow_id','WF_REVIEW'],['_x_reason','set-trigger-run-permission']])
                    and request.get('applied') is False,'Restriction read authority changed')
            result=dict(raw=RESTRICTION,error=None)
        else:
            require(request.get('applied') is False,'Editor read mutated settings');read_fields(method,request['args']);result=read_result(method,current)
        require(exact(response,dict(serial=request['serial'],result=result)),'Raw settings reply changed')
    require(exact(server.get('current'),current),'Unrelated fields changed or desired state fabricated')
    held=pickers[-1]['serial'] if state['pickerHeld'] else None
    require(exact(server.get('held'),held),'Wrong original held response')
    require(exact(settlements,[dict(serial=n,returned=True) for n in serials if n!=held]),'Original receiver settlements changed')
    require(type(server.get('completed')) is int and server['completed']==len(settlements)
            and type(server.get('cancelled')) is int and server['cancelled']==0,'Receiver lifetime differs')
    serial=16 if case=='run-cold-targets' else 14
    expected_release=[dict(serial=serial,receiverRetired=False,delivered=True,response=responses[serial-1])] if state['released'] else []
    require(exact(releases,expected_release),'Missing or forged original sender release')
    return dict(reads=len(requests)-int(state['sent']),writes=int(state['sent']))
