"""Original missing-file acquisition, metadata-only publication and draft custody."""
import copy
import canvas_canonical_file_roots as D
import canvas_canonical_file_custody as C
from canvas_canonical_native_destinations import visible, wire
from canvas_special_roots import require, same

PREFIX = 'canvas-cold-file-roots'
FIXTURES = ('canvas-canonical-file-custody', 'canvas-canonical-file-custody-narrow')
STAGES = ('offline', 'draft', 'loading', 'retired', 'malformed', 'limited',
          'limit-reopened', 'limit-expired', 'recovered', 'viewer', 'reopened')
READS = (2, 2, 2, 2, 3, 3, 5, 5, 5, 5, 7)
ACQUISITIONS = (2, 2, 3, 3, 4, 5, 5, 5, 6, 6, 7)
MODES = ('offline', 'offline', 'held', 'malformed', 'rate-limited', 'success', 'success')
ID = D.IDS[0]
NAME = 'Cold canonical FCANON001 & \u4e2d\U0001f98a'
OFFLINE = 'File details are unavailable while offline. Retry when connected.'
LOADING = 'Loading file details...'
RETIRED = 'File details changed. Refresh comments to load them again.'
FAILED = 'File details could not be loaded. Retry when ready.'
LIMITED = 'Slack asked us to wait before retrying file details.'
AVAILABLE = 'You can retry loading file details now.'
PHASES = ('unavailable', 'unavailable', 'loading', 'retired', 'unavailable',
          'unavailable', 'unavailable', 'unavailable', None, None, None)
NOTICES = (OFFLINE, OFFLINE, LOADING, RETIRED, FAILED, LIMITED, LIMITED, AVAILABLE, None, None, None)
STORED_NOTICES = (OFFLINE, OFFLINE, None, RETIRED, FAILED, LIMITED, LIMITED, LIMITED, None, None, None)
ARGS = {'file': ID, 'page': '1', 'count': '500', 'truncate': 'true',
        'skip_shares': 'true', '_x_reason': 'files-store-unknown-fetch'}


def roots():
    rows = D.roots()
    rows[0]['files'] = [{'id': ID, 'file_access': 'check_file_info'}]
    return rows


def envelope(thread=False):
    rows = roots()
    return {'ok': True, 'has_more': False, 'messages': [rows[0], D.REPLY] if thread else rows}


def responses():
    return [envelope(), envelope(True), envelope(True), envelope(), envelope(True),
            envelope(), envelope(True)]


def positive():
    file = D.file(ID)
    file.update(name=NAME, title='Different cold title must not label root')
    return {'ok': True, 'file': file}


def raw_response(mode):
    return None if mode in ('offline', 'rate-limited') else {'ok': True, 'file': False} if mode == 'malformed' else positive()


def acquisition(value, index):
    count = ACQUISITIONS[index]
    require(isinstance(value, dict) and value.get('networkCalls') == 0
            and value.get('httpCapability') is False, 'Acquisition has live capability')
    requests, raw = value.get('requests'), value.get('responses')
    require(isinstance(requests, list) and isinstance(raw, list)
            and len(requests) == len(raw) == count, 'Acquisition dispatched extra or missing requests')
    for serial, (request, response, mode) in enumerate(zip(requests, raw, MODES), 1):
        require(request.get('serial') == serial and request.get('id') == ID
                and request.get('method') == 'files.info' and request.get('path') == '/api/files.info'
                and request.get('mode') == mode and request.get('reason') == 'files-store-unknown-fetch'
                and same(request.get('args'), ARGS), 'Original singleton request changed')
        require(same(response, {'serial': serial, 'id': ID, 'disposition': mode,
                                'raw': raw_response(mode)}), 'Original singleton envelope changed')
    settlements = [{'serial': j, 'cancelled': j == 3} for j in range(1, count + 1)
                   if not (index == 2 and j == 3)]
    require(value.get('settlements') == settlements and value.get('completed') == len(settlements)
            and value.get('cancelled') == (1 if index >= 3 else 0), 'Original read cancellation differs')
    require(value.get('releases') == ([{'serial': 3, 'id': ID, 'delivered': False,
                                      'cancelledBeforeReply': True}] if index >= 3 else []),
            'Late metadata was delivered or rebound')
    require(value.get('held') == ([{'serial': 3, 'id': ID, 'receiverClosed': False}]
                                  if index == 2 else []), 'Held read custody differs')


