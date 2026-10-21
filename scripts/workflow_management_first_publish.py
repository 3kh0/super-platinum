"""Independent Finish Up wire, original-node, and publication receipt oracle.

Closed fixtures only. A matching read is never an acknowledgement. The optional
Win32 chooser adapter touches only a dialog owned by the admitted fixture PID.
"""
import copy
import json
import time
import hashlib
import workflow_management_avatar_cropper as C
from pathlib import Path
from workflow_management_delete import (WORKFLOW, TRIGGER, TITLE, DESCRIPTION,
    POLICY, EDITOR, INITIAL, exact, require, integer, read_fields, read_result)

LOCAL_TITLE = 'First publication \u4e2d'
LOCAL_DESCRIPTION = 'Keep these first publication details.'
AVATAR_PATH = (Path(__file__).resolve().parents[1] / 'src/desktop/app_home/about/management/fixture/first_publish/avatar.png').resolve()
# Pinned Dioxus native.ts creates a zero-byte DOM File from the IPC descriptor.
# This transport MIME is separate from the actual PNG multipart image below.
DOM_FILE_TYPE = 'text/html; charset=utf-8'


def avatar_path(case=None):
    return C.WIDE_PATH if case==C.WIDE else AVATAR_PATH


def avatar_request_path(value,case=None):
    """Accept slash spelling only for the exact absolute fixture file."""
    if not isinstance(value, str) or not value:
        return False
    try:
        path = Path(value)
        return path.is_absolute() and path.resolve() == avatar_path(case)
    except (OSError, ValueError):
        return False


PREFIX = ('about', 'editor', 'draft', 'finish', 'details')
STAGES = {
 'first-default': ('about','editor','finish','confirmed','checked','done'),
 'first-success': PREFIX + ('confirmed','checked','done'),
 'first-settings': ('about','editor','draft','finish','selected','confirmed','checked','done'),
 'first-metadata-rejected': PREFIX + ('confirmed','checked','done'),
 'first-settings-rejected-held': ('about','editor','draft','finish','selected','published-while-held','confirmed','checked','done'),
 'first-primary-unknown': PREFIX + ('unconfirmed','checked','checked-again'),
 'first-stale-readback': PREFIX + ('confirmed','checked','checked-again'),
 'first-cancel': PREFIX + ('cancelled','reopened-finish','cancelled-again'),
 'first-retirement': PREFIX + ('primary-held','successor','late-settled'),
 'first-avatar': PREFIX + ('uploaded','cropped','confirmed','checked','done'),
 'first-avatar-crop-retry': PREFIX + ('uploaded','crop-rejected','cropped','confirmed','checked','done'),
 'first-avatar-crop-cancel': PREFIX + ('uploaded','crop-rejected','crop-cancelled','cancelled'),
 'first-avatar-upload-back': PREFIX + ('upload-rejected','back-details','cancelled'),
 'first-avatar-upload-retry': PREFIX + ('upload-rejected','retry-details','uploaded','cropped','confirmed','checked','done'),
 'first-avatar-upload-cancel-held': PREFIX + ('upload-held','cancelled','reopened-finish','late-settled'),
 'first-avatar-crop-back': PREFIX + ('uploaded','back-details','cancelled'),
}
STAGES.update(C.STAGES)
PANE_CASES = ('first-avatar-upload-back','first-avatar-upload-retry','first-avatar-upload-cancel-held','first-avatar-crop-back')
AVATAR_CASES = ('first-avatar','first-avatar-crop-retry','first-avatar-crop-cancel') + PANE_CASES + tuple(C.STAGES)
CROP_REJECTION = dict(ok=False,error='invalid_crop',opaque_receipt='original-crop-rejected')
CROP_NOTICE = 'Sorry! Something went wrong. Please try again.'
UPLOAD_REJECTION = dict(ok=False,error='invalid_image',opaque_receipt='original-upload-rejected')


def avatar_case(case):
    return case in AVATAR_CASES


def avatar_count(case,stage):
    if case in C.STAGES: return C.avatar_count(case,stage)
    if not avatar_case(case) or stage in ('about','editor','draft','finish','details'): return 0
    if case in ('first-avatar-upload-back','first-avatar-upload-cancel-held','first-avatar-crop-back'): return 1
    if case=='first-avatar-upload-retry': return 1 if stage in ('upload-rejected','retry-details') else 2 if stage=='uploaded' else 3
    if stage=='uploaded': return 1
    if case=='first-avatar' or case=='first-avatar-crop-cancel' or stage=='crop-rejected': return 2
    return 3


def chooser_count(case):
    return 2 if case=='first-avatar-upload-retry' or case in C.HELD else int(avatar_case(case))


def chooser_request(request,case,fixture,pid,serial):
    require(avatar_case(case) and type(serial) is int and 1<=serial<=chooser_count(case)
        and request.get('serial')==serial and type(request.get('serial')) is int
        and request.get('pid')==pid and type(request.get('pid')) is int
        and request.get('fixture')==fixture and request.get('case')==case
        and request.get('input')=='workflow-finish-upload' and avatar_request_path(request.get('path'),case),
        'Foreign, repeated or unowned original file chooser request')


PUBLISH = 'functions.workflows.publish'
UPDATE = 'functions.workflows.update'
TRIGGER_UPDATE = 'workflows.triggers.update'
COPY = 'functions.workflows.permissions.copy.set'
RUN = 'workflows.triggers.permissions.set'
ADD = 'functions.workflows.collaborators.add'
REMOVE = 'functions.workflows.collaborators.remove'
RESTRICTION = 'functions.workflows.getIsRestricted'
DISCOVERY = 'api.getFlannelHttpUrl'
FACTS = ['apps.certified.list','workflows.triggers.types.list']
OWNER = dict(account='synthetic-workflow-first-publish',epoch=0,team='TWORKFLOW',user='UWORKFLOW')
DISCOVERY_RAW = dict(ok=True,url='https://edgeapi.slack.com/cache/TWORKFLOW',ttl_seconds=60)
CERTIFIED = dict(ok=True,apps=[],is_aaa_enabled=False,can_user_manage_app=True)
TYPES = dict(ok=True,trigger_types=[dict(trigger_type=dict(id='TT_SHORTCUT',type='shortcut'),
    is_available_to_user=True,private_channel_access=False,private_channel_team_ids=[])])


def settings(case):
    return case in ('first-settings','first-settings-rejected-held')


def original(case):
    w,t=copy.deepcopy(WORKFLOW),copy.deepcopy(TRIGGER)
    w.update(id='WfREVIEW01',unpublished_change_count=0,input_parameters={})
    w['steps'][0]['function'].update(type='builtin',input_parameters=[])
    w['steps'][0]['errors']=[]
    if settings(case): w['collaborators'].append('UMANOLD01')
    t['workflow']['workflow_id']='WfREVIEW01'
    t.update(trigger_type_id='TT_SHORTCUT',shortcut_url='https://slack.com/shortcuts/FIRSTFIXTURE')
    return dict(workflow=w,trigger=t,copy=dict(ok=True,permission_type='everyone' if settings(case) else 'collaborators'),
                run=dict(ok=True,permission_type='app_collaborators' if settings(case) else 'everyone'))


def latest(w):
    return dict(ok=True,decorated_workflow=copy.deepcopy(w),available_data={},expression_data={},opaque_receipt='first-publish')


def person(id,label):
    return dict(id=id,name=label.lower().replace(' ','_'),real_name=label,team_id='TWORKFLOW',updated=101,
        deleted=False,is_bot=False,is_restricted=False,is_ultra_restricted=False,is_stranger=False,
        profile=dict(real_name=label,real_name_normalized=label,display_name=label,display_name_normalized=label))


