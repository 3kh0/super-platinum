"""Independent cross-stage custody for the four closed manager-agent cases.

Wire/source and per-stage validation are separate: this module binds actual
native actions, original held receivers, and exact before/after observations.
"""
import copy
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION
from workflow_management_managers_exercises import local_pair, original_buffers, quiet

CASES = {'managers-agents', 'managers-agents-recovery',
         'managers-agents-query-retirement', 'managers-agents-account-retirement'}
ADDITIONS = ('UMANNEW01', 'UMANAFYES', 'UMANAFDEF')
LABELS = ('Agent Manager', 'Accessible Agent Manager', 'Default Agent Manager')


def _dict(value, message):
    require(isinstance(value, dict), message)
    return value


def _manager(value):
    return _dict(value['permissions'].get('confirmation'),
                 'Original manager confirmation missing')['managerDraft']


def _linked(actual, previous):
    _dict(actual, 'Missing original action admission')
    # Focus/scroll can legitimately move before the next actual action. The
    # authoritative model, receipts and retained draft must still be identical.
    for key in ('model', 'permissions', 'server', 'retainedDocument'):
        require(exact(actual.get(key), previous.get(key)),
                'Action is not linked to the preceding original ' + key)


def _control(value, id, label=None):
    matches = [c for c in value['native'].get('controls', []) if c.get('id') == id]
    require(len(matches) == 1 and matches[0].get('disabled') is False
            and matches[0].get('visible') is True and matches[0].get('hit') is True
            and (label is None or matches[0].get('text') == label),
            'Action lacks its actual enabled original control: ' + id)


def _same_editor(before, after):
    original_buffers(before)
    original_buffers(after)
    a, b = before['permissions'], after['permissions']
    require(a['visit'] == b['visit'] and a['document']['id'] == b['document']['id'],
            'Action replaced original manager visit/document')
    require(before['model']['writes'] == after['model']['writes'],
            'Read or local action dispatched a mutation')


def _query(before, after, text):
    _same_editor(before, after)
    a, b = _manager(before), _manager(after)
    native_query = before['native'].get('settings', {}).get('managers', {}).get('query', {})
    require(native_query.get('id') == 'workflow-permissions-manager-query'
            and native_query.get('readonly') is False and native_query.get('disabled') is False
            and native_query.get('value') == a.get('query'),
            'Latest query bypassed original editable input')
    require(before['permissions']['confirmation']['id'] == after['permissions']['confirmation']['id'],
            'Query replaced original confirmation')
    old = integer(a.get('generation'), 'Missing original query generation', 1)
    require(integer(b.get('generation'), 'Missing successor query generation', 1) > old
            and b.get('query') == text and exact(a.get('ids'), b.get('ids')),
            'Actual query did not retire generation while retaining selections')
    require(exact(before['retainedDocument'], after['retainedDocument']),
            'Query changed original retained metadata')
    requests = before['server']['requests']
    require(exact(requests, after['server']['requests'][:len(requests)])
            and len(after['server']['requests']) > len(requests),
            'Query lost original requests or bypassed acquisition')


def _release(exercise, before, after, retired):
    release = _dict(exercise.get('release'), 'Missing original sender release')
    serial = integer(before['server'].get('held'), 'Original held serial missing', 1)
    require(type(release.get('serial')) is int and release['serial'] == serial
            and release.get('receiverRetired') is retired
            and release.get('delivered') is (not retired),
            'Original held receiver disposition changed')
    originals = [r for r in before['server']['responses'] if r.get('serial') == serial]
    require(len(originals) == 1 and exact(release.get('response'), originals[0]),
            'Release replaced original captured raw response')
    require(exact(after['server']['releases'], before['server']['releases'] + [release])
            and after['server'].get('held') is None,
            'Original sender release was lost or duplicated')
    return release


def _retained(exercise, original):
    old = _dict(exercise.get('originalDocument'), 'Original retained document proof missing')
    require(exact(old, original) and exact(exercise.get('retainedDocument'), old)
            and old.get('title') == TITLE and old.get('description') == DESCRIPTION,
            'Retirement rewrote or replaced original rich metadata draft')


