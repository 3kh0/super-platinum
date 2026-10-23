"""Original Canvas comment source, selected-map and rich draft custody oracle."""
import math
import json
import sys
from pathlib import Path
from urllib.parse import urlsplit, parse_qs
from native_check import expected_viewport

PREFIX = 'canvas-comment-emoji'
FLAG = 'SUPER_PLATINUM_CANVAS_COMMENT_EMOJI_CHECK'
FIXTURES = (PREFIX, PREFIX + '-narrow')
STAGES = ('stream-unobserved', 'stream-assigned-pending', 'stream-assigned-ready',
          'opened-thread-draft', 'socket-retired', 'fresh-assigned-ready')


def stage_plan(fixture):
    return tuple((name, view, i) for i, name in enumerate(STAGES)
                 for view in (('root', 'preview') if i < 3 else ('root', 'reply', 'editor'))
                 ) if fixture.endswith('-narrow') else tuple((name, 'all', i) for i, name in enumerate(STAGES))


def capture_stages(fixture):
    return tuple(name if view == 'all' else name + '-' + view for name, view, _ in stage_plan(fixture))
ACCOUNT, TEAM, USER = 'ce733150-11d9-476f-80ef-9c49462dde33', 'T1', 'UCANEMOJI'
ROOT, REPLY, HIDDEN, FILE = ('1719800000.000100', '1719800001.000100',
                            '1719800020.000100', '1719800030.000100')
URL = 'https://fixture.invalid/canvas-comment-emoji.png'
HIDDEN_URL = 'https://fixture.invalid/canvas-hidden-never.png'
FILE_URL = 'https://fixture.invalid/canvas-override-never.png'
DRAFT = 'Canvas draft caf\u00e9 \u4e2d\U0001f98a'
CAPTURED_TEXT = DRAFT + '\n'
RAW = {'ok': True, 'assignments': {'emoji_v17': {'group': 'on'}}}
ISOLATION = dict(syntheticContext=True, noHttp=True, networkDispatched=False,
                 account=ACCOUNT, team=TEAM, user=USER)


def require(value, message):
    if not value:
        raise RuntimeError(message)


def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def same(left, right):
    return json.dumps(left, sort_keys=True, ensure_ascii=False, allow_nan=False) == json.dumps(right, sort_keys=True, ensure_ascii=False, allow_nan=False)


def captured_blocks():
    # Native root-end insertion leaves the original empty rich block in place.
    # rich_editor.read preserves it; content::fallback joins the two blocks with LF.
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=DRAFT)])]),
            dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[])])]


def context(receipt):
    return (receipt.get('noSession') is False and receipt.get('noTransport') is False
            and receipt.get('isolation') == ISOLATION)


def raw(ts, label, url):
    value = dict(type='message', ts=ts, user=USER, text='DECOY ' + label, blocks=[dict(
        type='rich_text', block_id='original-' + ts, opaque={'keep': [False, 0, None]},
        elements=[dict(type='rich_text_section', elements=[
            dict(type='text', text=label + ' '),
            dict(type='emoji', name='conflicting_original', unicode='1f44d-1f3fd',
                 skin_tone=6, display_alias='wrong_alias', display_url=url),
            dict(type='text', text=' unknown '),
            dict(type='emoji', name='unknown_original', unicode='0041',
                 display_url='https://fixture.invalid/canvas-unknown-never.png'),
            dict(type='text', text=' invalid '),
            dict(type='emoji', name='invalid_original', unicode='1F44D',
                 display_url='https://fixture.invalid/canvas-invalid-never.png')])])])
    if ts == REPLY:
        value['thread_ts'] = ROOT
    else:
        value.update(subtype='document_comment_root', reply_count=int(ts == ROOT),
                     document_comment=dict(thread_id='quip-' + ts, is_visible=ts != HIDDEN))
    if ts == FILE:
        value['files'] = [dict(id='FCANEMOJI', mode='hosted', filetype='svg',
                              name='Original canonical file', title='Not the file name',
                              created=101, permalink='https://example.test/canonical/FCANEMOJI',
                              is_deleted=False)]
    return value


def messages(thread=False):
    values = [raw(ROOT, 'Root', URL), raw(REPLY, 'Last reply', URL)]
    return values if thread else values + [raw(HIDDEN, 'Hidden', HIDDEN_URL), raw(FILE, 'Superseded', FILE_URL)]


def typed_file(value):
    # models::File serializes its optional typed fields as null; raw HTTP remains exact above.
    defaults = dict.fromkeys(('id', 'name', 'title', 'mimetype', 'filetype', 'pretty_type',
                              'url_private', 'thumb_64', 'thumb_80', 'thumb_160', 'thumb_360',
                              'size', 'is_external'))
    return defaults | value