def stage_state(case,stage):
    require(case in STAGES and stage in STAGES[case], 'Unknown first-Publish stage')
    sent=stage in ('confirmed','unconfirmed','checked','checked-again','done','primary-held','successor','late-settled','published-while-held')
    if case=='first-avatar-upload-cancel-held' or case in C.HELD: sent=False
    checked=stage in ('checked','checked-again','done')
    query=settings(case) and stage not in ('about','editor','draft','finish')
    reads=[] if stage=='about' else list(INITIAL)
    if query: reads += [DISCOVERY,'users/search']
    if sent: reads += POLICY+EDITOR+FACTS
    if checked: reads += POLICY+EDITOR
    if stage=='checked-again': reads += POLICY+EDITOR
    writes=0 if not sent else 1 if case=='first-default' else 8 if settings(case) else 3
    if stage=='published-while-held': writes=7
    return dict(sent=sent,checked=checked,reads=reads,writes=writes,
        retired=case=='first-retirement' and stage in ('successor','late-settled'),held=C.held(case,stage) or stage in ('primary-held','successor','published-while-held') or case=='first-avatar-upload-cancel-held' and stage in ('upload-held','cancelled','reopened-finish'))


def mutation_fields(case,method):
    reason='update-hermes-workflow-metadata'
    if method==UPDATE:
        args=dict(workflow_id='WfREVIEW01',title=LOCAL_TITLE,description=LOCAL_DESCRIPTION)
        if avatar_case(case): args['icon_url']='https://example.test/first-publish-crop.png'
    elif method==TRIGGER_UPDATE:
        args=dict(trigger_id='TR_REVIEW',workflow='WfREVIEW01',type='shortcut',inputs='{}',
            name=LOCAL_TITLE,description=LOCAL_DESCRIPTION,shortcut='{"schema":{}}',workflow_app_id='A_HOME')
    elif method==PUBLISH: args=dict(workflow_id='WfREVIEW01');reason='publish-workflow'
    elif method==COPY: args=dict(workflow_id='WfREVIEW01',permission_type='collaborators');reason='set-copy-permission'
    elif method==RUN: args=dict(trigger_id='TR_REVIEW',permission_type='everyone');reason='set-trigger-run-permission'
    elif method==RESTRICTION: args=dict(workflow_id='WfREVIEW01');reason='set-trigger-run-permission'
    elif method in (ADD,REMOVE):
        args=dict(workflow_id='WfREVIEW01',collaborator_ids='UMANNEW01' if method==ADD else 'UMANOLD01')
        reason='add-hermes-workflow-collaborators' if method==ADD else 'remove-hermes-workflow-collaborators'
    else: raise RuntimeError('Unexpected first-Publish mutation')
    return [[k,v] for k,v in sorted(args.items())]+[['_x_reason',reason]]


def mutation_result(case,method,current):
    rejected=(case=='first-metadata-rejected' and method==UPDATE or case=='first-settings-rejected-held' and method==COPY)
    if rejected: return {'raw':dict(ok=False,error='synthetic_rejected',opaque_receipt='rejected-original')}
    if method==UPDATE:
        current['workflow'].update(title=LOCAL_TITLE,description=LOCAL_DESCRIPTION)
        if avatar_case(case): current['workflow']['icons']={'image_192':'https://example.test/first-publish-crop.png'}
        raw=latest(current['workflow'])
    elif method==TRIGGER_UPDATE:
        current['trigger'].update(name=LOCAL_TITLE,description=LOCAL_DESCRIPTION,inputs={})
        raw=dict(ok=True,trigger=copy.deepcopy(current['trigger']),opaque_receipt='trigger-original')
    elif method==COPY:
        current['copy']=dict(ok=True,permission_type='collaborators');raw=copy.deepcopy(current['copy'])
    elif method==RUN:
        current['run']=dict(ok=True,permission_type='everyone',user_ids=[],channel_ids=[],team_ids=[],org_ids=[])
        raw=copy.deepcopy(current['run'])
    elif method==RESTRICTION: raw=read_result(RESTRICTION)['raw']
    elif method in (ADD,REMOVE):
        if method==ADD: current['workflow']['collaborators'].append('UMANNEW01')
        else: current['workflow']['collaborators'].remove('UMANOLD01')
        raw=latest(current['workflow'])
    elif method==PUBLISH:
        current['workflow'].update(is_published=True,last_published_version_id='VFIRST',unpublished_change_count=0,date_updated=2)
        if case=='first-primary-unknown': return {'error':'transport'}
        raw=latest(current['workflow'])
    else: raise RuntimeError('Unexpected mutation')
    return {'raw':raw}


def form(row):
    fields=row.get('fields')
    require(isinstance(fields,list) and all(isinstance(v,list) and len(v)==2 and all(isinstance(x,str) for x in v) for v in fields),'Safe original form missing')
    keys=[v[0] for v in fields]
    require(len(keys)==len(set(keys)) and not {'token','enterprise_token'}&set(keys),'Duplicate/secret form field')
    return dict(fields)


def read_wire(method,args):
    if method in FACTS:
        require(args=={'_x_reason':'fetch-connector-apps' if method==FACTS[0] else 'fetching-workflow-trigger-types-list-wfb'},'Validation request changed')
    elif method==DISCOVERY:
        require(args=={'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'},'Directory discovery body changed')
    else:
        expected=dict(args)
        if expected.get('workflow_id')=='WfREVIEW01': expected['workflow_id']='WF_REVIEW'
        read_fields(method,expected)


