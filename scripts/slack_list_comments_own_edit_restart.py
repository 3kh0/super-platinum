"""Independent per-process and cross-process own-edit SQLite recovery oracle."""
import copy
import runpy
from pathlib import Path

O = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
REC = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_recovery.py')))
N, G, R, C, READ, H, S = (O[k] for k in ('N', 'G', 'R', 'C', 'READ', 'H', 'S'))
require, exact = O['require'], O['exact']
STAGES = {
    'own-edit-restart-unsent-exercise': ('exercise-draft', 'exercise-edit'),
    'own-edit-restart-unknown-exercise': ('exercise-draft', 'exercise-edit', 'exercise-unconfirmed'),
    'own-edit-restart-unsent-restore': ('restored-reply', 'restored-edit', 'cancelled', 'reopened'),
    'own-edit-restart-unknown-restore': ('restored-reply', 'restored-edit', 'unconfirmed-checked', 'newer-edit', 'observed-newer', 'cancelled', 'reopened'),
}
EDIT = ' Persisted edit B \u65e5\U0001f98a'
NEWER = ' Newer edit C \u65e5\U0001f642'
RESTORED = 'Your saved edit was recovered. Current permissions are checked before any new save.'
RESTORED_UNKNOWN = 'Your saved edit was recovered. Its previous save is unconfirmed and will not be sent again. Check the result.'
OBSERVED = 'The saved content was found. Your newer edit is retained; Cancel discards that edit and returns to your reply.'
TS, CHANNEL = O['TS'], O['CHANNEL']


def counts(case):
    if case.endswith('-exercise'):
        return (4, 8, 12) if '-unknown-' in case else (4, 8)
    return (4, 4, 8, 8, 12, 12, 16) if '-unknown-' in case else (4, 4, 4, 8)


def native_state(case, name):
    active = name not in ('exercise-draft', 'restored-reply', 'cancelled', 'reopened')
    unknown = name == 'exercise-unconfirmed' or ('-unknown-' in case and name in ('restored-edit', 'unconfirmed-checked', 'newer-edit'))
    observed = name == 'observed-newer'
    notice = O['UNKNOWN_NOTICE'] if name == 'exercise-unconfirmed' else (
        (RESTORED_UNKNOWN if '-unknown-' in case else RESTORED) if name == 'restored-edit' else
        O['CHECKED_NOTICE'] if name in ('unconfirmed-checked', 'newer-edit') else OBSERVED if observed else None)
    return active, unknown, observed, notice


def seed_value(model):
    m = model['comments']
    return dict(workspace='T1', user=O['SELF'], ts=TS,
        target=dict(document=m['initialDocument'], record=m['records'][READ['KNOWN']], channel=CHANNEL, root=READ['ROOT_TS']),
        http=m['comments']['page'], editable=O['received']()['blocks'][:1], preserved=O['received']()['blocks'][1:])


