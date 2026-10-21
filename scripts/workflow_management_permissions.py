"""Independent copy-access Permissions receipts; no transport or app execution.

Server application, raw ACK and fresh verified readback are separate facts.
"""
import copy
from pathlib import Path
from workflow_management_delete import (TITLE, DESCRIPTION, WORKFLOW, TRIGGER, POLICY, EDITOR, INITIAL,
    exact, integer, require, read_fields, read_result as base_read_result)
from workflow_management_unpublish import decoded_form, measured_native, native_fields, slot_proof

STAGES = {
 'permissions-success': ('about','editor','draft','permissions','unchanged','selected','cancelled','acknowledged','checked','back','reopened'),
 'permissions-rejected': ('about','editor','draft','selected','rejected','checked','checked-again'),
 'permissions-unknown': ('about','editor','draft','selected','unconfirmed','checked','checked-again'),
 'permissions-retirement': ('about','editor','draft','selected','held','successor','late-settled'),
}
OWNER = {'account':None,'epoch':0,'team':'TWORKFLOW','user':'UWORKFLOW'}
METHOD = 'functions.workflows.permissions.copy.set'
ARGUMENTS = {'workflow_id':'WF_REVIEW','permission_type':'everyone'}
COPY = {'ok':True,'permission_type':'collaborators'}
RUN = {'ok':True,'permission_type':'app_collaborators'}
EXTERNAL = {'ok':True,'trippable_by_slack_connected_teams':{'allowed_teams':[]}}
ORIGINAL = dict(workflow=WORKFLOW,trigger=TRIGGER,copy=COPY,run=RUN,external=EXTERNAL)
ACK = {'ok':True,'permission_type':'everyone','opaque_receipt':{'preserve':[1,'copy']}}
CONFIRMED = 'Copy access acknowledged. Check current permissions to verify the setting; your local draft is retained.'
UNCONFIRMED = 'Copy-access change was rejected or remains unconfirmed. Check current permissions; no request will be replayed. Your draft is retained.'
VERIFIED = 'Copy access is verified. Your local draft is retained.'
OBSERVED = 'Current permissions were read. The operation remains unconfirmed or differs from the original settings; no request will be replayed. Your draft is retained.'
REOPENED = 'Current workflow loaded for review. Your proposed changes are retained.'


def result_for(case):
    if case == 'permissions-rejected': return {'raw':{'ok':False,'error':'not_allowed'},'error':None}
    if case == 'permissions-unknown': return {'raw':None,'error':'transport'}
    return {'raw':copy.deepcopy(ACK),'error':None}


def expected_form():
    return [['permission_type','everyone'],['workflow_id','WF_REVIEW'],['_x_reason','set-copy-permission']]


def read_result(method, current):
    if method == 'functions.workflows.permissions.copy.list':
        return {'raw':copy.deepcopy(current['copy']),'error':None}
    return base_read_result(method)


def stage_state(case, stage):
    initial = stage in ('about','editor','draft','permissions','unchanged','selected','cancelled')
    if initial:
        return dict(writes=0,phases=[],reads=[] if stage=='about' else INITIAL,
                    held=False,retired=False,finished=False,complete=False,resolved=False,checked=False)
    held = stage in ('held','successor')
    retired = stage in ('successor','late-settled')
    checked = stage in ('checked','checked-again','back','reopened')
    reads = INITIAL + POLICY + EDITOR
    if checked:
        reads += POLICY + EDITOR
        if stage == 'checked-again': reads += POLICY + EDITOR
        if stage == 'reopened': reads += INITIAL
    phase = 'in-flight' if held else {'permissions-rejected':'rejected','permissions-unknown':'unknown'}.get(case,'acknowledged')
    return dict(writes=1,phases=[phase],reads=reads,held=held,retired=retired,finished=not held,
                complete=phase=='acknowledged',resolved=case=='permissions-success' and checked,checked=checked)


