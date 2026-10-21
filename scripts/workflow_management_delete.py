"""Independent closed-fixture Delete receipts; no launching or live authority."""
from pathlib import Path
import copy
import json

STAGES = {
    'delete-success': ('about', 'editor', 'draft', 'confirmation', 'cancelled', 'reconfirmed',
                       'trigger-ack', 'deleted', 'managed-empty'),
    'delete-first-rejected': ('about', 'editor', 'draft', 'confirmation', 'rejected', 'checked', 'checked-again'),
    'delete-first-unknown': ('about', 'editor', 'draft', 'confirmation', 'unconfirmed', 'checked', 'checked-again'),
    'delete-second-unknown': ('about', 'editor', 'draft', 'confirmation', 'partial-unconfirmed', 'checked', 'checked-again'),
    'delete-preflight-retirement': ('about', 'editor', 'draft', 'confirmation', 'preflight-held', 'successor', 'late-refused'),
    'delete-mutation-retirement': ('about', 'editor', 'draft', 'confirmation', 'mutation-held', 'successor', 'late-settled'),
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)



TITLE = 'R\u00e9vision de projet'
DESCRIPTION = 'Keep the proposed details.\nReview \u4e2d\U0001f642 e\u0301 together.'
WORKFLOW = {'id': 'WF_REVIEW', 'app_id': 'A_HOME', 'title': 'Review requests',
            'description': 'Collect the details before a team review.', 'is_published': False,
            'is_empty': False, 'date_updated': 1, 'collaborators': ['UWORKFLOW'],
            'steps': [{'id': 'STEP_REVIEW', 'function': {'id': 'FN_REVIEW',
                      'product_level_availability': {'is_available': True}},
                      'inputs': {'message': {'value': 'Review this'}}}], 'retained_unknown': 42}
TRIGGER = {'id': 'TR_REVIEW', 'type': 'shortcut', 'name': 'Review requests',
           'description': 'Collect the details before a team review.',
           'workflow': {'workflow_id': 'WF_REVIEW', 'app_id': 'A_HOME', 'app': {'id': 'A_HOME'}},
           'inputs': {}, 'available_data': {}, 'retained_unknown': 19}
METHODS = ('workflows.triggers.delete', 'functions.workflows.delete')
ARGS = ({'trigger_id': 'TR_REVIEW'}, {'workflow_id': 'WF_REVIEW'})
REASONS = ('delete-hermes-workflow', 'delete-workflow')


def exact(actual, expected):
    return json.dumps(actual, sort_keys=True, ensure_ascii=True) == json.dumps(expected, sort_keys=True, ensure_ascii=True)


def integer(value, message, minimum=0):
    require(type(value) is int and value >= minimum, message)
    return value


def expected_result(case, index):
    if case == 'delete-first-rejected' and index == 0:
        return {'raw': {'ok': False, 'error': 'not_allowed'}, 'error': None}
    if (case == 'delete-first-unknown' and index == 0 or
            case == 'delete-second-unknown' and index == 1):
        return {'raw': None, 'error': 'transport'}
    return {'raw': {'ok': True}, 'error': None}