def boot():
    user = dict(id=USER, team_id=TEAM, name='canvas-reviewer', profile={'display_name': 'Canvas reviewer'})
    return dict(ok=True, team=dict(id=TEAM, name='Canvas emoji'), self=user, users=[user],
                channels=[dict(id='C1', name='canvas-review', is_channel=True,
                               is_member=True, updated=100, previous_names=[])], prefs={}, subteams={'self': []})


def exposure(value, viewport):
    r, p = value.get('rect', {}), value.get('hitPoint', {})
    require(value.get('exposed') is True and isinstance(p, dict) and set(p) == {'x', 'y'}, 'Original native exposure missing')
    require(set(r) == {'x', 'y', 'width', 'height'} and all(type(x) in (int, float) and math.isfinite(x) for x in (*r.values(), *p.values()))
            and r['width'] > 0 and r['height'] > 0
            and 0 < p['x'] < viewport[0] and 0 < p['y'] < viewport[1]
            and r['x'] <= p['x'] <= r['x'] + r['width'] and r['y'] <= p['y'] <= r['y'] + r['height'], 'Original hit point outside measured native control')


def opaque(src):
    p = urlsplit(src or '')
    require(not p.username and not p.password and not p.fragment and
            ((p.scheme == 'super-platinum-media' and p.netloc == 'emoji') or
             (p.scheme == 'http' and p.netloc == 'super-platinum-media.emoji')), 'Original opaque emoji protocol missing')
    q = parse_qs(p.query)
    require(set(q) == {'v'} and len(q['v']) == 1 and q['v'][0].isdigit() and p.path.startswith('/') and len(p.path) > 1, 'Original media identity/epoch absent')
    return p.path, int(q['v'][0])


def source(m, index, initial):
    require(m.get('contextCurrent') is True and m.get('account') == ACCOUNT and m.get('team') == TEAM
            and m.get('user') == USER and integer(m.get('epoch')) and integer(m.get('visit'))
            and same(m.get('boot'), boot()) and m.get('socket') == (7 if index < 4 else 8), 'Original synthetic source context changed')
    require(m.get('customEmoji') == {} and m.get('registered') == dict(known=int(index > 0), hidden=0, file=0, unknown=0, invalid=0), 'Discarded/unobserved media or custom catalog appeared')
    for key in ('epoch', 'visit', 'customRevision'):
        require(m.get(key) == initial.get(key), 'Original owner/catalog custody changed')
    platform = 'windows' if sys.platform == 'win32' else 'darwin' if sys.platform == 'darwin' else 'linux'
    count = (0, 1, 1, 1, 1, 2)[index]
    expected = [dict(serial=i + 1, socket=7 + i, method='POST', path='/api/experiments.getByUser',
                     args=dict(ssb_platform=platform, ssb_release_channel='prod'), tokenPresent=True,
                     factoryMatched=True, response=RAW) for i in range(count)]
    require(m.get('requests') == expected, 'Original assignment request factory/read changed')
    ws = m.get('witnesses', [])
    require(len(ws) == count, 'Original assignment witness missing')
    for i, w in enumerate(ws):
        o = w.get('owner', {})
        require(w.get('version') == 'v17' and w.get('response') == RAW and w.get('current') is (i == count - 1 and index != 4)
                and set(o) == {'account', 'epoch', 'team', 'user', 'socket', 'slotSerial'}
                and all(o.get(k) == m[k] for k in ('account', 'epoch', 'team', 'user'))
                and o.get('socket') == 7 + i and integer(o.get('slotSerial'), 1), 'Original map owner/currentness changed')
    require(m.get('selected') == (ws[-1] if index in (1, 2, 3, 5) else None), 'Unobserved or retired map acquired default authority')
    reads = m.get('reads', [])
    require(len(reads) == (1 if index < 3 else 2), 'Map publication caused extra comment read')
    for i, read in enumerate(reads):
        method = 'conversations.replies' if i else 'conversations.history'
        url = urlsplit(read.get('url', ''))
        fields = read.get('fields', [])
        actual = dict(fields)
        expected = (dict(channel='Cfixture', ts=ROOT, limit='28', inclusive='true', cached_latest_updates='{}') if i else
                    dict(channel='Cfixture', limit='20', document_stream='latest_reply', ignore_replies='true', inclusive='true',
                         include_pin_count='true', no_user_profile='true', include_stories='true', include_free_team_extra_messages='true',
                         include_date_joined='false', cached_latest_updates='{}'))
        require(url.scheme == 'https' and url.netloc == 'example.test' and url.path == '/api/' + method
                and not url.fragment and read.get('method') == 'POST' and len(fields) == len(actual)
                and actual == expected and read.get('tokenPresent') is True and integer(read.get('serial'), 1), 'Original comment factory/fields changed')
        require(same(read.get('response'), dict(ok=True, has_more=False, messages=messages(bool(i)))), 'Original authored raw body/URL/opaque fields changed')
    if index >= 3:
        require(reads[1]['serial'] == reads[0]['serial'] + 1, 'Original comment serial changed')
    retained = m.get('retained', {})
    require(retained.get('ownerCurrent') is True and retained.get('loading') is False and retained.get('error') is None
            and retained.get('serial') == reads[-1]['serial'] and retained.get('selected') == m['selected']
            and retained.get('owner') == dict(file='Ffixture', visit=m['visit'], team=TEAM), 'Original local Canvas read/owner changed')
    require(retained.get('query') == ('Some(Stream(Recent))' if index < 3 else 'Some(Replies("' + ROOT + '"))'), 'Original query changed')
    rows = retained.get('rows', [])
    expected_ids = {ROOT, HIDDEN, FILE} if index < 3 else {ROOT, REPLY}
    require(len(rows) == len(expected_ids) and {r.get('message', {}).get('ts') for r in rows} == expected_ids, 'Original retained row identity changed')
    original = {v['ts']: v for v in messages(False)}
    for row in rows:
        message = row['message']; want = original[message['ts']]
        require(all(same(message.get(k), [typed_file(f) for f in v] if k == 'files' else v)
                    for k, v in want.items()), 'Original typed message semantics changed')
        preview = row.get('preview')
        if index < 3 and message['ts'] == ROOT:
            require(isinstance(preview, dict) and all(same(preview.get(k), v) for k, v in original[REPLY].items()), 'Original raw preview lost or rewritten')
        else:
            require(preview is None, 'Unexpected preview source')
        require(row.get('file') == (dict(id='FCANEMOJI', name='Original canonical file', url='https://example.test/canonical/FCANEMOJI', ts=FILE) if message['ts'] == FILE else None), 'Original canonical first-file override changed')