def inventory(value, scope, seed, blocks, *, phase=None, original_attempt=None):
    require(isinstance(value, dict) and set(value) == {'scope', 'ts', 'revision', 'capture', 'seed', 'blocks', 'attempt', 'retry_at'}
        and exact(value['scope'], scope) and value['ts'] == TS and R['integer'](value['revision'])
        and exact(value['retry_at'], 0) and exact(value['seed'], seed) and exact(value['blocks'], blocks), 'Wrong independent persisted edit inventory')
    require((value['capture'] == '' and value['revision'] == 0) or (R['uuid'](value['capture']) and value['revision'] > 0), 'Missing durable edit capture identity')
    attempt = value['attempt']
    if phase is None:
        require(attempt is None, 'Unexpected persisted edit attempt')
        return
    require(isinstance(attempt, dict) and set(attempt) == {'id', 'record', 'capture', 'phase', 'raw', 'readback', 'retry_at'}
        and R['uuid'](attempt['id']) and R['uuid'](attempt['capture']) and attempt['phase'] == phase
        and attempt['raw'] is None and exact(attempt['retry_at'], 0), 'Persisted original update identity/disposition changed')
    record = attempt['record']
    require(isinstance(record, dict) and set(record) == {'version', 'seed', 'blocks', 'text'}
        and exact(record['version'], 1) and exact(record['seed'], seed), 'Wrong inert original update record')
    O['edited_content'](record['blocks'][:-1], EDIT)
    require(exact(record['blocks'][-1:], O['received']()['blocks'][1:])
        and record['text'] == G['display'](record['blocks'][:-1]), 'Persisted update lost rich/context body')
    if original_attempt is not None:
        require(all(exact(attempt[k], original_attempt[k]) for k in ('id', 'record', 'capture', 'raw', 'retry_at')), 'Readback rewrote original durable attempt')
    if phase == 'Uncertain':
        require(attempt['readback'] is None, 'Unconfirmed update fabricated readback')
    else:
        readback = attempt['readback']
        expected_page = copy.deepcopy(seed['http'])
        expected_page['messages'] = [O['update_row'](record['blocks'][:-1]) if m.get('ts') == TS else m for m in expected_page['messages']]
        require(isinstance(readback, dict) and set(readback) == {'target', 'raw'}
            and exact(readback['target'], seed['target']) and exact(readback['raw'], expected_page), 'Observed edit lacks separate exact canonical readback')


def geometry(g, case, name, reply, blocks, remembered):
    active, unknown, observed, expected_notice = native_state(case, name)
    viewport = g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport, list)
        and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport)
        and g.get('editing') is active and g.get('inertTextPaint') is None and g.get('captureFault') is None,
        'Restart native capture/mount changed')
    wanted = [('slack-list-comment-edit-cancel', unknown), ('slack-list-comment-edit-save', unknown or observed)] if active else [
        ('slack-list-comments-back', False), ('slack-list-comment-send', False)]
    if unknown:
        wanted.append(('slack-list-comment-edit-check', False))
    controls = g.get('controls')
    require(isinstance(controls, list) and [(c.get('id'), c.get('disabled')) for c in controls] == wanted, 'Restored unknown save can replay or controls changed')
    for c in controls:
        require(c.get('visible') is True and c.get('hit') is True, 'Restart control obscured')
        N['rectangle'](c.get('rect', {}), viewport)
    N['text_proof'](g.get('text'), viewport)
    reply_node = g.get('reply', {})
    require(exact(reply_node.get('blocks'), reply) and reply_node.get('sameNode') is (name != 'reopened')
        and reply_node.get('hidden') is active and reply_node.get('inert') is active
        and reply_node.get('visible') is (not active) and reply_node.get('composing') is False
        and g.get('replyNotice') is None, 'Original independent reply changed or became exposed')
    if active:
        editor = g.get('edit', {})
        require(exact(editor.get('blocks'), blocks) and editor.get('sameNode') is True and editor.get('editable') is True,
            'Restored edit native buffer or mounted node changed')
        N['rectangle'](editor.get('rect', {}), viewport)
    else:
        require(g.get('edit') is None, 'Inactive restart edit survived')
        N['rectangle'](reply_node.get('rect', {}), viewport)
    if expected_notice is None:
        require(g.get('notice') is None, 'Unexpected restart notice')
    else:
        notice = g.get('notice')
        require(isinstance(notice, dict) and notice.get('text') == expected_notice and notice.get('visible') is True
            and notice.get('hit') is True, 'Restored/uncertain/newer guidance missing')
        N['rectangle'](notice.get('rect', {}), viewport)
    body = g.get('row', {}).get('text')
    published = '-unknown-restore' in case and name in ('observed-newer', 'cancelled', 'reopened')
    expected = O['received']()['text'].split('\n')
    require(isinstance(body, str) and all(t in body for t in expected) and 'Preserved original context' in body
        and (EDIT in body) is published and NEWER not in body, 'Unsent/newer edit leaked to transcript or observed original missing')


