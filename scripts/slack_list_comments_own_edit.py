"""Closed own-comment update: original HTTP custody and independent reply journal.

The shared runner separately verifies process exit, binary identity and closed logs.
This oracle admits only the seven-stage successful ordinary-rich edit case.
"""
import copy
import json
import math
import runpy
from pathlib import Path

N = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_navigation.py')))
G = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_channel_completion.py')))
R, C, READ, H, S = (N[k] for k in ('R', 'C', 'READ', 'H', 'S'))
require, exact = G['require'], G['exact']
STAGES = {'own-edit': ('draft', 'editing', 'cancelled', 'saving', 'saved', 'readback', 'reopened')}
STAGES['own-edit-custody'] = ('draft', 'composition-refused', 'rejected', 'cancelled',
    'unconfirmed', 'unconfirmed-checked', 'observed', 'reopened')
TS, CHANNEL, SELF = '1781524801.000001', 'CCOMMENT01', 'ULIST0001'
REPLY = 'Independent reply A \u4e2d\U0001f642'
CANCEL = ' Cancel this edit'
CHANGE = ' Saved change B \u65e5\U0001f98a'
RELEASE = dict(delivered=True, cancelledBeforeReply=False)
READS = (4, 8, 8, 16, 16, 20, 24)
POLICIES = (0, 1, 1, 3, 3, 3, 3)
REJECTED_TEXT = ' Rejected edit B \u65e5\U0001f98a'
UNKNOWN_TEXT = ' Unconfirmed edit C \u4e2d\U0001f642'
COMPOSING_ERROR = 'Finish composing or close the link editor and recover the reply draft before editing a comment.'
REJECTED_NOTICE = 'Slack did not accept the edit. Your edit and reply draft are retained.'
UNKNOWN_NOTICE = 'The edit may have reached Slack. Check the result before saving again; both drafts are retained.'
CHECKED_NOTICE = 'The edited content could not be confirmed. Both drafts are retained; this save will not be sent again.'
CUSTODY_READS = (4, 4, 12, 12, 20, 24, 28, 32)
CUSTODY_POLICIES = (0, 0, 2, 2, 4, 4, 4, 4)
REJECTED_RAW = dict(ok=False, error='cant_update_message')


def received():
    return dict(type='message', ts=TS, thread_ts=READ['ROOT_TS'], user=SELF,
        text='Original own comment \u4e2d\U0001f642.\nSecond editable line.',
        edit_fixture_opaque={'keep': 'original receipt'},
        reactions=[dict(name='wave', count=1, users=['ULISTMEM1'])],
        blocks=[dict(type='rich_text', block_id='own-edit-rich', elements=[
            dict(type='rich_text_section', elements=[dict(type='text', text='Original own comment \u4e2d\U0001f642.', style={'bold': True})]),
            dict(type='rich_text_section', elements=[dict(type='text', text='Second editable line.', style={'italic': True})])]),
            dict(type='context', block_id='own-edit-context', elements=[dict(type='plain_text', text='Preserved original context')], edit_context_opaque={'keep': True})])


def reply_content(blocks, seed):
    expected = copy.deepcopy(seed)
    expected[0]['elements'][0]['elements'][-1] = dict(type='text', text=REPLY)
    require(exact(G['rich'](blocks), G['rich'](expected)), 'Independent reply text/style/opaque content changed')


def edited_content(blocks, suffix):
    expected = received()['blocks'][:1]
    expected[0]['elements'][-1]['elements'][-1]['text'] += suffix
    require(exact(G['rich'](blocks), G['rich'](expected)), 'Edit changed original rich text/style or appended content')


def update_row(blocks):
    row = received()
    row['blocks'] = copy.deepcopy(blocks) + row['blocks'][1:]
    row['text'] = G['display'](blocks)
    row['edited'] = dict(user=SELF, ts='1781525800.000001')
    return row


def wire(blocks):
    return dict(method='POST', path='/api/chat.update', args=dict(channel=CHANNEL, ts=TS,
        blocks=json.dumps(blocks + received()['blocks'][1:], ensure_ascii=False, separators=(',', ':')),
        skip_dlp_user_warning='false', _x_reason='saveMessageEdit'))


def update_wire(value, blocks):
    require(isinstance(value, dict) and set(value) == {'method', 'path', 'args'}
        and value['method'] == 'POST' and value['path'] == '/api/chat.update', 'Wrong update endpoint or request envelope')
    args = value['args']
    expected = wire(blocks)['args']
    require(isinstance(args, dict) and set(args) == set(expected)
        and all(args[k] == expected[k] for k in expected if k != 'blocks')
        and isinstance(args['blocks'], str)
        and exact(json.loads(args['blocks']), blocks + received()['blocks'][1:]), 'Update changed owner/body/context or added a text fallback')