def draft_identity(value):
    return {k: v for k, v in value.items() if k not in ('rect', 'hitPoint', 'exposed')}


def native(n, fixture, index, first, view='all'):
    require(n.get('ok') is True and n.get('viewport') == expected_viewport(fixture)
            and n.get('stream') is (index < 3) and n.get('sameOriginal') is True and integer(n.get('panel'), 1), 'Original native panel/viewport changed')
    rows = n.get('rows', [])
    require(len(rows) == (3 if index < 3 else 2), 'Original native rows absent')
    by_id = {r.get('ts'): r for r in rows}
    require(set(by_id) == ({ROOT, HIDDEN, FILE} if index < 3 else {ROOT, REPLY}), 'Original native row keys changed')
    selected, ready = index in (1, 2, 3, 5), index in (2, 3, 5)
    glyphs = [dict(text='A', title=':unknown_original:')]
    if not selected:
        glyphs.insert(0, dict(text='\U0001f44d\U0001f3fd', title=':conflicting_original:'))
    elif not ready:
        glyphs.insert(0, dict(text=':+1::skin-tone-4:', title=':+1::skin-tone-4:'))
    bodies = [(by_id[ROOT]['body'], 'Root', 'root'), (by_id[ROOT]['preview'] if index < 3 else by_id[REPLY]['body'], 'Last reply', 'preview' if index < 3 else 'reply')]
    for body, label, target_view in bodies:
        require(isinstance(body, dict) and integer(body.get('node'), 1) and body.get('glyphs') == glyphs
                and body.get('text') == label + ' ' + ('' if ready else ':+1::skin-tone-4:' if selected else '\U0001f44d\U0001f3fd') + ' unknown A invalid '
                and body.get('links') == [], 'Original body/preview semantic projection changed')
        if view in ('all', target_view):
            exposure(body, n['viewport'])
        images = body.get('images', [])
        require(len(images) == int(ready), 'Unexpected selected image or missing original image')
        for image in images:
            require(image.get('complete') is True and image.get('width') == 16 and image.get('height') == 16
                    and image.get('alt') == ':+1::skin-tone-4:' and image.get('title') == image['alt'], 'Original image bytes/name changed')
            opaque(image.get('src'))
            if view in ('all', target_view):
                exposure(image, n['viewport'])
    if index < 3:
        hidden, file = by_id[HIDDEN]['body'], by_id[FILE]['body']
        require(hidden.get('text') == 'This content was deleted.' and hidden.get('images') == [] and hidden.get('glyphs') == []
                and file.get('text') == 'Original canonical file' and file.get('images') == [] and file.get('glyphs') == []
                and file.get('links') == [dict(id='FCANEMOJI', root=FILE, name='Original canonical file', url='https://example.test/canonical/FCANEMOJI', text='Original canonical file')], 'Hidden or overridden media leaked into DOM')
        require(n.get('draft') is None, 'Stream invented reply native state')
    else:
        d = n.get('draft', {})
        require(d.get('connected') is True and d.get('composing') is False and d.get('start') == 2 and d.get('end') == 7
                and d.get('anchor') == d.get('focus') and integer(d.get('anchor'), 1) and integer(d.get('node'), 1)
                and isinstance(d.get('scope'), str) and d['scope'] and isinstance(d.get('mount'), str) and d['mount']
                and integer(d.get('generation'), 1), 'Original reply mount/caret changed')
        if view in ('all', 'editor'):
            exposure(d, n['viewport'])
        if first is not None:
            require((d == first['draft'] if view == 'all' else draft_identity(d) == draft_identity(first['draft'])), 'Original native draft/caret mutated across selected-map lifecycle')