def read_result(method, workflow_present=True, trigger_present=True):
    # Independent literal closed-server envelopes; no production parser invocation.
    if method == 'functions.workflows.getLatest':
        return ({'raw': {'ok': True, 'decorated_workflow': copy.deepcopy(WORKFLOW),
                         'available_data': {}, 'expression_data': {}}, 'error': None}
                if workflow_present else {'raw': None, 'error': 'workflow_not_found'})
    if method == 'workflows.triggers.info':
        return ({'raw': {'ok': True, 'trigger': copy.deepcopy(TRIGGER)}, 'error': None}
                if trigger_present else {'raw': None, 'error': 'trigger_not_found'})
    raw = {
        'experiments.getByUser': {'ok': True, 'assignments': {
            'slackforce_pnp_wfb': {'group': 'on'}, 'wfb_hermes_trials': {'group': 'off'}}},
        'client.init': {'ok': True, 'team': {'id': 'TWORKFLOW', 'plan': 'paid'},
                        'self': {'id': 'UWORKFLOW', 'is_restricted': False},
                        'workspaces': [{'id': 'TWORKFLOW', 'plan': 'paid'}]},
        'features.access.policies.list': {'ok': True, 'feature_policies': {
            'hermes__read': {'is_enabled': True}, 'hermes__write': {'is_enabled': True}}},
        'client.extras': {'ok': True, 'team_prefs': {'toplevel_prefs': {
            'workflow_builder_enabled': True, 'who_can_create_workflows': 'regular'}, 'workspace_prefs': {}}},
        'functions.workflows.getIsRestricted': {'ok': True, 'is_restricted': False,
                                               'restricted_by_connector_steps_only': False},
        'workflows.triggers.list': {'ok': True, 'triggers': [copy.deepcopy(TRIGGER)] if trigger_present else []},
        'workflows.triggers.prefs.get': {'ok': True, 'trippable_by_slack_connected_teams': {'allowed_teams': []}},
        'functions.workflows.permissions.copy.list': {'ok': True, 'permission_type': 'collaborators'},
        'workflows.triggers.permissions.list': {'ok': True, 'permission_type': 'app_collaborators'},
        'functions.workflows.list': {'ok': True, 'workflows': [copy.deepcopy(WORKFLOW)] if workflow_present else [],
                                    'response_metadata': {'next_cursor': ''}},
    }
    require(method in raw, 'Unexpected read endpoint')
    return {'raw': raw[method], 'error': None}


def server_proof(server, case, writes, *, held=False, retired=False):
    require(isinstance(server, dict), 'Missing original closed server')
    require(exact(server.get('original'), {'workflow': WORKFLOW, 'trigger': TRIGGER}), 'Original raw editor changed')
    owner = server.get('owner')
    require(exact(owner, {'account':None,'epoch':0,'team':'TWORKFLOW','user':'UWORKFLOW'}), 'Invalid original server owner')
    integer(owner.get('epoch'), 'Missing original epoch')
    require(server.get('ownerCurrent') is (not retired), 'Wrong original server currentness')
    require(type(server.get('networkCalls')) is int and server['networkCalls'] == 0, 'Unexpected network capability')
    requests, responses, settlements, releases = (server.get(k) for k in ('requests', 'responses', 'settlements', 'releases'))
    require(all(isinstance(v, list) for v in (requests, responses, settlements, releases)), 'Missing raw server ledgers')
    require([r.get('serial') for r in requests] == list(range(1, len(requests)+1)), 'Dispatch serial changed')
    require([r.get('serial') for r in responses] == list(range(1, len(requests)+1)), 'Original responses missing/reordered')
    held_id = server.get('held')
    require((held_id is not None) is held, 'Wrong held request state')
    if held:
        integer(held_id, 'Invalid held serial', 1)
        require(held_id == len(requests), 'Held request is not original last dispatch')
    expected_settled = [r['serial'] for r in requests if r['serial'] != held_id]
    require(settlements == [{'serial': n, 'returned': True} for n in expected_settled], 'Lost or fabricated returned response')
    require(type(server.get('completed')) is int and server['completed'] == len(settlements)
            and type(server.get('cancelled')) is int and server['cancelled'] == 0, 'Wrong settlement count')
    mutations = []
    workflow_present = trigger_present = True
    for request, response in zip(requests, responses):
        require(request.get('phase') in ('read', 'mutation'), 'Invalid dispatch phase')
        require(type(request.get('serial')) is int, 'Boolean dispatch serial')
        own = request.get('owner', {})
        require(all(own.get(k) == owner[k] for k in owner), 'Foreign dispatch owner')
        integer(own.get('visit'), 'Missing actual visit', 1)
        fields = request.get('fields')
        require(isinstance(fields, list) and all(isinstance(f, list) and len(f) == 2
                and all(isinstance(v, str) for v in f) for f in fields), 'Invalid original form')
        require(len({f[0] for f in fields}) == len(fields) and 'token' not in dict(fields)
                and request.get('args') == dict(fields), 'Repeated or altered safe fields')
        require(type(request.get('retrySafe')) is bool and type(request.get('applied')) is bool,
                'Missing original dispatch flags')
        if request['phase'] == 'mutation':
            index = len(mutations)
            require(index < 2 and request.get('method') == METHODS[index], 'Delete prerequisite order/replay changed')
            expected_fields = list(ARGS[index].items()) + [('_x_reason', REASONS[index])]
            require(fields == [list(p) for p in expected_fields] and request['retrySafe'] is False,
                    'Delete original arguments/reason/retry policy changed')
            result = expected_result(case, index)
            applied = not (case == 'delete-first-rejected' and index == 0)
            require(request['applied'] is applied, 'Server application differs from raw outcome')
            if index == 1:
                previous = mutations[0]
                require(previous['serial'] in expected_settled and
                        expected_result(case, 0) == {'raw': {'ok': True}, 'error': None}, 'Dependent delete lacked ACK')
            if applied:
                if index == 0: trigger_present = False
                else: workflow_present = False
            mutations.append(request)
        else:
            require(request['applied'] is False, 'Read applied a mutation')
            read_fields(request['method'], request['args'])
            result = read_result(request.get('method'), workflow_present, trigger_present)
        require(exact(response, {'serial': request['serial'], 'result': result}), 'Original raw response changed')
    require(len(mutations) == writes, 'Wrong exact Delete dispatch count')
    require(server.get('triggerPresent') is trigger_present and server.get('workflowPresent') is workflow_present,
            'Closed server presence fabricated')
    for release in releases:
        serial = integer(release.get('serial'), 'Invalid release serial', 1)
        require(1 <= serial <= len(responses) and release == {'serial': serial, 'receiverRetired': False,
                'delivered': True, 'response': responses[serial-1]}, 'Original held response was not delivered exactly')
    require(len(releases) <= 1, 'Duplicate original release')
    return mutations


