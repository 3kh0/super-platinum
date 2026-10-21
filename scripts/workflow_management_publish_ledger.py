"""Closed Publish request/reply ledger checks; never substitutes for native stages."""
import json
import copy
from workflow_management_delete import read_result as base_read_result, exact
from workflow_management_unpublish import WORKFLOW as OLD_WORKFLOW, TRIGGER as OLD_TRIGGER, LATEST_AVAILABLE, EXPRESSION, SYNC_INPUTS, SYNC_ARGUMENTS

WORKFLOW, TRIGGER = copy.deepcopy(OLD_WORKFLOW), copy.deepcopy(OLD_TRIGGER)
WORKFLOW.update(id='WfREVIEW01', last_published_version_id='V1', unpublished_change_count=2)
WORKFLOW['steps'][0]['function'].update(id='FnSTATIC', callback_id='noop', type='builtin', input_parameters=[])
WORKFLOW['steps'][0]['errors'] = []
TRIGGER['workflow']['workflow_id'] = WORKFLOW['id']
TRIGGER['trigger_type_id'] = 'TT_SHORTCUT'
PUBLISHED = dict(copy.deepcopy(WORKFLOW), last_published_version_id='V2', unpublished_change_count=0,
                 date_updated=2, input_parameters={'keep': {}, 'custom': {}, 'channel': {}})
SYNCED = dict(copy.deepcopy(TRIGGER), inputs=copy.deepcopy(SYNC_INPUTS))
REPUBLISH = {
    'publish-republish-same-version': 'publish-success',
    'publish-republish-new-version': 'publish-success',
    'publish-republish-changed': 'publish-success',
    'publish-republish-unknown': 'publish-primary-unknown',
    'publish-republish-stale': 'publish-stale-readback',
    'publish-republish-retirement': 'publish-retirement',
}


def scenario(case):
    return REPUBLISH.get(case, case)


def original_workflow(case):
    value = copy.deepcopy(WORKFLOW)
    if case in REPUBLISH:
        value.update(is_published=False, unpublished_change_count=2 if case == 'publish-republish-changed' else 0)
    return value


def published_workflow(case):
    value = copy.deepcopy(PUBLISHED)
    if case in ('publish-republish-same-version', 'publish-republish-stale'):
        value['last_published_version_id'] = 'V1'
    return value