def draft(row, first):
    reply, n = row.get('reply', {}), row['layout']['draft']
    runtime, journal = reply.get('runtime', {}), reply.get('journal', {})
    require(reply.get('ledger') == dict(requests=[], prepared=[]), 'Unexpected reply request/send preparation')
    require(all(runtime.get(k) == v for k, v in dict(account=ACCOUNT, workspace=TEAM, user=USER, file='Ffixture', root=ROOT,
            text=CAPTURED_TEXT, loading=False, saving=False, sending=False, error=None, nativePresent=True).items()), 'Original reply runtime custody changed')
    require(runtime.get('blocks') == n['blocks'] and integer(runtime.get('serial'), 1) and isinstance(runtime.get('capture'), str) and runtime['capture'], 'Original rich capture missing')
    require(same(n['blocks'], captured_blocks()), 'Actual input or original empty rich section changed')
    ds = journal.get('drafts', [])
    require(len(ds) == 1 and ds[0].get('blocks') == n['blocks'] and ds[0].get('text') == CAPTURED_TEXT
            and ds[0].get('capture') == runtime['capture'] and integer(ds[0].get('revision'), 1)
            and journal.get('attempt') is None and journal.get('history') == [], 'Original SQLite draft or send history changed')
    if first is not None:
        require(reply == first['reply'], 'Original durable draft/capture revised by display-only update')