def attempt_proof(value, phases, *, finished, terminal=False, no_send=False, current=None):
    require(isinstance(value, dict), 'Missing inert Delete attempt')
    for key in ('id', 'visit', 'document', 'revision'):
        integer(value.get(key), 'Invalid original attempt '+key, 1)
    require(value.get('title') == TITLE and value.get('description') == DESCRIPTION
            and value.get('workflow') == 'WF_REVIEW', 'Original Delete draft changed')
    require(value.get('finished') is finished and value.get('terminal') is terminal
            and value.get('noSend') is no_send and value.get('primarySucceeded') is (phases == ['acknowledged', 'acknowledged']),
            'Delete terminal flags do not follow ACKs')
    require(value.get('current') is (not finished if current is None else current), 'Wrong current original attempt')
    slots = value.get('slots')
    require(isinstance(slots, list) and len(slots) == 2, 'Lost ordered Delete slots')
    for i, (slot, phase) in enumerate(zip(slots, phases)):
        require(slot.get('index') == i and type(slot.get('index')) is int
                and slot.get('method') == METHODS[i] and slot.get('arguments') == ARGS[i]
                and (slot.get('prerequisite') is None if i == 0 else
                     type(slot.get('prerequisite')) is int and slot['prerequisite'] == 0)
                and slot.get('outcome') == phase, 'Wrong Delete slot or prerequisite')
        raw = {'ok': True} if phase == 'acknowledged' else {'ok': False, 'error': 'not_allowed'} if phase == 'rejected' else None
        failure = 'delivery unconfirmed' if phase == 'unknown' else None
        require(exact(slot.get('raw'), raw) and slot.get('failure') == failure, 'Original ACK/Unknown distinction changed')


