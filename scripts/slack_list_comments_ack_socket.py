"""Actual POST acknowledgement/socket ordering and native newer-draft custody."""
import copy
import math
from pathlib import Path
import runpy

I = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_incoming.py')))
R, C, READ, H = I['R'], I['C'], I['READ'], I['H']
require, rectangle = R['require'], R['rectangle']
STAGES = {case: ('draft', 'post-held', 'socket', 'reacted', 'acknowledged', 'b-edited', 'echo')
          for case in ('ack-socket-add', 'ack-socket-remove')}
STAGES['ack-first'] = ('draft', 'acknowledged', 'b-edited', 'socket', 'duplicate')
B_TEXT = 'Newer draft B \u65e5\U0001f98a\nRetain this draft after the original acknowledgement.'
CHANNEL, FIRST = 'CCOMMENT01', '1781524801.000001'
EVENT_KEYS = ('read', 'visibleTs', 'liveTs', 'acceptedTs', 'liveReactions', 'deleted', 'reactions', 'reveal')


def validator(case):
    require(case in STAGES, 'Unknown ACK acceptance case')
    def validate_case(fixture, receipt, directory, **kwargs):
        return validate(fixture, receipt, directory, expected_case=case, **kwargs)
    return validate_case


def rich_content(seed, a, b):
    sp, ap, bp = R['paragraphs'](seed['blocks']), R['paragraphs'](a), R['paragraphs'](b)
    source = [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Existing context '), dict(type='user', user_id='U_FIXTURE_OPAQUE', opaque=dict(retain=[1,2])),
        dict(type='text', text=' '), dict(type='link', url='https://example.test/reference', text='Reference', opaque=dict(source='fixture')),
        dict(type='text', text='\n')])])]
    require(seed['blocks'] == source, 'Original isolated rich seed changed')
    require(len(sp) == 1 and sp[0][0][-1:] == [('text', '\n', {})], 'Seed paragraph boundary changed')
    first, second = C['REACTION_TEXT'].split('\n')
    expected = sp[0][0][:-1] + [('text', char, {'bold': True}) for char in first] + sp[0][0][-1:]
    require(len(ap) == 2 and ap[0][0] == expected
            and ap[1][0] == [('text', char, {}) for char in second]
            and len(R['opaque'](seed['blocks'])) == 2 and R['opaque'](a) == R['opaque'](seed['blocks']),
            'Original rich content/style changed')
    require(len(bp) == 2 and [units for units, _ in bp] ==
            [[('text', char, {}) for char in text] for text in B_TEXT.split('\n')], 'Newer rich draft changed')


def expected_events(rows, case, raw):
    if case == 'ack-first':
        conflict = dict(copy.deepcopy(raw), text='Conflicting duplicate must not replace the accepted comment', blocks=[])
        specs = [('socket', raw, False, 2, 2), ('duplicate', conflict, False, 3, 3)]
    else:
        reaction = dict(type='reaction_added' if case == 'ack-socket-add' else 'reaction_removed',
                        user='U_RT_REACTOR', reaction='wave', item=dict(type='message', channel=CHANNEL, ts=raw['ts']))
        specs = [('socket', raw, True, 1, 2), ('reacted', reaction, True, 2, 3), ('echo', raw, False, 5, 5)]
    return [dict(stage=stage, team='T1', generation=7, frame=copy.deepcopy(frame), parsed=True, changed=changed,
                 before={k: rows[before]['model']['comments'][k] for k in EVENT_KEYS},
                 after={k: rows[after]['model']['comments'][k] for k in EVENT_KEYS})
            for stage, frame, changed, before, after in specs]


