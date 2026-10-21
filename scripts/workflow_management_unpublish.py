"""Independent Unpublish closed-fixture oracle; no transport or app execution.

Publication application, returned ACKs, synchronization and readback are distinct
facts. In particular a matching readback never upgrades an unknown send.
"""
import copy
import json
from pathlib import Path

from workflow_management_delete import (
    TITLE, DESCRIPTION, POLICY, EDITOR, INITIAL, exact, integer, require, read_fields,
    WORKFLOW as BASE_WORKFLOW, TRIGGER as BASE_TRIGGER, read_result as base_read_result,
)

STAGES = {
    'unpublish-success': ('about', 'editor', 'draft', 'confirmation', 'cancelled',
                         'reconfirmed', 'primary-ack', 'confirmed', 'checked', 'back', 'reopened'),
    'unpublish-primary-rejected': ('about', 'editor', 'draft', 'confirmation',
                                   'primary-rejected', 'checked', 'checked-again'),
    'unpublish-primary-unknown': ('about', 'editor', 'draft', 'confirmation',
                                  'primary-unconfirmed', 'checked', 'checked-again'),
    'unpublish-sync-rejected': ('about', 'editor', 'draft', 'confirmation',
                                'sync-rejected', 'checked', 'checked-again'),
    'unpublish-sync-unknown': ('about', 'editor', 'draft', 'confirmation',
                               'sync-unconfirmed', 'checked', 'checked-again'),
    'unpublish-retirement': ('about', 'editor', 'draft', 'confirmation',
                              'primary-held', 'successor', 'late-settled'),
}
OWNER = {'account': None, 'epoch': 0, 'team': 'TWORKFLOW', 'user': 'UWORKFLOW'}
METHODS = ('functions.workflows.unpublish', 'workflows.triggers.update')
REASONS = ('unpublish-workflow', 'publish-workflow')
WORKFLOW = dict(copy.deepcopy(BASE_WORKFLOW), is_published=True,
                input_parameters={'keep': {}, 'removed': {}, 'custom': {}})
KEEP = {'value': 'literal source', 'opaque': {'preserve': [1, True, 'original']}}
AVAILABLE = {
    '{{data.custom}}': {'name': 'custom', 'type': 'string', 'title': 'Custom',
                        'is_customizable': True, 'description': 'Original custom input'},
    '{{data.channel}}': {'name': 'channel', 'type': 'slack#/types/channel_id',
                         'is_customizable': False, 'title': 'Channel'},
}
TRIGGER = dict(copy.deepcopy(BASE_TRIGGER),
               inputs={'keep': copy.deepcopy(KEEP), 'removed': {'value': 'remove'}, 'custom': False},
               available_data=AVAILABLE)
UNPUBLISHED = dict(copy.deepcopy(WORKFLOW), is_published=False, date_updated=2,
                   input_parameters={'keep': {}, 'custom': {}, 'channel': {}})
SYNC_INPUTS = {'keep': copy.deepcopy(KEEP), 'custom': {'customizable': True},
               'channel': {'value': '{{data.channel}}'}}
SYNC_TRIGGER = dict(copy.deepcopy(TRIGGER), inputs=SYNC_INPUTS)
LATEST_AVAILABLE = {'{{steps.STEP_REVIEW.result}}': {'type': 'string', 'name': 'result', 'opaque': 7}}
EXPRESSION = {'opaque': {'keep': [1, 'publication']}}
PRIMARY_RAW = {'ok': True, 'decorated_workflow': UNPUBLISHED, 'available_data': LATEST_AVAILABLE,
               'expression_data': EXPRESSION, 'opaque_receipt': {'preserve': [1, 'unpublish']}}
SYNC_RAW = {'ok': True, 'trigger': SYNC_TRIGGER, 'opaque_receipt': {'preserve': [2, 'synchronize']}}
SYNC_ARGUMENTS = {
    'trigger_id': 'TR_REVIEW', 'workflow': 'WF_REVIEW', 'type': 'shortcut',
    'name': BASE_TRIGGER['name'], 'description': BASE_TRIGGER['description'], 'inputs': SYNC_INPUTS,
    'shortcut': {'schema': {'custom': AVAILABLE['{{data.custom}}']}},
}
ARGUMENTS = ({'workflow_id': 'WF_REVIEW'}, SYNC_ARGUMENTS)


