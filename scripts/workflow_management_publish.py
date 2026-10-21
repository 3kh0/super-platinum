"""Native Publish acceptance: actual acquisition, receipts, custody and readback."""
from pathlib import Path
from workflow_management_delete import require, exact, integer, TITLE, DESCRIPTION
from workflow_management_unpublish import measured_native, native_fields
from workflow_management_publish_ledger import validate as ledger, REPUBLISH, scenario, published_workflow

STAGES = {'publish-success': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'primary-ack', 'confirmed', 'checked', 'back', 'reopened'), 'publish-primary-rejected': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'primary-rejected', 'checked', 'checked-again'), 'publish-primary-unknown': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'primary-unconfirmed', 'checked', 'checked-again'), 'publish-sync-rejected': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'sync-rejected', 'checked', 'checked-again'), 'publish-sync-unknown': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'sync-unconfirmed', 'checked', 'checked-again'), 'publish-retirement': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'primary-held', 'successor', 'late-settled'), 'publish-stale-readback': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'confirmed', 'checked', 'checked-again'), 'publish-blocked': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'blocked', 'quiet'), 'publish-validation-recovery': ('about', 'editor', 'draft', 'dirty-refused', 'restored', 'validation-failed', 'retry-held', 'validation-ready', 'confirmed', 'checked')}
INITIAL = ('about', 'editor', 'draft', 'dirty-refused', 'restored')
METHODS = ('functions.workflows.publish', 'workflows.triggers.update')
STAGES.update({case: STAGES[behavior] for case, behavior in REPUBLISH.items()})


def observation(row):
    return dict(model=row['model'], publish=row['proof']['publish'],
                server=row['proof']['server'], native=row['proof']['native'])


def attempts_proof(product, server, case, stage):
    behavior = scenario(case)
    attempts = product.get('attempts')
    require(isinstance(attempts, list), 'Missing Publish attempt book')
    if stage in INITIAL:
        require(not attempts, 'Publication attempted before clean activation')
        return
    require(attempts, 'Missing actual Publish attempt')
    ids = [a.get('id') for a in attempts]
    require(len(ids) == len(set(ids)), 'Publish attempt identity reused')
    requests, responses, settlements = (server[k] for k in ('requests', 'responses', 'settlements'))
    returned = {v['serial'] for v in settlements if v['returned']}
    mutations = [r for r in requests if r['phase'] == 'mutation']
    for attempt in attempts:
        for key in ('id', 'visit', 'document', 'revision'):
            integer(attempt.get(key), 'Original Publish ' + key, 1)
        require(attempt.get('workflow') == 'WfREVIEW01', 'Original Publish workflow')
        require(attempt.get('title') == server['original']['workflow']['title']
                and attempt.get('description') == server['original']['workflow']['description'], 'Publish included unsaved fields')
        slots = attempt.get('slots')
        require(isinstance(slots, list) and len(slots) == 1, 'Publish primary slot count')
        slot = slots[0]
        require(slot.get('method') == METHODS[0] and slot.get('arguments') == {'workflow_id': 'WfREVIEW01'}, 'Publish primary plan')
        if attempt.get('noSend') is True:
            require(slot.get('outcome') in ('pending', 'not-sent') and slot.get('raw') is None,
                    'No-send validation invented primary receipt')
            require(attempt.get('complete') is False and attempt.get('resolved') is False, 'Read-only validation resolved publication')
            continue
        relevant = [r for r in mutations if r['owner'].get('document') == attempt['document']]
        if not relevant:
            require(attempt.get('finished') is False and slot.get('outcome') == 'pending'
                    and stage == 'retry-held', 'Sent attempt has no original dispatch')
            continue
        for index, request in enumerate(relevant):
            raw = responses[request['serial'] - 1]['result']
            current = slot if index == 0 else attempt.get('followup', {}).get('stages', [{}])[0].get('slots', [{}])[0]
            if request['serial'] not in returned:
                expected = 'in-flight'
            elif raw['error'] is not None:
                expected = 'unknown'
            elif raw['raw'].get('ok') is False:
                expected = 'rejected'
            else:
                expected = 'acknowledged'
            require(current.get('method') == METHODS[index] and current.get('outcome') == expected, 'Publish ACK/raw disposition')
            require(exact(current.get('raw'), raw['raw'] if expected in ('acknowledged', 'rejected') else None), 'Publish raw receipt custody')
        if slot.get('outcome') != 'acknowledged':
            require(attempt.get('followup') is None, 'Unknown/rejected primary authorized synchronization')
        if stage in ('checked', 'checked-again', 'back', 'reopened'):
            readback = attempt.get('readback')
            require(isinstance(readback, dict), 'Missing independent publication readback')
            resolved = behavior in ('publish-success', 'publish-validation-recovery')
            require(attempt.get('resolved') is resolved, 'Publication incorrectly resolved from readback')
            require(readback.get('matches') is (behavior in ('publish-success', 'publish-validation-recovery', 'publish-sync-unknown')),
                    'Publication observation confused with acknowledgement')
            if case == 'publish-stale-readback':
                require(readback.get('matches') is False and readback.get('workflow', {}).get('last_published_version_id') == 'V1',
                        'Stale published version accepted')
            if case == 'publish-republish-stale':
                expected = published_workflow(case)
                expected['is_published'] = False
                require(readback.get('matches') is False and exact(readback.get('workflow'), expected),
                        'Stale republish state accepted or independent contents lost')
            if resolved:
                require(attempt.get('complete') is True and readback.get('matches') is True
                        and exact(readback.get('workflow'), published_workflow(case)), 'Publish success lacks exact independent readback')