def policy_cycle():
    platform = N['F']['observations'](False)['experiments.getByUser'][0]
    return [
        (dict(method='POST', path='/api/conversations.genericInfo', args={'updated_channels': '{"CCOMMENT01":0}'}),
         dict(ok=True, channels=[dict(id=CHANNEL, is_channel=True, is_file=True, is_read_only=False, is_thread_only=False, context_team_id='T1')])),
        (dict(method='POST', path='/api/users.prefs.get', args={}), dict(ok=True, prefs={'plain_text_mode': False})),
        (dict(method='POST', path='/api/experiments.getByUser', args=platform),
         dict(ok=True, assignments={'portable_content_composer': {'group': 'control'}})),
        (dict(method='POST', path='/api/client.extras', args={'extras': '["team_prefs"]', '_x_reason': 'native-own-profile-capabilities'}),
         dict(ok=True, team_prefs={'toplevel_prefs': {'msg_edit_window_mins': -1}, 'workspace_prefs': {}})),
    ]


def boot():
    return dict(request=dict(method='POST', path='/api/client.userBoot', args={'include_min_version_bump_check': '1'}),
        response=dict(ok=True, team=dict(id='T1', plan='plus'), self=dict(id=SELF, team_id='T1'),
            account_types=dict(is_admin=[], is_owner=[], is_primary_owner=[]), workspaces=[dict(id='T1')],
            read_only_channels=[], thread_only_channels=[]), applyWorkspace=False)


def fixture_ledger(value, index, edited, previous):
    require(value.get('enabled') is True and exact(value.get('networkCalls'), 0)
        and value.get('httpCapability') is False, 'Own-edit fixture gained network capability')
    require(exact(value.get('received'), received()) and exact(value.get('boot'), boot()), 'Original received HTTP row/boot changed')
    policy = [dict(request=q, response=r) for _ in range(POLICIES[index]) for q, r in policy_cycle()]
    require(exact(value.get('policy'), policy), 'Fresh edit policy acquisition changed or was omitted')
    count = int(index >= 3)
    prepared, updates = value.get('prepared'), value.get('updates')
    require(isinstance(prepared, list) and len(prepared) == count and exact(updates, prepared), 'Missing or duplicate user update')
    for request in prepared:
        update_wire(request, edited)
    ack = dict(ok=True, channel=CHANNEL, ts=TS, message=update_row(edited)) if count else None
    require(exact(value.get('responses'), [dict(disposition='accepted', raw=ack)] if count else [])
        and exact(value.get('futureRows'), [ack['message']] if count else []), 'Original update ACK/future row was rewritten')
    require(value.get('held') is (index == 3) and exact(value.get('releases'), [RELEASE] if index >= 4 else [])
        and value.get('published') is (index >= 5)
        and exact(value.get('publishedRow'), ack['message'] if index >= 5 else None), 'Held ACK or explicit readback disposition changed')
    if previous:
        for key in ('policy', 'prepared', 'updates', 'responses', 'futureRows', 'releases'):
            require(exact(value[key][:len(previous[key])], previous[key]), 'Original request/response history rewritten')
    return ack


def one_message(messages):
    require(isinstance(messages, list), 'Missing original Page messages')
    rows = [m for m in messages if m.get('ts') == TS]
    require(len(rows) == 1, 'Original comment missing or duplicated')
    return rows[0]


