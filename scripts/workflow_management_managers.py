"""Closed native acceptance for workflow manager editing and original draft custody."""
from pathlib import Path
import copy
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION
from workflow_management_unpublish import measured_native
from workflow_management_managers_ledger import STAGES, original, selected, desired_run, stage_state, server_proof
from workflow_management_managers_product import attempt_proof, retained_proof
from workflow_management_managers_native import controls_proof
from workflow_management_run_access import CONFIRMED, UNCONFIRMED, VERIFIED
from workflow_management_permissions import OBSERVED, REOPENED

CONFIRMING = {'permissions', 'empty', 'named', 'selected', 'confirmation', 'cancelled-confirmation',
              'missing-offline', 'retry-held', 'hydrated'}


def boot_member(raw):
    """Expected canonical derivation for this fixture's ASCII boot names."""
    value = copy.deepcopy(raw)
    profile = value['profile']
    for key in ('first_name', 'last_name'):
        if profile.get(key) is None:
            profile[key] = ''
    for key in ('real_name', 'display_name'):
        if profile.get(key + '_normalized') is None:
            profile[key + '_normalized'] = profile.get(key) or ''
    if value['id'] == 'UWORKFLOW':
        value['is_self'] = True
    names = {'name': value.get('name') or '', **{k: profile.get(k) or '' for k in
             ('first_name', 'last_name', 'real_name', 'display_name', 'real_name_normalized', 'display_name_normalized')}}
    names['email_normalized'] = profile.get('email') or ''
    require(all(v.isascii() for v in names.values()), 'Fixture name needs independently pinned Unicode derivation')
    for key, text in names.items():
        value['_' + key + '_lc'] = text.lower()
    return value


def observation(row):
    return dict(model=row['model'], **{k: row['proof'][k] for k in
                ('permissions', 'server', 'retainedDocument', 'native')})


def confirmation_proof(confirmation, case, stage, product, document, source, server):
    require(isinstance(confirmation, dict) and confirmation.get('current') is True,
            'Missing original manager confirmation')
    for key, value in dict(visit=product['visit'], document=document['id'], revision=document['revision'],
                           title=TITLE, description=DESCRIPTION, workflow='WF_REVIEW', originalCopy=source['copy']).items():
        require(exact(confirmation.get(key), value), 'Confirmation changed original ' + key)
    integer(confirmation.get('id'), 'Missing original confirmation identity', 1)
    manager = confirmation.get('managerDraft')
    require(isinstance(manager, dict), 'Missing manager selection')
    ids = ([] if stage == 'empty' else source['workflow']['collaborators']
           if stage in ('permissions', 'named', 'missing-offline', 'retry-held', 'hydrated') else selected(case))
    require(manager.get('ids') == ids and manager.get('selfRemoval') is ('UWORKFLOW' not in ids)
            and manager.get('speedBump') is (stage == 'confirmation')
            and manager.get('pending') is (stage == 'retry-held'), 'Wrong actual manager draft state')
    require(confirmation.get('selected') == ('everyone' if case in
            ('managers-combined', 'managers-add-rejected', 'managers-add-unknown', 'managers-self-rejected-held')
            and stage not in ('named', 'permissions') else 'collaborators'), 'Wrong changed-only copy selection')
    run = confirmation.get('runDraft')
    if case == 'managers-no-trigger':
        require(run is None, 'Manager picker requires an invented trigger')
    else:
        require(isinstance(run, dict), 'Existing run settings lost')
        mode = ('named_entities' if case == 'managers-combined' else 'everyone'
                if case == 'managers-self-rejected-held' else 'app_collaborators')
        require(run.get('mode') == mode, 'Manager editing changed run mode')
        if case == 'managers-combined':
            expected = source['workflow']['collaborators'] if stage == 'named' else ['UWORKFLOW', 'UMANNEW01', 'UMANOLD01']
            require(run.get('ids') == expected, 'Manager selection lost ordered run grants')
    rows = manager.get('rows')
    require(isinstance(rows, list) and len({r.get('id') for r in rows}) == len(rows), 'Canonical manager rows missing/duplicated')
    by_id = {r['id']: r for r in rows}
    acquired = {r['id']: boot_member(r) for r in server['seed']['boot']['users']}
    settled = {v['serial'] for v in server['settlements']}
    for request, response in zip(server['requests'], server['responses']):
        if request['phase'] == 'picker' and request['serial'] in settled:
            raw = response['result']['raw']
            if isinstance(raw, dict) and raw.get('ok') is True:
                acquired.update({r['id']: r for r in raw.get('results', [])})
    require(all(id in acquired and exact(row.get('raw'), acquired[id]) for id, row in by_id.items()),
            'Manager rows were fabricated instead of acquired from original producer')
    results = manager.get('results')
    require(isinstance(results, list) and len(results) == len(set(results)) and len(results) <= 24,
            'Manager results duplicated or exceeded source limit')
    require(all(id in by_id and by_id[id].get('eligible') is True for id in results),
            'Unresolved/ineligible manager became selectable')
    if stage in ('selected', 'confirmation', 'cancelled-confirmation'):
        require(manager.get('query') == 'manager', 'Actual typed manager query lost')
        require({'UMANNEW01', 'UMANENTER', 'UMANLOCAL'} <= set(by_id), 'Acquired eligible ordinary members missing')
        require(all(by_id[id].get('eligible') is True for id in ('UMANNEW01', 'UMANENTER', 'UMANLOCAL')),
                'Local/enterprise ordinary members incorrectly filtered')
        require(not set(results) & {'UMANBOT01', 'UMANDEAD1', 'UMANGUEST', 'UMANPROF1', 'UMANFLOW1', 'UMANAWAY1', 'USLACKBOT'},
                'Ineligible or unresolved raw row became selectable')


