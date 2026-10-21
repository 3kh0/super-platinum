"""Independent raw-source, target-reader and original-draft destination oracle."""
import base64
import math
from urllib.parse import urlsplit
import canvas_canonical_file_roots as D
import canvas_canonical_file_custody as C
from canvas_special_roots import require, same, bounded

STAGES = ('thread', 'draft', 'held', 'source-retired', 'destination', 'reopened')
READS = (2, 2, 2, 3, 3, 5)
RESOLVES = (0, 0, 1, 1, 2, 2)
FIXTURES = tuple('canvas-canonical-native-' + kind + suffix
                 for kind in ('canvas', 'list') for suffix in ('', '-narrow'))
CANVAS, LIST = 'FCANON002', 'FCANON003'
TITLE = {CANVAS: 'Canonical destination Canvas', LIST: 'Fresh canonical List'}
BODY = {CANVAS: 'Native canonical Canvas destination content',
        LIST: 'Native canonical List destination row'}
READ_ONLY_REASON = 'You have read-only access to this List.'
EXPORT = 'https://files.slack.com/files-tmb/FCANON002/destination.html'
HTML = ("<div class='quip-canvas-content'><h1>Canonical destination Canvas</h1>"
        "<p>Native canonical Canvas destination content</p>"
        "<p>Original rich reply remains saved. &amp; \u4e2d\U0001f98a</p></div>")


def target_file(id):
    if id == CANVAS:
        return dict(id=id, mode='quip', filetype='quip', name='Canvas canonical name',
                    title=TITLE[id], created=101, is_deleted=False, editable=False,
                    quip_thread_id='canonical-destination-thread', url_private=EXPORT,
                    permalink='https://example.test/canonical/' + id,
                    destination_opaque={'keep': [0, False, None]})
    require(id == LIST, 'Unknown native target')
    return dict(id=id, mode='list', filetype='list', name='Canonical List destination',
                title=TITLE[id], permalink='https://fixture.slack.com/files/' + id,
                access='read', is_locked=True, list_metadata={'schema': [
                    {'id': 'C_CANON_NAME', 'key': 'NAME', 'name': 'Item',
                     'type': 'text', 'is_primary_column': True}], 'views': []})


def roots(id, renamed=False):
    rows = D.roots()
    first = D.file(id)
    first['name'] = 'EMBEDDED first ' + id
    last = ({'id': id, 'name': 'Renamed canonical & \u4e2d\U0001f98a',
             'permalink': 'https://example.test/canonical/renamed', 'updated': 102}
            if renamed else D.file(id))
    rows[0]['files'] = [first, last]
    return rows


def response(id, renamed=False, thread=False):
    rows = roots(id, renamed)
    return {'ok': True, 'has_more': False,
            'messages': [rows[0], D.REPLY] if thread else rows}


def source_responses(id):
    return [response(id), response(id, thread=True), response(id, True, True),
            response(id, True), response(id, True, True)]


def resolver(value, id, index):
    n = RESOLVES[index]
    require(isinstance(value, dict) and len(value.get('requests', [])) == n,
            'Resolver dispatch inventory differs')
    for req in value['requests']:
        wire(req, '/api/files.info', [['file', id]])
    require(same(value.get('responses'), [{'ok': True, 'file': target_file(id)}] * n),
            'Fresh resolver raw file was substituted')
    settled = 0 if index < 3 else 1 if index == 3 else 2
    require(value.get('settlements') == [{'serial': j, 'delivered': True}
                                        for j in range(1, settled + 1)],
            'Original held result settlement changed')
    require(value.get('held') == (1 if index == 2 else None),
            'Held resolver phase differs')
    require(value.get('releases') == ([{'serial': 1, 'delivered': True}]
                                    if index >= 3 else []),
            'Retired resolver was not actually delivered')


def wire(value, path, fields):
    require(isinstance(value, dict), 'Request missing')
    u = urlsplit(value.get('url', ''))
    require(value.get('method') == 'POST' and u.scheme == 'https'
            and u.netloc == 'fixture.slack.com' and u.path == path
            and not u.fragment and same(value.get('fields'), fields),
            'Original factory request differs')


def vint(n):
    result = bytearray()
    while n > 127:
        result.append((n & 127) | 128)
        n >>= 7
    result.append(n)
    return bytes(result)


def pb(tag, value):
    if isinstance(value, int):
        return vint(tag << 3) + vint(value)
    if isinstance(value, str):
        value = value.encode()
    return vint((tag << 3) | 2) + vint(len(value)) + value


def encoded(raw):
    return base64.b64encode(raw).decode('ascii')