def inert_text(raw, paint, viewport):
    """Read-only text is hit at its exact inert parent, never an arbitrary overlay."""
    require(isinstance(raw, dict) and isinstance(paint, dict)
        and set(paint) == {'locked', 'parentClass', 'lines', 'visibleLines', 'ok'}
        and paint['locked'] is True and paint['parentClass'] == 'slack-list-comment-compose-editor'
        and paint['ok'] is True, 'Missing exact inert editor paint evidence')
    N['rectangle'](raw.get('clip', {}), viewport)
    lines, painted = raw.get('lines'), paint.get('lines')
    require(isinstance(lines, list) and lines and isinstance(painted, list)
        and len(lines) == len(painted), 'Inert paint dropped original text lines')
    raw_visible, visible = 0, 0
    for line, measured in zip(lines, painted):
        require(isinstance(line, dict) and isinstance(measured, dict)
            and set(measured) == {'rect', 'fullVisible', 'fragments'}
            and exact(measured['rect'], line.get('rect'))
            and type(line.get('fullVisible')) is bool and measured['fullVisible'] is line['fullVisible']
            and type(line.get('hit')) is bool, 'Inert paint changed original line geometry/flags')
        N['text_rect'](line['rect'])
        fragments, hits = line.get('fragments'), measured['fragments']
        require(isinstance(fragments, list) and fragments and isinstance(hits, list)
            and len(fragments) == len(hits), 'Inert paint dropped original text fragments')
        for fragment, hit in zip(fragments, hits):
            require(isinstance(fragment, dict) and type(fragment.get('hit')) is bool
                and isinstance(hit, dict) and set(hit) == {'rect', 'hitParent'}
                and exact(hit['rect'], fragment.get('rect')) and type(hit['hitParent']) is bool,
                'Inert paint changed original fragment evidence')
            N['text_rect'](fragment['rect'])
            if hit['hitParent']:
                require(fragment['hit'] is False, 'Inert parent hit was rewritten as editable child hit')
        bounds = dict(left=min(f['rect']['left'] for f in fragments), top=min(f['rect']['top'] for f in fragments),
            right=max(f['rect']['right'] for f in fragments), bottom=max(f['rect']['bottom'] for f in fragments))
        require(all(math.isclose(line['rect'][key], value, abs_tol=1e-6, rel_tol=0) for key, value in bounds.items())
            and line['hit'] is all(f['hit'] for f in fragments), 'Raw text line union/hit flags are inconsistent')
        if line['fullVisible']:
            r, clip = line['rect'], raw['clip']
            require(r['left'] >= clip['left'] - 1 and r['right'] <= clip['right'] + 1
                and r['top'] >= clip['top'] - 1 and r['bottom'] <= clip['bottom'] + 1, 'Inert text is clipped')
            N['rectangle'](r, viewport)
            for fragment in fragments:
                N['rectangle'](fragment['rect'], viewport)
            raw_visible += int(line['hit'])
            visible += int(all(f['hitParent'] for f in hits))
    require(exact(raw.get('visibleLines'), raw_visible) and raw.get('ok') is (raw_visible > 0)
        and exact(paint['visibleLines'], visible) and visible > 0, 'Inert text visibility/counts are inconsistent or fully occluded')


def geometry(value, index, reply, cancelled, edited):
    require(value.get('ok') is True and value.get('noOverflow') is True, 'Own-edit controls overflow or native capture failed')
    viewport = value.get('viewport')
    require(isinstance(viewport, list) and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport), 'Missing native viewport')
    editing = index in (1, 3)
    require(value.get('editing') is editing, 'Wrong native edit mount')
    controls = value.get('controls')
    names = ['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save'] if editing else ['slack-list-comments-back', 'slack-list-comment-send']
    require(isinstance(controls, list) and [v.get('id') for v in controls] == names, 'Wrong edit/reply controls')
    for control in controls:
        require(control.get('visible') is True and control.get('hit') is True
            and control.get('disabled') is (index == 3), 'Pending/interactive edit controls changed')
        N['rectangle'](control.get('rect', {}), viewport)
    if index == 3:
        inert_text(value.get('text'), value.get('inertTextPaint'), viewport)
    else:
        require(value.get('inertTextPaint') is None, 'Inert paint evidence escaped the held-saving stage')
        N['text_proof'](value.get('text'), viewport)
    native = value.get('reply')
    require(isinstance(native, dict) and exact(native.get('blocks'), reply)
        and native.get('sameNode') is (index != 6)
        and native.get('hidden') is editing and native.get('inert') is editing
        and native.get('visible') is (not editing) and native.get('composing') is False
        and value.get('replyNotice') is None, 'Original mounted reply was changed, replaced, or exposed during edit')
    if not editing:
        N['rectangle'](native.get('rect', {}), viewport)
        require(value.get('edit') is None, 'Dismissed edit editor survived')
    else:
        active = value.get('edit', {})
        require(exact(active.get('blocks'), cancelled if index == 1 else edited)
            and active.get('editable') is (index == 1), 'Pending edit became mutable or native rich buffer changed')
        N['rectangle'](active.get('rect', {}), viewport)
    if index == 3:
        notice = value.get('notice')
        require(isinstance(notice, dict) and notice.get('text') == 'Saving the edited comment\u2026'
            and notice.get('visible') is True and notice.get('hit') is True, 'Held edit lost visible saving guidance')
        N['rectangle'](notice.get('rect', {}), viewport)
    else:
        require(value.get('notice') is None, 'Unexpected edit status after cancel/save')
    row = value.get('row')
    require(isinstance(row, dict) and isinstance(row.get('text'), str), 'Missing original comment DOM row')
    wanted = received()['text'].split('\n') if index < 4 else update_row(edited)['text'].split('\n')
    require(all(text in row['text'] for text in wanted) and 'Preserved original context' in row['text'], 'Edited body/context not present in native row')
    require((CHANGE in row['text']) is (index >= 4) and CANCEL not in row['text'], 'Cancelled/unacknowledged edit leaked to transcript')
    return viewport