def validate(fixture, case, receipt, captures, viewport, rectangle, validate_png, *, png=True):
    behavior = scenario(case)
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('scenario') == 'editor' and receipt.get('publishCase') == case, 'Publish terminal identity')
    require(receipt.get('syntheticSession') is True and receipt.get('noLiveNetwork') is True
            and receipt.get('aboutConfiguration') is True, 'Publish synthetic isolation')
    rows = receipt.get('stages')
    require(isinstance(rows, list) and tuple(r.get('stage') for r in rows) == STAGES[case], 'Publish stage sequence')
    capture_map = receipt.get('captures')
    require(isinstance(capture_map, dict) and set(capture_map) == set(STAGES[case]), 'Publish capture inventory')
    previous = None
    for row in rows:
        stage, proof, model = row['stage'], row.get('proof', {}), row.get('model', {})
        server, product, native = (proof.get(k) for k in ('server', 'publish', 'native'))
        methods = ledger(server, require, case)
        require(type(model.get('writes')) is int and model['writes'] == len(methods), 'Publish source write count')
        require(type(model.get('reads')) is int and model['reads'] == len(server['requests']) - len(methods), 'Publish source read count')
        require(model.get('networkCalls') == 0 and server.get('ownerCurrent') is (stage not in ('successor', 'late-settled')), 'Publish network/owner boundary')
        if previous:
            for key in ('requests', 'responses', 'settlements', 'releases'):
                require(server[key][:len(previous[key])] == previous[key], 'Original Publish ledger rewritten: ' + key)
        previous = server
        controls = measured_native(native, row['geometry'], viewport, rectangle)
        if 'workflow-management-publish' in controls:
            caption = 'Publish Changes' if server['original']['workflow']['unpublished_change_count'] > 0 else 'Publish'
            require(controls['workflow-management-publish'].get('text') == caption, 'Publication action caption')
        if stage not in ('about', 'back', 'successor', 'late-settled'):
            title, description = ((TITLE, DESCRIPTION) if stage in ('draft', 'dirty-refused') else
                                  (server['original']['workflow']['title'], server['original']['workflow']['description']))
            require(model.get('title') == title and model.get('description') == description, 'Publish fields changed')
            require(native.get('originalNodes') is True and native.get('fields', {}).get('title', {}).get('value') == title
                    and native.get('fields', {}).get('description', {}).get('value') == description, 'Publish original native controls')
        if stage in ('draft', 'dirty-refused'):
            require(controls.get('workflow-management-publish', {}).get('disabled') is True and not methods,
                    'Dirty Publish admitted')
        if stage in INITIAL:
            require(not methods, 'Unexpected early Publish dispatch')
        if stage in ('retry-held', 'validation-ready', 'validation-failed', 'blocked', 'quiet'):
            require(not methods, 'Publication validation dispatched a mutation')
        if stage == 'validation-ready':
            require(product['attempts'][-1].get('validation', {}).get('status') == 'ready', 'Retry did not acquire facts')
        attempts_proof(product, server, case, stage)
        if stage not in INITIAL:
            last = product['attempts'][-1]
            held = stage in ('primary-ack', 'primary-held', 'retry-held', 'successor')
            require(last.get('finished') is (not held), 'Publish worker completion state')
            require(last.get('primarySucceeded') is (last['slots'][0]['outcome'] == 'acknowledged'), 'Publish primary success classification')
            if stage in ('validation-failed', 'validation-ready', 'blocked', 'quiet'):
                expected_status = 'failed' if stage == 'validation-failed' else 'ready' if stage == 'validation-ready' else 'blocked'
                require(last.get('validation', {}).get('status') == expected_status, 'Publish validation state')
            if stage == 'validation-ready':
                require(last.get('validationOnly') is True and len(product['attempts']) == 2, 'Retry read-only attempt ownership')
            if methods and stage not in ('primary-held', 'successor'):
                validation = last.get('validation', {})
                require(validation.get('status') == 'ready' and validation.get('assessment', {}).get('triggerAvailable') is True,
                        'Publish lacks acquired validation')
                require(validation.get('certifiedApps') == {'ok': True, 'apps': [], 'is_aaa_enabled': False, 'can_user_manage_app': True},
                        'Publish acquired connector facts')
        exercise = proof.get('exercise', {})
        quiet = exercise.get('quiet', exercise)
        if stage in ('dirty-refused', 'primary-ack', 'primary-held', 'retry-held', 'quiet'):
            require(quiet.get('observedMs') == 500 and exact(quiet.get('before'), quiet.get('after')), 'Publish quiet custody')
            require(exact(quiet.get('after'), observation(row)), 'Publish quiet witness not current stage')
        if stage == 'late-settled':
            require(exercise.get('successorUnchanged') is True and not product.get('visible'), 'Late publication touched successor')
            for key in ('model', 'native'):
                require(exact(exercise['before'][key], exercise['after'][key]), 'Late Publish changed successor ' + key)
            require(exact(exercise['after'], observation(row)), 'Late Publish observer mismatch')
        path = Path(capture_map[stage])
        require(path.resolve().parent == Path(captures).resolve() and path.name == f'{fixture}-{stage}.png', 'Publish capture path')
        if png:
            validate_png(path, row['geometry']['pngDimensions'])
    final = rows[-1]
    require(exact(receipt.get('final'), observation(final)), 'Publish final observer mismatch')
    expected_writes = 0 if behavior == 'publish-blocked' else 1 if behavior in (
        'publish-primary-rejected', 'publish-primary-unknown', 'publish-retirement') else 2
    require(final['model']['writes'] == expected_writes, 'Publication final dispatch count')
    if behavior != 'publish-retirement':
        require(receipt.get('replacement') is None, 'Unexpected Publish account replacement')
    return receipt