def read_fields(method, args):
    if method == 'experiments.getByUser':
        require(args in ({'ssb_platform': os, 'ssb_release_channel': 'prod'} for os in ('windows','darwin','linux')),
                'Wrong experiment form')
        return
    policy = {
        'client.init': {'include_relevant_onboarding': 'false', '_x_reason': 'native-workflow-capabilities'},
        'features.access.policies.list': {'_x_reason': 'native-workflow-capabilities'},
        'client.extras': {'extras': '["team_prefs"]', '_x_reason': 'native-workflow-capabilities'},
    }
    if method in policy:
        require(args == policy[method], 'Wrong policy form')
        return
    if method == 'functions.workflows.list':
        require(set(args) == {'filter_options','limit','sort_options','workflow_builder_only','_x_reason'}
                and args['limit'] == '50' and args['workflow_builder_only'] == 'true'
                and args['_x_reason'] == 'automations_gallery', 'Wrong managed-list form')
        require(json.loads(args['filter_options']) == {'name':''} and
                json.loads(args['sort_options']) == {'column':'date_updated','direction':'desc'}, 'Wrong list query')
        return
    if method == 'workflows.triggers.list':
        require(args in ({'app_id':'A_HOME','_x_reason':'load-workflow-editor'},
                         {'app_ids':'','_x_reason':'automations_gallery'}), 'Wrong trigger-list form')
        return
    key, value = ('trigger_id','TR_REVIEW') if method in (
        'workflows.triggers.info','workflows.triggers.permissions.list') else ('workflow_id','WF_REVIEW')
    require(method in ('functions.workflows.getLatest','functions.workflows.getIsRestricted',
                       'workflows.triggers.info','workflows.triggers.prefs.get',
                       'functions.workflows.permissions.copy.list','workflows.triggers.permissions.list')
            and args == {key:value,'_x_reason':'load-workflow-editor'}, 'Wrong editor read form')


POLICY = ['experiments.getByUser', 'client.init', 'features.access.policies.list', 'client.extras']
EDITOR = ['functions.workflows.getIsRestricted','functions.workflows.getLatest','workflows.triggers.list',
          'workflows.triggers.info','workflows.triggers.prefs.get',
          'functions.workflows.permissions.copy.list','workflows.triggers.permissions.list']
INITIAL = POLICY + ['workflows.triggers.list'] + EDITOR
UNCONFIRMED = 'Deletion is not confirmed. Each response and your draft are retained. No deletion will be replayed.'
DELETED = 'Workflow deleted. Your local draft and deletion responses are retained.'
NO_SEND = 'Workflow or permissions changed. No deletion was sent; your draft is retained.'
WARNING = 'When you delete this workflow, it will stop working for anyone who uses it. This cannot be undone. Your local draft will be retained.'


def stage_expectation(case, stage):
    initial = ('about','editor','draft','confirmation','cancelled','reconfirmed')
    if stage in initial:
        return 0, [], False, False, [] if stage == 'about' else INITIAL
    if case == 'delete-preflight-retirement':
        return 0, ['pending','pending'], stage == 'late-refused', False, INITIAL + POLICY + EDITOR[:2]
    if case == 'delete-mutation-retirement':
        return 1, ['acknowledged' if stage == 'late-settled' else 'in-flight','pending'], stage == 'late-settled', False, INITIAL + POLICY + EDITOR
    if case == 'delete-success':
        reads = INITIAL + POLICY + EDITOR
        if stage == 'managed-empty': reads += POLICY + ['functions.workflows.list','workflows.triggers.list']
        return 2, ['acknowledged','in-flight' if stage == 'trigger-ack' else 'acknowledged'], stage != 'trigger-ack', stage != 'trigger-ack', reads
    phases = {'delete-first-rejected':['rejected','pending'], 'delete-first-unknown':['unknown','pending'],
              'delete-second-unknown':['acknowledged','unknown']}[case]
    return (2 if case == 'delete-second-unknown' else 1), phases, True, False, INITIAL + POLICY + EDITOR