def validate(fixture, receipt, directory, *, expected_case=None, dimensions=None, png=True):
    case = receipt.get('case'); directory = Path(directory)
    require(case in STAGES and (expected_case is None or case == expected_case)
            and fixture in C['FIXTURES'] and receipt.get('fixture') == fixture
            and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True, 'Wrong ACK/socket receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(STAGES[case]), 'Missing ACK/socket stages')
    ack, bi = (1, 2) if case == 'ack-first' else (4, 5)
    initial = rows[0]['model']; seed = rows[0]['proof']['seed']; a = rows[0]['proof']['originalA']
    b = rows[bi]['proof']['newerB']; rich_content(seed, a, b)
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']}
            and initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == READ['ROOT_TS'], 'Wrong original inventory')
    scope = seed.get('scope')
    require(scope == dict(workspace='T1', user='U0', list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
                         created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
            and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong restored owner')
    page = initial['comments']['page']; root = I['message'](page, READ['ROOT_TS'])
    require(root == dict(ts=READ['ROOT_TS'], subtype='list_record_comment',
                        slack_list=dict(list_id=READ['LIST'], list_record_id=READ['KNOWN']),
                        unknown=dict(keep=True), text='Record comment root')
            and I['message'](page, FIRST).get('reactions') == I['source_reactions'](2), 'Original source changed')
    original = rows[1]['proof']['originalMessage']; post = rows[1]['model']['composeLedger']['posts'][0]
    require(C['outgoing'](post) == a, 'POST did not contain exact original A')
    R['_message'](original, post, scope)
    wave = [dict(name='wave', count=1, users=['U_RT_REACTOR'])]
    require(original.get('ts') == '1781525001.000001' and original.get('type') == 'message'
            and original.get('reactions', []) == (wave if case == 'ack-socket-remove' else []), 'Wrong original raw ACK')
    sent_ts = original['ts']; viewport = None
    original_draft = initial['draftStore']['drafts'][0]
    ack_draft = rows[ack]['model']['draftStore']['drafts'][0]
    b_draft = rows[bi]['model']['draftStore']['drafts'][0]
    require(R['integer'](original_draft['generation'], 1) and R['integer'](ack_draft['generation'], 1)
            and ack_draft['generation'] != original_draft['generation'], 'ACK failed to rotate original generation')
    for i, row in enumerate(rows):
        model, g, proof = row['model'], row['geometry'], row['proof']; comments = model['comments']
        pending = 0 < i < ack; done = i >= ack; authored_b = i >= bi
        content = b if authored_b else [] if done else a
        require(proof.get('seed') == seed and proof.get('originalA') == a
                and proof.get('newerB') == (b if authored_b else None)
                and proof.get('originalMessage') == (original if i else None), 'Original message/draft proof rewritten')
        release = dict(delivered=True, cancelledBeforeReply=False) if done and case != 'ack-first' else None
        require(proof.get('release') == release, 'Actual held ACK release missing or replaced')
        if release is not None:
            require(proof['release']['delivered'] is True and proof['release']['cancelledBeforeReply'] is False, 'Invalid release types')
        require(model.get('account') == initial.get('account') and R['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and R['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated
                and model.get('detail') == initial['detail'], 'Canonical source changed')
        require(comments.get('active') is True and comments.get('loading') is False and comments.get('error') is None
                and comments.get('record') == READ['KNOWN'] and comments.get('root') == READ['ROOT_TS']
                and comments.get('hasMore') is True and comments.get('page') == page and comments.get('deleted') == []
                and all(comments.get(k) == initial['comments'].get(k) for k in ('generation', 'requestTs')), 'Original read owner/Page changed')
        require(R['integer'](comments.get('read'), 1) and (comments['read'] > initial['comments']['read'] if i
                else True) and (i < 2 or comments['read'] == rows[1]['model']['comments']['read']), 'Socket/ACK issued a read')
        count = 1 if i else 0; reads = 8 if i else 4; ledger = model['ledger']; requests = ledger.get('requests', [])
        require(len(requests) == reads and requests == rows[1 if i else 0]['model']['ledger']['requests']
                and requests[:4] == initial['ledger']['requests'], 'Read requests changed or replayed')
        C['validate_reads'](requests)
        require(ledger.get('completed') == reads and ledger.get('replies') == reads // 4
                and ledger.get('cancelled') == 0 and ledger.get('held') is False
                and all(type(ledger.get(k)) is int for k in ('completed','replies','cancelled')), 'Unexpected read lifecycle')
        compose = model['composeLedger']
        require(compose.get('posts') == compose.get('prepared') == ([post] if i else [])
                and compose.get('messages') == ([original] if i else []) and compose.get('published') is False
                and compose.get('held') is False and compose.get('heldPost') is pending, 'Original POST/ACK changed or replayed')
        require(compose.get('preflights') == ([dict(method='conversations.genericInfo', args={'updated_channels':'{"CCOMMENT01":0}'})] if i else [])
                and len(compose.get('policies', [])) == count, 'Posting observation missing or extra')
        if i:
            require(compose['policies'] == rows[1]['model']['composeLedger']['policies'], 'Posting policy replaced')
            policy = compose['policies'][0]
            require(policy.get('request') == dict(method='client.userBoot', args={'include_min_version_bump_check':'1'})
                    and policy.get('boot', {}).get('ok') is True and policy['boot'].get('team', {}).get('id') == scope['workspace']
                    and policy['boot'].get('self', {}).get('id') == scope['user']
                    and policy.get('channel', {}).get('id') == CHANNEL and policy.get('threadReadOnly') is False, 'Wrong posting authority')
        store = model['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1, 'Draft custody changed')
        draft, journal = entries[0], proof.get('journal', {})
        generation = ack_draft['generation'] if done else original_draft['generation']
        require(draft.get('id') == original_draft['id'] and draft.get('generation') == generation and draft.get('native') is True
                and store.get('active') == f"{draft['id']}:{comments['generation']}:{generation}"
                and draft.get('scope') == journal.get('scope') == scope
                and draft.get('blocks') == journal.get('blocks') == content
                and draft.get('capture') == journal.get('capture') and R['uuid'](draft.get('capture'))
                and draft.get('savedRevision') == journal.get('revision') and R['integer'](journal.get('revision'), 1)
                and draft.get('attempt') == journal.get('attempt') and R['integer'](draft.get('sequence')),
                'Native draft and independent journal disagree')
        require(draft.get('busy') is pending and all(draft.get(k) is False for k in ('saving','loading','captureFailed','terminalPending'))
                and draft.get('error') is None, 'Draft storage/POST state changed')
        require(draft.get('status') == ('Comment sent.' if i == ack else None), 'ACK status changed outside settlement')
        if i == ack:
            require(draft.get('status') == 'Comment sent.' and 'Comment sent.' in g.get('notices', []), 'ACK success guidance missing')
        if pending:
            attempt = journal.get('attempt'); require(isinstance(attempt, dict), 'Pending original attempt lost')
            record = attempt.get('record', {}); target = record.get('target', {}); args = post['args']
            require(attempt.get('raw') is None and attempt.get('capture') == journal['capture'] == original_draft['capture']
                    and record.get('client_msg_id') == args['client_msg_id'] and record.get('client_ts') == args['ts']
                    and record.get('blocks') == a and record.get('workspace') == scope['workspace'] and record.get('user') == scope['user']
                    and record.get('version') == 1 and type(record.get('version')) is int and record.get('root') == I['typed'](root)
                    and record.get('text') == original.get('text') and target.get('channel') == CHANNEL and target.get('root') == scope['root']
                    and target.get('document') == initial['initialDocument'] and target.get('record') == touched[READ['KNOWN']],
                    'Pending attempt lost original POST/source binding')
            require(journal == rows[1]['proof']['journal'], 'Socket changed pending durable attempt')
            require(draft.get('sequence') == original_draft['sequence'], 'Socket changed original draft sequence')
        else:
            require(journal.get('attempt') is None, 'Settled/unattempted draft remained locked')
        if i == ack:
            require(journal['revision'] > rows[i-1]['proof']['journal']['revision']
                    and draft['capture'] != original_draft['capture'], 'ACK did not persist exact clear')
        if authored_b:
            require(journal == rows[bi]['proof']['journal'] and draft == b_draft
                    and journal['revision'] > rows[ack]['proof']['journal']['revision']
                    and journal['capture'] != ack_draft['capture'], 'Socket disturbed newer unsent B')
        live_ts = [sent_ts] if pending and i >= 2 else []
        accepted_ts = [sent_ts] if done else []
        current = copy.deepcopy(I['typed'](original))
        if case != 'ack-first' and i >= 3: current['reactions'] = wave if case == 'ack-socket-add' else []
        require(comments.get('liveTs') == live_ts and comments.get('live') == ([current] if live_ts else [])
                and comments.get('acceptedTs') == accepted_ts and comments.get('accepted') == ([current] if done else [])
                and comments.get('liveReactions') == ([sent_ts] if pending and i == 3 else []), 'ACK lost reaction delta or duplicated live/accepted row')
        visible = [FIRST] + ([sent_ts] if done or live_ts else [])
        projection = [dict(ts=FIRST, reactions=I['reactions'](2))]
        if len(visible) == 2: projection.append(dict(ts=sent_ts, reactions=[dict(name=r['name'], count=r['count']) for r in current['reactions']]))
        dom = g.get('rows', [])
        require(comments.get('visibleTs') == visible and [r.get('ts') for r in dom] == visible
                and comments.get('reactions') == projection
                and [dict(ts=r.get('ts'), reactions=r.get('reactions')) for r in dom] == projection, 'Visible ACK row/reaction changed')
        require(all(isinstance(r.get('text'), str) and r['text'] for r in dom), 'Native comment body missing')
        if len(visible) == 2:
            require(all(dom[-1]['text'].count(text) == 1 for text in C['REACTION_TEXT'].split('\n')), 'Accepted native rich body missing/duplicated')
        reveal = comments.get('reveal')
        require(reveal == rows[ack]['model']['comments']['reveal'] if done else reveal is None, 'Socket changed reveal')
        if done:
            require(isinstance(reveal, list) and len(reveal) == 2 and R['integer'](reveal[0], 1) and reveal[1] == sent_ts
                    and g.get('reveal', {}).get('serial') == str(reveal[0]) and g['reveal'].get('ts') == sent_ts, 'ACK reveal missing')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True, 'Native layout unavailable')
        v = g.get('viewport')
        require(isinstance(v, list) and len(v) == 2 and all(type(n) in (int,float) and math.isfinite(n) and n > 0 for n in v), 'Invalid viewport')
        if viewport is None: viewport = v
        require(v == viewport, 'Viewport changed')
        controls = g.get('controls', []); ids = [c['id'] for c in controls]
        require(len(ids) == len(set(ids)) and set(ids) == {'slack-list-comments-back','slack-list-comments-next',
                'slack-list-comments-refresh','slack-list-comment-send'}, 'Wrong controls')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Control obscured'); rectangle(control['rect'], v)
            require(control.get('disabled') is (pending or i == ack if control['id'] == 'slack-list-comment-send' else False), 'Control admission changed')
        e = g.get('editor', {})
        require(e.get('visible') is True and e.get('editable') is (not pending) and e.get('blocks') == content
                and e.get('generation') == str(generation) and type(e.get('sameNode')) is bool
                and e['sameNode'] is (i != ack), 'Native node/content/generation changed')
        rectangle(e.get('rect'), v)
        require(e.get('text') in ('','\n') if i == ack else e.get('text') == R['visible_text'](content), 'Native paragraph text changed')
        require(e.get('bold') is (not done), 'Native rich style changed')
        require(isinstance(g.get('active'), str), 'Missing native focus witness')
        if pending and i > 1: require(g['active'] == rows[1]['geometry']['active'], 'Socket/reaction moved held editor focus')
        if authored_b: require(g['active'] == rows[bi]['geometry']['active'] == 'slack-list-comment-draft', 'Socket moved newer draft focus')
        target = FIRST if i == 0 or pending and i == 1 else sent_ts
        probe = row.get('ackProbe')
        require(probe == dict(ts=target, visibleTs=visible, liveTs=live_ts, acceptedTs=accepted_ts,
                liveReactions=comments['liveReactions'], channel=CHANNEL, editorGeneration=str(generation),
                hasMore=True, readRequests=reads, prepared=count, posts=count)
                and probe['hasMore'] is True and all(type(probe[k]) is int for k in ('readRequests','prepared','posts')), 'Independent ACK probe disagrees')
        capture = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture','')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    events = receipt.get('ackEvents')
    require(events == expected_events(rows, case, original) and all(e.get('parsed') is True
            and type(e.get('generation')) is int and type(e.get('changed')) is bool for e in events), 'Parsed ACK/socket event evidence changed')
    require(receipt.get('final') == rows[-1]['model'], 'Final model changed')
    return receipt