def validate(fixture, receipt, directory, *, expected_case='own-edit', dimensions=None, png=True):
    require(expected_case == 'own-edit' and receipt.get('case') == expected_case
        and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True
        and receipt.get('fixture') == fixture and fixture in C['FIXTURES'], 'Wrong own-edit terminal receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[expected_case]), 'Missing own-edit stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Original canonical inventory changed')
    initial = rows[0]['model']['comments']
    seed, reply = rows[0]['proof']['replySeed'], rows[0]['proof']['replyBlocks']
    scope = dict(workspace='T1', user=SELF, list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'), scope) and exact(seed.get('blocks'), G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong original persisted reply seed')
    reply_content(reply, seed['blocks'])
    cancelled, edited = rows[1]['proof']['cancelledBlocks'], rows[3]['proof']['editedBlocks']
    edited_content(cancelled, CANCEL)
    edited_content(edited, CHANGE)
    previous_fixture, previous_reads, previous_draft = None, [], None
    initial_journal = rows[0]['journal']
    for index, row in enumerate(rows):
        p, model = row['proof'], row['model']
        m, f, edit = model['comments'], model['fixture'], model['edit']
        require(exact(p.get('source'), dict(received=received(), boot=boot()))
            and exact(p.get('replySeed'), seed) and exact(p.get('replyBlocks'), reply)
            and p.get('actions') == [], 'Original edit/reply source proof changed')
        require(exact(p.get('cancelledBlocks'), cancelled if index >= 1 else None)
            and exact(p.get('editedBlocks'), edited if index >= 3 else None)
            and exact(p.get('doubleSave'), dict(ok=True, disabled=True) if index >= 3 else None)
            and exact(p.get('release'), RELEASE if index >= 4 else None)
            and p.get('runtimeForgotten', False) is (index == 6), 'Edit/release/forget proof changed')
        require(m.get('account') == initial.get('account') and R['uuid'](m.get('account'))
            and exact(m.get('epoch'), initial.get('epoch')) and R['integer'](m.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False
            and exact(m.get('writes'), 0) and exact(m.get('networkCalls'), 0), 'Original disconnected owner changed')
        require(exact(m.get('document'), initial.get('initialDocument')) and exact(m.get('initialDocument'), initial.get('initialDocument'))
            and initial['initialDocument'].get('id') == READ['LIST']
            and exact(m.get('records'), touched) and exact(m.get('initialRecords'), touched)
            and exact(m.get('unrelated'), unrelated) and exact(m.get('initialUnrelated'), unrelated), 'Editing mutated canonical List records')
        comments = m.get('comments', {})
        require(comments.get('active') is True and comments.get('record') == READ['KNOWN']
            and comments.get('root') == READ['ROOT_TS'] and comments.get('loading') is False
            and comments.get('error') is None and m.get('detail', {}).get('record') == READ['KNOWN']
            and comments.get('visibleTs') == [TS] and comments.get('deleted') == [], 'Original visible comment owner changed')
        if index < 6:
            require(comments.get('generation') == initial['comments']['generation'], 'Edit replaced original comment visit')
        else:
            require(comments.get('generation') != initial['comments']['generation'], 'Reopen did not create a fresh comment visit')
        ledger = m.get('ledger', {})
        requests = ledger.get('requests')
        require(isinstance(requests, list) and len(requests) == READS[index]
            and exact(requests[:len(previous_reads)], previous_reads)
            and exact(ledger.get('completed'), READS[index]) and exact(ledger.get('cancelled'), 0)
            and ledger.get('held') is False, 'Unexpected read, missing completion or rewritten read history')
        C['validate_reads'](requests)
        require([q['method'] for q in requests] == ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'] * (READS[index] // 4)
            and all(q['args'].get('id', READ['KNOWN']) == READ['KNOWN']
                and (q['method'] != 'conversations.replies' or q['args']['ts'] == READ['ROOT_TS']) for q in requests)
            and exact(ledger.get('replies'), READS[index] // 4), 'Wrong original item/root or acquisition order')
        previous_reads = requests
        compose = m.get('composeLedger', {})
        require(all(compose.get(k) == [] for k in ('prepared', 'posts', 'messages', 'preflights', 'policies'))
            and all(compose.get(k) is False for k in ('published', 'held', 'heldPost')), 'Own edit sent a new reply or used reply dispatch')
        require(exact(m.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])), 'Edit dispatched an unrelated reaction')
        ack = fixture_ledger(f, index, edited, previous_fixture)
        previous_fixture = f
        raw = one_message(comments.get('page', {}).get('messages'))
        require(exact(raw, received() if index < 5 else ack['message']), 'Immutable HTTP page changed before explicit readback')
        expected_page = copy.deepcopy(initial['comments']['page'])
        if index >= 5:
            expected_page['messages'] = [copy.deepcopy(ack['message']) if m.get('ts') == TS else m for m in expected_page['messages']]
        require(exact(comments.get('page'), expected_page)
            and exact(comments.get('hasMore'), initial['comments'].get('hasMore')), 'Root, pagination or original Page custody changed')
        known = one_message(comments.get('pageMessages'))
        expected = received() if index < 4 else ack['message']
        require(all(exact(known.get(k), expected[k]) for k in ('ts', 'thread_ts', 'user', 'text', 'blocks', 'reactions')),
            'Typed accepted comment body/context/reactions changed')
        require(exact(known.get('edited'), expected.get('edited')), 'Typed edit metadata did not match original ACK/readback')
        require(exact([m for m in comments['pageMessages'] if m.get('ts') != TS],
            [m for m in initial['comments']['pageMessages'] if m.get('ts') != TS]), 'Edit changed another typed comment/root')
        require(comments.get('reactions') == [dict(ts=TS, reactions=[dict(name='wave', count=1)])], 'Edit erased unrelated reaction')
        store = m.get('draftStore', {})
        drafts = store.get('drafts')
        require(isinstance(drafts, list) and len(drafts) == 1, 'Independent reply draft missing')
        draft = drafts[0]
        require(exact(draft.get('scope'), scope) and exact(draft.get('blocks'), reply)
            and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'terminalPending', 'captureFailed'))
            and draft.get('attempt') is None and draft.get('error') is None and draft.get('native') is True
            and store.get('active') == f"{draft['id']}:{comments['generation']}:{draft['generation']}", 'Edit consumed or locked independent reply draft')
        S['inventory'](row['journal'], scope)
        S['paired'](draft, row['journal'])
        require(exact(row['journal'], initial_journal), 'Unchanged reply journal was rewritten or consumed')
        if previous_draft:
            require((draft['id'] == previous_draft['id']) is (index != 6)
                and (draft['generation'] == previous_draft['generation']) is (index != 6), 'Wrong original/reopened reply binding')
        previous_draft = draft
        require(edit.get('opening') is False and R['integer'](edit.get('events')), 'Edit preflight did not settle')
        attempts = edit.get('attempts')
        require(isinstance(attempts, list) and len(attempts) == int(index >= 3), 'Double Save created another attempt')
        if attempts:
            attempt = attempts[0]
            require(R['integer'](attempt.get('id'), 1) and attempt.get('ts') == TS
                and exact(attempt.get('blocks'), edited + received()['blocks'][1:])
                and attempt.get('phase') == ('sent' if index == 3 else 'confirmed')
                and exact(attempt.get('receipt'), None if index == 3 else ack)
                and exact(attempt.get('retryAt'), 0), 'Original attempt/ACK custody changed')
            if index > 3:
                require(exact(attempt, rows[4]['model']['edit']['attempts'][0]), 'Confirmed original attempt was rewritten')
        active = edit.get('active')
        if index in (1, 3):
            require(isinstance(active, dict) and active.get('current') is True and active.get('ts') == TS
                and R['integer'](active.get('generation'), 1) and R['integer'](active.get('revision'), 1)
                and exact(active.get('source'), received()) and exact(active.get('preserved'), received()['blocks'][1:])
                and exact(active.get('blocks'), cancelled if index == 1 else edited)
                and active.get('busy') is (index == 3) and active.get('composing') is False
                and active.get('attempt') == (attempts[0]['id'] if attempts else None)
                and active.get('scope') == f"list-edit:{comments['generation']}:{active['generation']}"
                and active.get('notice') == ('Saving the edited comment\u2026' if index == 3 else None), 'Original edit buffer/session changed')
        else:
            require(active is None, 'Cancelled/accepted edit session still active')
        geometry(row['geometry'], index, reply, cancelled, edited)
        path = Path(directory) / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == path, 'Foreign own-edit screenshot')
        if png:
            H['png_signature'](path, dimensions)
    require(exact(receipt.get('final'), rows[-1]['model']), 'Final own-edit model differs')
    return receipt


