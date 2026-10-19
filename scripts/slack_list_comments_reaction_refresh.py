"""Offline proof that newer reactions retire stale reads without losing rich drafts."""
import copy
import math
from pathlib import Path
import runpy

R = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_newer_draft.py')))
C, READ, H = R['C'], R['READ'], R['H']
require, rectangle = R['require'], R['rectangle']
STAGES = ('draft', 'held', 'ignored-held', 'updated', 'released', 'refreshed',
          'sent', 'tail-draft', 'tail-held', 'tail-updated', 'tail-released', 'tail-refreshed')
B_TEXT = 'Newer draft B \u65e5\U0001f98a\nRetain this draft during comment refresh.'
ERROR = 'Newer comment updates arrived. Loaded comments are retained. Refresh to try again.'
READS = (4, 8, 8, 8, 8, 12, 16, 16, 20, 20, 20, 24)
COMPLETED = (4, 7, 7, 8, 8, 12, 16, 16, 19, 20, 20, 24)
CANCELLED = (0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2)
FIRST_TS, CHANNEL = '1781524801.000001', 'CCOMMENT01'


def reactions(count):
    return [{'name': 'wave', 'count': count}] if count else []


def source_reactions(count):
    return [dict(name='wave', count=count, users=['U1', 'U2', 'U_RT_REACTOR'][:count])] if count else []


