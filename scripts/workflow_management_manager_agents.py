"""Closed native acceptance for acquired manager app/agent facts."""
from pathlib import Path
from workflow_management_delete import exact, integer, require, TITLE, DESCRIPTION
from workflow_management_unpublish import measured_native
from workflow_management_manager_agents_ledger import STAGES, original, selected, desired_run, stage_state, server_proof
from workflow_management_managers_product import attempt_proof, retained_proof
from workflow_management_managers_native import controls_proof, measured
from workflow_management_managers import boot_member, observation
from workflow_management_run_access import CONFIRMED, UNCONFIRMED, VERIFIED
from workflow_management_permissions import OBSERVED, REOPENED

CONFIRMING = {'permissions', 'eligible', 'selected', 'facts-failed', 'retry-held',
              'recovered', 'facts-held', 'superseded', 'late-settled'}
FAILURE = 'Workflow manager eligibility could not be loaded. Your selections are retained. Retry to load eligibility details.'
ALLOWED = ['UMANNEW01', 'UMANAFYES', 'UMANAFDEF']
DENIED = ['UMANBOT01', 'UMANAFNO1', 'UMANAFDIS', 'UMANAFBAD']


def confirmation_proof(confirmation, case, stage, product, document, source, server):
    require(isinstance(confirmation, dict) and confirmation.get('current') is True,
            'Missing original agent-manager confirmation')
    for key, value in dict(visit=product['visit'], document=document['id'], revision=document['revision'],
                           title=TITLE, description=DESCRIPTION, workflow='WF_REVIEW', originalCopy=source['copy']).items():
        require(exact(confirmation.get(key), value), 'Confirmation changed original ' + key)
    integer(confirmation.get('id'), 'Missing original confirmation identity', 1)
    draft = confirmation.get('managerDraft')
    require(isinstance(draft, dict), 'Missing actual manager selection')
    ids = selected(case) if stage == 'selected' else source['workflow']['collaborators']
    loading = stage in ('retry-held', 'facts-held')
    require(draft.get('ids') == ids and draft.get('selfRemoval') is False
            and draft.get('speedBump') is False and draft.get('pending') is loading,
            'Wrong acquired manager selection/loading state')
    require(draft.get('notice') == (FAILURE if stage == 'facts-failed' else None),
            'Failure did not preserve an explicit recovery path')
    require(confirmation.get('selected') == 'collaborators', 'Agent selection changed copy access')
    run = confirmation.get('runDraft')
    require(isinstance(run, dict) and run.get('mode') == source['run']['permission_type'],
            'Agent selection changed original run mode')
    if case == 'managers-agents':
        require(run.get('ids') == (desired_run(case)['user_ids'] if stage == 'selected'
                                  else source['run']['user_ids']), 'Agent manager/run ordered union changed')
    rows = draft.get('rows')
    require(isinstance(rows, list) and len({r.get('id') for r in rows}) == len(rows),
            'Acquired manager rows missing/duplicated')
    by_id = {r['id']: r for r in rows}
    seed = {r['id']: boot_member(r) for r in server['seed']['boot']['users']}
    acquired = dict(seed)
    settled = {v['serial'] for v in server['settlements'] if v['returned']}
    queries = []
    for request, response in zip(server['requests'], server['responses']):
        if request['phase'] == 'picker' and request['serial'] in settled:
            raw = response['result']['raw']
            if isinstance(raw, dict) and raw.get('ok') is True:
                acquired.update({r['id']: r for r in raw.get('results', [])})
                queries.append(raw)
    require(all(id in acquired and exact(row.get('raw'), acquired[id]) for id, row in by_id.items()),
            'Manager row fabricated or stale relative to original producer')
    for id, row in by_id.items():
        raw = acquired[id]
        label = next((raw.get('profile', {}).get(k) for k in ('display_name', 'real_name')
                      if raw.get('profile', {}).get(k)), raw.get('real_name') or raw.get('name'))
        require(row.get('label') == label, 'Manager label was fabricated from another app profile')
        if id in seed:
            allowed = id in source['workflow']['collaborators']
            require(row.get('eligible') is allowed and row.get('eligibility') == ('allowed' if allowed else 'excluded'),
                    'Seed self/local identity was reclassified by app facts')
    results = draft.get('results')
    require(isinstance(results, list) and len(results) == len(set(results)) and len(results) <= 24,
            'Manager result duplication/limit changed')
    require(all(id in by_id and by_id[id].get('eligible') is True for id in results),
            'Unresolved manager exposed as selectable')
    successor = stage in ('superseded', 'late-settled')
    require(draft.get('query') == ('' if stage == 'permissions' else 'ordinary' if successor else 'manager'),
            'Original typed query lost')
    facts = draft.get('agentFacts')
    if stage == 'permissions' or successor:
        require(facts is None, 'Old query facts became successor authority')
        if stage == 'permissions':
            require(results == sorted(source['workflow']['collaborators']), 'Initial ordinary managers changed')
        if successor:
            require(results == ['UMANLOCAL'] and by_id['UMANLOCAL']['eligible'] is True
                    and not any(id in by_id for id in ALLOWED + DENIED),
                    'Retired agent batch was adopted by successor')
        return
    require(isinstance(facts, dict), 'Missing original acquired app facts')
    integer(facts.get('generation'), 'Missing app acquisition generation', 1)
    require((facts['generation'] < draft['generation'] if stage == 'selected'
             else facts['generation'] == draft['generation']) and facts.get('query') == 'manager'
            and facts.get('hydrate') is False and facts.get('clientContext') is True,
            'App facts lost original query/client binding')
    expected_rows = [v for _, v in sorted(seed.items())
                     if v['id'] in source['workflow']['collaborators']
                     and 'manager' in v['profile']['display_name'].lower()]
    require(queries, 'Fact request lacked original directory query')
    expected_rows += queries[0]['results']
    require(exact(facts.get('rows'), expected_rows), 'Fact batch differs from actual original query rows')
    expected_status = 'loading' if loading else 'failed' if stage == 'facts-failed' else 'ready'
    require(facts.get('status') == expected_status, 'Raw fact failure was promoted to success')
    fact_requests = [r for r in server['requests'] if r['phase'] == 'agent-fact']
    # Each retry starts a new original batch of receipts; failed raw evidence
    # remains independently preserved in the immutable server response ledger.
    start = max(i for i,r in enumerate(fact_requests) if r['method'] == 'client.init')
    replies = {r['serial']:r['result'] for r in server['responses']}
    receipts = [replies[r['serial']]['raw'] for r in fact_requests[start:]
                if r['serial'] in settled and replies[r['serial']]['error'] is None]
    require(exact(facts.get('receipts'), receipts), 'App raw receipts fabricated, reordered or omitted')
    if expected_status == 'ready':
        require(results == [r['id'] for r in expected_rows if r['id'] in ALLOWED or r['id'] in seed],
                'Acquired eligibility result order differs')
        require(all(by_id[id]['eligible'] is True and by_id[id]['eligibility'] == 'allowed' for id in ALLOWED),
                'Acquired ordinary/access/default agent was excluded')
        require(all(by_id[id]['eligible'] is False and by_id[id]['eligibility'] == 'excluded' for id in DENIED),
                'Ordinary/denied/disabled/wrong-bot app became eligible')
    else:
        require(results == [] and not any(id in by_id for id in ALLOWED + DENIED),
                'Incomplete app facts granted partial results')


def agent_native(native, controls, confirmation, stage, viewport, rectangle):
    draft = confirmation['managerDraft']
    query = native['settings']['managers']['query']
    require(query.get('readonly') is False, 'Original manager query cannot supersede app facts')
    retry = controls.get('workflow-permissions-manager-agent-retry')
    if stage == 'facts-failed':
        measured(retry, viewport, rectangle, disabled=False)
        require(retry.get('text') == 'Retry manager eligibility', 'Missing explicit app fact Retry')
    else:
        require(retry is None, 'Retry exposed without a failed original fact batch')
    rows = {r['id']: r for r in draft['rows']}
    for kind in ('selected', 'results'):
        for row in native['settings']['managers'][kind]:
            require(row.get('text') == rows[row['target']]['label'],
                    'Acquired agent label differs from actual native control')


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
        confirming = stage in CONFIRMING and not state['retired']
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
        if stage == 'about' or state['retired'] or stage == 'back' and not state['sent']:
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
        if confirming:
            agent_native(native, controls, confirmation, stage, viewport, rectangle)
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
    from workflow_management_manager_agents_exercises import exercises
    exercises(receipt, case, rows)