def validator(case):
    require(case in STAGES, 'Unknown own-edit case')
    if case == 'own-edit-custody':
        return validate_custody
    return lambda fixture, receipt, directory, **kwargs: validate(fixture, receipt, directory, expected_case=case, **kwargs)


def custody_geometry(g, i, reply, rejected, unknown):
    viewport = g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport, list)
        and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport), 'Custody native capture/viewport failed')
    active = i in (2, 4, 5)
    require(g.get('editing') is active, 'Custody edit mount changed')
    names = (['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save'] +
        (['slack-list-comment-edit-check'] if i in (4, 5) else [])) if active else ['slack-list-comments-back', 'slack-list-comment-send']
    # Preserve the actual helper's Cancel/Save/Check order.
    if i in (4, 5):
        names = ['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save', 'slack-list-comment-edit-check']
    controls = g.get('controls')
    require(isinstance(controls, list) and [v.get('id') for v in controls] == names, 'Wrong custody controls')
    for c in controls:
        disabled = (i == 1 and c['id'] == 'slack-list-comment-send') or (i in (4, 5) and c['id'] != 'slack-list-comment-edit-check')
        require(c.get('disabled') is disabled and c.get('visible') is True and c.get('hit') is True, 'Unconfirmed/interactive control policy changed')
        N['rectangle'](c.get('rect', {}), viewport)
    N['text_proof'](g.get('text'), viewport)
    require(g.get('inertTextPaint') is None, 'Failure custody used a held-saving paint exception')
    native = g.get('reply', {})
    require(exact(native.get('blocks'), reply) and native.get('sameNode') is (i != 7)
        and native.get('hidden') is active and native.get('inert') is active
        and native.get('visible') is (not active) and native.get('composing') is (i == 1), 'Custody lost original reply node/content/composition')
    if active:
        edit = g.get('edit', {})
        require(exact(edit.get('blocks'), rejected if i == 2 else unknown)
            and edit.get('editable') is True, 'Rejected/unconfirmed native edit content changed')
        N['rectangle'](edit.get('rect', {}), viewport)
        notice = g.get('notice')
        require(isinstance(notice, dict) and notice.get('text') == {2: REJECTED_NOTICE, 4: UNKNOWN_NOTICE, 5: CHECKED_NOTICE}[i]
            and notice.get('visible') is True and notice.get('hit') is True, 'Missing failure/readback guidance')
        N['rectangle'](notice.get('rect', {}), viewport)
    else:
        require(g.get('edit') is None and g.get('notice') is None, 'Inactive custody editor/status survived')
        N['rectangle'](native.get('rect', {}), viewport)
    if i == 1:
        notice = g.get('replyNotice')
        require(isinstance(notice, dict) and notice.get('text') == COMPOSING_ERROR and notice.get('visible') is True, 'Composition refusal is not visible')
        N['rectangle'](notice.get('rect', {}), viewport)
    else:
        require(g.get('replyNotice') is None, 'Recovered reply still shows failure')
    body = g.get('row', {}).get('text')
    expected = received() if i < 6 else update_row(unknown)
    require(isinstance(body, str) and all(v in body for v in expected['text'].split('\n'))
        and 'Preserved original context' in body and REJECTED_TEXT not in body
        and (UNKNOWN_TEXT in body) is (i >= 6), 'Rejected/unconfirmed edit leaked into transcript or positive readback missing')
    return viewport