def canvas_requests():
    thread, document = 'canonical-destination-thread', 'canonical-destination-document'
    options = pb(1, 'canonical-quip-user') + pb(33, 'U0') + pb(34, 'T1')
    payload = pb(3, pb(1, thread) + pb(2, 101) + pb(10, document))
    payload += pb(6, pb(1, document) + pb(2, 101) + pb(6, thread))
    load = pb(2, pb(1, 1) + pb(2, payload) + pb(3, pb(1, 'Section/' + document)))
    req = pb(1, thread) + pb(3, pb(1, 1) + pb(2, thread))
    shapes = [
        ('POST', '/api/files.info', {'file': CANVAS}, {'ok': True, 'file': target_file(CANVAS)}),
        ('POST', '/api/quip.getControllerInitData', {'_x_reason': 'canvas-controller-init'},
         {'ok': True, 'quip_user_id': 'canonical-quip-user', 'init_options': encoded(options)}),
        ('POST', '/canvas/-/load-data', {'request_binary': encoded(req)}, {'base64': encoded(load)}),
        ('POST', '/api/files.info', {'file': CANVAS}, {'ok': True, 'file': target_file(CANVAS)}),
        ('GET', EXPORT, {}, {'html': HTML})]
    return [dict(serial=i + 1, pair=1, target=CANVAS, method=m, path=p, args=a, response=r)
            for i, (m, p, a, r) in enumerate(shapes)]


def list_responses():
    return [
        {'ok': True, 'file': target_file(LIST)},
        {'ok': True, 'list_id': LIST, 'records': [
            {'id': 'R_CANON_DESTINATION', 'list_id': LIST, 'position': 0,
             'fields': [{'key': 'NAME', 'text': BODY[LIST]}],
             'canonical_navigation_opaque': {'keep': [0, False, None]}}]},
        {'ok': True, 'prefs': {'allow_lists': 'full'}},
        {'ok': True, 'prefs': {'task_lists': '[]', 'assigned_to_list': ''}},
        {'ok': True, 'assignments': {}}]


def list_wires(platform='windows'):
    return [
        ('/api/files.info', [['file', LIST]]),
        ('/api/lists.records.list', [['list_id', LIST], ['include_subtasks', 'true'],
          ['archived', 'false'], ['include_suggested', 'false'], ['_x_reason', 'get-list-records']]),
        ('/api/team.prefs.get', [['target_team', 'T1'], ['_x_reason', 'native-list-permission']]),
        ('/api/users.prefs.get', [['prefs', 'task_lists,assigned_to_list'],
          ['_x_reason', 'native-list-completion-permission']]),
        ('/api/experiments.getByUser', [['ssb_platform', platform], ['ssb_release_channel', 'prod']])]


def targets(value, id, arrived):
    require(isinstance(value, dict) and set(value) == {'canvas', 'list'}, 'Target reader proof missing')
    for kind in ('canvas', 'list'):
        v = value[kind]
        require(v.get('networkCalls') == 0 and v.get('httpCapability') is False
                and v.get('held') is None and v.get('releases') == [],
                'Target gained network capability or held read')
        active = arrived and kind == ('canvas' if id == CANVAS else 'list')
        if not active:
            require(v.get('requests') == [] and v.get('settlements') == []
                    and v.get('pairs' if kind == 'canvas' else 'responses') == [],
                    'Destination loaded before current departure or wrong reader ran')
        elif kind == 'canvas':
            require(same(v.get('requests'), canvas_requests())
                    and v.get('pairs') == [{'serial': 1, 'held': False}]
                    and v.get('settlements') == [{'serial': 1, 'status': 'ready'}],
                    'Original paired Canvas reader evidence differs')
        else:
            q = v.get('requests')
            require(isinstance(q, list) and len(q) == 1, 'List reader dispatched more than once')
            q = q[0]
            require(q.get('serial') == 1 and q.get('id') == LIST
                    and all(type(q.get(k)) is int and q[k] >= 0 for k in ('generation', 'read', 'revision')),
                    'List reader ownership missing')
            requests = q.get('requests')
            require(isinstance(requests, list) and len(requests) == 5, 'List factory count differs')
            platform = dict(requests[-1].get('fields', [])).get('ssb_platform')
            require(platform in ('windows', 'linux', 'darwin'), 'List experiment platform invalid')
            for actual, (path, fields) in zip(requests, list_wires(platform)):
                wire(actual, path, fields)
            require(same(v.get('responses'), [{'serial': 1, 'responses': list_responses()}])
                    and v.get('settlements') == [{'serial': 1, 'delivered': True,
                        'current': True, 'parsed': True, 'refreshAgain': False}],
                    'List raw decoder/settlement evidence differs')