def server_proof(server, case, state):
    require(isinstance(server,dict) and exact(server.get('owner'),OWNER),'Invalid original synthetic owner')
    require(exact(server.get('original'),ORIGINAL),'Original acquired settings changed')
    require(server.get('ownerCurrent') is (not state['retired']),'Original source owner changed')
    require(type(server.get('networkCalls')) is int and server['networkCalls']==0,'Unexpected network authority')
    ledgers=[server.get(k) for k in ('requests','responses','settlements','releases')]
    require(all(isinstance(x,list) for x in ledgers),'Missing original dispatch ledger')
    requests,responses,settlements,releases=ledgers
    methods=state['reads'][:23]+([METHOD] if state['writes'] else [])+state['reads'][23:]
    require([r.get('method') for r in requests]==methods,'Preflight/order/request count or no-replay changed')
    serials=list(range(1,len(requests)+1))
    require([r.get('serial') for r in requests]==serials and [r.get('serial') for r in responses]==serials,'Original serial order changed')
    held=24 if state['held'] else None
    require(server.get('held')==held,'Wrong original held request')
    require(exact(settlements,[{'serial':n,'returned':True} for n in serials if n!=held]),'Original settlements changed')
    require(type(server.get('completed')) is int and server['completed']==len(settlements)
            and type(server.get('cancelled')) is int and server['cancelled']==0,'Wrong settlement count')
    current=copy.deepcopy(ORIGINAL)
    for request,response in zip(requests,responses):
        own=request.get('owner',{})
        require(exact({k:own.get(k) for k in OWNER},OWNER),'Foreign dispatch owner')
        integer(own.get('visit'),'Missing owned visit',1)
        require(type(request.get('applied')) is bool and type(request.get('retrySafe')) is bool,'Missing dispatch facts')
        require(exact(request.get('args'),decoded_form(request.get('fields'))),'Wire fields differ from args')
        if request['method']==METHOD:
            require(request.get('phase')=='mutation' and request['fields']==expected_form()
                    and request['retrySafe'] is False,'Wrong copy permission wire/retry policy')
            result=result_for(case)
            applied=case!='permissions-rejected'
            require(request['applied'] is applied,'Application confused with ACK')
            if applied: current['copy']={'ok':True,'permission_type':'everyone'}
        else:
            require(request.get('phase')=='read' and request['applied'] is False,'Unexpected mutation')
            read_fields(request['method'],request['args'])
            result=read_result(request['method'],current)
        require(exact(response,{'serial':request['serial'],'result':result}),'Original raw response changed')
    require(exact(server.get('current'),current),'Manager/run/external/workflow state changed')
    expected=[]
    if case=='permissions-retirement' and state['finished']:
        expected=[{'serial':24,'receiverRetired':False,'delivered':True,'response':responses[23]}]
    require(exact(releases,expected),'Held original release changed or repeated')


def attempt_proof(attempt, case, state, current_server):
    require(isinstance(attempt,dict),'Missing original copy operation')
    for key in ('id','visit','document','revision'): integer(attempt.get(key),'Invalid attempt '+key,1)
    require(attempt.get('title')==TITLE and attempt.get('description')==DESCRIPTION
            and attempt.get('workflow')=='WF_REVIEW' and attempt.get('desired')=='everyone'
            and exact(attempt.get('originalCopy'),COPY),'Original admission changed')
    for key in ('finished','complete','resolved'): require(attempt.get(key) is state[key],'Wrong operation '+key)
    phase=state['phases'][0]
    require(attempt.get('noSend') is False and attempt.get('primarySucceeded') is (phase=='acknowledged')
            and attempt.get('current') is (not state['finished'] and not state['retired']),'Wrong operation ownership/ACK facts')
    slots=attempt.get('slots')
    require(isinstance(slots,list) and len(slots)==1,'Copy Save must retain one original slot')
    result=result_for(case) if phase!='in-flight' else {'raw':None,'error':None}
    slot_proof(slots[0],0,METHOD,ARGUMENTS,phase,result['raw'],'delivery unconfirmed' if phase=='unknown' else None)
    expected=dict(copy.deepcopy(current_server),matches=case in ('permissions-success','permissions-unknown')) if state['checked'] else None
    require(exact(attempt.get('readback'),expected),'Fresh readback replaced or fabricated ACK evidence')


def observation(row):
    return dict(model=row['model'],permissions=row['proof']['permissions'],server=row['proof']['server'],native=row['proof']['native'])