def native_button(value, id, viewport):
    require(isinstance(value, dict) and value.get('ok') is True, 'Missing actual native button action')
    c = value.get('proof', {})
    require(c.get('id') == id and c.get('disabled') is False and c.get('visible') is True
        and c.get('hit') is True, 'Native action was disabled, clipped or foreign')
    N['rectangle'](c.get('rect', {}), viewport)


def validate_custody(fixture, receipt, directory, *, dimensions=None, png=True):
    require(receipt.get('case') == 'own-edit-custody' and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture') == fixture
        and fixture in C['FIXTURES'], 'Wrong own-edit custody terminal')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES['own-edit-custody']), 'Missing own-edit custody stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Wrong custody inventory')
    first = rows[0]['model']['comments']
    seed, reply = rows[0]['proof']['replySeed'], rows[0]['proof']['replyBlocks']
    scope = dict(workspace='T1', user=SELF, list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'), scope) and exact(seed.get('blocks'), G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong original custody reply seed')
    reply_content(reply, seed['blocks'])
    rejected, unknown = rows[2]['proof']['rejectedBlocks'], rows[4]['proof']['unknownBlocks']
    edited_content(rejected, REJECTED_TEXT)
    edited_content(unknown, UNKNOWN_TEXT)
    journal = rows[0]['journal']
    previous = None
    for i, row in enumerate(rows):
        p, m, f, edit = row['proof'], row['model']['comments'], row['model']['fixture'], row['model']['edit']
        require(exact(p.get('source'), dict(received=received(), boot=boot())) and exact(p.get('replySeed'), seed)
            and exact(p.get('replyBlocks'), reply) and p.get('actions') == []
            and exact(p.get('rejectedBlocks'), rejected if i >= 2 else None)
            and exact(p.get('unknownBlocks'), unknown if i >= 4 else None)
            and p.get('runtimeForgotten', False) is (i == 7), 'Original custody source/proof changed')
        require(m.get('account') == first.get('account') and R['uuid'](m.get('account'))
            and exact(m.get('epoch'), first.get('epoch')) and R['integer'](m.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False
            and exact(m.get('networkCalls'), 0) and exact(m.get('writes'), 0), 'Custody owner/network changed')
        require(exact(m.get('document'), first.get('initialDocument')) and first['initialDocument'].get('id') == READ['LIST']
            and exact(m.get('initialDocument'), first.get('initialDocument'))
            and exact(m.get('records'), touched) and exact(m.get('initialRecords'), touched)
            and exact(m.get('unrelated'), unrelated) and exact(m.get('initialUnrelated'), unrelated), 'Custody changed canonical List records')
        c = m.get('comments', {})
        require(c.get('active') is True and c.get('record') == READ['KNOWN'] and c.get('root') == READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None and c.get('visibleTs') == [TS]
            and c.get('deleted') == [] and m.get('detail', {}).get('record') == READ['KNOWN']
            and (c.get('generation') == first['comments']['generation']) is (i != 7), 'Custody original comment owner changed')
        ledger = m['ledger']
        require(len(ledger.get('requests', [])) == CUSTODY_READS[i] and exact(ledger.get('completed'), CUSTODY_READS[i])
            and exact(ledger.get('replies'), CUSTODY_READS[i] // 4) and exact(ledger.get('cancelled'), 0)
            and ledger.get('held') is False, 'Custody read/check counts changed')
        C['validate_reads'](ledger['requests'])
        require([q['method'] for q in ledger['requests']] == ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'] * (CUSTODY_READS[i] // 4)
            and all(q['args'].get('id', READ['KNOWN']) == READ['KNOWN'] and (q['method'] != 'conversations.replies' or q['args']['ts'] == READ['ROOT_TS']) for q in ledger['requests']), 'Custody read targeted another item/root')
        require(exact(m.get('composeLedger'), dict(prepared=[], posts=[], preflights=[], policies=[], messages=[], published=False, held=False, heldPost=False))
            and exact(m.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])), 'Custody caused a new reply/reaction write')
        count = 2 if i >= 4 else 1 if i >= 2 else 0
        require(f.get('enabled') is True and exact(f.get('received'), received()) and exact(f.get('boot'), boot())
            and exact(f.get('networkCalls'), 0) and f.get('httpCapability') is False, 'Closed custody source changed')
        require(exact(f.get('policy'), [dict(request=q, response=r) for _ in range(CUSTODY_POLICIES[i]) for q, r in policy_cycle()]), 'Custody reused/omitted fresh edit policy')
        require(isinstance(f.get('prepared'), list) and len(f['prepared']) == count and exact(f.get('updates'), f['prepared']), 'Unknown save was replayed or blocked Save dispatched')
        for q, blocks in zip(f['prepared'], (rejected, unknown)):
            update_wire(q, blocks)
        responses = [dict(disposition='rejected', raw=REJECTED_RAW), dict(disposition='unconfirmed', raw=None)][:count]
        require(exact(f.get('responses'), responses) and f.get('held') is False and f.get('releases') == []
            and exact(f.get('futureRows'), [update_row(unknown)] if count == 2 else [])
            and f.get('published') is (i >= 6) and exact(f.get('publishedRow'), update_row(unknown) if i >= 6 else None), 'Unknown readback was fabricated as ACK or source published early')
        if previous:
            require(exact(ledger['requests'][:len(previous['comments']['ledger']['requests'])], previous['comments']['ledger']['requests']), 'Custody read history rewritten')
            for key in ('policy', 'prepared', 'updates', 'responses', 'futureRows'):
                require(exact(f[key][:len(previous['fixture'][key])], previous['fixture'][key]), 'Custody original response history rewritten')
        expected_page = copy.deepcopy(first['comments']['page'])
        if i >= 6:
            expected_page['messages'] = [update_row(unknown) if r.get('ts') == TS else r for r in expected_page['messages']]
        require(exact(c.get('page'), expected_page) and exact(c.get('hasMore'), first['comments'].get('hasMore')), 'Rejected/unknown changed Page or readback lost original root/cursor')
        typed, expected_row = one_message(c.get('pageMessages')), received() if i < 6 else update_row(unknown)
        require(all(exact(typed.get(k), expected_row.get(k)) for k in ('ts', 'thread_ts', 'user', 'text', 'blocks', 'reactions', 'edited'))
            and c.get('reactions') == [dict(ts=TS, reactions=[dict(name='wave', count=1)])], 'Custody transcript falsely changed or lost unrelated reactions')
        store = m['draftStore']
        require(len(store.get('drafts', [])) == 1, 'Original reply missing during custody')
        draft = store['drafts'][0]
        require(exact(draft.get('scope'), scope) and exact(draft.get('blocks'), reply) and draft.get('attempt') is None
            and draft.get('native') is True and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'terminalPending', 'captureFailed'))
            and draft.get('error') == (COMPOSING_ERROR if i == 1 else None)
            and store.get('active') == f"{draft['id']}:{c['generation']}:{draft['generation']}", 'Custody consumed/changed independent reply')
        S['inventory'](row['journal'], scope)
        S['paired'](draft, row['journal'])
        require(exact(row['journal'], journal), 'Custody changed independent SQLite draft')
        if previous:
            old = previous['comments']['draftStore']['drafts'][0]
            require((old['id'] == draft['id']) is (i != 7) and (old['generation'] == draft['generation']) is (i != 7), 'Original reply mount/reopen identity changed')
        require(edit.get('opening') is False and R['integer'](edit.get('events')) and len(edit.get('attempts', [])) == count, 'Custody edit did not settle or duplicate attempt')
        for j, a in enumerate(edit['attempts']):
            phase = 'rejected' if j == 0 else 'observed' if i >= 6 else 'unknown'
            require(R['integer'](a.get('id'), 1) and a.get('ts') == TS and exact(a.get('blocks'), (rejected if j == 0 else unknown) + received()['blocks'][1:])
                and a.get('phase') == phase and exact(a.get('receipt'), REJECTED_RAW if j == 0 else None) and exact(a.get('retryAt'), 0), 'Readback replaced original failure disposition or receipt')
            if previous and j < len(previous['edit']['attempts']):
                before = previous['edit']['attempts'][j]
                require(a['id'] == before['id'] and exact(a['receipt'], before['receipt']), 'Original uncertain attempt replaced')
        active = edit.get('active')
        if i in (2, 4, 5):
            require(isinstance(active, dict) and active.get('current') is True and active.get('ts') == TS
                and active.get('busy') is False and active.get('composing') is False
                and exact(active.get('source'), received()) and exact(active.get('preserved'), received()['blocks'][1:])
                and exact(active.get('blocks'), rejected if i == 2 else unknown) and active.get('attempt') == edit['attempts'][-1]['id']
                and active.get('notice') == {2: REJECTED_NOTICE, 4: UNKNOWN_NOTICE, 5: CHECKED_NOTICE}[i], 'Retained failure buffer/session changed')
            if i == 5:
                require(active.get('generation') == rows[4]['model']['edit']['active'].get('generation'), 'Check replaced original uncertain edit mount')
        else:
            require(active is None, 'Cancelled/observed edit remained active')
        viewport = custody_geometry(row['geometry'], i, reply, rejected, unknown)
        for key, present, composing in [('compositionStart', i >= 1, True), ('compositionEnd', i >= 2, False)]:
            witness = p.get(key)
            if present:
                require(isinstance(witness, dict) and witness.get('ok') is True and witness.get('active') is composing
                    and witness.get('native', {}).get('composing') is composing and exact(witness['native'].get('blocks'), reply), 'Missing actual composition event path')
            else:
                require(witness is None, 'Early composition witness')
        if i >= 2:
            native_button(p.get('replyRecovery'), 'slack-list-comment-retry', viewport)
        else:
            require(p.get('replyRecovery') is None, 'Early reply retry')
        require(exact(p.get('blockedControls'), dict(ok=True, saveDisabled=True, cancelDisabled=True) if i >= 4 else None), 'Unconfirmed duplicate Save/Cancel was not blocked')
        checks = p.get('checks', [])
        require(isinstance(checks, list) and [x.get('stage') for x in checks] == (['unconfirmed-checked', 'observed'] if i >= 6 else ['unconfirmed-checked'] if i == 5 else []), 'Missing explicit result checks')
        for check in checks:
            native_button(check.get('value'), 'slack-list-comment-edit-check', viewport)
        path = Path(directory) / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == path, 'Foreign custody screenshot')
        if png:
            H['png_signature'](path, dimensions)
        previous = row['model']
    require(exact(receipt.get('final'), rows[-1]['model']), 'Final custody model differs')
    return receipt
