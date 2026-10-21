"""Original native edit capture recovery; independent reply and wire custody.

The shared runner owns process exit, executable identity and closed-log checks.
"""
import copy
import runpy
from pathlib import Path

O = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
N, G, R, C, READ, H, S = (O[k] for k in ('N', 'G', 'R', 'C', 'READ', 'H', 'S'))
require, exact = O['require'], O['exact']
STAGES = ('draft', 'editing', 'capture-failed', 'capture-recovered', 'source-stale',
          'cancelled', 'unconfirmed', 'unconfirmed-capture-failed', 'unconfirmed-recovered',
          'unconfirmed-checked', 'observed', 'reopened')
READS = (4, 8, 8, 8, 8, 8, 20, 20, 20, 24, 28, 32)
POLICIES = (0, 4, 4, 4, 4, 4, 12, 12, 12, 12, 12, 12)
ACTIVE = (1, 2, 3, 4, 6, 7, 8, 9)
FAILED = 'The original edit capture stopped. Retry capture before saving or leaving.'
STALE = 'The original comment changed. Your edit and reply draft are retained.'
RECOVERY = ' Recovery edit B \u65e5\U0001f98a'
UNKNOWN = ' Unconfirmed recovery C \u4e2d\U0001f642'
SOCKET_TEXT = 'Socket changed original comment \u65e5'
TS, CHANNEL = O['TS'], O['CHANNEL']


def changed_row():
    row = O['received']()
    row['text'] = SOCKET_TEXT
    row['blocks'][0]['elements'][0]['elements'][0]['text'] = SOCKET_TEXT
    row['edited'] = dict(user=O['SELF'], ts='1781525900.000001')
    return row


def controls(i):
    if i not in ACTIVE:
        return [('slack-list-comments-back', False), ('slack-list-comment-send', False)]
    result = [('slack-list-comment-edit-cancel', i not in (1, 3, 4)),
              ('slack-list-comment-edit-save', i not in (1, 3))]
    if i in (6, 8, 9):
        result.append(('slack-list-comment-edit-check', False))
    if i in (2, 7):
        result.append(('slack-list-comment-edit-retry', False))
    return result


def notice(i, *, model=False):
    return {2: FAILED, 4: None if model else STALE, 6: O['UNKNOWN_NOTICE'],
            7: FAILED, 8: O['UNKNOWN_NOTICE'], 9: O['CHECKED_NOTICE']}.get(i)


def witness(value, blocks):
    require(isinstance(value, dict) and set(value) == {'ok', 'scope', 'mount', 'generation', 'blocks'}
        and value['ok'] is True and isinstance(value['scope'], str)
        and isinstance(value['mount'], str) and bool(value['mount'])
        and isinstance(value['generation'], str) and value['generation'].isascii()
        and value['generation'].isdecimal() and str(int(value['generation'])) == value['generation']
        and int(value['generation']) > 0 and exact(value['blocks'], blocks), 'Wrong original native edit witness')


def capture_state(model, g, p, i, first, unknown):
    active, cap = model['edit'].get('active'), model.get('capture')
    remembered = p.get('firstEdit' if i < 6 else 'unknownEdit')
    blocks = first if i < 6 else unknown
    if i not in ACTIVE:
        require(active is None and cap is None, 'Inactive edit retained a capture/session')
    else:
        require(isinstance(cap, dict) and set(cap) == {'scope', 'generation', 'mount', 'failed', 'canSave',
            'canCancel', 'canCheck', 'canRetry', 'busy'}, 'Missing original capture state')
        require(R['integer'](cap['generation'], 1) and str(cap['generation']) == remembered['generation']
            and cap['mount'] == remembered['mount'] and cap['scope'] == remembered['scope'], 'Capture recovery replaced the mount or generation')
        require(all(cap[k] is v for k, v in dict(failed=i in (2, 7), canSave=i in (1, 3),
            canCancel=i in (1, 3, 4), canCheck=i in (6, 8, 9), canRetry=i in (2, 7), busy=False).items()),
            'Capture failure/recovery changed permitted actions')
        require(isinstance(active, dict) and active.get('scope') == remembered['scope']
            and active.get('generation') == cap['generation'] and active.get('ts') == TS
            and active['scope'] == f"list-edit:{model['comments']['comments']['generation']}:{active['generation']}"
            and R['integer'](active.get('revision'), 1) and active.get('current') is (i != 4)
            and active.get('busy') is False and active.get('composing') is False
            and active.get('nativeReady') is (i not in (2, 7)) and active.get('captureFailed') is (i in (2, 7))
            and exact(active.get('source'), O['received']()) and exact(active.get('preserved'), O['received']()['blocks'][1:])
            and exact(active.get('blocks'), blocks) and active.get('notice') == notice(i, model=True)
            and active.get('attempt') == (model['edit']['attempts'][0]['id'] if i >= 6 else None),
            'Original edit source/buffer/currentness was lost')
    fault = g.get('captureFault')
    if i in (0, 1, 6):
        require(fault is None, 'Native fault existed before original one-shot injection')
    else:
        reference = p['firstEdit'] if i <= 5 else p['unknownEdit']
        require(exact(fault, dict(scope=reference['scope'], mount=reference['mount'], generation=reference['generation'],
            count=1, armed=False, originalNode=i in ACTIVE, apiRestored=True)), 'Snapshot fault was replayed, replaced, or left installed')