def validate(fixture, receipt, directory, *, case, dimensions=None, png=True):
    require(case in STAGES and receipt.get('case') == case and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture') == fixture and fixture in C['FIXTURES'], 'Wrong restart terminal')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[case]), 'Missing exact restart stages')
    restore, unknown_case = case.endswith('-restore'), '-unknown-' in case
    first = rows[0]
    initial = first['model']['comments']
    p0 = first['proof']
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Wrong original canonical inventory')
    scope = dict(workspace='T1', user=O['SELF'], list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    reply = p0['replyBlocks']
    O['reply_content'](reply, G['seed_blocks']())
    if restore:
        require(p0.get('replySeed') is None and exact(p0.get('initialReplyInventory'), first['journal']), 'Restore seeded/replaced independent reply')
    else:
        seed = p0.get('replySeed', {})
        require(exact(seed.get('scope'), scope) and exact(seed.get('blocks'), G['seed_blocks']())
            and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong original rich reply seed')
        require(exact(p0['initialReplyInventory'], dict(scope=scope, revision=0, capture='', blocks=[], attempt=None)), 'Exercise reused an existing reply database')
    initial_seed = seed_value(first['model'])
    edited = rows[1]['proof']['editedBlocks']
    O['edited_content'](edited, EDIT)
    newer = rows[3]['proof']['newerBlocks'] if restore and unknown_case else None
    if newer is not None:
        O['edited_content'](newer, EDIT + NEWER)
    remembered = rows[1]['proof']['editRemembered']
    REC['witness'](remembered, edited)
    initial_edit = p0['initialEditInventory']
    inventory(initial_edit, scope, initial_seed if restore else None, edited if restore else [],
        phase='Uncertain' if restore and unknown_case else None)
    require(exact(first['editInventory'], initial_edit), 'Reply opening mutated original edit inventory')
    previous = None
    for i, row in enumerate(rows):
        name, p, model, inv = row['stage'], row['proof'], row['model'], row['editInventory']
        m, f, edit = (model[k] for k in ('comments', 'fixture', 'edit'))
        c, ledger = m['comments'], m['ledger']
        active, unknown, observed, expected_notice = native_state(case, name)
        published = restore and unknown_case and i >= 4
        cleared = name in ('cancelled', 'reopened')
        blocks = newer if restore and unknown_case and i >= 3 and not cleared else edited
        expected_attempt = unknown_case and (restore or name == 'exercise-unconfirmed') and not cleared
        phase = 'Observed' if observed else 'Uncertain' if expected_attempt else None
        require(exact(p.get('source'), dict(received=O['received'](), boot=O['boot']())) and exact(p.get('replyBlocks'), reply)
            and exact(p.get('replySeed'), p0['replySeed']) and exact(p.get('initialEditInventory'), initial_edit)
            and exact(p.get('initialReplyInventory'), p0['initialReplyInventory']) and exact(p.get('processId'), ready['pid'])
            and p.get('scenario') == ('unknown' if unknown_case else 'unsent') and p.get('phase') == ('restore' if restore else 'exercise')
            and p.get('editSuffix') == EDIT and p.get('newerSuffix') == NEWER
            and p.get('runtimeForgotten', False) is (name == 'reopened'), 'Restart process/source proof changed')
        require(exact(p.get('editedBlocks'), edited if i >= 1 else None) and exact(p.get('editRemembered'), remembered if i >= 1 else None)
            and exact(p.get('newerBlocks'), newer if restore and unknown_case and i >= 3 else None), 'Original/newer edit proof changed')
        require(exact(p.get('blocked'), dict(ok=True, saveDisabled=True, cancelDisabled=True)
            if unknown_case and ((restore and i >= 1) or name == 'exercise-unconfirmed') else None), 'Unknown Save/Cancel blocking was not exercised')
        require(m.get('account') == initial.get('account') and R['uuid'](m.get('account')) and exact(m.get('epoch'), initial.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False and exact(m.get('writes'), 0) and exact(m.get('networkCalls'), 0), 'Original closed owner/network changed')
        require(exact(m.get('document'), initial.get('initialDocument')) and initial['initialDocument'].get('id') == READ['LIST']
            and exact(m.get('initialDocument'), initial['initialDocument']) and exact(m.get('records'), touched) and exact(m.get('initialRecords'), touched)
            and exact(m.get('unrelated'), unrelated) and exact(m.get('initialUnrelated'), unrelated), 'Restart changed canonical List inventory')
        require(c.get('active') is True and c.get('record') == READ['KNOWN'] and c.get('root') == READ['ROOT_TS']
            and c.get('visibleTs') == [TS] and c.get('loading') is False and c.get('error') is None and c.get('deleted') == []
            and m.get('detail', {}).get('record') == READ['KNOWN']
            and (c.get('generation') == initial['comments']['generation']) is (name != 'reopened'), 'Restart lost original comment visit')
        reads = counts(case)[i]
        require(len(ledger.get('requests', [])) == reads and exact(ledger.get('completed'), reads) and exact(ledger.get('replies'), reads // 4)
            and exact(ledger.get('cancelled'), 0) and ledger.get('held') is False, 'Unexpected restore/policy/read request')
        C['validate_reads'](ledger['requests'])
        require([q['method'] for q in ledger['requests']] == ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'] * (reads // 4)
            and all(q['args'].get('id', READ['KNOWN']) == READ['KNOWN'] and (q['method'] != 'conversations.replies' or q['args']['ts'] == READ['ROOT_TS']) for q in ledger['requests']), 'Restart read another record/root')
        require(exact(m.get('composeLedger'), dict(prepared=[], posts=[], preflights=[], policies=[], messages=[], published=False, held=False, heldPost=False))
            and exact(m.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])), 'Restart issued unrelated write')
        update_count = int(name == 'exercise-unconfirmed')
        require(f.get('enabled') is True and exact(f.get('received'), O['received']()) and exact(f.get('boot'), O['boot']())
            and f.get('httpCapability') is False and exact(f.get('networkCalls'), 0)
            and exact(f.get('policy'), [dict(request=q, response=r) for _ in range(i if not restore else 0) for q, r in O['policy_cycle']()]), 'Restored inert data acquired send authority or original policy changed')
        require(isinstance(f.get('prepared'), list) and len(f['prepared']) == update_count and exact(f.get('updates'), f['prepared']), 'Restore replayed update or original Save duplicated')
        for request in f['prepared']:
            O['update_wire'](request, edited)
        require(exact(f.get('responses'), [dict(disposition='unconfirmed', raw=None)] if update_count else [])
            and exact(f.get('futureRows'), [O['update_row'](edited)] if update_count else []) and f.get('held') is False and f.get('releases') == []
            and f.get('published') is published and exact(f.get('publishedRow'), O['update_row'](edited) if published else None), 'Original unknown response replaced by readback ACK')
        page = copy.deepcopy(initial['comments']['page'])
        if published:
            page['messages'] = [O['update_row'](edited) if r.get('ts') == TS else r for r in page['messages']]
        require(exact(c.get('page'), page) and exact(c.get('hasMore'), initial['comments'].get('hasMore')), 'Unsent edit changed original HTTP Page/cursor')
        known, wanted = O['one_message'](c.get('pageMessages')), O['update_row'](edited) if published else O['received']()
        require(all(exact(known.get(k), wanted[k]) for k in ('ts', 'thread_ts', 'user', 'text', 'blocks', 'reactions'))
            and exact(known.get('edited'), wanted.get('edited')) and exact([r for r in c['pageMessages'] if r.get('ts') != TS],
                [r for r in initial['comments']['pageMessages'] if r.get('ts') != TS]), 'Restored reply source/readback changed')
        require(c.get('reactions') == [dict(ts=TS, reactions=[dict(name='wave', count=1)])], 'Restart lost original reaction')
        store = m['draftStore']
        require(len(store.get('drafts', [])) == 1, 'Original reply missing')
        draft = store['drafts'][0]
        require(exact(draft.get('scope'), scope) and exact(draft.get('blocks'), reply) and draft.get('attempt') is None and draft.get('error') is None
            and draft.get('native') is True and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'terminalPending', 'captureFailed'))
            and store.get('active') == f"{draft['id']}:{c['generation']}:{draft['generation']}", 'Independent reply mutated or locked')
        S['inventory'](row['journal'], scope)
        S['paired'](draft, row['journal'])
        require(exact(row['journal'], first['journal']), 'Independent reply SQLite inventory changed')
        inventory(inv, scope, None if cleared or (not restore and i == 0) else initial_seed,
            [] if cleared or (not restore and i == 0) else blocks, phase=phase,
            original_attempt=initial_edit['attempt'] if restore and unknown_case and not cleared else None)
        require(edit.get('opening') is False and exact(edit.get('events'), 0) and edit.get('retired') == [], 'Restart unexpectedly retired original owner')
        attempts = edit.get('attempts')
        attempt_count = int(unknown_case and ((restore and i >= 1) or update_count))
        require(isinstance(attempts, list) and len(attempts) == attempt_count, 'Restored original attempt lost or duplicated')
        if attempts:
            a = attempts[0]
            require(R['integer'](a.get('id'), 1) and a.get('ts') == TS and exact(a.get('blocks'), edited + O['received']()['blocks'][1:])
                and a.get('phase') == ('observed' if published else 'unknown') and a.get('receipt') is None and exact(a.get('retryAt'), 0), 'Restored attempt became sendable/confirmed or lost body')
        cap, session = model.get('capture'), edit.get('active')
        if active:
            require(isinstance(cap, dict) and R['integer'](cap.get('generation'), 1)
                and str(cap['generation']) == remembered['generation'] and cap.get('scope') == remembered['scope'] and cap.get('mount') == remembered['mount']
                and all(cap.get(k) is v for k, v in dict(failed=False, canSave=not unknown and not observed, canCancel=not unknown,
                    canCheck=unknown, canRetry=False, busy=False).items()), 'Restored native mount/control authority changed')
            require(isinstance(session, dict) and session.get('scope') == remembered['scope'] and session.get('generation') == cap['generation']
                and session.get('scope') == f"list-edit:{c['generation']}:{cap['generation']}" and session.get('ts') == TS
                and session.get('current') is (not observed) and session.get('busy') is False and session.get('composing') is False
                and session.get('nativeReady') is True and session.get('captureFailed') is False and R['integer'](session.get('revision'))
                and exact(session.get('source'), O['received']()) and exact(session.get('preserved'), O['received']()['blocks'][1:])
                and exact(session.get('blocks'), blocks) and session.get('notice') == expected_notice
                and session.get('attempt') == (attempts[0]['id'] if attempts else None), 'Original restored edit/session changed')
        else:
            require(cap is None and session is None, 'Inactive restart edit survived')
        geometry(row['geometry'], case, name, reply, blocks, remembered)
        if published:
            a = initial_edit['attempt']
            require(exact(p.get('publication'), dict(attemptId=a['id'], record=a['record'], row=O['update_row'](edited),
                originalReceipt=None, updates=0, fromPersistedRecord=True)), 'Synthetic readback was not independently bound to original inert record')
        else:
            require(p.get('publication') is None, 'Readback published before explicit Check')
        if previous:
            old = previous['model']['comments']
            require(exact(ledger['requests'][:len(old['ledger']['requests'])], old['ledger']['requests']), 'Original read history rewritten')
            prior_draft = old['draftStore']['drafts'][0]
            require((draft['id'] == prior_draft['id']) is (name != 'reopened') and (draft['generation'] == prior_draft['generation']) is (name != 'reopened'), 'Reply mount changed without actual reopen')
            prior_inv = previous['editInventory']
            unchanged = restore and (name in ('restored-edit', 'unconfirmed-checked', 'reopened'))
            if unchanged:
                require(exact(inv, prior_inv), 'Inert load/negative Check/reopen rewrote saved edit')
            else:
                require(inv['revision'] > prior_inv['revision'], 'Native change/settlement not durably committed')
            if name == 'newer-edit':
                require(inv['capture'] != prior_inv['capture'] and exact(inv['attempt'], prior_inv['attempt']), 'Newer buffer replaced original uncertain attempt')
            if name == 'observed-newer':
                require(inv['capture'] == prior_inv['capture'] and exact(inv['blocks'], prior_inv['blocks']), 'Older positive readback cleared newer buffer')
        path = Path(directory) / f'{fixture}-{name}.png'
        require(Path(row.get('capture', '')) == path, 'Foreign restart capture')
        if png:
            H['png_signature'](path, dimensions)
        previous = row
    require(exact(receipt.get('final'), rows[-1]['model']) and exact(receipt.get('editInventory'), rows[-1]['editInventory']), 'Final durable/native restart state differs')
    return receipt


def validator(case):
    require(case in STAGES, 'Unknown exact restart case')
    return lambda fixture, receipt, directory, **kwargs: validate(fixture, receipt, directory, case=case, **kwargs)


def sql_custody(snapshot, row, *, history):
    require(snapshot.get('account') == row['model']['comments']['account'], 'SQLite account differs from original owner')
    decoded = snapshot['inventories']
    require(all(decoded.get(name, []) == [] and snapshot['tables'].get(name, []) == []
        for name in ('delete_state', 'delete_history')), 'Own-edit recovery contains deletion authority')
    require(exact(decoded.get('drafts'), [row['journal']]) and decoded.get('history') == []
        and exact(decoded.get('edit_drafts'), [row['editInventory']]) and exact(decoded.get('edit_history'), history),
        'Independent read-only SQL inventory differs from native original records')
    scope = row['journal']['scope']
    for table in ('drafts', 'edit_drafts', 'edit_history'):
        for record in snapshot['tables'][table]:
            owner = __import__('json').loads(record['owner'])
            require(exact(owner, scope if table == 'drafts' else [scope, TS]), 'Persisted row owner key changed')
            if table == 'edit_history':
                require(record['id'] == history[0]['id'], 'Durable attempt history key changed')


def validate_pair(mode, exercise, restore, original_db, copied_db, final_db):
    require(mode in ('unsent', 'unknown') and exercise['case'] == 'own-edit-restart-' + mode + '-exercise'
        and restore['case'] == 'own-edit-restart-' + mode + '-restore' and exercise['fixture'] == restore['fixture'], 'Wrong paired restart cases')
    last, first, final = exercise['stages'][-1], restore['stages'][0], restore['stages'][-1]
    require(exercise['readiness']['pid'] != restore['readiness']['pid'] and exercise['readiness']['nonce'] != restore['readiness']['nonce'], 'Restart did not use a fresh process owner')
    require(exact(first['proof']['initialEditInventory'], last['editInventory']) and exact(first['editInventory'], last['editInventory'])
        and exact(first['proof']['initialReplyInventory'], last['journal']) and exact(first['journal'], last['journal'])
        and exact(first['proof']['replyBlocks'], last['proof']['replyBlocks']) and first['proof']['replySeed'] is None,
        'Fresh process did not restore exact original edit/reply inventories')
    require(original_db['sha256'] == copied_db['sha256'] and exact(original_db['tables'], copied_db['tables'])
        and original_db['path'] != copied_db['path'], 'Second process did not receive DB-only exact bytes')
    original_attempt = last['editInventory']['attempt']
    sql_custody(original_db, last, history=[original_attempt] if mode == 'unknown' else [])
    sql_custody(copied_db, first, history=[original_attempt] if mode == 'unknown' else [])
    history = [restore['stages'][4]['editInventory']['attempt']] if mode == 'unknown' else []
    sql_custody(final_db, final, history=history)
    if mode == 'unknown':
        require(history[0]['phase'] == 'Observed' and history[0]['raw'] is None
            and all(exact(history[0][k], original_attempt[k]) for k in ('id', 'record', 'capture', 'retry_at')),
            'Cancel/positive Check lost original unknown history or fabricated ACK')
    require(all(row['model']['fixture']['updates'] == [] and row['model']['fixture']['prepared'] == [] for row in restore['stages']),
        'Fresh process replayed original update')
    return True