def settings_proof(native, controls, selected, viewport, rectangle):
    setting=native.get('settings')
    require(isinstance(setting,dict) and setting.get('id')=='workflow-permissions'
            and setting.get('title')=='Permissions','Missing actual Permissions panel')
    label,select=setting.get('label',{}),setting.get('copy',{})
    require(label.get('text')=='Who can copy this workflow?','Wrong copy-access question')
    require(select.get('id')=='workflow-permissions-copy' and select.get('value')==selected,'Wrong actual selected permission')
    require(exact(select.get('options'),[
        {'value':'everyone','text':'Everyone in Home fixture','disabled':False},
        {'value':'collaborators','text':'Workflow managers only','disabled':False}]),'Wrong original permission options')
    actions=setting.get('actions')
    require(actions==[controls.get('workflow-permissions-cancel'),controls.get('workflow-permissions-save')]
            and all(a and a.get('disabled') is False for a in actions)
            and [a['text'] for a in actions]==['Cancel','Save'],'Wrong original settings actions')
    for item in [label,select]+actions:
        require(item.get('visible') is True and item.get('hit') is True and item.get('disabled') is False,'Settings control hidden or disabled')
        rectangle(item.get('rect'),viewport)


def native_controls(native, controls, stage, route, *, confirming, busy, sealed, resolved, viewport, rectangle,
                    settings_validator=settings_proof):
    require(native.get('present') is (route=='editor'),'Actual editor presence differs from owned route')
    require(native.get('leasePresent') is (confirming or busy),'Original native lease lifecycle changed')
    if route!='editor':
        require(native.get('fields') is None and native.get('originalNodes') is None and native.get('settings') is None,'Retired editor exposed')
        require('workflow-configuration' in controls,'About Configuration missing')
        return
    title,description=(WORKFLOW['title'],WORKFLOW['description']) if stage=='editor' else (TITLE,DESCRIPTION)
    native_fields(native,title,description,confirming or busy or sealed)
    for name,disabled in [('save',stage=='editor' or confirming or busy or sealed),('managed',confirming or busy),
                           ('delete',confirming or busy or sealed),('permissions',confirming or busy or sealed)]:
        control=controls.get('workflow-management-'+name)
        require(isinstance(control,dict) and control.get('disabled') is disabled,'Wrong action availability: '+name)
    check='workflow-permissions-check' if sealed else 'workflow-management-check'
    require(controls.get(check,{}).get('disabled') is (confirming or busy),'Wrong Check availability')
    # A fresh reopen retains dirty metadata and records a newly observed Editor.
    # Only the resolved successful reopen may expose the existing adoption action.
    use_current = stage=='reopened' and resolved and not sealed
    require(('workflow-management-use-current' in controls) is use_current,'Wrong current-workflow adoption availability')
    if use_current:
        require(controls['workflow-management-use-current'].get('disabled') is False
                and controls['workflow-management-use-current'].get('text')=='Use current workflow',
                'Reopened observed workflow adoption is unavailable')
    require(not any('publish' in k for k in controls),'Unrequested operation exposed')
    if confirming: settings_validator(native,controls,'collaborators' if stage=='permissions' else 'everyone',viewport,rectangle)
    else: require(native.get('settings') is None,'Unexpected settings panel')