def server_proof(case,stage,server):
    spec=stage_state(case,stage)
    require(server.get('networkCalls')==0 and exact(server.get('owner'),OWNER),'Original first-Publish synthetic owner')
    require(server.get('ownerCurrent') is (not spec['retired']),'First-Publish owner retirement')
    require(exact(server.get('original'),original(case)),'Original never-published raw state changed')
    seed=server.get('seed',{});users=seed.get('boot',{}).get('users',[])
    require([u.get('id') for u in users]==['UWORKFLOW','UHOME0001']+(['UMANOLD01'] if settings(case) else []),'Initial canonical member producer changed')
    require(seed.get('boot',{}).get('team')==dict(id='TWORKFLOW',name='Home fixture')
        and seed.get('init',{}).get('team')==seed['boot']['team'] and 'TWORKFLOW' in seed.get('canonicalTeams',{}),'Genuine canonical seed absent')
    requests,responses,settlements,releases=(server.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(v,list) for v in (requests,responses,settlements,releases)),'Missing original first-Publish ledgers')
    serials=list(range(1,len(requests)+1))
    require([v.get('serial') for v in requests]==serials and [v.get('serial') for v in responses]==serials,'Dispatch/response serial custody')
    settled=[v.get('serial') for v in settlements];held=server.get('held')
    require(isinstance(held,list) and len(held)==int(spec['held']) and len(set(settled))==len(settled)
        and all(v.get('returned') is True for v in settlements) and set(settled)==set(serials)-set(held),'Original settlement/held identity')
    current=original(case);mutations=[];reads=[];avatars=[]
    for row,response in zip(requests,responses):
        method,phase=row.get('method'),row.get('phase')
        if phase!='avatar':
            require(exact({k:row.get('owner',{}).get(k) for k in ('account','epoch','team')},{k:OWNER[k] for k in ('account','epoch','team')}),'Dispatch owner changed')
        if phase=='mutation':
            require(row.get('fields')==mutation_fields(case,method),'Exact first-Publish mutation wire')
            require(row.get('reason')==dict(row['fields'])['_x_reason'],'Mutation reason witness')
            require(method not in mutations,'First-Publish replay')
            if method in (COPY,RUN,ADD,REMOVE,RESTRICTION):
                require(settings(case) and UPDATE in mutations and TRIGGER_UPDATE in mutations,'Settings before metadata / unsupported settings')
            if method==REMOVE: require(ADD in mutations,'Remove before acknowledged add')
            if method==RESTRICTION: require(RUN in mutations,'Restriction before run')
            if method==PUBLISH:
                require(reads[-2:]==FACTS or all(k in reads for k in FACTS),'Publication lacks acquired validation')
                if case!='first-default': require(UPDATE in mutations and TRIGGER_UPDATE in mutations,'Publication before original metadata dispatch')
                if case=='first-settings': require(set((COPY,RUN,ADD,REMOVE,RESTRICTION))<=set(mutations),'Normal settings not complete before publication')
                if case=='first-settings-rejected-held': require(COPY in mutations and RUN in mutations and RESTRICTION not in mutations,'Fail-fast publication did not precede held run followup')
            expected=mutation_result(case,method,current)
            require(row.get('applied') is (not (expected.get('raw',{}).get('ok') is False)),'Application confused with reply')
            mutations.append(method)
        elif phase=='picker':
            require(settings(case) and method=='users/search' and row.get('kind')=='Members','Unrequested picker route')
            body=dict(query='manager',count=24,fuzz=1,enable_workspace_ranking=True,filter='NOT deactivated AND NOT guests',include_profile_only_users=True)
            require(row.get('body')==body and row.get('query')==body and row.get('applied') is False and row.get('retrySafe') is False,'Actual picker wire changed')
            expected={'raw':dict(ok=True,results=[person('UMANNEW01','New Manager')],next_marker='')};reads.append(method)
        elif phase=='read':
            read_wire(method,form(row));require(row.get('applied') is False,'Read applied mutation')
            if method==FACTS[0]: raw=CERTIFIED
            elif method==FACTS[1]: raw=TYPES
            elif method==DISCOVERY: raw=DISCOVERY_RAW
            elif method=='functions.workflows.getLatest':
                w=copy.deepcopy(current['workflow'])
                if case=='first-stale-readback' and w['is_published']: w['is_published']=False
                raw=latest(w)
            elif method=='workflows.triggers.list': raw=dict(ok=True,triggers=[copy.deepcopy(current['trigger'])])
            elif method=='workflows.triggers.info': raw=dict(ok=True,trigger=copy.deepcopy(current['trigger']))
            elif method=='functions.workflows.permissions.copy.list': raw=current['copy']
            elif method=='workflows.triggers.permissions.list': raw=current['run']
            else: raw=read_result(method)['raw']
            expected={'raw':raw};reads.append(method)
        elif phase=='avatar':
            require(avatar_case(case),'Unexpected avatar dispatch')
            inspected=row.get('request',{});require(inspected.get('tokenPresent') is True,'Avatar original credential-presence fact')
            pairs=inspected.get('fields',[])
            require(isinstance(pairs,list) and all(isinstance(v,list) and len(v)==2 and all(isinstance(x,str) for x in v) for v in pairs),'Malformed avatar fields')
            fields=dict(pairs);require(len(fields)==len(pairs),'Duplicate avatar fields')
            if method=='avatars.upload':
                image=inspected.get('image',{})
                require((avatars==[] or case=='first-avatar-upload-retry' and avatars==['avatars.upload'] or case in C.HELD and avatars==['avatars.upload','avatars.crop']) and fields=={'_x_reason':'workflows_upload_workflow_avatar'} and image.get('filename')==avatar_path(case).name
                    and image.get('width')==(640 if case==C.WIDE else 256) and image.get('height')==(320 if case==C.WIDE else 256) and image.get('mime')=='image/png','Original selected image request')
                require(image.get('length')==avatar_path(case).stat().st_size,'Original selected image bytes length')
                raw=UPLOAD_REJECTION if case in ('first-avatar-upload-back','first-avatar-upload-retry') and not avatars else dict(ok=True,id='AVATAR_FIRST',url='https://example.test/first-publish-upload.png',opaque_receipt='original-upload')
            else:
                require(method=='avatars.crop' and (avatars==['avatars.upload'] or case in ('first-avatar-crop-retry',C.WIDE) and avatars==['avatars.upload','avatars.crop'] or case=='first-avatar-upload-retry' and avatars==['avatars.upload','avatars.upload']) and fields==dict(avatar='AVATAR_FIRST',cropbox=C.expected_wire(case) if case in C.STAGES else '64,64,128',_x_reason='workflows_crop_workflow_avatar') and inspected.get('image') is None,'Crop original identity/coordinates')
                raw=CROP_REJECTION if case in ('first-avatar-crop-retry','first-avatar-crop-cancel',C.WIDE) and len(avatars)==1 else dict(ok=True,url='https://example.test/first-publish-crop.png',opaque_receipt='original-crop')
            require(row.get('applied') is (raw['ok'] is True),'Avatar application confused with rejection')
            avatars.append(method);expected={'raw':raw}
        else: raise RuntimeError('Unknown fixture request phase')
        require(exact(response.get('result'),expected),'Original raw response drift: '+str(method))
    require(reads==spec['reads'] and len(mutations)==spec['writes'],'Exact read sequence / no replay count')
    require(exact(server.get('current'),current),'Independent server application changed')
    expected_avatars=avatar_count(case,stage)
    require(len(avatars)==expected_avatars,'Avatar dispatch count')
    expected_release=int(case=='first-settings-rejected-held' and stage in ('confirmed','checked','done') or stage=='late-settled')
    require(len(releases)==expected_release,'Original release inventory')
    for release in releases:
        n=release.get('serial');integer(n,'Original release serial',1)
        require(release.get('delivered') is True and release.get('receiverRetired') is False and n in settled,'Original held response delivery')
        require(requests[n-1]['method']==(RUN if case=='first-settings-rejected-held' else 'avatars.upload' if case=='first-avatar-upload-cancel-held' else 'avatars.crop' if case in C.HELD else PUBLISH),'Released foreign response')
    if held: require(requests[held[0]-1]['method']==(RUN if case=='first-settings-rejected-held' else 'avatars.upload' if case=='first-avatar-upload-cancel-held' else 'avatars.crop' if case in C.HELD else PUBLISH),'Held foreign response')
    return spec


def observation(row):
    p=row['proof'];return dict(model=row['model'],firstPublish=p['firstPublish'],server=p['server'],native=p['native'])


def all_slots(attempt):
    slots=list(attempt.get('metadata') or [])
    if attempt.get('copy'): slots.append(attempt['copy'])
    run=attempt.get('run')
    if run:
        slots.append(run['primary']);slots.extend((run.get('followup') or {}).get('slots',[]))
    manager=attempt.get('managers')
    if manager: slots.extend(manager.get('slots',[]))
    slots.append(attempt['publication'])
    for stage in (attempt.get('followup') or {}).get('stages',[]): slots.extend(stage.get('slots',[]))
    return slots