def validate(receipt, fixture):
    require(fixture in FIXTURES and receipt.get('ok') is True and context(receipt), 'Unknown/unclosed Canvas emoji fixture')
    captures = receipt.get('stages', [])
    plan = stage_plan(fixture)
    require([r.get('stage') for r in captures] == list(capture_stages(fixture)), 'Missing or reordered original exposed captures')
    stages = [next(row for row, (_, _, index) in zip(captures, plan) if index == i) for i in range(6)]
    first = stages[0]['model']
    for ordinal, (row, (logical, view, i)) in enumerate(zip(captures, plan)):
        require(row.get('ok') is True and type(row.get('ordinal')) is int and row['ordinal'] == ordinal and context(row), 'Original stage/isolation changed')
        model, proof, layout = row['model'], row['proof'], row['layout']
        source(model, i, first); native(layout, fixture, i, stages[3]['layout'] if i >= 3 else None, view)
        if view != 'all':
            require(row.get('logicalStage') == logical and row.get('view') == view and layout.get('view') == view, 'Sequential exposure phase changed')
            keyed = {r['ts']: r for r in layout['rows']}
            target = (keyed[ROOT]['body'] if view == 'root' else keyed[ROOT]['preview'] if view == 'preview'
                      else keyed[REPLY]['body'] if view == 'reply' else layout['draft'])
            evidence = row.get('exposure', {})
            require(evidence.get('ok') is True and evidence.get('action') == 'scroll-original-target'
                    and evidence.get('view') == view and evidence.get('target') == target['node'] == layout.get('target')
                    and same(evidence.get('before'), evidence.get('after'))
                    and set(evidence.get('before', {})) == {'active', 'anchor', 'focus', 'start', 'end'}, 'Actual original scroll/focus/selection proof missing')
            exposure(evidence, layout['viewport']); exposure(target, layout['viewport'])
            for scroll in (evidence.get('scrollBefore', {}), evidence.get('scrollAfter', {}), layout.get('scroll', {})):
                require(set(scroll) == {'top', 'left'} and all(type(v) in (int, float) and math.isfinite(v) and v >= 0 for v in scroll.values()), 'Actual scroll offsets missing')
            if i >= 3:
                require(all(evidence['before'].get(k) == layout['draft'][k] for k in ('anchor', 'focus', 'start', 'end')), 'Scrolling used another caret or restored a lost selection')
            phase = stages[i]['model']
            require(same({k:v for k,v in model.items() if k != 'mediaEpoch'},
                         {k:v for k,v in phase.items() if k != 'mediaEpoch'}), 'Original model changed between sequential exposures')
        baseline = stages[0 if i < 3 else 3]['model']
        before_rows = baseline['retained']['rows']
        require(same([(r['message'], r.get('preview')) for r in model['retained']['rows']],
                     [(r['message'], r.get('preview')) for r in before_rows])
                and same(model['retained'].get('root'), baseline['retained'].get('root')), 'Complete original typed rows/preview/root changed')
        if ordinal:
            previous_reads = captures[ordinal - 1]['model']['reads']
            require(same(model['reads'][:len(previous_reads)], previous_reads), 'Original HTTP request/response receipt replaced')
        require(layout['panel'] == stages[0]['layout']['panel'], 'Original Panel remounted')
        if view != 'all' or i in (1, 2, 4, 5):
            previous = stages[0 if i < 3 else 3]['layout']
            require([(r['ts'], r['node'], r['body']['node'], r['preview']['node'] if r['preview'] else None) for r in layout['rows']] ==
                    [(r['ts'], r['node'], r['body']['node'], r['preview']['node'] if r['preview'] else None) for r in previous['rows']], 'Original row/body/preview nodes replaced')
        require(proof.get('stream') == stages[0]['proof'].get('stream') and proof['stream'].get('ok') is True, 'Original stream arming lost')
        if i:
            require(proof.get('assignment1') == stages[1]['model']['selected'], 'Original accepted assignment proof replaced')
        if i >= 2:
            media = proof.get('media', {}); path, epoch = opaque(media.get('uriBefore'))
            require(media.get('url') == URL and media.get('id') == 'super-platinum-media://emoji' + path
                    and media.get('mime') == 'image/png' and media.get('bytes') == len((Path(__file__).resolve().parent.parent / 'assets/icons/png/icon_16.png').read_bytes())
                    and media.get('epochBefore') == epoch and media.get('readyBefore') is False and media.get('readyAfter') is True, 'Original registry media delivery differs')
            for r in layout['rows']:
                for b in [r['body'], r['preview']]:
                    for image in (b['images'] if b else []):
                        actual, generation = opaque(image['src'])
                        require(actual == path and epoch <= generation <= model['mediaEpoch'], 'Current native image not original delivered asset')
        if i < 3:
            require(row.get('reply') is None, 'Stream created reply journal entry')
        else:
            draft(row, stages[3])
            inp, arm = proof.get('input', {}), proof.get('draft', {})
            require(inp.get('ok') is True and inp.get('action') == 'synthetic-native-insertText' and inp.get('isTrusted') is False
                    and inp.get('text') == DRAFT and inp.get('node') == layout['draft']['node'], 'Actual exposed native input missing')
            exposure(inp.get('before', {}), layout['viewport'])
            require(arm.get('ok') is True and all(arm.get(k) == layout['draft'][k] for k in ('node', 'anchor', 'focus', 'start', 'end', 'scope', 'generation', 'mount', 'blocks')), 'Original caret/capture arming differs')
        if i >= 4:
            before = stages[1]['model']['selected']
            require(proof.get('retirement') == dict(kind='synthetic-socket-connected', before=before, after=dict(before, current=False),
                    socket=8, accessorAvailable=False, networkDispatched=False, workResumed=False), 'Old socket map not retired')
        if i == 5:
            require(proof.get('assignment2') == model['selected'] and model['selected']['owner']['slotSerial'] != stages[1]['model']['selected']['owner']['slotSerial'], 'Fresh source reused stale slot')
    require(receipt.get('layout') == captures[-1]['layout'] and receipt.get('reply') == captures[-1]['reply']
            and receipt.get('requestCount') == 2 and receipt.get('syntheticInput') is True and receipt.get('previewRetirementNative') is False, 'Terminal claim/custody changed')
    source(receipt['model'], 5, first)
    require({k:v for k,v in receipt['model'].items() if k != 'mediaEpoch'} ==
            {k:v for k,v in captures[-1]['model'].items() if k != 'mediaEpoch'}, 'Terminal original model differs')
