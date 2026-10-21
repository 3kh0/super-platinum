"""Measured native controls for the closed manager-permissions cases."""
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION
from workflow_management_unpublish import native_fields


def measured(item, viewport, rectangle, *, disabled=None):
    require(isinstance(item, dict) and item.get('visible') is True and item.get('hit') is True,
            'Manager control is absent, clipped or covered')
    if disabled is not None:
        require(item.get('disabled') is disabled, 'Wrong manager control availability')
    rectangle(item.get('rect'), viewport)


def settings(native, controls, confirmation, stage, viewport, rectangle):
    panel = native.get('settings')
    require(isinstance(panel, dict) and panel.get('id') == 'workflow-permissions'
            and panel.get('title') == 'Permissions' and panel.get('originalControls') is True,
            'Permissions lost original native controls')
    label, choice = panel.get('label'), panel.get('copy')
    measured(label, viewport, rectangle, disabled=False)
    measured(choice, viewport, rectangle, disabled=False)
    require(label.get('text') == 'Who can copy this workflow?'
            and choice.get('id') == 'workflow-permissions-copy'
            and choice.get('value') == confirmation.get('selected'), 'Wrong copy selection')
    require(exact(choice.get('options'), [
        {'value': 'everyone', 'text': 'Everyone in Home fixture', 'disabled': False},
        {'value': 'collaborators', 'text': 'Workflow managers only', 'disabled': False}]),
        'Copy options changed')
    draft = confirmation.get('managerDraft')
    require(isinstance(draft, dict), 'Missing acquired manager draft')
    integer(draft.get('epoch'), 'Missing manager epoch', 1)
    integer(draft.get('generation'), 'Missing manager generation', 1)
    managers = panel.get('managers')
    require(isinstance(managers, dict) and managers.get('originalControls') is True
            and managers.get('epoch') == str(draft['epoch']), 'Manager native identity changed')
    query = managers.get('query')
    measured(query, viewport, rectangle, disabled=False)
    require(query.get('id') == 'workflow-permissions-manager-query'
            and query.get('value') == draft.get('query'), 'Manager query buffer differs')
    for name, key in [('selected', 'ids'), ('results', 'results')]:
        rows = managers.get(name)
        require(isinstance(rows, list) and [r.get('target') for r in rows] == draft.get(key),
                'Manager native rows differ from acquired draft')
        for row in rows:
            measured(row, viewport, rectangle, disabled=draft['pending'] or draft['speedBump'])
    feedback = managers.get('feedback')
    require(isinstance(feedback, list), 'Missing manager feedback observations')
    expected = ([] if draft['ids'] else ['You must provide at least 1 workflow manager'])
    if draft['pending']:
        expected.append('Loading workflow managers\u2026')
    if draft['notice']:
        expected.append(draft['notice'])
    require([v.get('text') for v in feedback] == expected, 'Manager validation/loading feedback changed')
    for item in feedback:
        measured(item, viewport, rectangle)
    bump = managers.get('speedBump')
    if draft['speedBump']:
        require(isinstance(bump, dict)
                and bump.get('heading') == 'Remove yourself as a workflow manager?'
                and bump.get('message') == 'Removing yourself as a workflow manager means you\u2019ll immediately lose access to edit this workflow. To become a workflow manager of this workflow again, a current manager will need to add you.',
                'Self-removal lost explicit consequence and confirmation')
        actions = [controls.get('workflow-permissions-self-cancel'), controls.get('workflow-permissions-self-save')]
        require(exact(bump.get('actions'), actions) and [a.get('text') for a in actions] == ['Cancel', 'Save'],
                'Wrong self-removal actions')
        for action in actions:
            measured(action, viewport, rectangle, disabled=False)
    else:
        require(bump is None and all(k not in controls for k in ('workflow-permissions-self-save', 'workflow-permissions-self-cancel')),
                'Unexpected self-removal confirmation')
    actions = [controls.get('workflow-permissions-cancel'), controls.get('workflow-permissions-save')]
    require(exact(panel.get('actions'), actions) and [a.get('text') for a in actions] == ['Cancel', 'Save'],
            'Wrong Permissions actions')
    measured(actions[0], viewport, rectangle, disabled=False)
    measured(actions[1], viewport, rectangle, disabled=draft['speedBump'] or draft['pending'] or not draft['ids'])
    run = confirmation.get('runDraft')
    if run is None:
        require(all(panel.get(k) is None for k in ('run', 'runLabel', 'external', 'query', 'externalQuery', 'epoch'))
                and panel.get('selected') == [] and panel.get('results') == [], 'No-trigger panel invented run authority')
    else:
        control = panel.get('run')
        measured(control, viewport, rectangle, disabled=False)
        require(control.get('id') == 'workflow-permissions-run' and control.get('value') == run.get('mode')
                and panel.get('epoch') == str(run.get('epoch')), 'Run selection escaped original epoch')
        require([r.get('target') for r in panel.get('selected', [])] == run.get('ids'),
                'Manager/run union differs from native selected targets')
        for item in panel.get('selected', []) + panel.get('results', []):
            measured(item, viewport, rectangle, disabled=False)


def controls_proof(native, controls, confirmation, state, stage, route, original, viewport, rectangle):
    confirming = confirmation is not None
    busy = state['sent'] and state['held'] and not state['retired'] and not state['departed']
    sealed = state['sent'] and not state['resolved']
    require(native.get('present') is (route in ('editor', 'managed')),
            'Native surface differs from owned route')
    require(native.get('leasePresent') is (confirming or busy), 'Wrong original native lease lifecycle')
    if route != 'editor':
        require(native.get('fields') is None and native.get('originalNodes') is None
                and native.get('settings') is None, 'Detached original editor still exposed')
        if route == 'about':
            require('workflow-configuration' in controls, 'About Configuration missing')
        else:
            require(any(k.startswith('workflow-management-') for k in controls), 'Managed navigation unavailable')
        return
    title, description = (original['workflow']['title'], original['workflow']['description']) if stage == 'editor' else (TITLE, DESCRIPTION)
    native_fields(native, title, description, confirming or busy or sealed)
    for name, disabled in [('save', stage == 'editor' or confirming or busy or sealed),
                           ('managed', confirming or busy), ('delete', confirming or busy or sealed),
                           ('permissions', confirming or busy or sealed)]:
        require(controls.get('workflow-management-' + name, {}).get('disabled') is disabled,
                'Unsafe original action: ' + name)
    check = 'workflow-permissions-check' if sealed else 'workflow-management-check'
    require(controls.get(check, {}).get('disabled') is (confirming or busy), 'Wrong Check availability')
    require(('workflow-management-use-current' in controls) is (stage == 'reopened'),
            'Wrong retained draft adoption availability')
    if confirming:
        settings(native, controls, confirmation, stage, viewport, rectangle)
    else:
        require(native.get('settings') is None, 'Unexpected Permissions panel')
