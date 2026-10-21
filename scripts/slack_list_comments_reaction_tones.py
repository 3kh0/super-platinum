"""Closed List reaction glyphs; exact wire names remain separate from display."""
import copy
import math
from pathlib import Path
import runpy

X = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_actions.py')))
E, A, I, R, C, READ, H = (X[k] for k in ('E', 'A', 'I', 'R', 'C', 'READ', 'H'))
require, rectangle, exact = X['require'], X['rectangle'], X['exact']
CHANNEL, FIRST, ROOT, USER = X['CHANNEL'], X['FIRST'], X['ROOT'], 'U0'
STAGES = ('draft', 'socket', 'refreshed', 'offline')
NAMES = ('thumbsup::skin-tone-3', '+1::skin-tone-3', 'handshake::skin-tone-2-6',
         'wave', 'fixture_tone_custom', 'not_real::skin-tone-3')
GLYPHS = ('\U0001f44d\U0001f3fc', '\U0001f44d\U0001f3fc',
          '\U0001faf1\U0001f3fb\u200d\U0001faf2\U0001f3ff', '\U0001f44b',
          ':fixture_tone_custom:', ':not_real::skin-tone-3:')
READS = (4, 4, 8, 12)
OFFLINE = 'You are offline. Previously loaded comments are retained.'


def common(fixture, receipt, directory, dimensions, png):
    case = receipt.get('case'); directory = Path(directory)
    require(case == 'reaction-tones'
            and fixture in C['FIXTURES'] and receipt.get('fixture') == fixture
            and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True, 'Wrong reaction action receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(STAGES), 'Missing reaction action stages')
    initial = rows[0]['model']; proof0 = rows[0]['proof']; seed = proof0['seed']; a = proof0['originalA']
    A['rich_content'](seed, a, E['b_blocks']())
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']}
            and initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == ROOT, 'Wrong original source inventory')
    scope = dict(workspace='T1', user=USER, list=READ['LIST'], record=READ['KNOWN'], root=ROOT,
                 created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(seed.get('scope') == scope and seed.get('reopened') is True
            and R['integer'](seed.get('revision'), 1), 'Wrong restored journal scope')
    store0 = initial['draftStore']; draft0 = store0['drafts'][0]; journal0 = proof0['journal']
    generation = draft0['generation']; viewport = None
    require(R['integer'](generation, 1), 'Missing native generation')
    previous_draft = None
    for row in rows:
        model, g, proof = row['model'], row['geometry'], row['proof']; comments = model['comments']
        require(model.get('account') == initial.get('account') and R['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and R['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated
                and model.get('detail') == initial['detail'], 'Original List source/visit changed')
        require(comments.get('active') is True and comments.get('record') == READ['KNOWN']
                and comments.get('root') == ROOT and comments.get('hasMore') is True
                and all(comments.get(k) == initial['comments'].get(k) for k in ('generation', 'requestTs'))
                and all(comments.get(k) == [] for k in ('deleted', 'live', 'liveTs', 'accepted', 'acceptedTs', 'liveReactions', 'editCustody'))
                and comments.get('reveal') is None, 'Original thread ownership/projection changed')
        compose = model['composeLedger']
        require(all(compose.get(k) == [] for k in ('prepared', 'posts', 'messages', 'policies'))
                and all(compose.get(k) is False for k in ('published', 'held', 'heldPost')), 'Reaction sent a comment or posting request')
        require(proof.get('seed') == seed and proof.get('originalA') == a
                and proof.get('newerB') is None,
                'Original rich draft or independent disk proof changed')
        store = model['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1 and store.get('active') == store0.get('active'), 'Action changed native draft custody')
        draft, journal = entries[0], proof.get('journal', {})
        require(draft.get('id') == draft0['id'] and draft.get('generation') == generation
                and draft.get('native') is True and store.get('active') == f"{draft['id']}:{comments['generation']}:{generation}"
                and draft.get('scope') == journal.get('scope') == scope
                and draft.get('blocks') == journal.get('blocks') == a
                and draft.get('capture') == journal.get('capture') and R['uuid'](draft.get('capture'))
                and draft.get('savedRevision') == journal.get('revision') and R['integer'](journal.get('revision'), 1)
                and R['integer'](draft.get('sequence')) and draft.get('attempt') is journal.get('attempt') is None
                and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'captureFailed', 'terminalPending'))
                and draft.get('error') is None and draft.get('status') is None, 'Draft/model/journal disagreed or became busy')
        if previous_draft is not None:
            require(draft['sequence'] >= previous_draft['sequence'] and draft['savedRevision'] >= previous_draft['savedRevision'],
                    'Draft capture/revision went backwards')
        previous_draft = draft
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True, 'Native action surface unavailable')
        v = g.get('viewport')
        require(isinstance(v, list) and len(v) == 2 and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in v), 'Invalid viewport')
        if viewport is None: viewport = v
        require(v == viewport, 'Viewport changed')
        editor = g.get('editor', {})
        require(editor.get('visible') is True and editor.get('editable') is True and editor.get('blocks') == a
                and editor.get('generation') == str(generation) and editor.get('sameNode') is True
                and editor.get('bold') is True and editor.get('text') == R['visible_text'](a), 'Original native rich text/style/node changed')
        rectangle(editor.get('rect'), v)
        X['editor_text'](g)
        require(isinstance(g.get('active'), str), 'Missing native focus witness')
        controls = g.get('controls', []); ids = [c['id'] for c in controls]
        require(len(ids) == len(set(ids)) and set(ids) == {'slack-list-comments-back', 'slack-list-comments-next',
                'slack-list-comments-refresh', 'slack-list-comment-send'}, 'Wrong native controls')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Native control obscured')
            rectangle(control.get('rect'), v)
        capture = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Final snapshot changed')
    return rows, initial, scope