def geometry(g, i, reply, first, unknown):
    viewport = g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport, list)
        and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport), 'Recovery native capture/viewport failed')
    require(g.get('editing') is (i in ACTIVE) and g.get('inertTextPaint') is None, 'Wrong edit mount/paint mode')
    measured = g.get('controls')
    require(isinstance(measured, list) and [(c.get('id'), c.get('disabled')) for c in measured] == controls(i),
        'Capture/uncertain/stale controls are not correctly locked')
    for c in measured:
        require(c.get('visible') is True and c.get('hit') is True, 'Recovery controls clipped or covered')
        N['rectangle'](c.get('rect', {}), viewport)
    N['text_proof'](g.get('text'), viewport)
    native = g.get('reply', {})
    require(exact(native.get('blocks'), reply) and native.get('sameNode') is (i != 11)
        and native.get('hidden') is (i in ACTIVE) and native.get('inert') is (i in ACTIVE)
        and native.get('visible') is (i not in ACTIVE) and native.get('composing') is False
        and g.get('replyNotice') is None, 'Independent reply was changed, exposed, or replaced')
    if i in ACTIVE:
        edit = g.get('edit', {})
        require(exact(edit.get('blocks'), first if i < 6 else unknown) and edit.get('editable') is True
            and edit.get('sameNode') is True, 'Recovery replaced or changed original native edit buffer')
        N['rectangle'](edit.get('rect', {}), viewport)
    else:
        require(g.get('edit') is None, 'Inactive native edit survived')
        N['rectangle'](native.get('rect', {}), viewport)
    if notice(i) is None:
        require(g.get('notice') is None, 'Unexpected edit failure notice')
    else:
        measured = g.get('notice')
        require(isinstance(measured, dict) and measured.get('text') == notice(i)
            and measured.get('visible') is True and measured.get('hit') is True, 'Recovery/stale/check notice not visible')
        N['rectangle'](measured.get('rect', {}), viewport)
    expected = changed_row() if i in (4, 5) else O['update_row'](unknown) if i >= 10 else O['received']()
    body = g.get('row', {}).get('text')
    require(isinstance(body, str) and all(t in body for t in expected['text'].split('\n'))
        and 'Second editable line.' in body and 'Preserved original context' in body
        and RECOVERY not in body and (UNKNOWN in body) is (i >= 10)
        and (SOCKET_TEXT in body) is (i in (4, 5)), 'Unsent edit leaked to row or readback/source change missing')
    return viewport