def native_proof(native, stage, model, rectangle, viewport, *, confirming, busy, sealed):
    require(isinstance(native, dict) and native.get('ok') is True and native.get('noOverflow') is True
            and native.get('viewport') == viewport, 'Unproven original native layout')
    controls, feedback = native.get('controls'), native.get('feedback')
    require(isinstance(controls, list) and controls and isinstance(feedback,list), 'Missing native controls')
    ids = [c.get('id') for c in controls]
    require(len(ids) == len(set(ids)) and all(isinstance(i,str) and i for i in ids), 'Control identity changed')
    for item in controls+feedback:
        require(item.get('visible') is True and item.get('hit') is True and type(item.get('disabled')) is bool,
                'Control or feedback is hidden or occluded')
        rectangle(item.get('rect'),viewport)
    require(native.get('composing') is False, 'Unsettled native composition')
    if model.get('route') != 'editor':
        require(native.get('fields') is None and native.get('originalNodes') is None, 'Foreign editor remained visible')
        require(native.get('confirmation') is None and native.get('leasePresent') is False, 'Hidden original lease/confirmation')
        require(('workflow-configuration' in ids) if model.get('route')=='about' else ('workflow-management-search' in ids),
                'Missing actual destination controls')
        return
    require(native.get('originalNodes') is True and native.get('originalDetached') is False, 'Original native controls replaced')
    fields = native.get('fields')
    require(isinstance(fields,dict), 'Missing original native fields')
    title, description = (WORKFLOW['title'],WORKFLOW['description']) if stage=='editor' else (TITLE,DESCRIPTION)
    for key,value in [('title',title),('description',description)]:
        field = fields.get(key,{})
        require(field.get('value') == value and field.get('readonly') is (confirming or busy or sealed),
                'Original native buffer/readonly state changed')
        for bound in ('start','end'):
            integer(field.get(bound),'Missing original native selection')
        require(0 <= field['start'] <= field['end'] <= len(value.encode('utf-16-le'))//2, 'Native UTF16 selection invalid')
    require(native.get('leasePresent') is (confirming or busy), 'Actual native lease lifecycle changed')
    byid={c['id']:c for c in controls}
    for key in ('workflow-management-save','workflow-management-check','workflow-management-managed'):
        require(key in byid, 'Missing actual original control '+key)
    require(byid['workflow-management-save']['disabled'] is (stage=='editor' or confirming or busy or sealed), 'Unsafe Save availability')
    require(byid['workflow-management-check']['disabled'] is (confirming or busy or sealed), 'Unsafe Check availability')
    require(byid['workflow-management-managed']['disabled'] is (confirming or busy), 'Wrong managed navigation availability')
    if not sealed or stage != 'deleted':
        require('workflow-management-delete' in byid and byid['workflow-management-delete']['disabled'] is (confirming or busy or sealed),
                'Wrong Delete action availability')
    confirmation=native.get('confirmation')
    if confirming:
        require(isinstance(confirmation,dict) and confirmation.get('id')=='workflow-delete-confirmation'
                and WARNING in confirmation.get('text','') and 'Permanently delete this workflow?' in confirmation['text'],
                'Original confirmation warning missing')
        require(confirmation.get('visible') is True and confirmation.get('hit') is True, 'Confirmation obscured')
        rectangle(confirmation.get('rect'),viewport)
        actions=confirmation.get('actions')
        require(actions == [byid.get('workflow-delete-cancel'),byid.get('workflow-delete-confirm')]
                and all(a and not a['disabled'] for a in actions)
                and [a['text'] for a in actions] == ['Cancel','Delete'], 'Confirmation original actions changed')
    else: require(confirmation is None, 'Unexpected live confirmation')
    texts=[f['text'] for f in feedback]
    if model.get('notice'): require(model['notice'] in texts,'Retained outcome guidance missing')
    for outcome in model.get('outcomes') or []: require(outcome in texts,'Per-slot outcome not visibly retained')


def validate(fixture, case, receipt, captures, *, viewport, rectangle, validate_png, png=True):
    require(case in STAGES and fixture in ('workflow-editor','workflow-editor-narrow'), 'Wrong Delete fixture/case')
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture and receipt.get('scenario') == 'editor'
            and receipt.get('deleteCase') == case, 'Wrong or failed Delete receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','syntheticSession','noLiveNetwork')),
            'Missing actual About/synthetic isolation')
    require(receipt.get('viewport') == viewport, 'Wrong native viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows] == list(STAGES[case]), 'Wrong Delete stage inventory/order')
    require(isinstance(paths,dict) and set(paths)==set(STAGES[case]), 'Wrong Delete capture inventory')
    previous = None
    original_identity = None
    attempt_id = None
    for row in rows:
        stage, model, geometry, proof = row.get('stage'),row.get('model'),row.get('geometry'),row.get('proof')
        require(all(isinstance(v,dict) for v in (model,geometry,proof)), 'Missing stage evidence')
        product, server, native = proof.get('delete'), proof.get('server'), proof.get('native')
        require(isinstance(product,dict), 'Missing genuine Delete observer')
        writes, phases, finished, terminal, read_methods = stage_expectation(case,stage)
        held = stage in ('trigger-ack','preflight-held','mutation-held','successor')
        retired = stage in ('successor','late-refused','late-settled')
        busy = stage in ('trigger-ack','preflight-held','mutation-held')
        confirming = stage in ('confirmation','reconfirmed')
        sealed = bool(phases) and not (case=='delete-preflight-retirement')
        mutations = server_proof(server,case,writes,held=held,retired=retired)
        ordered_methods = read_methods[:23] + list(METHODS[:writes]) + read_methods[23:]
        require([r['method'] for r in server['requests']] == ordered_methods,
                'Mutation dispatched before original complete preflight or after replay')
        require([r['method'] for r in server['requests'] if r['phase']=='read'] == read_methods,
                'Wrong exact preflight/read sequence or replay')
        for key in ('reads','writes','networkCalls','retainedDocuments','retainedAttempts'):
            integer(model.get(key),'Missing actual '+key)
        require(model['writes']==writes and model['reads']==len(read_methods) and model['networkCalls']==0,
                'Model counters differ from original ledger')
        require(model.get('busy') is busy, 'Wrong current read/dispatch state')
        route='about' if stage=='about' or retired else 'managed' if stage=='managed-empty' else 'editor'
        require(model.get('route')==route,'Wrong actual Delete route')
        require(model['retainedDocuments']==(0 if stage=='about' else 1)
                and model.get('dirty') is (route=='editor' and stage!='editor')
                and model.get('pending') is (route=='editor' and bool(phases))
                and model.get('observed') is False, 'Original draft/metadata custody changed')
        outcome_labels={'pending':'Not sent','in-flight':'Waiting for response','acknowledged':'Acknowledged',
                        'rejected':'Rejected','unknown':'Unconfirmed'}
        expected_outcomes=([f'{name}: {outcome_labels[phase]}' for name,phase in zip(('Trigger','Workflow'),phases)]
                           if route=='editor' else None if route=='about' else [])
        require(model.get('outcomes')==expected_outcomes,'Actual outcome copy/order differs from original slots')
        require(product.get('visible') is (stage!='about' and not retired), 'Original visit visibility changed')
        require(product.get('confirmation') is not None if confirming else product.get('confirmation') is None,
                'Wrong live confirmation state')
        attempts=product.get('attempts')
        require(isinstance(attempts,list) and len(attempts)==(1 if phases else 0),'Attempt dropped/duplicated')
        if phases:
            attempt=attempts[0]
            attempt_proof(attempt,phases,finished=finished,terminal=terminal,
                          no_send=case=='delete-preflight-retirement' and finished,current=not finished and not retired)
            if attempt_id is None: attempt_id=attempt['id']
            require(attempt['id']==attempt_id,'New attempt recreated from retained state')
            require(model['retainedAttempts']==0,'Delete leaked into metadata attempt book')
        else: require(model['retainedAttempts']==0,'Attempt created before confirmation')
        document=product.get('document')
        if route=='editor':
            require(isinstance(document,dict) and exact(document.get('editor'),WORKFLOW)
                    and exact(document.get('trigger'),TRIGGER),'Original raw document custody changed')
            values=(WORKFLOW['title'],WORKFLOW['description']) if stage=='editor' else (TITLE,DESCRIPTION)
            require((document.get('title'),document.get('description'))==values
                    and (model.get('title'),model.get('description'))==values,'Original editable draft changed')
            require(model.get('notice') == document.get('notice'), 'Draft notice differs from actual product model')
            for key in ('id','revision'): integer(document.get(key),'Invalid document '+key,1)
            identity=(product.get('visit'),document['id'])
            if original_identity is None: original_identity=identity
            require(identity==original_identity,'Original document/visit replaced')
            require(native.get('scope') == {'visit':str(identity[0]),'document':str(identity[1]),'revision':str(document['revision'])},
                    'Actual native scope differs from original document')
            require(document.get('pending') == (attempt_id if phases else None),'Attempt lock not attached to original draft')
            require(product.get('terminal') is terminal,'Visible terminal does not match original ACKs')
            if confirming:
                c=product['confirmation']
                require(c.get('current') is True and c.get('visit')==identity[0] and c.get('document')==identity[1]
                        and c.get('revision')==document['revision'] and c.get('title')==TITLE
                        and c.get('description')==DESCRIPTION and c.get('workflow')=='WF_REVIEW','Confirmation not bound to current draft')
                integer(c.get('id'),'Invalid original confirmation',1)
            if phases and finished:
                require(document.get('notice') == (DELETED if terminal else UNCONFIRMED),'Wrong retained deletion guidance')
        else:
            require(document is None,'Hidden original document projected into successor')
        native_proof(native,stage,model,rectangle,viewport,confirming=confirming,busy=busy,sealed=sealed)
        require(geometry.get('ok') is True and geometry.get('viewport')==viewport
                and geometry.get('noOverflow') is True,'Missing stage geometry')
        # Native measurements are the actual capture probe, not a second fabricated control inventory.
        for key in ('controls','feedback'):
            require(exact(geometry.get(key),native.get(key)),'Geometry/native measured controls differ')
        dimensions=geometry.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2
                and all(type(n)is int and n>0 for n in dimensions),'Missing actual PNG dimensions')
        path=Path(captures)/f'{fixture}-{stage}.png'
        require(paths[stage]==str(path),'Capture escaped owned stage')
        if png: validate_png(path,dimensions)
        if previous:
            old=previous['proof']['server']
            for key in ('requests','responses','settlements','releases'):
                require(exact(server[key][:len(old[key])],old[key]),'Original ledger history rewritten')
            require(exact(server['owner'],old['owner']),'Original server owner replaced')
        previous=row
    require(exact(receipt.get('final'),observation(rows[-1])),'Terminal differs from final capture')
    exercises(receipt,case,rows)