def attempt_proof(case,stage,product,server,spec):
    attempts=product.get('attempts');require(isinstance(attempts,list) and len(attempts)==int(spec['sent']),'Original first attempt count')
    if not attempts: return
    a=attempts[0];integer(a.get('id'),'Attempt identity',1)
    require(a.get('originalTitle')==(original(case)['workflow']['title'] if case=='first-default' else TITLE)
        and a.get('originalDescription')==(original(case)['workflow']['description'] if case=='first-default' else DESCRIPTION),'Parent dirty draft identity')
    require(a.get('validation',{}).get('status')=='ready' and a['validation'].get('certifiedApps')==CERTIFIED and a['validation'].get('triggerTypes')==TYPES,'Actual publication acquired facts')
    require(a['validation'].get('assessment')==dict(blockingErrors=[],hasSteps=True,publishRestricted=False,canBePublished=True,unavailableSteps=[],triggerAvailable=True),'Publication eligibility facts')
    returned={r['serial'] for r in server['settlements'] if r['returned']}
    slots=all_slots(a);sent=[r for r in server['requests'] if r['phase']=='mutation']
    require(len(slots)==len(sent),'Planned operation slot differs from actual dispatch')
    by_method={s['method']:s for s in slots};require(len(by_method)==len(slots),'Duplicate operation slot')
    for r in sent:
        slot=by_method.get(r['method']);require(slot is not None,'Missing actual operation slot')
        args=dict(mutation_fields(case,r['method']));args.pop('_x_reason')
        for key in ('inputs','shortcut'):
            if key in args: args[key]=json.loads(args[key])
        if 'collaborator_ids' in args: args['collaborator_ids']=args['collaborator_ids'].split(',')
        require(exact(slot.get('arguments'),args),'Retained exact operation plan changed')
        result=server['responses'][r['serial']-1]['result'];raw=result.get('raw')
        outcome='in-flight' if r['serial'] not in returned else 'unknown' if 'error' in result else 'rejected' if raw['ok'] is False else 'acknowledged'
        require(slot.get('outcome')==outcome and exact(slot.get('raw'),raw if outcome!='in-flight' else None),'Independent slot/raw acknowledgement')
        require(slot.get('failure')==('delivery unconfirmed' if outcome=='unknown' else None),'Unknown delivery detail')
    require(a.get('finished') is (not spec['held']),'Original async completion')
    complete=case!='first-primary-unknown' and not spec['held']
    require(a.get('complete') is complete,'Publication complete boundary')
    ack=next((server['responses'][r['serial']-1]['result'].get('raw') for r in sent if r['method']==PUBLISH),None)
    readback=a.get('readback') or {}
    matches=bool(ack and exact(ack.get('decorated_workflow'),readback.get('workflow')) and readback.get('trigger',{}).get('id')=='TR_REVIEW' and readback.get('trigger',{}).get('inputs')=={})
    resolved=spec['checked'] and complete and matches
    if case!='first-settings-rejected-held': require(resolved is (spec['checked'] and case not in ('first-primary-unknown','first-stale-readback')), 'Expected first publication matching boundary')
    require(a.get('resolved') is resolved,'Readback manufactured first publication')
    if spec['checked']:
        readback=a.get('readback',{});require(readback.get('matches') is resolved,'Independent readback matching')
        w=copy.deepcopy(server['current']['workflow'])
        if case=='first-stale-readback': w['is_published']=False
        require(exact(readback.get('workflow'),w) and exact(readback.get('trigger'),server['current']['trigger']),'Full first publication readback raw')
    else: require(a.get('readback') is None,'Readback before explicit Check')
    if case=='first-metadata-rejected': require(a.get('continuation')=='metadata-rejected','Metadata failure did not continue to publication')
    elif settings(case): require(a.get('continuation')==('settings-rejected' if case.endswith('held') else 'settings-complete'),'Settings continuation')
    else: require(a.get('continuation')=='settings-complete','Original source continuation')


def expected_pane(stage):
    if stage in ('resized','dragged','crop-held','reentered-crop'): return 'crop'
    return 'upload' if stage in ('upload-held','upload-rejected') else 'crop' if stage in ('uploaded','crop-rejected') else 'details'


def expected_status(stage):
    if stage=='crop-held': return 'saving'
    if stage in ('resized','dragged','reentered-crop'): return 'ready'
    return 'uploading' if stage=='upload-held' else 'error' if stage in ('upload-rejected','crop-rejected') else 'ready' if stage=='uploaded' else 'idle'


def pane_native(case,stage,native,submitted):
    m=native['modal'];pane='crop' if case in C.HELD and stage=='late-settled' else expected_pane(stage);details=pane=='details'
    require(m.get('pane')==pane,'Wrong active avatar pane')
    d=m.get('details',{});active=m.get('activePane',{})
    require(d.get('id')=='workflow-finish-details' and d.get('connected') is True and d.get('displayed') is details and d.get('savedSame') is True,'Original Details pane visibility/custody')
    integer(d.get('nodeId'),'Original Details node',1)
    require(active.get('id')==('workflow-finish-details' if details else 'workflow-finish-'+pane+'-pane')
        and all(active.get(k) is True for k in ('connected','displayed','savedSame')),'Original active pane binding')
    integer(active.get('nodeId'),'Original active pane node',1)
    require(active.get('bindingEpoch')==m.get('epoch'),'Original pane binding epoch')
    hidden=m.get('inactiveControls');require(isinstance(hidden,list),'Missing original hidden controls')
    visible={v['id']:v for v in native['controls'] if v.get('id')}
    require(len(visible)==len([v for v in native['controls'] if v.get('id')]),'Duplicate visible control')
    for v in hidden:
        require(v.get('connected') is True and v.get('displayed') is False and v.get('id') not in visible,'Hidden pane control admitted as visible')
        integer(v.get('nodeId'),'Hidden original node',1)
    for k in ('title','description'):
        f=m['fields'][k];require(f.get('connected') is True and f.get('displayed') is details and f.get('readonly') is (submitted or not details),'Hidden original field state')
    upload=m['upload'];require(upload.get('connected') is True and upload.get('displayed') is details and upload.get('disabled') is (submitted or not details),'Hidden original File control state')
    if details: return
    required={'workflow-finish-title','workflow-finish-description','workflow-finish-upload','workflow-finish-cancel','workflow-finish-submit'}
    require(required<=set(v['id'] for v in hidden) and not required&set(visible),'Hidden Details controls missing or exposed')
    for k in ('title','description'):
        v=next(v for v in hidden if v['id']=='workflow-finish-'+k)
        require(v['nodeId']==m['fields'][k]['nodeId'] and v['value']==m['fields'][k]['value'],'Hidden original detail field replaced')
    require(next(v for v in hidden if v['id']=='workflow-finish-upload')['nodeId']==upload['nodeId'],'Hidden original File replaced')
    if pane=='upload':
        captions={'workflow-finish-upload-back':'Back','workflow-finish-upload-cancel':'Cancel'}
        if stage=='upload-rejected': captions['workflow-finish-upload-retry']='Try a Different Image'
        require(m.get('heading')==('Eek' if stage=='upload-rejected' else 'Uploading your photo\u2026'),'Source Upload heading')
    else:
        captions={'workflow-finish-crop-back':'Back','workflow-finish-crop-apply':'Save','workflow-finish-crop-cancel':'Cancel'}
        require(m.get('heading')=='Crop your photo','Source Crop heading')
    expected=set(captions)|({'workflow-finish-crop-'+k for k in ('x','y','size')} if pane=='crop' else set())
    require(set(visible)==expected,'Active pane exposes foreign or missing controls')
    for key,text in captions.items():require(visible[key]['text']==text and visible[key]['disabled'] is (stage=='crop-held' and key=='workflow-finish-crop-apply'),'Source pane action caption/eligibility')
    notice=CROP_NOTICE if stage in ('crop-rejected','upload-rejected') else 'Uploading your photo\u2026' if stage=='upload-held' else 'Saving your photo\u2026' if stage=='crop-held' else None
    if notice: require(notice in [v.get('text') for v in native.get('feedback',[])],'Active pane guidance hidden')