def source(updated):
    return [dict(name=name, count=2 if updated and n == 1 else 1,
                 users=['U1', 'U_RT_REACTOR'] if updated and n == 1 else ['U1'])
            for n, name in enumerate(NAMES)]


def page(updated):
    value = X['original_page']()
    value['messages'][1].update(reactions=source(updated),
                                 tone_fixture_opaque={'keep': 'original raw names'})
    return value


def probes(updated):
    return [dict(name=name, glyph=None if n == 4 else GLYPHS[n], media=n == 4,
                 count=2 if updated and n == 1 else 1, own=False, available=False)
            for n, name in enumerate(NAMES)]


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    rows, initial, _ = common(fixture, receipt, directory, dimensions, png)
    original, future = page(False), page(True)
    template = initial['ledger']['requests']
    require([r.get('method') for r in template] ==
            ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'],
            'Wrong original read sequence')
    event = dict(team='T1', generation=7, parsed=True,
                 frame=dict(type='reaction_added', user='U_RT_REACTOR', reaction=NAMES[1],
                            item=dict(type='message', channel=CHANNEL, ts=FIRST)),
                 before=probes(False), after=probes(True))
    media_source = dict(name=NAMES[4], source='https://fixture.invalid/reaction-tone-custom.png',
                        ready=True, asset='assets/icons/png/icon_16.png', network=False)
    previous_read, media_identity = None, None
    for i, row in enumerate(rows):
        model, g, proof = row['model'], row['geometry'], row['proof']
        comments, ledger = model['comments'], model['ledger']
        raw = original if i < 2 else future
        typed = [I['typed'](m) for m in raw['messages']]
        typed[1]['reactions'] = source(i > 0)
        rx = [dict(name=r['name'], count=r['count']) for r in source(i > 0)]
        require(exact(comments.get('page'), raw) and exact(comments.get('pageMessages'), typed)
                and exact(proof.get('initialResponse'), original)
                and exact(proof.get('serverResponse'), future if i >= 2 else None),
                'Original/future raw source or typed reaction identity changed')
        require(exact(proof.get('toneEvent'), event if i else None), 'Parsed alias socket provenance changed')
        require(exact(model.get('toneProbe'), probes(i > 0)) and exact(model.get('toneMedia'), media_source),
                'Model glyph/raw alias/custom media distinction changed')
        require(comments.get('visibleTs') == [FIRST]
                and exact(comments.get('reactions'), [dict(ts=FIRST, reactions=rx)]),
                'Projected reply or independent reaction rows changed')
        dom = g.get('rows', [])
        require(len(dom) == 1 and dom[0].get('ts') == FIRST and exact(dom[0].get('reactions'), rx)
                and isinstance(dom[0].get('text'), str)
                and all(dom[0]['text'].count(t) == 1 for t in ('First page comment.', 'Review ready \u4e2d\U0001f642.')),
                'Native reply body/reaction projection changed')
        require(comments.get('loading') is False and comments.get('error') == (OFFLINE if i == 3 else None)
                and exact(comments.get('retryAt'), 0) and exact(comments.get('retryAfter'), 0),
                'Unexpected loading/error/cooldown state')
        require(R['integer'](comments.get('read'), 1), 'Missing original read serial')
        if previous_read is not None:
            require(comments['read'] > previous_read if i >= 2 else comments['read'] == previous_read,
                    'Socket issued a read or explicit Refresh lost authority')
        previous_read = comments['read']
        reads = ledger.get('requests', [])
        C['validate_reads'](reads)
        expected_reads = [dict(copy.deepcopy(r), serial=batch * 4 + n + 1)
                          for batch in range(READS[i] // 4) for n, r in enumerate(template)]
        require(exact(reads, expected_reads)
                and all(exact(ledger.get(k), value) for k, value in
                        (('completed', READS[i]), ('replies', READS[i] // 4), ('cancelled', 0), ('held', False))),
                'Extra/missing/reordered read or changed completion custody')
        require(exact(model.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0,
                                                       held=False, networkCalls=0, fullReads=[]))
                and exact(model.get('reactionActions'), dict(attempts=[], pending=0))
                and model['composeLedger'].get('preflights') == [], 'Display case dispatched an action/readback/preflight')
        require(all(proof.get(k) is None for k in ('heldResponse', 'release', 'originalMessage')),
                'Display case invented a held response or original POST')
        if i == 1:
            require(model['draftStore'] == initial['draftStore']
                    and proof['journal'] == rows[0]['proof']['journal']
                    and g['active'] == rows[0]['geometry']['active'], 'Alias socket changed draft/node/focus custody')
        for control in g['controls']:
            require(control.get('disabled') is False, 'Settled display unexpectedly disabled a read/draft control')
        tone_geometry = g.get('tones', {})
        require(exact(tone_geometry.get('tools'), 0), 'Generic message toolbar introduced')
        pills = tone_geometry.get('pills', [])
        require([p.get('name') for p in pills] == list(NAMES), 'Raw alias names merged/reordered/normalized')
        for n, pill in enumerate(pills):
            require(pill.get('tag') == 'BUTTON' and pill.get('type') == 'button'
                    and pill.get('disabled') is True and pill.get('ariaDisabled') == 'true'
                    and pill.get('pressed') == 'false' and pill.get('own') is False
                    and exact(pill.get('count'), 2 if i > 0 and n == 1 else 1)
                    and all(pill.get(k) is True for k in ('visible', 'hit', 'glyphVisible', 'glyphHit')),
                    'Tone pill count/own/disabled/visibility semantics changed')
            rectangle(pill.get('rect'), g['viewport'])
            rectangle(pill.get('glyphRect'), g['viewport'])
            art, box = pill['glyphRect'], pill['rect']
            require(art['left'] >= box['left']-1 and art['right'] <= box['right']+1
                    and art['top'] >= box['top']-1 and art['bottom'] <= box['bottom']+1,
                    'Reaction glyph escaped its visible pill')
            if n == 4:
                media = pill.get('media') or {}
                src = media.get('src', '')
                require(pill.get('glyph') is None and pill.get('codepoints') == []
                        and media.get('alt') == ':fixture_tone_custom:' and media.get('complete') is True
                        and exact(media.get('naturalWidth'), 16)
                        and isinstance(src, str) and src.startswith(('super-platinum-media://emoji/',
                                                                 'http://super-platinum-media.emoji/')),
                        'Custom local art replaced by fallback/remote/missing image')
                identity = src.split('?', 1)[0]
                if media_identity is not None: require(identity == media_identity, 'Custom media identity changed')
                media_identity = identity
            else:
                require(pill.get('media') is None and pill.get('glyph') == GLYPHS[n]
                        and exact(pill.get('codepoints'), [ord(c) for c in GLYPHS[n]]),
                        'Exact tone/ZWJ/fallback glyph changed')
        notices = tone_geometry.get('notices', [])
        require(len(notices) == int(i == 3), 'Unexpected/missing tone read notice')
        if notices:
            require(notices[0].get('text') == OFFLINE and notices[0].get('visible') is True
                    and OFFLINE in g.get('notices', []), 'Offline retention guidance hidden/changed')
            rectangle(notices[0].get('rect'), g['viewport'])
    return receipt