def validate(server, require, case):
    behavior = scenario(case)
    initial, published = original_workflow(case), published_workflow(case)
    require(isinstance(server, dict) and server.get('networkCalls') == 0, 'Publish network isolation')
    require(exact(server.get('owner'), {'account': None, 'epoch': 0, 'team': 'TWORKFLOW', 'user': 'UWORKFLOW'}), 'Publish original account owner')
    original = server.get('original', {})
    workflow, trigger = original.get('workflow', {}), original.get('trigger', {})
    require(workflow.get('id') == 'WfREVIEW01' and workflow.get('is_published') is initial['is_published']
            and workflow.get('last_published_version_id') == 'V1'
            and workflow.get('unpublished_change_count') == initial['unpublished_change_count'], 'Original published version')
    require(exact(original, {'workflow': initial, 'trigger': TRIGGER}), 'Original acquired Publish payload changed')
    requests, responses = server.get('requests'), server.get('responses')
    settlements, releases = server.get('settlements'), server.get('releases')
    require(all(isinstance(v, list) for v in (requests, responses, settlements, releases)), 'Publish ledger arrays')
    require([r.get('serial') for r in requests] == list(range(1, len(requests) + 1)), 'Publish request serials')
    require([r.get('serial') for r in responses] == list(range(1, len(requests) + 1)), 'Publish response serials')
    settled = [r.get('serial') for r in settlements]
    require(len(set(settled)) == len(settled) and set(settled) <= set(range(1, len(requests) + 1)), 'Publish settlement ownership')
    require(all(r.get('returned') is True for r in settlements), 'Publish original delivered settlement')
    held = server.get('held')
    require(held is None or type(held) is int and held == len(requests), 'Publish held serial')
    require(set(settled) == set(range(1, len(requests) + 1)) - ({held} if held is not None else set()), 'Publish original pending receiver')
    require(server.get('completed') == len(settlements)
            and server.get('cancelled') == sum(r['returned'] is False for r in settlements), 'Publish completion counts')
    methods = []
    current_workflow, current_trigger = copy.deepcopy(initial), copy.deepcopy(TRIGGER)
    certified_reads = 0
    for row in requests:
        fields = row.get('fields')
        require(isinstance(fields, list) and all(isinstance(f, list) and len(f) == 2 for f in fields), 'Publish fields')
        keys = [f[0] for f in fields]
        require(len(set(keys)) == len(keys) and 'token' not in keys, 'Publish duplicate/secret fields')
        args = dict(fields)
        require(args == row.get('args'), 'Publish argument ledger')
        method = row.get('method')
        own = row.get('owner', {})
        require(exact({k: own.get(k) for k in ('account', 'epoch', 'team', 'user')}, server['owner'])
                and type(own.get('visit')) is int and own['visit'] > 0, 'Publish dispatch original owner')
        require(type(row.get('retrySafe')) is bool, 'Publish retry observation')
        if row.get('phase') == 'mutation':
            require(row.get('retrySafe') is False, 'Publish mutation retry flag')
            require(method in ('functions.workflows.publish', 'workflows.triggers.update'), 'Foreign Publish mutation')
            require(args.get('_x_reason') == 'publish-workflow', 'Publish mutation reason')
            if method == 'functions.workflows.publish':
                require(set(args) == {'workflow_id', '_x_reason'} and args['workflow_id'] == workflow['id'], 'Publish primary wire')
            else:
                require(methods == ['functions.workflows.publish'], 'Sync before primary')
                require(args.get('trigger_id') == trigger['id'], 'Publish sync identity')
                expected = {'keep': trigger['inputs']['keep'], 'custom': {'customizable': True},
                            'channel': {'value': '{{data.channel}}'}}
                require(json.loads(args.get('inputs', 'null')) == expected, 'Publish derived sync inputs')
                exact_args = dict(SYNC_ARGUMENTS, workflow='WfREVIEW01')
                wire = {k: v if isinstance(v, str) else json.dumps(v, sort_keys=True, separators=(',', ':'), ensure_ascii=False) for k, v in exact_args.items()}
                wire['_x_reason'] = 'publish-workflow'
                require(exact(args, wire), 'Publish complete synchronization wire')
            index = len(methods)
            rejected = behavior == ('publish-primary-rejected' if index == 0 else 'publish-sync-rejected')
            lost = behavior == ('publish-primary-unknown' if index == 0 else 'publish-sync-unknown')
            require(row.get('applied') is (not rejected), 'Publish applied/ACK distinction')
            if not rejected:
                if index == 0: current_workflow = copy.deepcopy(published)
                else: current_trigger = copy.deepcopy(SYNCED)
            if rejected: expected = {'raw': {'ok': False, 'error': 'not_allowed'}, 'error': None}
            elif lost: expected = {'raw': None, 'error': 'transport'}
            else:
                raw = ({'ok': True, 'decorated_workflow': published, 'available_data': LATEST_AVAILABLE,
                        'expression_data': EXPRESSION, 'opaque_receipt': {'preserve': [1, 'publish']}}
                       if index == 0 else {'ok': True, 'trigger': SYNCED, 'opaque_receipt': {'preserve': [2, 'synchronize']}})
                expected = {'raw': raw, 'error': None}
            methods.append(method)
        else:
            require(row.get('phase') == 'read' and row.get('applied') is False, 'Publish read mutation')
            if method in ('apps.certified.list', 'workflows.triggers.types.list'):
                reason = 'fetch-connector-apps' if method == 'apps.certified.list' else 'fetching-workflow-trigger-types-list-wfb'
                require(args == {'_x_reason': reason}, 'Publish acquired-facts wire')
            if method == 'apps.certified.list':
                certified_reads += 1
                raw = ({'ok': True, 'apps': None} if case == 'publish-validation-recovery' and certified_reads == 1 else
                       {'ok': True, 'apps': [], 'is_aaa_enabled': False, 'can_user_manage_app': True})
            elif method == 'workflows.triggers.types.list':
                raw = {'ok': True, 'trigger_types': [{'trigger_type': {'id': 'TT_SHORTCUT', 'type': 'shortcut'},
                       'is_available_to_user': case != 'publish-blocked', 'private_channel_access': False, 'private_channel_team_ids': []}]}
            elif method == 'functions.workflows.getLatest':
                observed = copy.deepcopy(current_workflow)
                if behavior == 'publish-stale-readback' and methods:
                    if case == 'publish-republish-stale': observed['is_published'] = False
                    else: observed.update(last_published_version_id='V1', unpublished_change_count=2)
                raw = {'ok': True, 'decorated_workflow': observed, 'available_data': LATEST_AVAILABLE, 'expression_data': EXPRESSION}
            elif method == 'workflows.triggers.info': raw = {'ok': True, 'trigger': current_trigger}
            elif method == 'workflows.triggers.list': raw = {'ok': True, 'triggers': [current_trigger]}
            elif method == 'functions.workflows.list':
                raw = {'ok': True, 'workflows': [current_workflow], 'response_metadata': {'next_cursor': ''}}
            else: raw = base_read_result(method)['raw']
            expected = {'raw': raw, 'error': None}
        require(exact(responses[row['serial'] - 1]['result'], expected), 'Actual Publish read/write response changed')
    require(exact(server.get('current'), {'workflow': current_workflow, 'trigger': current_trigger}), 'Actual Publish server state changed')
    require(len(methods) == len(set(methods)), 'Publish mutation replay')
    for release in releases:
        serial = release.get('serial')
        require(type(serial) is int and 1 <= serial <= len(responses), 'Publish release original serial')
        require(release.get('response') == responses[serial - 1], 'Publish release response ownership')
        require(type(release.get('receiverRetired')) is bool and type(release.get('delivered')) is bool,
                'Publish release disposition')
        require(release['receiverRetired'] != release['delivered'], 'Publish retired receiver delivered')
    return methods