def result_for(case, index):
    rejected = (case == 'unpublish-primary-rejected' and index == 0 or
                case == 'unpublish-sync-rejected' and index == 1)
    unknown = (case == 'unpublish-primary-unknown' and index == 0 or
               case == 'unpublish-sync-unknown' and index == 1)
    return ({'raw': {'ok': False, 'error': 'not_allowed'}, 'error': None} if rejected else
            {'raw': None, 'error': 'transport'} if unknown else
            {'raw': copy.deepcopy((PRIMARY_RAW, SYNC_RAW)[index]), 'error': None})


def read_result(method, workflow, trigger):
    if method == 'functions.workflows.getLatest':
        return {'raw': {'ok': True, 'decorated_workflow': copy.deepcopy(workflow),
                        'available_data': LATEST_AVAILABLE, 'expression_data': EXPRESSION}, 'error': None}
    if method == 'workflows.triggers.info':
        return {'raw': {'ok': True, 'trigger': copy.deepcopy(trigger)}, 'error': None}
    if method == 'workflows.triggers.list':
        return {'raw': {'ok': True, 'triggers': [copy.deepcopy(trigger)]}, 'error': None}
    return base_read_result(method)


def expected_form(index):
    # Actual RequestPlan serializes sorted object keys and keeps opaque nested data.
    args = ARGUMENTS[index]
    fields = [[key, value if isinstance(value, str) else
               json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)]
              for key, value in sorted(args.items())]
    return fields + [['_x_reason', REASONS[index]]]


def decoded_form(fields):
    require(isinstance(fields, list) and all(isinstance(x, list) and len(x) == 2
            and all(isinstance(v, str) for v in x) for x in fields), 'Missing original form fields')
    require(len(fields) == len({x[0] for x in fields}) and 'token' not in dict(fields),
            'Repeated or secret form field')
    return dict(fields)


def stage_state(case, stage):
    initial = ('about', 'editor', 'draft', 'confirmation', 'cancelled', 'reconfirmed')
    if stage in initial:
        return dict(writes=0, phases=[], reads=[] if stage == 'about' else INITIAL,
                    held=False, retired=False, finished=False, complete=False, resolved=False, checked=False)
    retired = stage in ('successor', 'late-settled')
    held = stage in ('primary-ack', 'primary-held', 'successor')
    reads = INITIAL + POLICY + EDITOR
    checked = stage in ('checked', 'checked-again', 'back', 'reopened')
    if checked:
        reads += POLICY + EDITOR
        if stage == 'checked-again': reads += POLICY + EDITOR
        if stage == 'reopened': reads += INITIAL
    if case == 'unpublish-retirement':
        phases = ['acknowledged' if stage == 'late-settled' else 'in-flight']
    elif case == 'unpublish-success':
        phases = ['acknowledged', 'in-flight' if held else 'acknowledged']
    else:
        phases = {'unpublish-primary-rejected': ['rejected'], 'unpublish-primary-unknown': ['unknown'],
                  'unpublish-sync-rejected': ['acknowledged', 'rejected'],
                  'unpublish-sync-unknown': ['acknowledged', 'unknown']}[case]
    return dict(writes=len(phases), phases=phases, reads=reads, held=held, retired=retired,
                finished=not held, complete=case == 'unpublish-success' and not held,
                resolved=case == 'unpublish-success' and checked, checked=checked)