def confirmation_proof(case,stage,product,native,model):
    editing=stage in ('finish','details','selected','reopened-finish','uploaded','cropped','crop-rejected','crop-cancelled','upload-rejected','upload-held','back-details','retry-details') or case=='first-avatar-upload-cancel-held' and stage=='late-settled' or case in C.STAGES and stage in ('resized','dragged','crop-held','reentered-crop','late-settled')
    c=product.get('confirmation')
    require((c is not None)==editing,'Local confirmation lifetime')
    if not editing: return
    require(c.get('current') is True and c.get('visit')==model.get('visit')
        and c.get('document')==product.get('document',{}).get('id'),'Original modal owner')
    integer(c.get('id'),'Local confirmation identity',1)
    m=native['modal'];require(m.get('owner')==str(c['id']) and m.get('epoch')==str(c['local']['epoch']),'Actual original modal epoch')
    local=c.get('local',{});avatar=local.get('avatar',{})
    require(avatar.get('pane')==('crop' if case in C.HELD and stage=='late-settled' else expected_pane(stage)) and avatar.get('status')==('ready' if case in C.HELD and stage=='late-settled' else expected_status(stage)) and avatar.get('paneBound') is True,'Current bound avatar pane/status')
    require(m.get('pane')==avatar['pane'],'Rendered pane differs from model')
    require(local.get('defaults') is (not settings(case)) and local.get('caption')==('Publish' if settings(case) else 'Next'),'Source Finish Up defaults')
    require(local.get('title')==m['fields']['title']['value'] and local.get('description')==m['fields']['description']['value'],'Native local values differ from original model')
    selected=settings(case) and stage=='selected'
    require(local.get('showMore') is selected and m.get('moreVisible') is selected,'Actual Show more selection')
    require(c.get('copy')==('everyone' if settings(case) and not selected else 'collaborators'),'Embedded copy choice')
    require(c.get('runDraft',{}).get('mode')==('app_collaborators' if settings(case) and not selected else 'everyone'),'Embedded run choice')
    ids=['UWORKFLOW']+(['UMANNEW01'] if selected else ['UMANOLD01'] if settings(case) else [])
    require(c.get('managerDraft',{}).get('ids')==ids and c['managerDraft'].get('pending') is False
        and c['managerDraft'].get('speedBump') is False,'Embedded manager choice or invented standalone speed bump')
    if selected:
        found=[r for r in c['managerDraft'].get('rows',[]) if r.get('id')=='UMANNEW01']
        require(len(found)==1 and found[0].get('label')=='New Manager' and found[0].get('eligible') is True,'Acquired selectable manager lost')
    require(local.get('valid') is (stage not in ('uploaded','crop-rejected','upload-held','upload-rejected','resized','dragged','crop-held','reentered-crop') and not (case in C.HELD and stage=='late-settled')),'Unresolved crop admitted publication')
    if avatar_case(case) and (stage in ('uploaded','cropped','crop-rejected','crop-cancelled','upload-rejected','upload-held','back-details','retry-details','resized','dragged','crop-held','reentered-crop') or case in C.HELD and stage=='late-settled'):
        avatar=local.get('avatar',{});custody=m.get('custody',{});nodes=custody.get('cropNodes')
        require(avatar.get('pending') is (stage in ('upload-held','crop-held')) and custody.get('savedPresent') is True
            and custody.get('savedModalSame') is True and custody.get('savedFinishSame') is True,'Original avatar binding missing')
        require(isinstance(nodes,list) and len(nodes)==3 and [n.get('id') for n in nodes]==['workflow-finish-crop-'+k for k in ('x','y','size')],'Original crop control inventory')
        file=custody.get('file',{});require(file.get('selectedCount')==1 and file.get('sameSaved') is True
            and file.get('selectedIdentity')==file.get('savedIdentity'),'Original selected File replaced')
        integer(file.get('selectedIdentity'),'Original File identity',1)
        require(file.get('name')==str(avatar_path(case)),'Selected File path differs from original chooser request')
        require(m.get('icon')==local.get('iconUrl'),'Rendered acknowledged icon differs')
        if stage in ('uploaded','crop-rejected','resized','dragged','crop-held','reentered-crop') or case in C.HELD and stage=='late-settled':
            require(avatar.get('cropBound') is True and m.get('crop') is True and custody.get('cropSavedPresent') is True
                and custody.get('cropSavedLength')==3,'Uploaded crop not bound')
            binding=custody.get('finishBinding',{});require(binding.get('status')=='ready'
                and str(binding.get('epoch'))==str(local['epoch']),'Original crop binding epoch not ready')
            for n,k in zip(nodes,('x','y','size')):
                integer(n.get('nodeId'),'Original crop node',1)
                require(n.get('savedSame') is True and n.get('connected') is True and n.get('disabled') is False
                    and n.get('badInput') is False and n.get('value')!='','Original crop control unavailable')
                try:value=float(n['value'])
                except (ValueError,TypeError):raise RuntimeError('Invalid raw crop value')
                require(value==avatar.get('crop',{}).get(k),'Original crop value differs from model')
        else:
            require(avatar.get('uploaded') is None and m.get('crop') is False and custody.get('cropSavedPresent') is False
                and custody.get('cropSavedLength') is None and all(n.get('nodeId') is None and n.get('connected') is False for n in nodes),
                'Acknowledged crop subtree retained')
            require(local.get('iconUrl')==('https://example.test/first-publish-crop.png' if stage=='cropped' else ''),'Acknowledged/cancelled crop icon lost')
        if case!='first-avatar':
            require(avatar.get('notice')==(CROP_NOTICE if stage in ('crop-rejected','upload-rejected') else None),'Stale or missing crop guidance')
            if stage=='crop-rejected' and case not in C.STAGES:
                require(exact(avatar.get('crop'),dict(x=64.0,y=64.0,size=128.0)) and exact(avatar.get('uploaded'),dict(id='AVATAR_FIRST',url='https://example.test/first-publish-upload.png',width=256,height=256)) and local.get('iconUrl')=='','Rejected crop changed source or icon')
                require(CROP_NOTICE in [v.get('text') for v in native.get('feedback',[])],'Crop rejection guidance not visible')


def native_proof(case,stage,native,model,viewport,rectangle):
    require(native.get('ok') is True and native.get('noOverflow') is True and native.get('viewport')==viewport,'Original Finish Up geometry')
    controls=native.get('controls');require(isinstance(controls,list) and controls,'Missing actual original controls')
    for item in controls+native.get('feedback',[])+native.get('outcomes',[]):
        require(item.get('visible') is True and item.get('hit') is True,'Occluded original control/feedback')
        rectangle(item.get('rect'),viewport)
        integer(item.get('nodeId'),'Actual retained DOM identity',1)
    require(native.get('composing') is False,'Native composition unsettled')
    about=stage=='about' or case=='first-retirement' and stage in ('successor','late-settled')
    modal=stage not in ('about','editor','draft','done','cancelled','cancelled-again','successor','late-settled') or case=='first-avatar-upload-cancel-held' and stage=='late-settled' or case in C.STAGES and stage in ('resized','dragged','crop-held','reentered-crop','late-settled')
    require((native.get('modal') is not None)==modal and native.get('present') is (not about),'Actual modal/parent route')
    if about:
        require(native.get('fields') is None and native.get('originalNodes') is None and [v['id'] for v in controls]==['workflow-configuration'],'Foreign original editor survived')
        return
    require(native.get('originalNodes') is True,'Parent original native nodes changed')
    parent_title=original(case)['workflow']['title'] if case=='first-default' or stage=='editor' else TITLE
    parent_description=original(case)['workflow']['description'] if case=='first-default' or stage=='editor' else DESCRIPTION
    require(model.get('title')==parent_title and model.get('description')==parent_description,'Parent draft overwritten')
    for key,value in [('title',parent_title),('description',parent_description)]:
        f=native.get('fields',{}).get(key,{});require(f.get('value')==value and f.get('readonly') is (modal or model.get('pending') is True),'Original parent buffer/lock')
    if not modal: return
    m=native['modal'];require(m.get('originalNodes') is True,'Original modal/file input replaced')
    changed=stage not in ('finish','reopened-finish') and case!='first-default' and not (case=='first-avatar-upload-cancel-held' and stage=='late-settled' or case=='first-avatar-crop-cancel-held' and stage in ('reentered-crop','late-settled'))
    for key,value in [('title',LOCAL_TITLE if changed else original(case)['workflow']['title']),('description',LOCAL_DESCRIPTION if changed else original(case)['workflow']['description'])]:
        require(m.get('fields',{}).get(key,{}).get('value')==value,'Exact modal draft lost')
    upload=m.get('upload',{});require(upload.get('type')=='file' and upload.get('accept')=='image/jpeg,image/gif,image/png','Actual native file control')
    if avatar_case(case) and (stage in ('uploaded','cropped','crop-rejected','crop-cancelled','confirmed','checked','upload-rejected','upload-held','back-details','retry-details','resized','dragged','crop-held','reentered-crop') or case in C.HELD and stage=='late-settled'):
        files=upload.get('files');require(isinstance(files,list) and len(files)==1 and files[0].get('name')==str(avatar_path(case)) and files[0].get('type')==DOM_FILE_TYPE and type(files[0].get('size')) is int and files[0]['size']==0,'Actual chooser selection absent')
    else: require(upload.get('files')==[],'Unexpected selected file')
    ids={v['id']:v for v in controls if v.get('id')}
    submitted=stage_state(case,stage)['sent']
    pane='crop' if case in C.HELD and stage=='late-settled' else expected_pane(stage)
    pane_native(case,stage,native,submitted)
    if pane!='details': return
    if submitted:
        require('workflow-finish-submit' not in ids and 'workflow-finish-cancel' not in ids and {'workflow-finish-check','workflow-finish-done'}<=set(ids),'Submitted operation controls')
        busy=stage in ('primary-held','published-while-held');require(ids['workflow-finish-check']['disabled'] is busy and ids['workflow-finish-done']['disabled'] is busy,'Held/settled controls')
    else:
        require(ids.get('workflow-finish-submit',{}).get('text')==('Publish' if settings(case) else 'Next') and 'workflow-finish-cancel' in ids,'Default/nondefault source action caption')
        require(ids['workflow-finish-submit']['disabled'] is False,'Original submit eligibility')


