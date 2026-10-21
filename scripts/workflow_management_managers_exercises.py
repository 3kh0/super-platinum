"""Cross-observation proofs for Cancel, hydration and original late settlements."""
import copy
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION


def original_buffers(value):
    fields = value.get('native', {}).get('fields', {}) or {}
    require(fields.get('title', {}).get('value') == TITLE
            and fields.get('description', {}).get('value') == DESCRIPTION
            and value.get('native', {}).get('originalNodes') is True,
            'Native action lost original proposed metadata')


def quiet(value, observed):
    require(isinstance(value, dict), 'Missing passive held observation')
    integer(value.get('observedMs'), 'Held stability was not observed', 500)
    require(exact(value.get('before'), observed) and exact(value.get('after'), observed),
            'Held work changed without original response delivery')


def local_pair(value):
    require(isinstance(value, dict), 'Missing native action observation')
    before, after = value.get('before'), value.get('after')
    require(isinstance(before, dict) and isinstance(after, dict), 'Missing original before/after')
    original_buffers(before)
    original_buffers(after)
    require(exact(before['server'], after['server']), 'Local selection or Cancel sent a request')
    require(before['permissions']['document']['id'] == after['permissions']['document']['id'],
            'Local action replaced original document')
    return before, after


def retained_finish(before, after):
    old, new = before.get('retainedDocument'), after.get('retainedDocument')
    require(isinstance(old, dict) and isinstance(new, dict), 'Original retained document missing on delivery')
    require(type(new.get('revision')) is int and new['revision'] > old['revision'],
            'Original finish did not advance retained revision')
    expected = copy.deepcopy(old)
    expected['revision'] = new['revision']
    require(exact(expected, new), 'Late response rewrote original pending metadata or baseline')


def managed_entry(exercise, current):
    entry = exercise.get('managementEntry')
    require(isinstance(entry, dict), 'Triggerless workflow bypassed actual Managed list')
    managed, clicked = entry.get('managed'), entry.get('clicked')
    require(isinstance(managed, dict) and isinstance(clicked, dict), 'Missing actual Managed row observation')
    require(managed['model']['route'] == 'managed' and managed['model']['busy'] is False
            and managed['permissions']['document'] is None
            and managed['native']['fields'] is None,
            'Triggerless entry fabricated an editor before the workflow row')
    require(clicked.get('id') == 'workflow-management-open-WF_REVIEW'
            and isinstance(clicked.get('text'), str) and bool(clicked['text'].strip())
            and all(clicked.get(k) is True for k in ('ok', 'connected', 'hit')),
            'Triggerless workflow bypassed actual Managed row control')
    require(managed['model']['visit'] == current['model']['visit']
            and managed['model']['writes'] == current['model']['writes']
            and managed['model']['reads'] + 9 == current['model']['reads'],
            'Managed row changed visit, wrote, or skipped actual editor acquisition')
    for key in ('requests', 'responses', 'settlements'):
        before, after = managed['server'][key], current['server'][key]
        require(len(before) + 9 == len(after) and exact(before, after[:len(before)]),
                'Managed entry lost original ordered ' + key)