def server_proof(server, case, state):
    require(isinstance(server, dict) and exact(server.get('owner'), OWNER), 'Invalid original synthetic owner')
    require(exact(server.get('original'), {'workflow': WORKFLOW, 'trigger': TRIGGER}), 'Original raw source changed')
    require(server.get('ownerCurrent') is (not state['retired']), 'Original source owner currentness changed')
    require(type(server.get('networkCalls')) is int and server['networkCalls'] == 0, 'Unexpected network capability')
    ledgers = [server.get(k) for k in ('requests', 'responses', 'settlements', 'releases')]
    require(all(isinstance(x, list) for x in ledgers), 'Missing actual dispatch/response ledger')
    requests, responses, settlements, releases = ledgers
    methods = state['reads'][:23] + list(METHODS[:state['writes']]) + state['reads'][23:]
    require([r.get('method') for r in requests] == methods, 'Preflight/order/request count or no-replay changed')
    serials = list(range(1, len(requests) + 1))
    require([r.get('serial') for r in requests] == serials
            and [r.get('serial') for r in responses] == serials, 'Original serial ordering changed')
    held = len(requests) if state['held'] else None
    require(server.get('held') == held, 'Wrong held original dispatch')
    require(exact(settlements, [{'serial': n, 'returned': True} for n in serials if n != held]),
            'Original completion/retirement receipt changed')
    require(type(server.get('completed')) is int and server['completed'] == len(settlements)
            and type(server.get('cancelled')) is int and server['cancelled'] == 0, 'Wrong actual settlement counts')
    workflow, trigger = copy.deepcopy(WORKFLOW), copy.deepcopy(TRIGGER)
    mutations = []
    for request, response in zip(requests, responses):
        integer(request.get('serial'), 'Invalid original dispatch serial', 1)
        own = request.get('owner', {})
        require(exact({k: own.get(k) for k in OWNER}, OWNER), 'Foreign dispatch source owner')
        integer(own.get('visit'), 'Missing original visit', 1)
        require(type(request.get('applied')) is bool and type(request.get('retrySafe')) is bool,
                'Missing actual dispatch flags')
        require(exact(request.get('args'), decoded_form(request.get('fields'))), 'Original wire arguments changed')
        if request.get('phase') == 'mutation':
            index = len(mutations)
            require(index < 2 and request['method'] == METHODS[index]
                    and request['fields'] == expected_form(index) and request['retrySafe'] is False,
                    'Wrong publication form/reason/order/retry policy')
            result = result_for(case, index)
            applied = result['raw'] is None or result['raw'].get('ok') is True
            require(request['applied'] is applied, 'Server application confused with returned ACK')
            if index == 1:
                require(mutations[0]['serial'] in [x['serial'] for x in settlements]
                        and exact(result_for(case, 0), {'raw': PRIMARY_RAW, 'error': None}),
                        'Synchronization lacks delivered decorated primary ACK')
            if applied:
                if index == 0: workflow = copy.deepcopy(UNPUBLISHED)
                else: trigger = copy.deepcopy(SYNC_TRIGGER)
            mutations.append(request)
        else:
            require(request.get('phase') == 'read' and request['applied'] is False, 'Unexpected endpoint phase')
            read_fields(request['method'], request['args'])
            result = read_result(request['method'], workflow, trigger)
        require(exact(response, {'serial': request['serial'], 'result': result}), 'Original raw response changed')
    require(exact(server.get('current'), {'workflow': workflow, 'trigger': trigger}), 'Actual server state fabricated')
    expected_releases = 1 if case in ('unpublish-success', 'unpublish-retirement') and state['finished'] else 0
    require(len(releases) == expected_releases, 'Original held release lost or repeated')
    for release in releases:
        serial = 25 if case == 'unpublish-success' else 24
        require(exact(release, {'serial': serial, 'receiverRetired': False, 'delivered': True,
                               'response': responses[serial-1]}), 'Release did not deliver the original held response')
    return mutations


def slot_proof(slot, index, method, arguments, outcome, raw, failure=None):
    require(isinstance(slot, dict) and type(slot.get('index')) is int and slot['index'] == index
            and slot.get('method') == method and exact(slot.get('arguments'), arguments),
            'Wrong original ordered publication slot')
    require(slot.get('outcome') == outcome and exact(slot.get('raw'), raw)
            and slot.get('failure') == failure, 'Publication ACK/Unknown/raw custody changed')
    require(slot.get('prerequisite') is None, 'Invented cross-plan prerequisite')