def recovery_proof(case,row,previous):
    """Actual recovery observations; constructed tests never imply native acceptance."""
    if not avatar_case(case): return
    stage=row['stage'];p=row['proof'];product=p['firstPublish'];server=p['server']
    if case in C.STAGES:
        C.avatar_receipts(case,row)
        return
    settled={r['serial'] for r in server['settlements']}
    sent=[r for r in server['requests'] if r['phase']=='avatar' and r['serial'] in settled]
    receipts=product.get('avatarReceipts')
    require(isinstance(receipts,list) and len(receipts)==len(sent),'Original avatar receipt inventory')
    owner=(product.get('confirmation') or (product.get('attempts') or [{}])[0]).get('id')
    generations=[]
    for received,request in zip(receipts,sent):
        integer(received.get('id'),'Avatar original confirmation',1)
        if owner is not None and case!='first-avatar-upload-cancel-held' and case not in C.HELD: require(received['id']==owner,'Avatar reply borrowed another confirmation')
        integer(received.get('generation'),'Avatar generation',1);generations.append(received['generation'])
        require(received.get('failed') is False and exact(received.get('request'),request['request'])
            and exact(received.get('raw'),server['responses'][request['serial']-1]['result']['raw']),'Original avatar raw/request receipt changed')
    require(generations==sorted(set(generations)),'Avatar operation generation replay')
    local=(product.get('confirmation') or (product.get('attempts') or [{}])[0]).get('local')
    if local and sent and not (case=='first-avatar-upload-cancel-held' and stage=='late-settled' or case=='first-avatar-crop-cancel-held' and stage in ('reentered-crop','late-settled')):
        avatar=local['avatar']
        expected=[dict(kind='upload' if r['method']=='avatars.upload' else 'crop',raw=server['responses'][r['serial']-1]['result']['raw']) for r in sent]
        require(exact(avatar.get('receipts'),expected),'Local crop rejection/ACK receipt lost')
        require(avatar.get('notice')==(CROP_NOTICE if stage in ('crop-rejected','upload-rejected') else None),'Stale crop notice after transition')
    if case not in ('first-avatar-crop-retry','first-avatar-crop-cancel') or stage not in ('crop-rejected','cropped','crop-cancelled','cancelled'): return
    ex=p.get('exercise',{});before=ex.get('before');after=ex.get('after')
    require(isinstance(before,dict) and exact(after,observation(row)),'Actual recovery before/after absent')
    require(exact(before['model'],after['model']) if stage!='cancelled' else before['model']['title']==after['model']['title'] and before['model']['description']==after['model']['description'],'Recovery changed parent draft/model')
    if stage=='cancelled':
        require(exact(before,observation(previous)) and exact(before['server'],after['server']),'Whole Cancel replayed or lost prior crop cancellation')
        require(exact(product.get('avatarReceipts'),previous['proof']['firstPublish'].get('avatarReceipts')),'Whole Cancel lost original avatar receipts')
        require(all(exact(before['firstPublish']['document'][k],product['document'][k]) for k in ('id','revision','title','description','editor','trigger','pending')),'Whole Cancel changed original document')
        return
    b=before['firstPublish']['confirmation'];a=after['firstPublish']['confirmation']
    require(all(exact(b[k],a[k]) for k in ('id','visit','document','revision','copy','runDraft','managerDraft')),'Recovery replaced original local owner/settings')
    require(all(b['local'][k]==a['local'][k] for k in ('title','description')),'Recovery changed local draft')
    bm=before['native']['modal'];am=after['native']['modal']
    require(exact(before['native']['fields'],after['native']['fields']),'Recovery changed original parent native buffers')
    require([bm['nodeId'],bm['upload']['nodeId']]==[am['nodeId'],am['upload']['nodeId']]
        and all(bm['fields'][k]['nodeId']==am['fields'][k]['nodeId'] for k in ('title','description')),'Recovery replaced original modal controls')
    require(exact(bm['upload']['files'],am['upload']['files']) and exact(bm['custody']['file'],am['custody']['file']),'Recovery replaced original selected File')
    require(exact(b['local']['avatar']['crop'],dict(x=64.0,y=64.0,size=128.0)),'Explicit crop did not retain original coordinates')
    if stage=='crop-rejected':
        require(exact(b['local']['avatar']['uploaded'],a['local']['avatar']['uploaded']) and b['local']['iconUrl']==a['local']['iconUrl']=='','Rejected crop changed uploaded source/icon')
        require([n['nodeId'] for n in bm['custody']['cropNodes']]==[n['nodeId'] for n in am['custody']['cropNodes']],'Rejected crop replaced original crop controls')
        require(len(after['server']['requests'])==len(before['server']['requests'])+1,'Crop rejection request count')
        quiet=ex.get('quiet',{})
        require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),after),'Crop rejection auto-retried or changed during bounded quiet')
    else:
        require(exact(before,observation(previous)),'Retry/Cancel did not use original rejected observation')
        if stage=='crop-cancelled':
            require(exact(before['server'],after['server']) and b['local']['iconUrl']==a['local']['iconUrl']=='','Crop Cancel dispatched or changed icon')
        else: require(len(after['server']['requests'])==len(before['server']['requests'])+1,'Explicit crop Retry was not one request')