def exercises(receipt, case, rows):
    from workflow_management_managers import observation
    require(case in CASES, 'Unknown manager-agent exercise case')
    bystage = {r['stage']: observation(r) for r in rows}
    previous = None
    original_retired = None
    for row in rows:
        stage, current = row['stage'], observation(row)
        exercise = _dict(row['proof'].get('exercise'), 'Missing native agent exercise')
        if stage == 'eligible':
            before = exercise.get('admission')
            _linked(before, previous)
            require(exact(exercise.get('after'), current), 'Eligible capture differs from actual acquisition')
            _query(before, current, 'manager')
        elif stage == 'selected':
            choices = exercise.get('choices')
            require(isinstance(choices, list) and len(choices) == 4,
                    'Agent selection did not exercise three adds and original removal')
            prior = previous
            for i, (proof, target) in enumerate(zip(choices, (*ADDITIONS, 'UMANOLD01'))):
                before, after = local_pair(proof)
                _linked(before, prior)
                _same_editor(before, after)
                click = _dict(proof.get('clicked'), 'Missing original result click')
                require(click.get('target') == target and click.get('label') == (LABELS[i] if i < 3 else 'Original Manager')
                        and all(click.get(k) is True for k in ('ok', 'connected', 'insideOriginalBox', 'hit')),
                        'Manager choice bypassed actual original labelled control')
                a, b = _manager(before), _manager(after)
                ids = a['ids'] + [target] if i < 3 else [id for id in a['ids'] if id != target]
                require(target not in a['ids'] if i < 3 else target in a['ids'],
                        'Manager action did not change its original target')
                require(exact(b['ids'], ids) and b['query'] == a['query'] == 'manager'
                        and exact(a.get('agentFacts'), b.get('agentFacts'))
                        and b['generation'] > a['generation'] and b['epoch'] > a['epoch'],
                        'Manager action lost ordered identity or original fact custody')
                require(exact(before['retainedDocument'], after['retainedDocument']),
                        'Manager choice replaced retained draft')
                prior = after
            require(exact(prior, current), 'Selected stage differs from final original control action')
        elif stage == 'acknowledged':
            admission = exercise.get('admission')
            _linked(admission, previous)
            original_buffers(admission)
            _control(admission, 'workflow-permissions-save', 'Save')
            original_buffers(current)
            require(admission['permissions']['attempts'] == [],
                    'Save reused a previously dispatched attempt')
        elif stage == 'checked':
            before, after = exercise.get('before'), exercise.get('after')
            _linked(before, previous)
            _same_editor(before, after)
            _control(before, 'workflow-permissions-check')
            require(exact(after, current) and after['model']['reads'] == before['model']['reads'] + 11,
                    'Check bypassed original fresh readback or replayed a write')
        elif stage in ('facts-failed', 'facts-held'):
            before = exercise.get('admission')
            _linked(before, previous)
            quiet(exercise.get('quiet'), current)
            _query(before, current, 'manager')
        elif stage == 'retry-held':
            before = exercise.get('admission')
            _linked(before, previous)
            quiet(exercise.get('quiet'), current)
            _same_editor(before, current)
            _control(before, 'workflow-permissions-manager-agent-retry', 'Retry manager eligibility')
            a, b = _manager(before), _manager(current)
            require(a['agentFacts']['status'] == 'failed' and b['agentFacts']['status'] == 'loading'
                    and a['generation'] == b['generation'] and a['query'] == b['query'] == 'manager'
                    and exact(a['ids'], b['ids'])
                    and exact(a['agentFacts']['rows'], b['agentFacts']['rows']),
                    'Retry replaced original failed query/rows or selections')
            n = len(before['server']['requests'])
            require(exact(before['server']['requests'], current['server']['requests'][:n])
                    and len(current['server']['requests']) > n
                    and all(r.get('phase') == 'agent-fact' for r in current['server']['requests'][n:]),
                    'Retry replayed search/mutation instead of original fact acquisition')
        elif stage == 'recovered':
            before, after = exercise.get('before'), exercise.get('after')
            _linked(before, previous)
            _same_editor(before, after)
            require(exact(after, current), 'Recovered observation differs from original release')
            _release(exercise, before, after, False)
            a, b = _manager(before), _manager(after)
            require(a['agentFacts']['status'] == 'loading' and b['agentFacts']['status'] == 'ready'
                    and a['generation'] == b['generation'] and exact(a['ids'], b['ids'])
                    and exact(a['agentFacts']['rows'], b['agentFacts']['rows'])
                    and exact(before['retainedDocument'], after['retainedDocument']),
                    'Recovery did not adopt only original fact batch')
        elif stage in ('superseded', 'successor'):
            before, after = exercise.get('before'), exercise.get('after')
            _linked(before, previous)
            original_buffers(before)
            require(exact(after, current) and exercise.get('receiverRetired') is True,
                    'Original receiver remained live after retirement')
            original_retired = before['retainedDocument']
            _retained(exercise, original_retired)
            if stage == 'superseded':
                require(case == 'managers-agents-query-retirement' and exercise.get('replacement') is None,
                        'Query retirement fabricated account replacement')
                _query(before, after, 'ordinary')
                require(_manager(after).get('agentFacts') is None,
                        'New query reused original query eligibility facts')
            else:
                require(case == 'managers-agents-account-retirement'
                        and exact(exercise.get('replacement'), receipt.get('replacement')),
                        'Successor is not actual account replacement')
                replacement = _dict(exercise.get('replacement'), 'Account replacement proof absent')
                require(replacement.get('foreignDocumentHidden') is True
                        and type(replacement.get('currentEpoch')) is int
                        and replacement['currentEpoch'] == replacement.get('previousEpoch', -2) + 1
                        and after['server'].get('ownerCurrent') is False
                        and after['permissions'].get('visible') is False
                        and after['native'].get('fields') is None,
                        'Retired original account retained visible editor authority')
            serial = before['server']['held']
            settled = [s for s in after['server']['settlements'] if s.get('serial') == serial]
            require(len(settled) == 1 and settled[0].get('returned') is False,
                    'Actual held receiver was not cancelled before late release')
        elif stage == 'late-settled':
            before, after = exercise.get('before'), exercise.get('after')
            _linked(before, previous)
            integer(exercise.get('observedMs'), 'Late response was not passively observed', 500)
            require(exact(after, current), 'Late stage differs from original observed release')
            _retained(exercise, original_retired)
            release = _release(exercise, before, after, True)
            expected = copy.deepcopy(before)
            expected['server']['held'] = None
            expected['server']['releases'].append(release)
            require(exact(expected, after), 'Late original fact changed successor model/native/draft/requests')
        elif stage == 'cancelled':
            before, after = local_pair(exercise)
            _linked(before, previous)
            _control(before, 'workflow-permissions-cancel', 'Cancel')
            require(exercise.get('action') == 'workflow-permissions-cancel' and exact(after, current)
                    and after['permissions']['confirmation'] is None
                    and after['permissions']['document']['revision'] > before['permissions']['document']['revision'],
                    'Cancel did not release original settings without dispatch')
            fields = after['native']['fields']
            require(all(fields[k].get('readonly') is False for k in ('title', 'description')),
                    'Cancel stranded original metadata controls readonly')
        elif stage == 'back':
            before, after = exercise.get('before'), exercise.get('after')
            _linked(before, previous)
            original_buffers(before)
            require(exact(after, current) and exact(before['server'], after['server'])
                    and after['permissions']['document'] is None and after['native'].get('fields') is None,
                    'Back dispatched or retained a visible old editor')
        elif stage == 'reopened':
            before, after = exercise.get('before'), exercise.get('after')
            # The existing helper retains the original pre-Back editor as its
            # admission; the immediately preceding stage itself is About.
            _linked(before, bystage['checked'])
            original_buffers(before)
            original_buffers(after)
            require(exact(after, current)
                    and all(exercise.get('remount', {}).get(k) is True for k in
                            ('ok', 'originalDetached', 'newRoot', 'newTitle', 'newDescription')),
                    'Reopen did not replace detached original native controls')
            require(before['permissions']['document']['id'] == after['permissions']['document']['id']
                    and before['permissions']['visit'] != after['permissions']['visit']
                    and exact(before['permissions']['attempts'], after['permissions']['attempts'])
                    and after['permissions']['document']['pending'] is None
                    and after['model']['writes'] == before['model']['writes']
                    and after['model']['reads'] == before['model']['reads'] + 12,
                    'Reopen lost original draft/receipts or replayed mutation')
        else:
            require(stage in ('about', 'editor', 'draft', 'permissions') and exercise == {},
                    'Unexpected manager-agent exercise payload')
        previous = current