def attempt_proof(attempt, case, state, current_server):
    require(isinstance(attempt, dict), 'Missing original publication attempt')
    for key in ('id', 'visit', 'document', 'revision'):
        integer(attempt.get(key), 'Invalid original attempt ' + key, 1)
    require(attempt.get('title') == TITLE and attempt.get('description') == DESCRIPTION
            and attempt.get('workflow') == 'WF_REVIEW', 'Original unpublication draft changed')
    phases = state['phases']
    for key in ('finished', 'complete', 'resolved'):
        require(attempt.get(key) is state[key], 'Wrong operation ' + key)
    require(attempt.get('noSend') is False and attempt.get('current') is (not state['finished'] and not state['retired'])
            and attempt.get('primarySucceeded') is (phases[0] == 'acknowledged'), 'Wrong primary/current operation facts')
    slots = attempt.get('slots')
    require(isinstance(slots, list) and len(slots) == 1, 'Primary must have exactly one original slot')
    result = result_for(case, 0) if phases[0] != 'in-flight' else {'raw': None, 'error': None}
    slot_proof(slots[0], 0, METHODS[0], ARGUMENTS[0], phases[0], result['raw'],
               'delivery unconfirmed' if phases[0] == 'unknown' else None)
    followup = attempt.get('followup')
    if phases[0] != 'acknowledged':
        require(followup is None, 'Failed/unknown primary granted synchronization authority')
    else:
        require(isinstance(followup, dict), 'Validated primary lost derived synchronization plan')
        sync_phase = phases[1] if len(phases) == 2 else 'pending'
        complete = sync_phase in ('acknowledged', 'rejected')
        require(followup.get('phase') == ('Complete' if complete else 'TriggerInputSynchronization')
                and followup.get('fullySucceeded') is (sync_phase == 'acknowledged'),
                'Complete/rejected/unknown synchronization distinction changed')
        require(exact(followup.get('workflow'), UNPUBLISHED)
                and exact(followup.get('trigger'), SYNC_TRIGGER if sync_phase == 'acknowledged' else TRIGGER),
                'Derived plan source/retained trigger changed')
        stages = followup.get('stages')
        require(isinstance(stages, list) and len(stages) == 1
                and stages[0].get('phase') == 'TriggerInputSynchronization'
                and isinstance(stages[0].get('slots'), list) and len(stages[0]['slots']) == 1,
                'Missing or invented source-derived synchronization stage')
        result = result_for(case, 1) if sync_phase not in ('pending', 'in-flight') else {'raw': None, 'error': None}
        slot_proof(stages[0]['slots'][0], 0, METHODS[1], ARGUMENTS[1], sync_phase, result['raw'],
                   'delivery unconfirmed' if sync_phase == 'unknown' else None)
    expected_readback = None
    if state['checked']:
        expected_readback = dict(copy.deepcopy(current_server), matches=case in ('unpublish-success', 'unpublish-sync-unknown'))
    require(exact(attempt.get('readback'), expected_readback), 'Canonical readback changed or replaced ACK evidence')


def measured_native(native, geometry, viewport, rectangle):
    require(isinstance(native, dict) and native.get('ok') is True
            and native.get('noOverflow') is True and native.get('viewport') == viewport,
            'Unproven original native layout')
    controls, feedback = native.get('controls'), native.get('feedback')
    require(isinstance(controls, list) and controls and isinstance(feedback, list), 'Missing native measurements')
    ids = [c.get('id') for c in controls]
    require(len(ids) == len(set(ids)) and all(isinstance(i, str) and i for i in ids),
            'Duplicate or invalid original control identity')
    for item in controls + feedback:
        require(item.get('visible') is True and item.get('hit') is True
                and type(item.get('disabled')) is bool, 'Actual control/feedback hidden or occluded')
        rectangle(item.get('rect'), viewport)
    require(native.get('composing') is False, 'Unsettled original native composition')
    require(geometry.get('ok') is True and geometry.get('noOverflow') is True
            and geometry.get('viewport') == viewport, 'Missing actual captured geometry')
    for key in native:
        require(exact(geometry.get(key), native[key]), 'Capture/native measurement changed: ' + key)
    dimensions = geometry.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2
            and all(type(n) is int and n > 0 for n in dimensions), 'Invalid native PNG dimensions')
    return {c['id']: c for c in controls}