def observation(row):
    return dict(model=row['model'], delete=row['proof']['delete'], server=row['proof']['server'], native=row['proof']['native'])


def exercises(receipt, case, rows):
    bystage={r['stage']:r for r in rows}
    for row in rows:
        stage=row['stage'];e=row['proof'].get('exercise')
        require(isinstance(e,dict),'Missing actual action exercise')
        if stage=='cancelled':
            require(e.get('before')==observation(bystage['confirmation']) and e.get('after')==observation(row),
                    'Cancel lost original before/after')
            a,b=e['before'],e['after']
            require(exact(a['server'],b['server']) and a['delete']['attempts']==b['delete']['attempts']==[],
                    'Cancel dispatched or created attempt')
            old,new=a['delete']['document'],b['delete']['document']
            require(new['revision']>old['revision'] and
                    {k:v for k,v in old.items() if k!='revision'}=={k:v for k,v in new.items() if k!='revision'},
                    'Cancel altered original draft beyond release revision')
            require(a['native']['originalNodes'] is b['native']['originalNodes'] is True,
                    'Cancel replaced original native inputs')
            for field in ('title','description'):
                require({k:v for k,v in a['native']['fields'][field].items() if k!='readonly'} ==
                        {k:v for k,v in b['native']['fields'][field].items() if k!='readonly'},
                        'Cancel lost exact native buffer/selection')
        elif stage in ('trigger-ack','preflight-held','mutation-held','checked','checked-again'):
            integer(e.get('observedMs'),'Missing bounded observation',500)
            require(exact(e.get('before'),e.get('after')) and exact(e.get('after'),observation(row)),
                    'Held/no-replay state changed during bounded observation')
            if stage.startswith('checked'):
                require(e.get('activation')=={'ok':True,'id':'workflow-management-check','disabled':True,'sameNode':True},
                        'No-replay did not exercise original disabled Check')
                previous=rows[rows.index(row)-1]
                require(exact(row['proof']['server'],previous['proof']['server'])
                        and exact(row['proof']['delete'],previous['proof']['delete']), 'Check replayed or lost original attempt')
        elif stage=='deleted':
            require(e.get('release')==row['proof']['server']['releases'][-1], 'Missing original second-slot release')
        elif stage=='successor':
            require(e.get('before')==observation(rows[rows.index(row)-1]) and e.get('after')==observation(row),
                    'Missing actual retirement boundary')
            replacement=e.get('replacement')
            require(isinstance(replacement,dict) and replacement.get('before')==e['before']['model']
                    and replacement.get('after')==e['after']['model'],'Wrong actual account replacement')
            integer(replacement.get('previousEpoch'),'Missing original epoch')
            require(type(replacement.get('currentEpoch'))is int and replacement['currentEpoch']>replacement['previousEpoch']
                    and replacement.get('foreignDocumentHidden')is True,'Original owner was not retired')
            require(row['proof']['native'].get('originalDetached')is True,'Old native editor still connected')
        elif stage in ('late-refused','late-settled'):
            require(e.get('before')==observation(bystage['successor']) and e.get('after')==observation(row),
                    'Missing original late-response observations')
            a,b=e['before'],e['after']
            require(exact(a['model'],b['model']) and exact(a['native'],b['native'])
                    and e.get('successorUnchanged')is True,'Late original result altered successor UI/draft')
            require(exact(a['server']['requests'],b['server']['requests'])
                    and exact(a['server']['responses'],b['server']['responses']), 'Late original result dispatched more work')
            require(e.get('release')==b['server']['releases'][-1], 'Late release not original delivered receipt')
        else: require(e=={},'Unexpected unvalidated exercise')
    confirmation=bystage.get('reconfirmed',bystage['confirmation'])['proof']['delete']['confirmation']
    for row in rows:
        for attempt in row['proof']['delete']['attempts']:
            require(all(attempt[k]==confirmation[k] for k in ('id','visit','document','revision','title','description','workflow')),
                    'Attempt was not the exact admitted original confirmation')
        for request in row['proof']['server']['requests']:
            if request['phase']=='mutation':
                require(all(request['owner'].get(k)==confirmation[k] for k in ('visit','document','revision')),
                        'Mutation was not dispatched under original confirmation custody')
    if 'reconfirmed' in bystage:
        require(bystage['reconfirmed']['proof']['delete']['confirmation']['id'] !=
                bystage['confirmation']['proof']['delete']['confirmation']['id'], 'Cancelled confirmation identity reused')
    if 'successor' in bystage:
        require(receipt.get('replacement')==bystage['successor']['proof']['exercise']['replacement'],
                'Terminal account boundary changed')
    else: require(receipt.get('replacement') is None,'Unrequested account replacement')