def resolver(value, index):
    count = int(index >= 9)
    require(isinstance(value, dict) and len(value.get('requests', [])) == count,
            'Metadata acquisition navigated or duplicate click resolved')
    for request in value['requests']:
        wire(request, '/api/files.info', [['file', ID]])
    file = D.file(ID)
    file.update(title='Fresh hosted preview', mimetype='image/svg+xml', url_private=C.PREVIEW)
    file.pop('thumb_360')
    require(same(value.get('responses'), [{'ok': True, 'file': file}] * count)
            and value.get('settlements') == ([{'serial': 1, 'delivered': True}] if count else [])
            and value.get('held') is None and value.get('releases') == [],
            'Fresh navigation evidence substituted metadata read')


def source(proof, index):
    rows = [roots()[0], D.REPLY]
    retained = D.wire_proof(proof, responses()[:READS[index]], {1, 2, 4, 6}, rows, True,
                           visible=index != 9)
    require(retained.get('pendingFile') is (index == 9), 'Unexpected native departure marker')
    resolver(proof.get('resolver'), index)
    catalog = retained.get('catalog')
    require(isinstance(catalog, dict) and catalog.get('serial') == retained['serial'],
            'Canonical source witness absent')
    origins = [o for o in catalog.get('origins', []) if o.get('ts') == D.ROOT]
    require(origins == [{'ts': D.ROOT, 'firstId': ID, 'blockId': 'original-' + D.ROOT}],
            'Cold demand lost original first-file occurrence')
    canonical = [r for r in catalog.get('rows', []) if r.get('id') == ID]
    if index < 8:
        require(canonical == [], 'Absent canonical row fabricated before success')
    else:
        require(len(canonical) == 1 and same(canonical[0].get('source'), positive()['file'])
                and canonical[0].get('effective', {}).get('name') == NAME
                and canonical[0].get('effective', {}).get('permalink') == D.file(ID)['permalink']
                and canonical[0].get('current') is True
                and same(canonical[0].get('link'), {'name': NAME, 'url': D.file(ID)['permalink']}),
                'Positive canonical file facts substituted title or target result')
    return retained


def published_state(retained, index):
    value = retained.get('coldAcquisition')
    require(isinstance(value, dict) and value.get('cooling') is (index in (5, 6))
            and value.get('running') is (index == 2), 'Actual acquisition worker/debt differs')
    rows = value.get('rows')
    if index == 10:
        require(rows == [], 'Warm thread automatically reacquired existing canonical row')
        return
    phase = 'ready' if index >= 8 else PHASES[index]
    # Presentation derives loading and expired-debt guidance. The underlying
    # attempt retains its original failure notice; it is not a second outcome.
    notice = STORED_NOTICES[index]
    raw = positive() if index >= 8 else {'ok': True, 'file': False} if index == 4 else None
    require(rows == [{'id': ID, 'rootTs': D.ROOT, 'phase': phase, 'current': index != 3,
                      'retryable': index in (0, 1, 4, 7), 'notice': notice, 'receipt': raw}],
            'Actual owner-bound acquisition state/raw receipt differs')


def geometry(value, index, viewport, expected):
    require(value.get('ok') is True and value.get('viewport') == viewport
            and value.get('inScroll') is True, 'Cold acquisition target unavailable')
    visible(value.get('target'), viewport, None if index == 9 else value.get('scroll'))
    kind = 'viewer-title' if index == 9 else 'editor-line' if index in (1, 10) else 'link' if index == 8 else 'metadata-status'
    require(value.get('targetKind') == kind, 'Capture targeted another element')
    if index == 9:
        viewer = value.get('viewer')
        require(isinstance(viewer, dict) and viewer.get('title') == 'Fresh hosted preview'
                and viewer.get('ready') is True and value.get('native') is None,
                'Explicit click did not open the fresh native viewer')
        visible(viewer.get('close'), viewport)
        return
    require(value.get('viewer') is None and value.get('images') == [],
            'Cold display opened a target or registered discarded images')
    native = value.get('native')
    require(isinstance(native, dict) and same(native.get('blocks'), expected)
            and native.get('composing') is False and native.get('running') is True
            and isinstance(native.get('mount'), str) and bool(native['mount'])
            and isinstance(native.get('scope'), str) and bool(native['scope']),
            'Original native rich draft changed')
    if index:
        require(native.get('sameBlocks') is True, 'Native draft differs from original remembered edit')
    if index in (1, 2, 3):
        require(native.get('sameNode') is True and native.get('sameMount') is True,
                'Metadata-only work replaced original editor')
    if index in (6, 7, 8, 10):
        require(native.get('sameNode') is False and native.get('sameMount') is False,
                'Reopen borrowed old native holder')
    if PHASES[index]:
        require(value.get('status') == {'text': NOTICES[index], 'phase': PHASES[index]}
                and value.get('link') is None and value.get('body') == roots()[0]['text'],
                'Unavailable/loading status or original rich fallback differs')
        if index in (0, 1, 4, 5, 6, 7):
            retry = value.get('retry')
            require(isinstance(retry, dict) and retry.get('disabled') is (index in (5, 6))
                    and retry.get('text') == 'Retry file details', 'Actual retry missing or disabled')
            style = retry.get('style')
            disabled = index in (5, 6)
            require(isinstance(style, dict) and type(style.get('opacity')) in (int, float)
                    and style['opacity'] == (.45 if disabled else 1)
                    and style.get('cursor') == ('default' if disabled else 'pointer'),
                    'Retry enabled/disabled appearance differs')
            # The draft stage intentionally centers the edited line, not the status toolbar.
            if index != 1:
                visible(retry, viewport)
        else:
            require(value.get('retry') is None, 'Loading/retired read offers competing retry')
    else:
        require(value.get('status') is None and value.get('retry') is None
                and value.get('body') == NAME
                and value.get('link') == {'id': ID, 'name': NAME,
                    'url': D.file(ID)['permalink'], 'root': D.ROOT},
                'Canonical metadata did not replace fallback with exact source name')