def visible(p, viewport, clip=None):
    require(isinstance(p, dict) and p.get('inView') is True and p.get('hit') is True,
            'Painted destination target missing or occluded')
    r = p.get('rect')
    require(isinstance(r, dict) and all(type(r.get(k)) in (int, float)
            and math.isfinite(r[k]) for k in ('x', 'y', 'width', 'height')),
            'Nonfinite target geometry')
    require(r['width'] > 0 and r['height'] > 0
            and bounded({**r, 'right': r['x'] + r['width'], 'bottom': r['y'] + r['height']}, viewport),
            'Painted target clipped')
    if clip is not None:
        require(r['y'] >= clip['y'] - 1 and r['y'] + r['height'] <= clip['y'] + clip['height'] + 1,
                'Painted target outside scroll viewport')


def source_geometry(v, index, viewport, expected):
    require(v.get('ok') is True and v.get('viewport') == viewport and v.get('inScroll') is True
            and v.get('viewer') is None, 'Source capture replaced by destination')
    visible(v.get('target'), viewport, v.get('scroll'))
    require(v.get('targetKind') == ('link' if index == 2 else 'editor-line'), 'Source target changed')
    n = v.get('native')
    require(isinstance(n, dict) and same(n.get('blocks'), expected)
            and n.get('composing') is False and n.get('running') is True,
            'Original native rich draft not retained')
    if index in (1, 2):
        require(all(n.get(k) is True for k in ('sameNode', 'sameMount', 'sameBlocks')),
                'Pre-refresh original native holder replaced')
    if index == 5:
        require(n.get('sameNode') is False and n.get('sameMount') is False
                and n.get('sameBlocks') is True, 'Original editor was not freshly restored')


def list_guidance(value, viewport):
    require(isinstance(value, dict) and value.get('reason') == READ_ONLY_REASON,
            'Actual List read-only guidance missing')
    reasons = value.get('visibleReasons')
    require(isinstance(reasons, list) and len(reasons) == 1,
            'List read-only explanation must have exactly one visible owner')
    reason = reasons[0]
    require(reason.get('text') == READ_ONLY_REASON, 'Visible List explanation changed')
    visible(reason, viewport)
    add = value.get('add')
    require(isinstance(add, dict) and add.get('text') == 'Add item'
            and add.get('disabled') is True and add.get('title') == READ_ONLY_REASON,
            'Read-only Add control lost its disabled state or original explanation')
    described = add.get('describedBy')
    require(isinstance(described, str) and described.strip(), 'Add has no accessible description reference')
    ids = described.split()
    require(len(ids) == len(set(ids)) and ids == ['slack-list-create-help'],
            'Add describes a duplicate or unrelated help element')
    descriptions = add.get('descriptions')
    require(isinstance(descriptions, list) and len(descriptions) == len(ids),
            'Add description reference does not resolve')
    for id, description in zip(ids, descriptions):
        require(isinstance(description, dict) and description.get('id') == id
                and type(description.get('matches')) is int and description['matches'] == 1
                and description.get('text') == READ_ONLY_REASON
                and type(description.get('visible')) is bool,
                'Add accessible help is missing, ambiguous or incorrectly associated')
        require(not description['visible'] or reason.get('id') == id,
                'Visible Add help duplicates the independently visible List reason')
    # Add can lie outside the narrow header scroll viewport. Its accessibility
    # association is separate from the fully painted visible read-only notice.
    r = add.get('rect')
    require(isinstance(r, dict) and all(type(r.get(k)) in (int, float) and math.isfinite(r[k])
            for k in ('x', 'y', 'width', 'height')) and r['width'] > 0 and r['height'] > 0,
            'Add control has no measured layout box')


def destination_geometry(v, id, viewport):
    require(v.get('ok') is True and v.get('ready') is True and v.get('viewport') == viewport
            and v.get('kind') == ('canvas' if id == CANVAS else 'list') and v.get('id') == id
            and v.get('native') is None, 'Native route did not replace original editor')
    for k in ('title', 'body', 'close'):
        visible(v.get(k), viewport, v.get('scroll') if k == 'body' else None)
    require(v['title'].get('text') == TITLE[id] and BODY[id] in v['body'].get('text', ''),
            'Actual canonical destination title/body differs')
    require(isinstance(v.get('generation'), str) and v['generation'], 'Destination visit missing')
    if id == LIST:
        list_guidance(v.get('guidance'), viewport)
        require(v.get('rowId') == 'R_CANON_DESTINATION'
                and isinstance(v.get('schema'), list) and len(v['schema']) == 1
                and v['schema'][0].get('id') == 'C_CANON_NAME'
                and 'Item' in v['schema'][0].get('text', ''), 'Destination List schema/row differs')
    else:
        require(v.get('rowId') is None and v.get('schema') is None
                and v.get('guidance') is None, 'Canvas inherited List identity or guidance')