def source_event(proof, rows):
    frame = dict(type='message', subtype='message_changed', channel=CHANNEL, message=changed_row())
    expected_witness = dict(kind='Changed', outerChannel=CHANNEL, typedTs=TS, typedChannel=CHANNEL, raw=changed_row())
    require(isinstance(proof, dict) and set(proof) == {'team', 'generation', 'frame', 'parsed', 'witness', 'before', 'after'}
        and proof['team'] == 'T1' and R['integer'](proof['generation']) and proof['parsed'] is True
        and exact(proof['frame'], frame) and exact(proof['witness'], expected_witness), 'Selected event lost parsed original provenance')
    for key, index in (('before', 3), ('after', 4)):
        require(exact(proof[key], {k: rows[index]['model'][k] for k in ('edit', 'comments')}),
            'Selected event witness differs from actual retained stages')
    before, after = proof['before'], proof['after']
    require(exact(before['comments']['comments']['page'], after['comments']['comments']['page'])
        and exact(before['comments']['draftStore'], after['comments']['draftStore'])
        and exact(before['edit']['active']['blocks'], after['edit']['active']['blocks'])
        and before['edit']['active']['current'] is True and after['edit']['active']['current'] is False,
        'Selected event replaced original HTTP/reply/edit custody')


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    require(receipt.get('case') == 'own-edit-recovery' and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture') == fixture
        and fixture in C['FIXTURES'], 'Wrong own-edit recovery terminal')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES), 'Missing recovery stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Wrong original canonical inventory')
    initial = rows[0]['model']['comments']
    seed, reply = rows[0]['proof']['replySeed'], rows[0]['proof']['replyBlocks']
    scope = dict(workspace='T1', user=O['SELF'], list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'), scope) and exact(seed.get('blocks'), G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong independent reply seed')
    O['reply_content'](reply, seed['blocks'])
    first, unknown = rows[1]['proof']['recoveryBlocks'], rows[6]['proof']['unknownBlocks']
    O['edited_content'](first, RECOVERY)
    O['edited_content'](unknown, UNKNOWN)
    first_witness, unknown_witness = rows[1]['proof']['firstEdit'], rows[6]['proof']['unknownEdit']
    witness(first_witness, first)
    witness(unknown_witness, unknown)
    require(all(first_witness[k] != unknown_witness[k] for k in ('scope', 'mount', 'generation')), 'Second edit reused the cancelled mount')
    source_event(rows[4]['proof'].get('sourceChanged'), rows)
    journal, previous = rows[0]['journal'], None
    for i, row in enumerate(rows):
        p, model = row['proof'], row['model']
        m, f, edit = (model[k] for k in ('comments', 'fixture', 'edit'))
        require(exact(p.get('source'), dict(received=O['received'](), boot=O['boot']())) and exact(p.get('replySeed'), seed)
            and exact(p.get('replyBlocks'), reply) and p.get('actions') == []
            and p.get('runtimeForgotten', False) is (i == 11), 'Recovery rewrote original source/reply proof')
        for key, expected, start in (('recoveryBlocks', first, 1), ('unknownBlocks', unknown, 6),
                ('firstEdit', first_witness, 1), ('unknownEdit', unknown_witness, 6),
                ('firstFailure', dict(first_witness, armed=True), 2), ('unknownFailure', dict(unknown_witness, armed=True), 7),
                ('sourceChanged', rows[4]['proof']['sourceChanged'], 4), ('staleSave', dict(ok=True, disabled=True), 4),
                ('blockedControls', dict(ok=True, saveDisabled=True, cancelDisabled=True), 6)):
            require(exact(p.get(key), expected if i >= start else None), 'Original recovery/event/action proof changed: ' + key)
        require(m.get('account') == initial.get('account') and R['uuid'](m.get('account'))
            and exact(m.get('epoch'), initial.get('epoch')) and R['integer'](m.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False
            and exact(m.get('writes'), 0) and exact(m.get('networkCalls'), 0), 'Recovery changed disconnected original owner')
        require(exact(m.get('document'), initial.get('initialDocument')) and initial['initialDocument'].get('id') == READ['LIST']
            and exact(m.get('initialDocument'), initial.get('initialDocument')) and exact(m.get('records'), touched)
            and exact(m.get('initialRecords'), touched) and exact(m.get('unrelated'), unrelated)
            and exact(m.get('initialUnrelated'), unrelated), 'Recovery changed canonical List records')
        c = m['comments']
        require(c.get('active') is True and c.get('record') == READ['KNOWN'] and c.get('root') == READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None and c.get('visibleTs') == [TS]
            and c.get('deleted') == [] and m.get('detail', {}).get('record') == READ['KNOWN']
            and (c.get('generation') == initial['comments']['generation']) is (i != 11), 'Recovery changed original visit')
        ledger = m['ledger']
        require(len(ledger.get('requests', [])) == READS[i] and exact(ledger.get('completed'), READS[i])
            and exact(ledger.get('replies'), READS[i] // 4) and exact(ledger.get('cancelled'), 0)
            and ledger.get('held') is False, 'Recovery issued unexpected reads or replayed Save')
        C['validate_reads'](ledger['requests'])
        require([q['method'] for q in ledger['requests']] == ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'] * (READS[i] // 4)
            and all(q['args'].get('id', READ['KNOWN']) == READ['KNOWN'] and (q['method'] != 'conversations.replies'
                or q['args']['ts'] == READ['ROOT_TS']) for q in ledger['requests']), 'Recovery read another item/root')
        require(exact(m.get('composeLedger'), dict(prepared=[], posts=[], preflights=[], policies=[], messages=[], published=False, held=False, heldPost=False))
            and exact(m.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])), 'Recovery dispatched an unrelated write')
        count = int(i >= 6)
        require(f.get('enabled') is True and exact(f.get('received'), O['received']()) and exact(f.get('boot'), O['boot']())
            and exact(f.get('networkCalls'), 0) and f.get('httpCapability') is False
            and exact(f.get('policy'), [dict(request=q, response=r) for _ in range(POLICIES[i] // 4) for q, r in O['policy_cycle']()]),
            'Recovery lost closed original policy acquisition')
        require(isinstance(f.get('prepared'), list) and len(f['prepared']) == count and exact(f.get('updates'), f['prepared']), 'Capture Retry/stale Save replayed update')
        for request in f['prepared']:
            O['update_wire'](request, unknown)
        require(exact(f.get('responses'), [dict(disposition='unconfirmed', raw=None)] if count else [])
            and exact(f.get('futureRows'), [O['update_row'](unknown)] if count else []) and f.get('held') is False
            and f.get('releases') == [] and f.get('published') is (i >= 10)
            and exact(f.get('publishedRow'), O['update_row'](unknown) if i >= 10 else None), 'Readback fabricated an ACK or rewrote uncertain wire receipt')
        expected_page = copy.deepcopy(initial['comments']['page'])
        if i >= 10:
            expected_page['messages'] = [O['update_row'](unknown) if r.get('ts') == TS else r for r in expected_page['messages']]
        require(exact(c.get('page'), expected_page) and exact(c.get('hasMore'), initial['comments'].get('hasMore')), 'Socket/edit mutated immutable raw Page or pagination')
        known = O['one_message'](c.get('pageMessages'))
        expected = changed_row() if i in (4, 5) else O['update_row'](unknown) if i >= 10 else O['received']()
        require(all(exact(known.get(k), expected[k]) for k in ('ts', 'thread_ts', 'user', 'text', 'blocks', 'reactions'))
            and exact(known.get('edited'), expected.get('edited')), 'Typed original/stale/readback row changed')
        require(exact([r for r in c['pageMessages'] if r.get('ts') != TS], [r for r in initial['comments']['pageMessages'] if r.get('ts') != TS])
            and c.get('reactions') == [dict(ts=TS, reactions=[dict(name='wave', count=1)])], 'Recovery changed unrelated root/reactions')
        store = m['draftStore']
        require(len(store.get('drafts', [])) == 1, 'Independent reply missing')
        draft = store['drafts'][0]
        require(exact(draft.get('scope'), scope) and exact(draft.get('blocks'), reply) and draft.get('attempt') is None
            and draft.get('native') is True and draft.get('error') is None
            and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'terminalPending', 'captureFailed'))
            and store.get('active') == f"{draft['id']}:{c['generation']}:{draft['generation']}", 'Recovery consumed/changed independent reply')
        S['inventory'](row['journal'], scope)
        S['paired'](draft, row['journal'])
        require(exact(row['journal'], journal), 'Recovery changed independent SQLite reply')
        require(edit.get('opening') is False and exact(edit.get('events'), int(i >= 4)) and edit.get('retired') == []
            and isinstance(edit.get('attempts'), list) and len(edit['attempts']) == count, 'Unexpected retirement/event/attempt')
        if count:
            a = edit['attempts'][0]
            require(R['integer'](a.get('id'), 1) and a.get('ts') == TS and exact(a.get('blocks'), unknown + O['received']()['blocks'][1:])
                and a.get('phase') == ('observed' if i >= 10 else 'unknown') and a.get('receipt') is None and exact(a.get('retryAt'), 0), 'Recovery resent or reclassified original uncertain attempt')
            require(a['id'] == rows[6]['model']['edit']['attempts'][0]['id'], 'Uncertain original attempt replaced')
        capture_state(model, row['geometry'], p, i, first, unknown)
        viewport = geometry(row['geometry'], i, reply, first, unknown)
        for key, expected, id in (('recoveries', ['capture-recovered'] * int(i >= 3) + ['unconfirmed-recovered'] * int(i >= 8), 'slack-list-comment-edit-retry'),
                ('checks', ['unconfirmed-checked'] * int(i >= 9) + ['observed'] * int(i >= 10), 'slack-list-comment-edit-check')):
            values = p.get(key, [])
            require(isinstance(values, list) and [v.get('stage') for v in values] == expected, 'Missing actual recovery/check clicks')
            for value in values:
                O['native_button'](value.get('value'), id, viewport)
        if i >= 6:
            O['native_button'](p.get('refreshAfterStale'), 'slack-list-comments-refresh', viewport)
        else:
            require(p.get('refreshAfterStale') is None, 'Canonical source refreshed before stale refusal')
        if previous:
            old = previous['model']['comments']
            require(exact(ledger['requests'][:len(old['ledger']['requests'])], old['ledger']['requests']), 'Read history rewritten')
            prior_draft = old['draftStore']['drafts'][0]
            require((draft['id'] == prior_draft['id']) is (i != 11) and (draft['generation'] == prior_draft['generation']) is (i != 11), 'Reply replaced before real SQLite reopen')
            for key in ('policy', 'prepared', 'updates', 'responses', 'futureRows'):
                previous_values = previous['model']['fixture'][key]
                require(exact(f[key][:len(previous_values)], previous_values), 'Original wire/policy history rewritten')
        path = Path(directory) / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == path, 'Foreign recovery capture')
        if png:
            H['png_signature'](path, dimensions)
        previous = row
    require(exact(receipt.get('final'), rows[-1]['model']), 'Final recovery model differs')
    return receipt