def native_fields(native, title, description, locked):
    require(native.get('originalNodes') is True and native.get('originalDetached') is False,
            'Original publication draft controls replaced')
    fields = native.get('fields')
    require(isinstance(fields, dict), 'Missing original native fields')
    for key, value in [('title', title), ('description', description)]:
        field = fields.get(key, {})
        require(field.get('value') == value and field.get('readonly') is locked,
                'Original dirty buffer or editing lock changed')
        for bound in ('start', 'end'):
            integer(field.get(bound), 'Missing original selection')
        require(field['start'] <= field['end'] <= len(value.encode('utf-16-le')) // 2,
                'Original UTF16 selection out of bounds')


def observation(row):
    return dict(model=row['model'], unpublish=row['proof']['unpublish'],
                server=row['proof']['server'], native=row['proof']['native'])


WARNING = ('When you unpublish this workflow, it will stop working for anyone who uses it. '
           'Shared links will not start the workflow unless it is published again. Your local draft will be retained.')
UNCONFIRMED = 'Unpublication is not fully confirmed. Your draft and each response are retained. No operation will be replayed.'
CONFIRMED = 'Unpublication and trigger synchronization acknowledged. Check the current workflow to verify its state; your local draft is retained.'
VERIFIED = 'Workflow is unpublished and trigger synchronization is verified. Your local draft is retained.'
REOPENED = 'Current workflow loaded for review. Your proposed changes are retained.'
OBSERVED = 'Current workflow is unpublished. The operation remains unconfirmed or incomplete; no request will be replayed. Your draft is retained.'
NOT_OBSERVED = 'Current workflow does not confirm unpublication. Each response and your draft are retained; no request will be replayed.'


def native_controls(native, controls, stage, route, *, confirming, busy, sealed, resolved):
    require(native.get('leasePresent') is (confirming or busy), 'Original native lease lifecycle changed')
    if route != 'editor':
        require(native.get('fields') is None and native.get('originalNodes') is None
                and native.get('confirmation') is None, 'Hidden original editor leaked into successor')
        require('workflow-configuration' in controls, 'Actual About Configuration control missing')
        return
    title, description = (WORKFLOW['title'], WORKFLOW['description']) if stage == 'editor' else (TITLE, DESCRIPTION)
    native_fields(native, title, description, confirming or busy or sealed)
    for name, disabled in [('save', stage == 'editor' or confirming or busy or sealed),
                           ('check', confirming or busy), ('managed', confirming or busy),
                           ('delete', confirming or busy or sealed)]:
        control = controls.get('workflow-management-' + name)
        require(isinstance(control, dict) and control.get('disabled') is disabled,
                'Wrong actual action availability: ' + name)
    if resolved:
        require('workflow-management-unpublish' not in controls, 'Unpublished baseline exposes Unpublish')
    else:
        control = controls.get('workflow-management-unpublish')
        require(isinstance(control, dict) and control.get('disabled') is (confirming or busy or sealed),
                'Wrong actual Unpublish availability')
    require(not any('publish' in k and k not in ('workflow-management-unpublish', 'workflow-unpublish-cancel',
                'workflow-unpublish-confirm') for k in controls), 'Unrequested Publish action exposed')
    confirmation = native.get('confirmation')
    if confirming:
        require(isinstance(confirmation, dict) and confirmation.get('id') == 'workflow-unpublish-confirmation'
                and 'Unpublish this workflow?' in confirmation.get('text', '') and WARNING in confirmation['text'],
                'Actual unpublication warning missing')
        require(confirmation.get('visible') is True and confirmation.get('hit') is True, 'Confirmation obscured')
        actions = confirmation.get('actions')
        require(actions == [controls.get('workflow-unpublish-cancel'), controls.get('workflow-unpublish-confirm')]
                and all(a and a['disabled'] is False for a in actions)
                and [a['text'] for a in actions] == ['Cancel', 'Unpublish'], 'Original confirmation actions changed')
    else:
        require(confirmation is None, 'Unexpected live Unpublish confirmation')