def exercises(receipt, case, rows):
    from workflow_management_managers import observation
    bystage = {r['stage']: r for r in rows}
    for row in rows:
        stage, exercise = row['stage'], row['proof'].get('exercise')
        require(isinstance(exercise, dict), 'Missing original native exercise')
        current = observation(row)
        if case == 'managers-no-trigger' and stage in ('editor', 'reopened'):
            managed_entry(exercise, current)
        if stage in ('unchanged', 'cancelled'):
            before, after = local_pair(exercise)
            require(exercise.get('action') == ('workflow-permissions-save' if stage == 'unchanged' else 'workflow-permissions-cancel')
                    and exact(after, current) and after['permissions']['confirmation'] is None
                    and after['permissions']['document']['revision'] > before['permissions']['document']['revision'],
                    'No-op/Cancel did not release original native admission')
        elif stage == 'empty':
            removals = exercise.get('removals')
            require(isinstance(removals, list) and len(removals) == 2, 'Empty manager validation not exercised')
            for proof, target in zip(removals, ['UWORKFLOW', 'UMANOLD01']):
                before, after = local_pair(proof)
                require(proof.get('clicked', {}).get('target') == target
                        and all(proof['clicked'].get(k) is True for k in ('ok', 'connected', 'insideOriginalBox', 'hit')),
                        'Manager removal bypassed actual original control')
            require(exact(removals[-1]['after'], current), 'Empty stage differs from original removal')
        elif stage == 'selected':
            search, add, remove = (exercise.get(k) for k in ('search', 'add', 'remove'))
            require(isinstance(search, dict), 'Missing actual typed acquisition')
            original_buffers(search['before']); original_buffers(search['after'])
            require(search['after']['permissions']['confirmation']['managerDraft']['query'] == 'manager',
                    'Query acquisition bypassed native input')
            for value, target in [(add, 'UMANNEW01'), (remove, 'UWORKFLOW' if case.startswith('managers-self-') else 'UMANOLD01')]:
                local_pair(value)
                click = value.get('clicked', {})
                require(click.get('target') == target and all(click.get(k) is True for k in ('ok', 'connected', 'insideOriginalBox', 'hit')),
                        'Manager selection lacks actual original hit control')
            require(add['after']['permissions']['confirmation']['managerDraft']['ids'][-1] == 'UMANNEW01',
                    'Add selection did not append original acquired identity')
        elif stage in ('confirmation', 'cancelled-confirmation'):
            before, after = local_pair(exercise)
            require(exact(after, current)
                    and before['permissions']['confirmation']['managerDraft']['speedBump'] is (stage != 'confirmation')
                    and after['permissions']['confirmation']['managerDraft']['speedBump'] is (stage == 'confirmation'),
                    'Self-removal speed bump bypassed explicit local transition')
        elif stage in ('checked', 'checked-again'):
            before, after = exercise.get('before'), exercise.get('after')
            require(exact(after, current), 'Checked observation differs from captured stage')
            original_buffers(before); original_buffers(after)
            require(before['model']['writes'] == after['model']['writes']
                    and len(after['server']['requests']) > len(before['server']['requests']),
                    'Check replayed a write or did not acquire fresh reads')
        elif stage == 'missing-offline':
            quiet(exercise, current)
        elif stage in ('held', 'retry-held'):
            quiet(exercise.get('quiet'), current)
            require(isinstance(exercise.get('admission'), dict), 'Missing original held admission')
        elif stage == 'hydrated':
            before, after = exercise.get('before'), exercise.get('after')
            require(exact(after, current) and before['model']['writes'] == after['model']['writes'] == 0,
                    'Hydration wrote or bypassed original acquisition')
            require(exact(exercise.get('release'), after['server']['releases'][-1]), 'Hydration release identity changed')
            original_buffers(before); original_buffers(after)
        elif stage == 'successor':
            require(exact(exercise.get('after'), current)
                    and exact(exercise.get('replacement'), receipt.get('replacement')),
                    'Account replacement did not produce captured successor')
            replacement = exercise['replacement']
            require(replacement.get('foreignDocumentHidden') is True
                    and replacement.get('currentEpoch') == replacement.get('previousEpoch', -2) + 1,
                    'Actual account replacement/currentness absent')
        elif stage in ('departed', 'departed-held'):
            require(exact(exercise.get('departure'), current), 'Intentional departure differs from captured route')
            admission = exercise.get('admission')
            original_buffers(admission)
            require(admission['permissions']['confirmation']['managerDraft']['speedBump'] is True,
                    'Departure was not admitted by explicit self-removal confirmation')
        elif stage == 'late-settled':
            before, after = exercise.get('before'), exercise.get('after')
            require(exact(after, current) and exact(exercise.get('release'), after['server']['releases'][-1]),
                    'Late settlement lost original receiver/response')
            expected = copy.deepcopy(before['model'])
            if case == 'managers-self-rejected-held':
                expected['reads'] += 1
            require(exact(expected, after['model']) and exact(before['native'], after['native']),
                    'Old response mutated successor/departed UI')
            retained_finish(before, after)
            if case == 'managers-retirement':
                require(exercise.get('successorUnchanged') is True
                        and exact(before['server']['requests'], after['server']['requests']),
                        'Account retirement allowed original manager remove')
        elif stage == 'settled':
            quiet(exercise, current)
        elif stage == 'back':
            require(exact(exercise.get('after'), current)
                    and exact(exercise['before']['server'], exercise['after']['server']),
                    'Back dispatched or changed original responses')
        elif stage == 'reopened':
            require(exact(exercise.get('after'), current)
                    and all(exercise.get('remount', {}).get(k) is True for k in
                            ('ok', 'originalDetached', 'newRoot', 'newTitle', 'newDescription')),
                    'Reopen did not bind fresh actual native controls')
            original_buffers(current)
        elif stage in ('acknowledged', 'rejected', 'unconfirmed'):
            original_buffers(exercise.get('admission'))
        elif stage == 'editor' and case == 'managers-no-trigger':
            require(set(exercise) == {'managementEntry'}, 'Unexpected Managed entry exercise')
        else:
            require(exercise == {}, 'Unexpected fixture exercise payload')