def validate(receipt):
    require(type(receipt.get('rateElapsedMs')) is int and receipt['rateElapsedMs'] >= 15000,
            'Retry became available before actual 15-second cooldown')
    require(receipt.get('rateRetained') is True and receipt.get('explicitRetry') is True,
            'Cooldown or explicit retry evidence absent')
    stages = receipt.get('stages')
    require(isinstance(stages, list) and len(stages) == len(STAGES), 'Cold stage inventory differs')
    viewport = receipt.get('layout', {}).get('viewport')
    saved = None
    for i, (stage, name) in enumerate(zip(stages, STAGES)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int
                and stage['ordinal'] == i and all(stage.get(k) is True
                    for k in ('ok', 'noSession', 'noTransport')), 'Stage order or isolation differs')
        retained = source(stage['proof'], i)
        acquisition(stage.get('acquisition'), i)
        require(same(stage['proof'].get('cold'), stage['acquisition']),
                'Original source proof and standalone acquisition ledger differ')
        published_state(retained, i)
        expected = C.draft(stage.get('reply'), 0 if i == 0 else 1)
        runtime, item = stage['reply']['runtime'], stage['reply']['journal']['drafts'][0]
        text = ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.'
                + (' Canonical tail \u4e2d\U0001f98a' if i else ''))
        require(runtime.get('error') is None and runtime.get('text') == item.get('text') == text
                and runtime.get('capture') == item.get('capture')
                and isinstance(item.get('id'), str) and item['id']
                and isinstance(item.get('capture'), str) and item['capture']
                and type(item.get('revision')) is int and item['revision'] > 0,
                'Original independent journal capture/text/revision differs')
        if i == 1:
            saved = copy.deepcopy(item)
            initial = stages[0]['reply']['journal']['drafts'][0]
            require(item['id'] == initial['id'] and item['revision'] > initial['revision'],
                    'Actual edited draft did not advance original journal')
        if i >= 2:
            require(same(item, saved), 'Acquisition or navigation changed saved rich draft')
        geometry(stage['layout'], i, viewport, expected)
    for start, end in ((1, 2), (2, 3), (4, 5), (6, 7), (7, 8)):
        a, b = stages[start], stages[end]
        require(a['proof']['retained']['serial'] == b['proof']['retained']['serial'],
                'Metadata-only completion retired comment read')
        require(all(a['layout']['native'].get(k) == b['layout']['native'].get(k)
                    for k in ('mount', 'scope', 'generation')), 'Metadata-only work remounted draft')
        require(same(a['reply']['journal'], b['reply']['journal']),
                'Metadata-only work changed independent draft journal')
        require(all(a['reply']['runtime'].get(k) == b['reply']['runtime'].get(k)
                    for k in ('serial', 'capture', 'text', 'blocks')),
                'Metadata-only work changed reply capture authority')
    require(all(stages[6]['layout']['retry']['rect'][key] == stages[7]['layout']['retry']['rect'][key]
                for key in ('width', 'height')), 'Cooldown expiry resized the Retry control')
    for key in ('layout', 'proof', 'acquisition'):
        require(same(receipt.get(key), stages[-1][key]), 'Terminal substituted captured ' + key)