def validate(fixture, case, receipt, captures, *, viewport, rectangle, validate_png, png=True):
    require(case in STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Wrong Unpublish fixture/case')
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('scenario') == 'editor' and receipt.get('unpublishCase') == case,
            'Wrong or failed Unpublish receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'syntheticSession', 'noLiveNetwork')),
            'Missing actual entry/synthetic isolation')
    require(receipt.get('viewport') == viewport, 'Wrong actual viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[case]), 'Wrong Unpublish stage order')
    require(isinstance(paths, dict) and set(paths) == set(STAGES[case]), 'Wrong original capture inventory')
    previous, identity, attempt_id = None, None, None
    for row in rows:
        stage, model, geometry, proof = (row.get(k) for k in ('stage', 'model', 'geometry', 'proof'))
        require(all(isinstance(v, dict) for v in (model, geometry, proof)), 'Missing stage model/proof')
        product, server, native = (proof.get(k) for k in ('unpublish', 'server', 'native'))
        require(isinstance(product, dict), 'Missing actual publication observer')
        state = stage_state(case, stage)
        server_proof(server, case, state)
        route = 'about' if stage in ('about', 'back') or state['retired'] else 'editor'
        busy, confirming = state['held'] and not state['retired'], stage in ('confirmation', 'reconfirmed')
        sealed = bool(state['phases']) and not state['resolved']
        for key in ('reads', 'writes', 'networkCalls', 'retainedDocuments', 'retainedAttempts'):
            integer(model.get(key), 'Invalid actual model ' + key)
        require(model['reads'] == len(state['reads']) and model['writes'] == state['writes']
                and model['networkCalls'] == 0 and model['retainedAttempts'] == 0,
                'Actual model differs from publication ledger')
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
            outcomes = ['Unpublish: ' + labels[state['phases'][0]]]
            if len(state['phases']) == 2:
                outcomes.append('Trigger synchronization: ' + labels[state['phases'][1]])
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
                    and (model.get('title'), model.get('description')) == expected, 'Unpublish implicitly saved/replaced dirty metadata')
            require(exact(document.get('editor'), UNPUBLISHED if state['resolved'] else WORKFLOW)
                    and exact(document.get('trigger'), SYNC_TRIGGER if state['resolved'] else TRIGGER),
                    'Publication baseline adopted without confirmed matching readback')
            require(document.get('pending') == (attempt_id if sealed else None)
                    and document.get('editable') is (not confirming and not busy and not sealed), 'Unsafe draft unlocking')
            notice = None
            if state['finished']:
                notice = (REOPENED if stage == 'reopened' else VERIFIED if state['resolved'] else
                          NOT_OBSERVED if state['checked'] and case == 'unpublish-primary-rejected' else
                          OBSERVED if state['checked'] else CONFIRMED if state['complete'] else UNCONFIRMED)
            require(document.get('notice') == model.get('notice') == notice, 'Wrong operation/readback guidance')
        else:
            require(document is None and model.get('title') is None and model.get('description') is None,
                    'Hidden draft projected into successor')
        confirmation = product.get('confirmation')
        if confirming:
            require(isinstance(confirmation, dict) and confirmation.get('current') is True
                    and all(confirmation.get(k) == v for k, v in dict(visit=product['visit'], document=document['id'],
                        revision=document['revision'], title=TITLE, description=DESCRIPTION, workflow='WF_REVIEW').items()),
                    'Confirmation lacks exact original draft custody')
            integer(confirmation.get('id'), 'Missing original confirmation id', 1)
        else: require(confirmation is None, 'Unexpected confirmation outside original admission')
        controls = measured_native(native, geometry, viewport, rectangle)
        native_controls(native, controls, stage, route, confirming=confirming, busy=busy, sealed=sealed, resolved=state['resolved'])
        if confirming: rectangle(native['confirmation'].get('rect'), viewport)
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
    exercises(receipt, case, rows)