def message(page, ts):
    matches = [m for m in page.get('messages', []) if m.get('ts') == ts]
    require(len(matches) == 1, 'Missing/duplicate source message')
    return matches[0]


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    directory = Path(directory)
    require(fixture in C['FIXTURES'] and receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('case') == 'reaction-refresh' and receipt.get('noLiveNetwork') is True,
            'Wrong reaction-refresh receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(STAGES), 'Missing reaction-refresh stages')
    first = rows[0]; initial = first['model']; seed = first['proof']['seed']
    a, b = first['proof']['originalA'], rows[7]['proof']['newerB']
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']}
            and initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == READ['ROOT_TS'], 'Wrong original inventory')
    scope = seed.get('scope', {})
    require(scope == dict(workspace='T1', user='U0', list=READ['LIST'], record=READ['KNOWN'],
                         root=READ['ROOT_TS'], created=touched[READ['KNOWN']].get('date_created'),
                         creator=touched[READ['KNOWN']].get('created_by'))
            and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong restored draft owner')
    ap, bp = R['paragraphs'](a), R['paragraphs'](b)
    seed_paragraphs = R['paragraphs'](seed.get('blocks'))
    # The restored seed ends in an existing plain LF. Native typing appends
    # before that terminal line break; it does not insert after the LF.
    require(len(seed_paragraphs) == 1 and seed_paragraphs[0][0][-1:] == [('text', '\n', {})],
            'Seed terminal paragraph boundary changed')
    seed_units = seed_paragraphs[0][0]
    first_text, second_text = C['REACTION_TEXT'].split('\n')
    expected_first = seed_units[:-1] + [('text', char, {'bold': True}) for char in first_text] + seed_units[-1:]
    require(len(ap) == 2 and ap[0][1].endswith(C['REACTION_TEXT'].split('\n')[0] + '\n')
            and ap[1][1] == C['REACTION_TEXT'].split('\n')[1]
            and ap[0][0] == expected_first and ap[1][0] == [('text', char, {}) for char in second_text]
            and len(R['opaque'](seed.get('blocks'))) == 2 and R['opaque'](a) == R['opaque'](seed['blocks'])
            and any(style.get('bold') is True for _, style in R['text_units'](a)), 'Original rich content/style changed')
    # B starts in the cleared editor: sections alone carry the paragraph break.
    require(len(bp) == 2 and [text for _, text in bp] == B_TEXT.split('\n')
            and all(units == [('text', char, {}) for char in text] for units, text in bp)
            and not R['opaque'](b) and a != b, 'Wrong new native draft B')
    posts = rows[6]['model']['composeLedger']['posts']
    positive = rows[6]['model']['composeLedger']['messages']
    require(len(posts) == len(positive) == 1 and C['outgoing'](posts[0]) == a, 'Wrong explicit send')
    R['_message'](positive[0], posts[0], scope)
    tail = positive[0]['ts']
    require(tail != FIRST_TS, 'Accepted reply reused loaded timestamp')
    page0 = initial['comments']['page']
    require(initial['comments']['visibleTs'] == [FIRST_TS]
            and message(page0, FIRST_TS).get('reactions') == source_reactions(2), 'Wrong initial reaction source')
    page1 = copy.deepcopy(page0); message(page1, FIRST_TS)['reactions'] = source_reactions(3)
    page2 = copy.deepcopy(page1); accepted = copy.deepcopy(positive[0])
    accepted['reactions'] = [dict(name='wave', count=1, users=['U_RT_REACTOR'])]
    page2['messages'].append(accepted)
    held_tail = copy.deepcopy(page1); held_tail['messages'].append(copy.deepcopy(positive[0]))
    read_template = initial['ledger']['requests']
    require(len(read_template) == 4 and [r['method'] for r in read_template] ==
            ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'], 'Wrong initial reads')
    journals = [rows[i]['proof']['journal'] for i in (0, 6, 7)]
    drafts = [rows[i]['model']['draftStore']['drafts'][0] for i in (0, 6, 7)]
    require(all(R['integer'](d['generation'], 1) for d in drafts)
            and drafts[0]['generation'] != drafts[1]['generation'] == drafts[2]['generation'], 'Send mount did not rotate once')
    require(journals[0]['revision'] < journals[1]['revision'] < journals[2]['revision']
            and len({j['capture'] for j in journals}) == 3, 'Durable captures did not advance')
    viewport = None
    previous_read = None
    for i, row in enumerate(rows):
        model, g, proof = row['model'], row['geometry'], row['proof']
        comments = model['comments']; sent = i >= 6; content = a if i < 6 else [] if i == 6 else b
        group = 0 if i < 6 else 1 if i == 6 else 2
        expected_draft, journal = drafts[group], journals[group]
        loading = i in (1, 2, 8); retired = i in (3, 4, 9, 10)
        held = i in (1, 2, 3, 8, 9)
        require(proof.get('seed') == seed and proof.get('originalA') == a
                and proof.get('newerB') == (b if i >= 7 else None), 'Rich proof rewritten')
        require(model.get('account') == initial.get('account') and R['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and R['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated, 'Canonical source mutated')
        require(comments.get('active') is True and comments.get('loading') is loading
                and comments.get('error') == (ERROR if retired else None)
                and comments.get('record') == READ['KNOWN'] and comments.get('root') == READ['ROOT_TS']
                and model.get('detail') == initial['detail'] and comments.get('hasMore') is True
                and all(comments.get(k) == initial['comments'].get(k) for k in ('generation', 'requestTs')),
                'Read owner/loading/error/pagination changed')
        require(comments.get('page') == (page0 if i < 5 else page2 if i == 11 else page1), 'Stale response replaced canonical Page')
        serial = comments.get('read')
        require(R['integer'](serial, 1), 'Missing read authority')
        if previous_read is not None:
            require(serial > previous_read if i in (1, 3, 5, 6, 8, 9, 11) else serial == previous_read,
                    'Read authority did not retire exactly at explicit load/live delta')
        previous_read = serial
        reads = model['ledger']['requests']; C['validate_reads'](reads)
        expected_reads = [dict(copy.deepcopy(r), serial=batch * 4 + n + 1)
                          for batch in range(READS[i] // 4) for n, r in enumerate(read_template)]
        require(reads == expected_reads, 'Missing/automatic/rewritten read')
        for key, expected in (('completed', COMPLETED[i]), ('cancelled', CANCELLED[i]), ('replies', READS[i] // 4)):
            require(type(model['ledger'].get(key)) is int and model['ledger'][key] == expected, 'Read completion/cancellation changed')
        require(model['ledger'].get('held') is held, 'Held transport state changed')
        ledger = model['composeLedger']
        require(ledger.get('prepared') == ledger.get('posts') == (posts if sent else [])
                and ledger.get('messages') == (positive if sent else [])
                and ledger.get('published') is (i >= 7)
                and ledger.get('held') is False and ledger.get('heldPost') is False, 'Extra send or rewritten accepted receipt')
        require(len(ledger.get('preflights', [])) == len(ledger.get('policies', [])) == int(sent), 'Wrong posting admission count')
        if sent:
            require(ledger['preflights'] == rows[6]['model']['composeLedger']['preflights']
                    and ledger['policies'] == rows[6]['model']['composeLedger']['policies'], 'Posting observation changed')
            require(ledger['preflights'][0] == dict(method='conversations.genericInfo', args={'updated_channels': '{"CCOMMENT01":0}'}),
                    'Wrong channel admission')
            policy = ledger['policies'][0]
            require(policy.get('request') == dict(method='client.userBoot', args={'include_min_version_bump_check': '1'})
                    and policy.get('boot', {}).get('ok') is True and policy['boot'].get('team', {}).get('id') == 'T1'
                    and policy['boot'].get('self', {}).get('id') == 'U0' and policy.get('channel', {}).get('id') == CHANNEL
                    and policy.get('threadReadOnly') is False, 'Wrong posting authority')
        store = model['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1, 'Draft custody changed')
        draft = entries[0]; generation = expected_draft['generation']
        require(proof.get('journal') == journal and journal.get('scope') == draft.get('scope') == scope
                and journal.get('blocks') == draft.get('blocks') == content
                and journal.get('capture') == draft.get('capture') and R['uuid'](draft.get('capture'))
                and journal.get('revision') == draft.get('savedRevision') and R['integer'](journal.get('revision'), 1)
                and journal.get('attempt') is None and draft.get('attempt') is None, 'Native model/journal disagree')
        require(draft.get('id') == drafts[0]['id'] and draft.get('generation') == generation
                and draft.get('sequence') == expected_draft['sequence'] and R['integer'](draft.get('sequence'))
                and draft.get('native') is True
                and store.get('active') == f"{draft['id']}:{comments['generation']}:{generation}"
                and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'captureFailed', 'terminalPending'))
                and draft.get('error') is None, 'Draft mount/storage disturbed')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True, 'Native layout unavailable')
        v = g.get('viewport')
        require(isinstance(v, list) and len(v) == 2 and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in v), 'Invalid viewport')
        if viewport is None: viewport = v
        require(v == viewport, 'Viewport changed')
        controls = g.get('controls', []); ids = [c['id'] for c in controls]
        require(len(set(ids)) == len(ids) and set(ids) == {'slack-list-comments-back', 'slack-list-comments-next',
                'slack-list-comments-refresh', 'slack-list-comment-send'}, 'Wrong controls')
        for c in controls:
            require(c.get('visible') is True and c.get('hit') is True, 'Control obscured'); rectangle(c.get('rect'), v)
            disabled = loading if c['id'] in ('slack-list-comments-next', 'slack-list-comments-refresh') else i == 6 if c['id'] == 'slack-list-comment-send' else False
            require(c.get('disabled') is disabled, 'Control admission changed')
        require(ERROR in g.get('notices', []) if retired else ERROR not in g.get('notices', []), 'Read retirement notice missing/stale')
        require(any(isinstance(n, str) and n.startswith('Loading comments') for n in g.get('notices', [])) is loading,
                'Native loading notice disagrees')
        e = g.get('editor', {})
        require(e.get('visible') is True and e.get('editable') is True and e.get('blocks') == content
                and str(e.get('generation')) == str(generation) and type(e.get('sameNode')) is bool
                and (i == 6 or e['sameNode'] is True), 'Original native editor/content lost')
        rectangle(e.get('rect'), v)
        require((e.get('text') in ('', '\n') if i == 6 else e.get('text') == R['visible_text'](content))
                and (i >= 6 or e.get('bold') is True), 'Native text/style changed')
        if i in (2, 3, 4, 9, 10):
            require(g.get('active') == rows[1 if i < 5 else 8]['geometry'].get('active'), 'Reaction/release stole native focus')
        else:
            require(isinstance(g.get('active'), str), 'Missing native focus witness')
        visible = [FIRST_TS] + ([tail] if sent else [])
        dom = g.get('rows', [])
        require(comments.get('visibleTs') == visible and [r.get('ts') for r in dom] == visible, 'Missing/duplicate displayed reply')
        counts = [2 if i < 3 else 3] + ([1 if i >= 9 else 0] if sent else [])
        expected_projection = [dict(ts=ts, reactions=reactions(count)) for ts, count in zip(visible, counts)]
        require(comments.get('reactions') == expected_projection
                and [dict(ts=r.get('ts'), reactions=r.get('reactions')) for r in dom] == expected_projection,
                'Model/DOM reaction counts disagree')
        require(all(isinstance(r.get('text'), str) and r['text'] for r in dom)
                and all(type(p['count']) is int for r in dom for p in r['reactions']), 'Missing row text or invalid pill count')
        probe = row.get('reactionProbe')
        require(probe == dict(ts=tail if sent else FIRST_TS, count=counts[-1], channel=CHANNEL,
                editorGeneration=str(generation), hasMore=True, readRequests=READS[i], prepared=int(sent), posts=int(sent))
                and all(type(probe[k]) is int for k in ('count', 'readRequests', 'prepared', 'posts'))
                and probe['hasMore'] is True, 'Independent reaction probe disagrees')
        require(proof.get('heldResponse') == (page0 if i in (1, 2, 3, 4) else held_tail if i in (8, 9, 10) else None),
                'Held old response was rewritten')
        require(proof.get('release') == (dict(delivered=False, cancelledBeforeReply=True) if i in (4, 10) else None),
                'Stale response was delivered')
        if i in (4, 10):
            require(proof['release']['delivered'] is False and proof['release']['cancelledBeforeReply'] is True,
                    'Invalid transport release proof')
        update = (dict(ts=FIRST_TS, reactions=source_reactions(3), heldUnchanged=True) if i in (3, 4) else
                  dict(ts=tail, reactions=accepted['reactions'], heldUnchanged=True) if i in (9, 10) else None)
        require(proof.get('serverUpdate') == update, 'Future response did not retain newer reaction')
        if update is not None:
            require(proof['serverUpdate']['heldUnchanged'] is True
                    and all(type(r.get('count')) is int for r in proof['serverUpdate']['reactions']), 'Invalid future response proof')
        capture = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    events = []
    for stage, team, generation, channel, ts, user, changed in (
        ('ignored-held', 'T1', 7, CHANNEL, FIRST_TS, 'U1', False),
        ('ignored-held', 'T_OTHER', 7, CHANNEL, FIRST_TS, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 6, CHANNEL, FIRST_TS, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 7, 'COTHER', FIRST_TS, 'U_RT_REACTOR', False),
        ('ignored-held', 'T1', 7, CHANNEL, '1999999999.000001', 'U_RT_REACTOR', False),
        ('updated', 'T1', 7, CHANNEL, FIRST_TS, 'U_RT_REACTOR', True),
        ('tail-updated', 'T1', 7, CHANNEL, tail, 'U_RT_REACTOR', True)):
        events.append(dict(stage=stage, team=team, generation=generation, changed=changed,
                           frame=dict(type='reaction_added', user=user, reaction='wave',
                                      item=dict(type='message', channel=channel, ts=ts))))
    require(receipt.get('reactionEvents') == events and all(type(e['generation']) is int and type(e['changed']) is bool
            for e in receipt['reactionEvents']), 'Parsed event routing proof changed')
    require(receipt.get('final') == rows[-1]['model'], 'Final model changed')
    return receipt