def validate(fixture, case, receipt, captures, *, viewport, rectangle, validate_png, png=True):
    require(case in STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid manager fixture/case')
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('scenario') == 'editor' and receipt.get('managersCase') == case,
            'Wrong or failed manager receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'syntheticSession', 'noLiveNetwork')),
            'Missing real entry or closed fixture isolation')
    require(receipt.get('viewport') == viewport, 'Wrong native viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[case]), 'Wrong manager stage order')
    require(isinstance(paths, dict) and set(paths) == set(STAGES[case]), 'Wrong original PNG inventory')
    source = original(case)
    identity = attempt_id = previous = None
    for row in rows:
        stage, model, geometry, proof = (row.get(k) for k in ('stage', 'model', 'geometry', 'proof'))
        require(all(isinstance(v, dict) for v in (model, geometry, proof)), 'Missing original stage evidence')
        product, server, native = (proof.get(k) for k in ('permissions', 'server', 'native'))
        require(isinstance(product, dict), 'Missing actual manager product observer')
        state = stage_state(case, stage)
        counts = server_proof(server, case, stage)
        route = 'about' if stage in ('about', 'back') or state['retired'] else 'managed' if state['departed'] else 'editor'
        busy = state['held'] and not state['retired'] and not state['departed'] and state['sent']
        confirming = stage in CONFIRMING
        sealed = state['sent'] and not state['resolved']
        for key in ('reads', 'writes', 'networkCalls', 'retainedDocuments', 'retainedAttempts'):
            integer(model.get(key), 'Invalid actual ' + key)
        require(model['reads'] == counts['reads'] and model['writes'] == counts['writes']
                and model['networkCalls'] == 0 and model['retainedAttempts'] == 0,
                'Actual product counts differ from original request ledger')
        require(model.get('route') == route and model.get('busy') is busy
                and model['retainedDocuments'] == (0 if stage == 'about' else 1), 'Route/draft custody changed')
        require(model.get('pending') is (route == 'editor' and sealed)
                and model.get('dirty') is (route == 'editor' and stage != 'editor')
                and model.get('observed') is (route == 'editor' and (stage == 'reopened' or state['checked'] and not state['resolved'])),
                'Wrong metadata draft or fresh observation state')
        require(product.get('visible') is (route in ('editor', 'managed')), 'Original surface visibility differs from owned route')
        attempts = product.get('attempts')
        require(isinstance(attempts, list) and len(attempts) == int(state['sent']), 'Original attempt lost/replayed')
        if attempts:
            attempt_proof(attempts[0], state, server, selected(case), desired_run(case))
            if attempt_id is None:
                attempt_id = attempts[0]['id']
            require(attempts[0]['id'] == attempt_id, 'Original attempt identity replaced')
        outcomes = []
        if route == 'editor' and sealed:
            labels = {'pending': 'Not sent', 'not-sent': 'Not sent', 'in-flight': 'Waiting for response',
                      'acknowledged': 'Acknowledged', 'rejected': 'Rejected', 'unknown': 'Unconfirmed'}
            attempt = attempts[0]
            if state['copy_changed']:
                outcomes.append('Copy access: ' + labels[attempt['slots'][0]['outcome']])
            for item in attempt['managerGroup']['slots']:
                outcomes.append('Workflow managers (' + item['method'].rsplit('.', 1)[1] + '): ' + labels[item['outcome']])
            if attempt['runGroup']:
                outcomes.append('Run access: ' + labels[attempt['runGroup']['primary']['outcome']])
                follow = attempt['runGroup']['followup']
                if follow:
                    outcomes.extend('Workflow restrictions: ' + labels[item['outcome']] for item in follow['slots'])
        require(model.get('outcomes') == (None if route == 'about' else outcomes),
                'Visible independent operation outcomes changed')
        document = product.get('document')
        if route == 'editor':
            require(isinstance(document, dict), 'Original visible draft missing')
            for key in ('id', 'revision'):
                integer(document.get(key), 'Missing original document ' + key, 1)
            if identity is None:
                identity = product['visit'], document['id']
            require(document['id'] == identity[1] and (stage == 'reopened' or product['visit'] == identity[0]),
                    'Original document/visit changed')
            if stage == 'reopened':
                require(product['visit'] != identity[0], 'Reopen reused retired visit')
            require(exact(native.get('scope'), dict(visit=str(product['visit']), document=str(document['id']), revision=str(document['revision']))),
                    'Native nodes escaped document revision')
            title, description = (source['workflow']['title'], source['workflow']['description']) if stage == 'editor' else (TITLE, DESCRIPTION)
            retained_proof(document, source, server['current'], identity[1], attempt_id if sealed else None,
                           state['resolved'], title, description)
            require((model.get('title'), model.get('description')) == (title, description), 'Manager save replaced metadata buffers')
            require(exact(document.get('copy'), server['current']['copy'] if state['resolved'] else source['copy'])
                    and exact(document.get('run'), server['current']['run'] if state['resolved'] else source['run'])
                    and exact(document.get('external'), source['external']), 'Permission baseline adopted without exact readback')
            require(document.get('editable') is (not confirming and not busy and not sealed), 'Unsafe metadata unlock')
            notice = REOPENED if stage == 'reopened' else (VERIFIED if state['resolved'] else OBSERVED if state['checked']
                     else CONFIRMED if state['complete'] else UNCONFIRMED) if state['finished'] else None
            require(document.get('notice') == model.get('notice') == notice, 'Wrong original operation/readback guidance')
        else:
            require(document is None and model.get('title') is None and model.get('description') is None, 'Hidden metadata projected')
        retained = proof.get('retainedDocument')
        if stage == 'about' or stage == 'back' and not state['sent']:
            require(retained is None, 'Unadmitted retained document exposed')
        else:
            require(identity is not None, 'Retained document lacks original admission')
            title, description = (source['workflow']['title'], source['workflow']['description']) if stage == 'editor' else (TITLE, DESCRIPTION)
            retained_proof(retained, source, server['current'], identity[1], attempt_id if sealed else None,
                           state['resolved'], title, description)
        confirmation = product.get('confirmation')
        if confirming:
            confirmation_proof(confirmation, case, stage, product, document, source, server)
        else:
            require(confirmation is None, 'Unexpected confirmation outside original admission')
        controls = measured_native(native, geometry, viewport, rectangle)
        controls_proof(native, controls, confirmation, state, stage, route, source, viewport, rectangle)
        if model.get('notice'):
            require(model['notice'] in [v.get('text') for v in native['feedback']], 'Visible operation guidance missing')
        require(all(message in [v.get('text') for v in native['feedback']] for message in outcomes),
                'Visible per-group response feedback missing')
        path = Path(captures) / f'{fixture}-{stage}.png'
        require(paths[stage] == str(path), 'Original capture escaped owned path')
        if png:
            validate_png(path, geometry['pngDimensions'])
        if previous:
            old = previous['proof']['server']
            for key in ('requests', 'responses', 'settlements', 'releases'):
                require(exact(server[key][:len(old[key])], old[key]), 'Original immutable ledger rewritten')
        previous = row
    require(exact(receipt.get('final'), observation(rows[-1])), 'Terminal receipt differs from final captured observation')
    from workflow_management_managers_exercises import exercises
    exercises(receipt, case, rows)