def validate(fixture, case, receipt, captures, *, viewport, rectangle, validate_png, png=True):
    require(case in STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Wrong Permissions fixture/case')
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('scenario') == 'editor' and receipt.get('permissionsCase') == case,
            'Wrong or failed Permissions receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'syntheticSession', 'noLiveNetwork')),
            'Missing actual entry/synthetic isolation')
    require(receipt.get('viewport') == viewport, 'Wrong actual viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[case]), 'Wrong Permissions stage order')
    require(isinstance(paths, dict) and set(paths) == set(STAGES[case]), 'Wrong original capture inventory')
    previous, identity, attempt_id = None, None, None
    for row in rows:
        stage, model, geometry, proof = (row.get(k) for k in ('stage', 'model', 'geometry', 'proof'))
        require(all(isinstance(v, dict) for v in (model, geometry, proof)), 'Missing stage model/proof')
        product, server, native = (proof.get(k) for k in ('permissions', 'server', 'native'))
        require(isinstance(product, dict), 'Missing actual copy-access observer')
        state = stage_state(case, stage)
        server_proof(server, case, state)
        route = 'about' if stage in ('about', 'back') or state['retired'] else 'editor'
        busy, confirming = state['held'] and not state['retired'], stage in ('permissions', 'selected')
        sealed = bool(state['phases']) and not state['resolved']
        for key in ('reads', 'writes', 'networkCalls', 'retainedDocuments', 'retainedAttempts'):
            integer(model.get(key), 'Invalid actual model ' + key)
        require(model['reads'] == len(state['reads']) and model['writes'] == state['writes']
                and model['networkCalls'] == 0 and model['retainedAttempts'] == 0,
                'Actual model differs from copy-access ledger')
        require(model.get('route') == route and model.get('busy') is busy
                and model['retainedDocuments'] == (0 if stage == 'about' else 1), 'Wrong current route/document custody')
        require(model.get('pending') is (route == 'editor' and sealed)
                and model.get('dirty') is (route == 'editor' and stage != 'editor')
                and model.get('observed') is (route == 'editor' and (stage == 'reopened' or state['checked'] and not state['resolved'])),
                'Draft or canonical readback custody changed')
        labels = {'pending': 'Not sent', 'in-flight': 'Waiting for response', 'acknowledged': 'Acknowledged',
                  'rejected': 'Rejected', 'unknown': 'Unconfirmed'}
        outcomes = []
        if route == 'editor' and sealed:
            outcomes = ['Copy access: ' + labels[state['phases'][0]]]
        require(model.get('outcomes') == (None if route == 'about' else outcomes), 'Per-slot visible outcomes changed')
        require(product.get('visible') is (route == 'editor'), 'Retired/hidden original visit leaked')
        attempts = product.get('attempts')
        require(isinstance(attempts, list) and len(attempts) == bool(state['phases']), 'Attempt lost or recreated')
        if attempts:
            attempt_proof(attempts[0], case, state, server['current'])
            if attempt_id is None: attempt_id = attempts[0]['id']
            require(attempts[0]['id'] == attempt_id, 'Replayed original attempt identity')
        document = product.get('document')
        if route == 'editor':
            require(isinstance(document, dict), 'Original draft missing')
            for key in ('id', 'revision'): integer(document.get(key), 'Invalid original draft ' + key, 1)
            if identity is None: identity = (product['visit'], document['id'])
            require(document['id'] == identity[1] and (stage == 'reopened' or product['visit'] == identity[0]),
                    'Original draft document/visit replaced')
            if stage == 'reopened': require(product['visit'] != identity[0], 'Reopen did not create a fresh visit')
            require(native.get('scope') == {'visit': str(product['visit']), 'document': str(document['id']),
                                            'revision': str(document['revision'])}, 'Actual native scope differs from original draft')
            expected = (WORKFLOW['title'], WORKFLOW['description']) if stage == 'editor' else (TITLE, DESCRIPTION)
            require((document.get('title'), document.get('description')) == expected
                    and (model.get('title'), model.get('description')) == expected, 'Permissions implicitly saved/replaced dirty metadata')
            require(exact(document.get('editor'), WORKFLOW) and exact(document.get('trigger'), TRIGGER)
                    and exact(document.get('copy'), server['current']['copy'] if state['resolved'] else COPY)
                    and exact(document.get('run'), RUN) and exact(document.get('external'), EXTERNAL),
                    'Copy baseline adopted without confirmed matching readback')
            require(document.get('pending') == (attempt_id if sealed else None)
                    and document.get('editable') is (not confirming and not busy and not sealed), 'Unsafe draft unlocking')
            notice = None
            if state['finished']:
                notice = (REOPENED if stage == 'reopened' else VERIFIED if state['resolved'] else
                          OBSERVED if state['checked'] else CONFIRMED if state['complete'] else UNCONFIRMED)
            require(document.get('notice') == model.get('notice') == notice, 'Wrong operation/readback guidance')
        else:
            require(document is None and model.get('title') is None and model.get('description') is None,
                    'Hidden draft projected into successor')
        confirmation = product.get('confirmation')
        if confirming:
            require(isinstance(confirmation, dict) and confirmation.get('current') is True
                    and all(confirmation.get(k) == v for k, v in dict(visit=product['visit'], document=document['id'],
                        revision=document['revision'], title=TITLE, description=DESCRIPTION, workflow='WF_REVIEW', selected='collaborators' if stage=='permissions' else 'everyone', originalCopy=COPY).items()),
                    'Confirmation lacks exact original draft custody')
            integer(confirmation.get('id'), 'Missing original confirmation id', 1)
        else: require(confirmation is None, 'Unexpected confirmation outside original admission')
        controls = measured_native(native, geometry, viewport, rectangle)
        native_controls(native, controls, stage, route, confirming=confirming, busy=busy, sealed=sealed, resolved=state['resolved'], viewport=viewport, rectangle=rectangle)
        messages = [x['text'] for x in native['feedback']]
        for message in ([model['notice']] if model.get('notice') else []) + (model.get('outcomes') or []):
            require(message in messages, 'Actual operation feedback absent from viewport')
        path = Path(captures) / f'{fixture}-{stage}.png'
        require(paths[stage] == str(path), 'Capture escaped owned stage path')
        if png: validate_png(path, geometry['pngDimensions'])
        if previous:
            old = previous['proof']['server']
            for key in ('requests', 'responses', 'settlements', 'releases'):
                require(exact(server[key][:len(old[key])], old[key]), 'Original ledger history rewritten')
        previous = row
    require(exact(receipt.get('final'), observation(rows[-1])), 'Terminal differs from final stage')
    exercises(receipt, case, rows, rectangle)


def exercises(receipt, case, rows, _rectangle):
    bystage = {r['stage']: r for r in rows}
    for index, row in enumerate(rows):
        stage = row['stage']; exercise = row['proof'].get('exercise')
        require(isinstance(exercise, dict), 'Missing original action exercise')
        current = observation(row)
        if stage == 'held':
            integer(exercise.get('observedMs'), 'Missing bounded held observation', 500)
            require(exact(exercise.get('before'), current) and exact(exercise.get('after'), current),
                    'Held original changed before release')
        elif stage == 'acknowledged':
            require(set(exercise)=={'admission'},'Missing final Save admission')
        elif stage in ('unchanged', 'cancelled', 'checked', 'checked-again', 'back'):
            before = observation(rows[index-1])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current),
                    'Action before/after not original captured observations')
            if stage in ('unchanged','cancelled'):
                require(exercise.get('action') == ('workflow-permissions-save' if stage=='unchanged' else 'workflow-permissions-cancel'),'Wrong local action')
                require(exact(before['server'], current['server']) and before['permissions']['attempts'] == []
                        and current['permissions']['attempts'] == [], 'Cancel dispatched copy-access work')
                a, b = before['permissions']['document'], current['permissions']['document']
                require(b['revision'] > a['revision'] and exact(
                    {k:v for k,v in a.items() if k not in ('revision','editable')},
                    {k:v for k,v in b.items() if k not in ('revision','editable')}), 'Cancel replaced dirty document')
                for key in ('title','description'):
                    require(exact({k:v for k,v in before['native']['fields'][key].items() if k != 'readonly'},
                                  {k:v for k,v in current['native']['fields'][key].items() if k != 'readonly'}),
                            'Cancel altered original native buffer/selection')
            elif stage == 'back':
                require(exact(before['server'], current['server']) and current['native'].get('originalDetached') is True
                        and exact(before['permissions']['attempts'], current['permissions']['attempts']),
                        'Back dispatched or lost retained operation')
            else:
                require(current['model']['reads'] == before['model']['reads'] + 11
                        and current['model']['writes'] == before['model']['writes'], 'Check replayed or lacked fresh read')
                a, b = before['permissions']['attempts'][0], current['permissions']['attempts'][0]
                require(exact({k:v for k,v in a.items() if k not in ('resolved','readback')},
                              {k:v for k,v in b.items() if k not in ('resolved','readback')}),
                        'Readback rewrote original operation receipt')
                require(exact(before['server']['current'], current['server']['current']), 'Check applied a mutation')
                require(current['permissions']['document']['revision'] > before['permissions']['document']['revision'],
                        'Readback not a newly owned observation')
        elif stage == 'reopened':
            before = observation(bystage['checked'])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current)
                    and exact(exercise.get('remount'), {'ok': True, 'originalDetached': True, 'newRoot': True,
                                                      'newTitle': True, 'newDescription': True}),
                    'Reopen lacks actual old-detached/new-node evidence')
            require(exact(before['permissions']['attempts'], current['permissions']['attempts'])
                    and current['model']['reads'] == before['model']['reads'] + 12,
                    'Reopen rewrote attempt or skipped original acquisition')
        elif stage == 'successor':
            before = observation(rows[index-1])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current),
                    'Missing genuine original retirement boundary')
            replacement = exercise.get('replacement', {})
            require(exact(replacement.get('before'), before['model']) and exact(replacement.get('after'), current['model'])
                    and replacement.get('foreignDocumentHidden') is True, 'Wrong actual replacement')
            integer(replacement.get('previousEpoch'), 'Missing original epoch')
            require(type(replacement.get('currentEpoch')) is int
                    and replacement['currentEpoch'] > replacement['previousEpoch']
                    and current['native'].get('originalDetached') is True, 'Original owner/native not retired')
        elif stage == 'late-settled':
            before = observation(bystage['successor'])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current)
                    and exercise.get('successorUnchanged') is True, 'Missing late original response boundary')
            require(exact(before['model'], current['model']) and exact(before['native'], current['native']),
                    'Late original response altered successor UI/draft')
            for key in ('requests', 'responses', 'current'):
                require(exact(before['server'][key], current['server'][key]), 'Late response dispatched or changed server state')
            require(exact(exercise.get('release'), current['server']['releases'][-1]), 'Late result not original held response')
        else:
            require(exercise == {}, 'Unexpected unvalidated exercise')
    admission=observation(bystage['selected'])
    if case=='permissions-success':
        admission=bystage['acknowledged']['proof']['exercise']['admission']
        require(isinstance(admission,dict),'Missing actual changed-Save admission')
        prior=observation(bystage['cancelled'])
        require(exact(admission.get('server'),prior['server']) and exact(admission.get('model'),prior['model']),
                'Opening changed Save dispatched or changed draft')
        p=admission.get('permissions',{});n=admission.get('native',{})
        c=p.get('confirmation',{})
        require(c.get('id')!=bystage['selected']['proof']['permissions']['confirmation']['id'], 'Cancelled admission reused')
        require(exact(p.get('attempts'),[]) and p.get('visible') is True,'Unexpected pre-Save attempt')
        d=p.get('document',{})
        require(exact({k:v for k,v in d.items() if k!='editable'},
                      {k:v for k,v in prior['permissions']['document'].items() if k!='editable'}) and d.get('editable') is False,
                'Reopened settings changed original document')
        native_fields(n,TITLE,DESCRIPTION,True)
        require(n.get('scope')==prior['native']['scope'] and n.get('leasePresent') is True and n.get('composing') is False,
                'Reopened settings changed native custody')
        # Original geometry is measured independently, never inferred from selected values.
        controls={x['id']:x for x in n.get('controls',[])}
        require(n.get('ok') is True and n.get('noOverflow') is True and n.get('viewport')==receipt['viewport'],
                'Changed Save admission lacks original viewport proof')
        require(len(controls)==len(n['controls']) and all(controls),'Changed Save controls have invalid identities')
        for item in n['controls']+n.get('feedback',[]):
            require(item.get('visible') is True and item.get('hit') is True and type(item.get('disabled')) is bool,
                    'Changed Save admission has hidden controls')
            _rectangle(item.get('rect'),receipt['viewport'])
        settings_proof(n,controls,'everyone',receipt['viewport'],_rectangle)
    confirmation=admission['permissions']['confirmation']
    d=admission['permissions']['document']
    require(confirmation.get('current') is True and confirmation.get('selected')=='everyone'
            and exact(confirmation.get('originalCopy'),COPY)
            and all(confirmation.get(k)==v for k,v in dict(visit=admission['permissions']['visit'],document=d['id'],
                revision=d['revision'],title=TITLE,description=DESCRIPTION,workflow='WF_REVIEW').items()),
            'Changed Save lacks original selected source custody')
    for row in rows:
        for attempt in row['proof']['permissions']['attempts']:
            require(all(attempt[k]==confirmation[k] for k in ('id','visit','document','revision','title','description','workflow')),
                    'Attempt escaped admitted settings')
        for request in row['proof']['server']['requests']:
            if request['phase']=='mutation':
                require(all(request['owner'].get(k)==confirmation[k] for k in ('visit','document','revision')),
                        'Dispatch escaped original settings custody')
    expected_replacement = bystage['successor']['proof']['exercise']['replacement'] if 'successor' in bystage else None
    require(exact(receipt.get('replacement'), expected_replacement), 'Terminal account replacement changed')