def validate(receipt):
    stages = receipt.get('stages')
    id = receipt.get('destinationId')
    require(id in (CANVAS, LIST) and isinstance(stages, list) and len(stages) == 6,
            'Destination/stage inventory invalid')
    viewport = receipt.get('layout', {}).get('viewport')
    draft_identity = None
    saved_edit = None
    for i, (s, name, count) in enumerate(zip(stages, STAGES, READS)):
        require(s.get('stage') == name and type(s.get('ordinal')) is int and s['ordinal'] == i
                and all(s.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Destination stage/isolation changed')
        p = s['proof']
        rows = [roots(id, i >= 3)[0], D.REPLY]
        v = D.wire_proof(p, source_responses(id)[:count], {1, 2, 4}, rows, True, visible=i != 4)
        resolver(p.get('resolver'), id, i)
        # Destination retains the final original-model snapshot taken before
        # unmount; that snapshot still contains the committed request marker.
        require(v.get('pendingFile') is (i in (2, 4)), 'Original resolver pending state differs')
        catalog = v.get('catalog')
        require(isinstance(catalog, dict) and catalog.get('serial') == v['serial'],
                'Original canonical display provenance missing')
        origin = next((o for o in catalog.get('origins', []) if o.get('ts') == D.ROOT), None)
        require(origin == {'ts': D.ROOT, 'firstId': id, 'blockId': 'original-' + D.ROOT},
                'First-file source identity changed')
        row = next((r for r in catalog.get('rows', []) if r.get('id') == id), None)
        raw = rows[0]['files'][-1]
        require(isinstance(row, dict) and same(row.get('source'), raw)
                and row.get('effective', {}).get('name') == raw['name']
                and row.get('effective', {}).get('permalink') == raw['permalink'],
                'Canonical source was replaced with embedded/fresh route data')
        targets(s.get('targetReads'), id, i >= 4)
        expected = C.draft(s.get('reply'), 0 if i == 0 else 1)
        runtime = s['reply']['runtime']
        require(runtime.get('error') is None, 'Reply storage error unresolved')
        item = s['reply']['journal']['drafts'][0]
        expected_text = ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\n'
                         'Opaque attributes retained.' +
                         (' Canonical tail \u4e2d\U0001f98a' if i else ''))
        require(runtime.get('text') == expected_text and item.get('text') == expected_text
                and isinstance(item.get('capture'), str) and item['capture']
                and runtime.get('capture') == item['capture']
                and type(item.get('revision')) is int and item['revision'] > 0,
                'Durable native capture/text/revision differs')
        ident = item.get('id')
        require(isinstance(ident, str) and ident, 'Original durable draft identity missing')
        draft_identity = ident if draft_identity is None else draft_identity
        require(ident == draft_identity, 'Destination borrowed a different journal draft')
        if i == 1:
            saved_edit = item
            require(item['revision'] > stages[0]['reply']['journal']['drafts'][0]['revision'],
                    'Actual native edit did not advance durable revision')
        if i >= 2:
            require(same(item, saved_edit), 'Unedited draft changed during resolver/departure/reopen')
        if i == 4:
            require(runtime.get('nativePresent') is False, 'Original native capture holder not retired')
            destination_geometry(s['layout'], id, viewport)
        else:
            source_geometry(s['layout'], i, viewport, expected)
    departure = receipt.get('departure')
    require(isinstance(departure, dict), 'Predeparture original witness absent')
    C.draft(departure.get('reply'), 1)
    n = departure.get('native', {}).get('native', {})
    require(same(n.get('blocks'), C.edited()) and all(n.get(k) is True
            for k in ('sameNode', 'sameMount', 'sameBlocks', 'running')),
            'Predeparture witness was not captured on the original native holder')
    require(departure['reply']['journal']['drafts'][0]['id'] == draft_identity,
            'Predeparture disk witness changed identity')
    require(same(departure['reply']['journal']['drafts'][0], saved_edit),
            'Predeparture original capture/revision changed')
    require(same(receipt.get('proof'), stages[-1]['proof'])
            and same(receipt.get('layout'), stages[-1]['layout']),
            'Terminal substituted captured source/layout')
