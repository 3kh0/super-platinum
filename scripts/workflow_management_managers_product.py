"""Independent product receipt checks: raw ACKs never stand in for readback."""
import copy
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION

ADD = 'functions.workflows.collaborators.add'
REMOVE = 'functions.workflows.collaborators.remove'
COPY = 'functions.workflows.permissions.copy.set'
RUN = 'workflows.triggers.permissions.set'
RESTRICTION = 'functions.workflows.getIsRestricted'


def operation(server, method):
    requests = [r for r in server['requests'] if r['method'] == method and r['phase'] in ('mutation', 'followup')]
    require(len(requests) <= 1, 'Original operation replayed')
    if not requests:
        return 'pending', None, None
    request = requests[0]
    serial = request['serial']
    if not any(s['serial'] == serial for s in server['settlements']):
        return 'in-flight', None, None
    response = next(r['result'] for r in server['responses'] if r['serial'] == serial)
    raw, error = response['raw'], response['error']
    if error:
        return ('not-sent', None, 'refused before dispatch') if error == 'offline' else ('unknown', None, 'delivery unconfirmed')
    return ('acknowledged' if raw.get('ok') is True else 'rejected'), raw, None


def slot(slot, index, method, arguments, server, prerequisite=None):
    phase, raw, failure = operation(server, method)
    require(exact(slot, dict(index=index, method=method, arguments=arguments, prerequisite=prerequisite,
                             outcome=phase, raw=raw, failure=failure)),
            'Original plan, raw ACK, delivery or dependency changed')


def attempt_proof(attempt, state, server, selected, desired_run):
    original = server['original']
    for key in ('id', 'visit', 'document', 'revision'):
        integer(attempt.get(key), 'Missing original attempt ' + key, 1)
    require(attempt.get('title') == TITLE and attempt.get('description') == DESCRIPTION
            and attempt.get('workflow') == 'WF_REVIEW'
            and exact(attempt.get('originalCopy'), original['copy']), 'Attempt lost admitted metadata')
    for key in ('finished', 'complete', 'resolved'):
        require(attempt.get(key) is state[key], 'Wrong manager attempt ' + key)
    require(attempt.get('noSend') is False and attempt.get('current') is
            (not state['finished'] and not state['retired'] and not state['departed']),
            'Intentional departure/retirement confused with current editor authority')
    self_removal = 'UWORKFLOW' not in selected
    require(exact(attempt.get('departure'), dict(armed=self_removal, departed=state['departed'], revoked=False)),
            'Self-removal departure token forged or lost')
    group = attempt.get('managerGroup')
    require(isinstance(group, dict) and group.get('selected') == selected
            and group.get('selfRemoval') is self_removal, 'Manager plan selection changed')
    original_ids = original['workflow']['collaborators']
    # The fixture contract uses actual collaborator ID strings, with order retained.
    added = [v for v in selected if v not in original_ids]
    removed = [v for v in original_ids if v not in selected]
    slots = group.get('slots')
    require(isinstance(slots, list) and len(slots) == 2 and added and removed,
            'Changed-only add/remove plan missing')
    slot(slots[0], 0, ADD, {'workflow_id': 'WF_REVIEW', 'collaborator_ids': added}, server)
    slot(slots[1], 1, REMOVE, {'workflow_id': 'WF_REVIEW', 'collaborator_ids': removed}, server, 0)
    require(group.get('complete') is all(s['outcome'] == 'acknowledged' for s in slots),
            'Rejected/unknown manager group claimed complete')
    copy_changed = state['copy_changed']
    require(attempt.get('copyChanged') is copy_changed
            and attempt.get('desired') == ('everyone' if copy_changed else 'collaborators')
            and attempt.get('primarySucceeded') is copy_changed, 'Copy group changed or ACK fabricated')
    copies = attempt.get('slots')
    require(isinstance(copies, list) and len(copies) == 1, 'Lost original copy plan')
    slot(copies[0], 0, COPY, {'workflow_id': 'WF_REVIEW', 'permission_type': attempt['desired']}, server)
    run = attempt.get('runGroup')
    if not state['run_changed']:
        require(run is None, 'Unchanged run group dispatched')
    else:
        require(isinstance(run, dict), 'Independent run group lost')
        phase, raw, failure = operation(server, RUN)
        arguments = {'trigger_id': 'TR_REVIEW', **{k: v for k, v in desired_run.items() if k != 'ok' and v != []}}
        require(exact(run.get('primary'), dict(method=RUN, arguments=arguments, outcome=phase, raw=raw, failure=failure)),
                'Original run response lost across manager departure')
        follow = run.get('followup')
        if phase == 'acknowledged':
            fphase, fraw, ffailure = operation(server, RESTRICTION)
            complete = fphase == 'acknowledged'
            require(exact(follow, dict(phase='Complete' if complete else 'Restriction', fullySucceeded=complete,
                                      restriction=fraw, slots=[dict(method=RESTRICTION,
                                      arguments={'workflow_id': 'WF_REVIEW'}, outcome=fphase, raw=fraw, failure=ffailure)])),
                    'Original run follow-up changed or was dispatched without ACK')
            require(run.get('complete') is complete, 'Wrong run completion')
        else:
            require(follow is None and run.get('complete') is False, 'Unacknowledged run authorized follow-up')
    readback = None
    if state['checked']:
        readback = {k: copy.deepcopy(v) for k, v in server['current'].items()
                    if k in ('workflow', 'trigger', 'copy', 'run', 'external')}
        readback['matches'] = state['resolved']
    require(exact(attempt.get('readback'), readback), 'Fresh observation replaced or fabricated original ACK')


def retained_proof(value, original, current, document_id, pending, resolved, title, description):
    require(isinstance(value, dict), 'Original retained document missing')
    integer(value.get('revision'), 'Missing retained revision', 1)
    require(value.get('id') == document_id and value.get('title') == title
            and value.get('description') == description and value.get('pending') == pending,
            'Original metadata draft identity/content changed')
    require(exact(value.get('editor'), current['workflow'] if resolved else original['workflow'])
            and exact(value.get('trigger'), original['trigger']), 'Retained baseline changed without verified readback')