def pane_exercise(case,row,previous):
    if case not in PANE_CASES: return
    stage=row['stage'];p=row['proof'];after=observation(row);ex=p.get('exercise',{})
    if stage in ('about','editor','draft','finish','details','confirmed','checked','done'): return
    before=ex.get('before');require(isinstance(before,dict) and exact(ex.get('after'),after),'Actual pane action before/after absent')
    require(exact(before['model'],after['model']),'Pane action changed original parent model')
    bf,af=before['native']['fields'],after['native']['fields']
    if stage in ('cancelled','reopened-finish'):
        require(all(exact({k:v for k,v in bf[key].items() if k!='readonly'},{k:v for k,v in af[key].items() if k!='readonly'}) and bf[key]['readonly'] is (stage=='cancelled') and af[key]['readonly'] is (stage=='reopened-finish') for key in ('title','description')),'Modal close/reopen lost original parent buffers or lock boundary')
        require(all(exact(before['firstPublish']['document'][k],after['firstPublish']['document'][k]) for k in ('id','revision','title','description','editor','trigger','pending')),'Modal close/reopen changed original document')
    else:require(exact(bf,af),'Pane action changed original parent native buffers')
    if stage in ('upload-held','upload-rejected','uploaded','cropped','back-details','retry-details','resized','dragged','crop-held','reentered-crop','late-settled'):
        b=before['firstPublish']['confirmation'];a=after['firstPublish']['confirmation']
        require(all(exact(b[k],a[k]) for k in ('id','visit','document','revision','copy','runDraft','managerDraft')),'Pane action replaced original modal/settings owner')
        require(all(b['local'][k]==a['local'][k] for k in ('title','description')),'Pane action changed local draft')
        bn=before['native']['modal'];an=after['native']['modal']
        require([bn['nodeId'],bn['upload']['nodeId'],bn['details']['nodeId']]==[an['nodeId'],an['upload']['nodeId'],an['details']['nodeId']]
            and all(bn['fields'][k]['nodeId']==an['fields'][k]['nodeId'] for k in ('title','description')),'Pane transition remounted hidden original controls')
        if stage!='cropped':require(b['local']['iconUrl']==a['local']['iconUrl']=='','Pane action changed icon without crop ACK')
        if stage in ('upload-held','upload-rejected','uploaded'):
            old=bn['custody']['file']['selectedIdentity'];new=an['custody']['file']['selectedIdentity']
            require(type(new) is int and new>0 and old!=new,'Explicit chooser did not supply a distinct original File')
            require(len(after['server']['requests'])==len(before['server']['requests'])+1,'Chooser dispatch inventory')
        else:
            require(exact(bn['custody']['file'],an['custody']['file']) and exact(bn['upload']['files'],an['upload']['files']),'Pane action replaced original selected File')
        if stage in ('back-details','retry-details'):
            expected='workflow-finish-upload-retry' if stage=='retry-details' else 'workflow-finish-crop-back' if case=='first-avatar-crop-back' else 'workflow-finish-upload-back'
            require(ex.get('action')==expected and exact(before['server'],after['server']),'Back/Try Different dispatched work or wrong original action')
            require(a['local']['avatar']['generation']>b['local']['avatar']['generation'] and a['local']['avatar']['notice'] is None and a['local']['avatar']['uploaded'] is None,'Back did not retire original generation')
        if stage=='cropped':
            require(exact(b['local']['avatar']['crop'],dict(x=64.0,y=64.0,size=128.0)) and len(after['server']['requests'])==len(before['server']['requests'])+1,'Crop Save did not use original coordinates/one request')
    if stage in ('upload-rejected','upload-held','late-settled'):
        quiet=ex.get('quiet',{})
        require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),after),'Pane bounded quiet changed or replayed work')
    if stage=='cancelled':
        require(exact(before,observation(previous)) and exact(before['server'],after['server']) and after['firstPublish']['confirmation'] is None,'Whole Cancel dispatched or lost original observation')
        require(exact(before['firstPublish']['avatarReceipts'],after['firstPublish']['avatarReceipts']),'Cancel lost original response evidence')
    if stage=='reopened-finish':
        require(exact(before,observation(previous)) and exact(before['server'],after['server']),'Reopen replayed request')
        require(all(ex.get('remount',{}).get(k) is True for k in ('ok','originalDetached','newRoot','newTitle','newDescription','newUpload')),'Successor modal original remount absent')
    if stage=='late-settled':
        require(exact(before,observation(previous)) and exact(before['native'],after['native']),'Late upload changed successor native controls')
        b=before['firstPublish'];a=after['firstPublish']
        require(exact({k:v for k,v in b.items() if k!='avatarReceipts'},{k:v for k,v in a.items() if k!='avatarReceipts'}),'Late upload changed successor source/local state')
        require(len(a['avatarReceipts'])==len(b['avatarReceipts'])+1 and exact(a['avatarReceipts'][:-1],b['avatarReceipts']),'Late actual raw receipt not retained separately')
        require(a['avatarReceipts'][-1]['id']!=a['confirmation']['id'],'Late upload borrowed successor identity')
        require(exact(before['server']['requests'],after['server']['requests']) and exact(before['server']['current'],after['server']['current']),'Late upload dispatched or changed workflow')
        release=ex.get('release');require(exact(release,after['server']['releases'][-1]),'Original sender release proof changed')


def held_upload_origin(row,held):
    original=held['proof']['firstPublish']['confirmation'];reply=row['proof']['firstPublish']['avatarReceipts'][-1]
    require(reply['id']==original['id'] and reply['generation']==original['local']['avatar']['generation'],
        'Late upload receipt lost original dispatched modal/generation')


def retain_native_failure(evidence,receipts,fixture,case):
    """Preserve an owned failed terminal for reporting, never acceptance."""
    if case not in STAGES or len(receipts)!=1:return False
    receipt=receipts[0]
    if not (isinstance(receipt,dict) and receipt.get('ok') is False and receipt.get('fixture')==fixture
            and receipt.get('scenario') in (None,'editor') and receipt.get('firstPublishCase') in (None,case)
            and isinstance(receipt.get('error'),str) and receipt['error'].strip()):return False
    evidence['native']=copy.deepcopy(receipt)
    return True


def validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,*,png=True):
    # The native harness emits a compact failure before a successful terminal's
    # scenario/case fields exist. Report that error only for this owned fixture;
    # the complete success identity and every acceptance check remain below.
    if case==C.WIDE:
        require(C.WIDE_PATH.stat().st_size==C.WIDE_BYTES and hashlib.sha256(C.WIDE_PATH.read_bytes()).hexdigest()==C.WIDE_SHA256,'Original rectangular fixture image bytes changed')
    if receipt.get('ok') is False:
        require(case in STAGES and receipt.get('fixture')==fixture
            and receipt.get('scenario') in (None,'editor')
            and receipt.get('firstPublishCase') in (None,case), 'First-Publish terminal identity')
        error=receipt.get('error');require(isinstance(error,str) and error.strip(),'First-Publish native failure missing error')
        summary=' '.join(error.split('; avatar native=',1)[0].split())
        if len(summary)>600:summary=summary[:600]+'...'
        raise RuntimeError(f'First-Publish native failure ({case}): {summary}')
    require(case in STAGES and receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('firstPublishCase')==case,'First-Publish terminal identity')
    require(all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Actual synthetic Configuration entry')
    require(receipt.get('viewport')==viewport,'Actual viewport')
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r.get('stage') for r in rows)==STAGES[case],'First-Publish stage sequence')
    paths=receipt.get('captures');require(isinstance(paths,dict) and set(paths)==set(STAGES[case]),'First-Publish capture inventory')
    parent_nodes=None;modal_nodes=None;previous=None
    for row in rows:
        stage=row['stage'];proof=row.get('proof',{});server=proof.get('server',{});model=row.get('model',{});native=proof.get('native',{});product=proof.get('firstPublish',{})
        spec=server_proof(case,stage,server)
        require(model.get('networkCalls')==0 and model.get('reads')==len(spec['reads']) and model.get('writes')==spec['writes'],'Parent actual IO ledger')
        expected_geometry=dict(native,pngDimensions=row.get('geometry',{}).get('pngDimensions'))
        require(exact(row.get('geometry'),expected_geometry),'Native geometry/observation mismatch')
        native_proof(case,stage,native,model,viewport,rectangle)
        attempt_proof(case,stage,product,server,spec)
        confirmation_proof(case,stage,product,native,model)
        recovery_proof(case,row,previous)
        pane_exercise(case,row,previous)
        if case in C.STAGES: C.validate_stage(case,row,previous,rows)
        if case=='first-avatar-upload-cancel-held' and stage=='late-settled':
            held_upload_origin(row,next(v for v in rows if v['stage']=='upload-held'))
        if spec['sent'] and not spec['retired']:
            a=product['attempts'][0]
            if stage=='done': require(a.get('closed') is True and (product['document'].get('pending') is None)==a['resolved'],'Done lost sealed/verified ownership')
            else:
                notice=a['local'].get('notice')
                require(notice is None or notice in [v.get('text') for v in native['feedback']],'Current publication guidance hidden')
                texts=[v.get('text') for v in native['outcomes']]
                require('Publication: '+a['publication']['outcome'] in texts,'Original publication outcome not visible')
                if case=='first-metadata-rejected': require('Workflow details: rejected' in texts,'Metadata refusal hidden by later publication')
                if case=='first-settings-rejected-held': require('Copy access: rejected' in texts,'Settings refusal hidden by later publication')
        if native.get('fields'):
            ids=[native['fields'][key]['nodeId'] for key in ('title','description')]
            if parent_nodes is None: parent_nodes=ids
            require(ids==parent_nodes,'Original parent input remounted')
        if native.get('modal'):
            m=native['modal'];ids=[m['nodeId'],m['fields']['title']['nodeId'],m['fields']['description']['nodeId'],m['upload']['nodeId']]
            if stage in ('finish','reopened-finish'):
                if modal_nodes is not None: require(not set(ids)&set(modal_nodes),'Cancelled modal incorrectly reused as original')
                modal_nodes=ids
            require(ids==modal_nodes,'Modal input/file ownership changed')
        exercise=proof.get('exercise',{})
        if stage in ('primary-held','published-while-held'):
            quiet=exercise.get('quiet',{});require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),observation(row)),'Held original response quiet custody')
        if stage in ('cancelled','cancelled-again'):
            require(exact(exercise['before']['server'],exercise['after']['server']) and exact(exercise['after'],observation(row)),'Cancel dispatched or changed evidence')
            require(product.get('attempts')==[] and product.get('confirmation') is None,'Cancel created attempt')
        if stage in ('checked','checked-again'):
            require(exact(exercise.get('after'),observation(row)) and exercise['after']['model']['reads']==exercise['before']['model']['reads']+11
                and exercise['after']['model']['writes']==exercise['before']['model']['writes'],'Explicit Check replayed or skipped readback')
        if stage=='late-settled' and case=='first-retirement':
            for key in ('model','native'): require(exact(exercise['before'][key],exercise['after'][key]),'Late publication changed successor '+key)
            require(exact(exercise.get('after'),observation(row)) and product.get('visible') is False and product['attempts'][0].get('resolved') is False,'Late response regained authority')
        path=Path(paths[stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Capture path ownership')
        if png: validate_png(path,row['geometry']['pngDimensions'])
        previous=row
    require(exact(receipt.get('final'),observation(rows[-1])),'Final observation differs')
    if case!='first-retirement': require(receipt.get('replacement') is None,'Unexpected account replacement')
    return receipt


def choose_owned_file(pid,request,expected_path,require_running):
    """Answer only the original fixture PID's actual common Open dialog."""
    import ctypes
    from ctypes import wintypes as W
    import sys
    require(sys.platform=='win32','Native chooser acceptance currently requires Windows')
    expected_path=Path(expected_path).resolve()
    require(request.get('pid')==pid and avatar_case(request.get('case'))
        and request.get('input')=='workflow-finish-upload' and Path(request.get('path','')).resolve()==expected_path
        and expected_path.is_file(),'Unowned file chooser request')
    u=ctypes.WinDLL('user32',use_last_error=True)
    callback=ctypes.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    u.EnumWindows.argtypes=[callback,W.LPARAM];u.EnumChildWindows.argtypes=[W.HWND,callback,W.LPARAM]
    u.GetWindowThreadProcessId.argtypes=[W.HWND,ctypes.POINTER(W.DWORD)]
    u.GetClassNameW.argtypes=[W.HWND,W.LPWSTR,ctypes.c_int]
    u.GetParent.argtypes=[W.HWND];u.GetParent.restype=W.HWND
    u.GetDlgCtrlID.argtypes=[W.HWND];u.GetDlgItem.argtypes=[W.HWND,ctypes.c_int];u.GetDlgItem.restype=W.HWND
    u.IsWindow.argtypes=[W.HWND];u.IsWindowVisible.argtypes=[W.HWND]
    u.SendMessageTimeoutW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM,W.UINT,W.UINT,ctypes.POINTER(ctypes.c_size_t)]
    u.SendMessageTimeoutW.restype=W.LPARAM
    def owner(hwnd):
        p=W.DWORD();u.GetWindowThreadProcessId(hwnd,ctypes.byref(p));return p.value
    def classname(hwnd):
        buf=ctypes.create_unicode_buffer(128);u.GetClassNameW(hwnd,buf,128);return buf.value
    def enumerate_windows(parent=None):
        found=[]
        @callback
        def collect(hwnd,_): found.append(hwnd);return True
        if parent is None: u.EnumWindows(collect,0)
        else: u.EnumChildWindows(parent,collect,0)
        return found
    deadline=time.monotonic()+15;dialog=None
    while time.monotonic()<deadline:
        require_running()
        found=[h for h in enumerate_windows() if owner(h)==pid and classname(h)=='#32770' and u.IsWindowVisible(h)]
        require(len(found)<=1,'Multiple fixture-owned common dialogs')
        if found: dialog=found[0];break
        time.sleep(.05)
    require(dialog is not None,'Owned file Open dialog not found')
    edits=[]
    for h in enumerate_windows(dialog):
        if owner(h)!=pid or classname(h)!='Edit': continue
        p=h
        while p and p!=dialog:
            if u.GetDlgCtrlID(p)==1148: edits.append(h);break
            p=u.GetParent(p)
    require(len(edits)==1,'Original filename Edit/1148 ancestry not unique')
    edit=edits[0];button=u.GetDlgItem(dialog,1)
    require(button and owner(button)==pid and classname(button)=='Button','Owned Open/IDOK absent')
    def send(h,msg,w=0,l=0):
        require(owner(h)==pid,'Chooser ownership changed')
        out=ctypes.c_size_t()
        require(u.SendMessageTimeoutW(h,msg,w,l,2,2000,ctypes.byref(out))!=0,'Owned dialog message timed out')
        return out.value
    path_buffer=ctypes.create_unicode_buffer(str(expected_path))
    require(send(edit,0x000C,0,ctypes.addressof(path_buffer))!=0,'Filename selection refused')
    observed=ctypes.create_unicode_buffer(32768);send(edit,0x000D,len(observed),ctypes.addressof(observed))
    require(observed.value==str(expected_path),'Original filename buffer mismatch')
    send(button,0x00F5)
    deadline=time.monotonic()+10
    while u.IsWindow(dialog) and time.monotonic()<deadline:
        require_running();time.sleep(.05)
    require(not u.IsWindow(dialog),'Owned Open dialog did not close')
    return dict(pid=pid,dialog=int(dialog),filenameEdit=int(edit),filenameAncestorId=1148,
        openButton=int(button),openButtonId=1,path=str(expected_path),observedFilename=observed.value,
        selectedThroughOriginalDialog=True,dialogClosed=True)