def exercises(receipt, case, rows):
    bystage = {r['stage']: r for r in rows}
    for index, row in enumerate(rows):
        stage = row['stage']; exercise = row['proof'].get('exercise')
        require(isinstance(exercise, dict), 'Missing original action exercise')
        current = observation(row)
        if stage in ('primary-ack', 'primary-held'):
            integer(exercise.get('observedMs'), 'Missing bounded held observation', 500)
            require(exact(exercise.get('before'), current) and exact(exercise.get('after'), current),
                    'Held original changed before release')
        elif stage == 'confirmed':
            require(exact(exercise, {'release': current['server']['releases'][-1]}), 'Missing actual synchronization release')
        elif stage in ('cancelled', 'checked', 'checked-again', 'back'):
            before = observation(rows[index-1])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current),
                    'Action before/after not original captured observations')
            if stage == 'cancelled':
                require(exact(before['server'], current['server']) and before['unpublish']['attempts'] == []
                        and current['unpublish']['attempts'] == [], 'Cancel dispatched publication work')
                a, b = before['unpublish']['document'], current['unpublish']['document']
                require(b['revision'] > a['revision'] and exact(
                    {k:v for k,v in a.items() if k not in ('revision','editable')},
                    {k:v for k,v in b.items() if k not in ('revision','editable')}), 'Cancel replaced dirty document')
                for key in ('title','description'):
                    require(exact({k:v for k,v in before['native']['fields'][key].items() if k != 'readonly'},
                                  {k:v for k,v in current['native']['fields'][key].items() if k != 'readonly'}),
                            'Cancel altered original native buffer/selection')
            elif stage == 'back':
                require(exact(before['server'], current['server']) and current['native'].get('originalDetached') is True
                        and exact(before['unpublish']['attempts'], current['unpublish']['attempts']),
                        'Back dispatched or lost retained operation')
            else:
                require(current['model']['reads'] == before['model']['reads'] + 11
                        and current['model']['writes'] == before['model']['writes'], 'Check replayed or lacked fresh read')
                a, b = before['unpublish']['attempts'][0], current['unpublish']['attempts'][0]
                require(exact({k:v for k,v in a.items() if k not in ('resolved','readback')},
                              {k:v for k,v in b.items() if k not in ('resolved','readback')}),
                        'Readback rewrote original operation receipt')
                require(exact(before['server']['current'], current['server']['current']), 'Check applied a mutation')
                require(current['unpublish']['document']['revision'] > before['unpublish']['document']['revision'],
                        'Readback not a newly owned observation')
        elif stage == 'reopened':
            before = observation(bystage['checked'])
            require(exact(exercise.get('before'), before) and exact(exercise.get('after'), current)
                    and exact(exercise.get('remount'), {'ok': True, 'originalDetached': True, 'newRoot': True,
                                                      'newTitle': True, 'newDescription': True}),
                    'Reopen lacks actual old-detached/new-node evidence')
            require(exact(before['unpublish']['attempts'], current['unpublish']['attempts'])
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
    confirmation = bystage.get('reconfirmed', bystage['confirmation'])['proof']['unpublish']['confirmation']
    for row in rows:
        for attempt in row['proof']['unpublish']['attempts']:
            require(all(attempt[k] == confirmation[k] for k in ('id','visit','document','revision','title','description','workflow')),
                    'Attempt was not the original admitted confirmation')
        for request in row['proof']['server']['requests']:
            if request['phase'] == 'mutation':
                require(all(request['owner'].get(k) == confirmation[k] for k in ('visit','document','revision')),
                        'Dispatch escaped original confirmation custody')
    if 'reconfirmed' in bystage:
        require(confirmation['id'] != bystage['confirmation']['proof']['unpublish']['confirmation']['id'],
                'Cancelled confirmation identity reused')
    expected_replacement = bystage['successor']['proof']['exercise']['replacement'] if 'successor' in bystage else None
    require(exact(receipt.get('replacement'), expected_replacement), 'Terminal account replacement changed')
