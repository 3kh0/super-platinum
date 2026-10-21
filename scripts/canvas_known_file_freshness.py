"""Independent event-owned known file refresh and durable reply custody oracle."""
import copy

import canvas_canonical_file_roots as D
import canvas_canonical_file_custody as C
from canvas_canonical_native_destinations import visible
from canvas_special_roots import require, same

PREFIX = 'canvas-known-file-freshness'
FIXTURES = ('canvas-canonical-file-custody', 'canvas-canonical-file-custody-narrow')
STAGES = {
    'success': ('known', 'draft', 'loading', 'renamed', 'offline', 'malformed',
                'rejected', 'recovered'),
    'retirement': ('known', 'draft', 'loading', 'superseded', 'late-refused', 'recovered'),
}
COUNTS = {'success': (0, 0, 1, 1, 2, 3, 4, 5), 'retirement': (0, 0, 1, 2, 2, 2)}
MODES = {'success': ('held', 'offline', 'malformed', 'error', 'success'),
         'retirement': ('held', 'held')}
ID = D.IDS[0]
REASON = 'files-store.ConditionalFetchManager.fetch'
ARGS = {'file': ID, 'page': '1', 'count': '500', 'truncate': 'true', '_x_reason': REASON}
LOADING = 'Loading file details...'
OFFLINE = 'File details are unavailable while offline. Retry when connected.'
FAILED = 'File details could not be loaded. Retry when ready.'


def positive(version):
    require(type(version) is int and version in (1, 2), 'Unknown file source version')
    value = D.file(ID)
    value.update(name=f'Fresh canonical {version} & \u4e2d\U0001f98a',
                 title='Different freshness title must not label root',
                 updated=200 + version,
                 permalink=f'https://example.test/canonical/fresh-{version}')
    return {'ok': True, 'file': value}


def response(mode, version):
    if mode == 'offline':
        return None
    if mode == 'malformed':
        return {'ok': True, 'file': False}
    if mode == 'error':
        return {'ok': False, 'error': 'file_not_found'}
    require(mode in ('held', 'success'), 'Unknown original request mode')
    return positive(version)


def release(serial, delivered):
    return {'serial': serial, 'id': ID, 'delivered': delivered,
            'cancelledBeforeReply': not delivered}


def acquisition(value, case, index):
    count = COUNTS[case][index]
    require(isinstance(value, dict) and type(value.get('networkCalls')) is int
            and value['networkCalls'] == 0
            and value.get('httpCapability') is False, 'Live metadata capability')
    requests, raw = [], []
    for n, mode in enumerate(MODES[case][:count], 1):
        version = 1 if n == 1 else 2
        requests.append(dict(serial=n, id=ID, method='files.info', path='/api/files.info',
                             args=ARGS, mode=mode, version=version, reason=REASON))
        raw.append(dict(serial=n, id=ID, disposition=mode, version=version,
                        raw=response(mode, version)))
    require(same(value.get('requests'), requests) and same(value.get('responses'), raw),
            'Original known request/envelope/count changed')
    if case == 'success':
        completed = 0 if index <= 2 else count
        settlements = [dict(serial=n, cancelled=False) for n in range(1, completed + 1)]
        releases = [release(1, True)] if index >= 3 else []
        held = [dict(serial=1, id=ID, receiverClosed=False)] if index == 2 else []
    else:
        completed = 0 if index <= 2 else 1 if index <= 4 else 2
        settlements = [dict(serial=n, cancelled=n == 1) for n in range(1, completed + 1)]
        releases = ([release(1, False)] if index >= 4 else []) + ([release(2, True)] if index == 5 else [])
        held = ([dict(serial=1, id=ID, receiverClosed=index == 3)] if index in (2, 3) else [])
        if index in (3, 4):
            held.append(dict(serial=2, id=ID, receiverClosed=False))
    require(same(value.get('settlements'), settlements) and same(value.get('releases'), releases)
            and same(value.get('held'), held) and type(value.get('completed')) is int
            and value['completed'] == completed and type(value.get('cancelled')) is int
            and value['cancelled'] == int(case == 'retirement' and index >= 3),
            'Original held carrier/settlement/release changed')


def state_at(case, index):
    name = STAGES[case][index]
    version = 2 if name == 'recovered' else 1 if case == 'success' and index >= 3 else 0
    current = name in ('known', 'draft', 'renamed', 'recovered')
    phase = None if index < 2 else 'ready' if current else 'unavailable' if name in (
        'offline', 'malformed', 'rejected') else 'loading'
    notice = OFFLINE if name == 'offline' else FAILED if name in ('malformed', 'rejected') else None
    raw = positive(version) if phase == 'ready' else response('malformed', 2) if name == 'malformed' else response('error', 2) if name == 'rejected' else None
    return name, version, current, phase, notice, raw


def stable(observation):
    retained = observation['proof']['retained']
    return {**{k: retained.get(k) for k in ('serial', 'query', 'rows', 'root', 'ownerCurrent',
              'loading', 'error', 'pendingFile', 'navigationError')},
            'native': observation['layout']['native'],
            'sourceIdentity': observation['layout']['sourceIdentity'], 'reply': observation['reply']}


def observation(stage):
    return {k: stage[k] for k in ('layout', 'proof', 'acquisition', 'reply')}


def source(stage, case, index, event_revision):
    name, version, current, phase, notice, raw = state_at(case, index)
    proof = stage['proof']
    retained = D.wire_proof(proof, [D.envelope(), D.envelope(thread=True)], {1},
                            [D.roots()[0], D.REPLY], True)
    require(retained.get('pendingFile') is False and retained.get('navigationError') is None,
            'Metadata refresh started navigation')
    require(proof.get('resolver') == dict(requests=[], responses=[], settlements=[], releases=[], held=None),
            'Known metadata acquired navigation authority')
    require(same(proof.get('known'), stage['acquisition']), 'Standalone/source ledger differs')
    catalog = retained.get('catalog')
    require(isinstance(catalog, dict) and catalog.get('serial') == retained['serial'],
            'Original canonical source owner missing')
    origins = [o for o in catalog.get('origins', []) if o.get('ts') == D.ROOT]
    require(origins == [dict(ts=D.ROOT, firstId=ID, blockId='original-' + D.ROOT)],
            'Original first-ID/control occurrence changed')
    rows = [r for r in catalog.get('rows', []) if r.get('id') == ID]
    expected = positive(version)['file'] if version else D.file(ID)
    require(len(rows) == 1 and same(rows[0].get('source'), expected)
            and rows[0].get('current') is current
            and rows[0].get('effective', {}).get('name') == expected['name']
            and rows[0].get('effective', {}).get('permalink') == expected['permalink']
            and same(rows[0].get('link'), dict(name=expected['name'], url=expected['permalink'])),
            'Canonical raw preimage/currentness/label changed')
    state = retained.get('knownAcquisition')
    require(isinstance(state, dict) and state.get('cooling') is False
            and state.get('running') is (phase == 'loading'), 'Known worker/debt differs')
    expected_rows = [] if index < 2 else [dict(id=ID, rootTs=D.ROOT, phase=phase,
        current=True, retryable=phase == 'unavailable', notice=notice, receipt=raw,
        demand='known-event', eventRevision=event_revision)]
    require(same(state.get('rows'), expected_rows), 'Original event-bound attempt differs')
    return expected


def geometry(value, case, index, viewport, expected_file, blocks):
    name, _, current, phase, notice, _ = state_at(case, index)
    require(value.get('ok') is True and value.get('viewport') == viewport
            and value.get('inScroll') is True, 'Known file target unavailable')
    visible(value.get('target'), viewport, value.get('scroll'))
    kind = 'editor-line' if name == 'draft' else 'link' if current else 'metadata-status'
    require(value.get('targetKind') == kind and value.get('viewer') is None
            and value.get('images') == [], 'Metadata opened target/media or capture changed')
    require(value.get('sourceIdentity') == dict(sameRow=True, sameEditor=True,
            sameControls=True, root=D.ROOT), 'Original row/editor/controls replaced')
    native = value.get('native')
    require(isinstance(native, dict) and same(native.get('blocks'), blocks)
            and native.get('composing') is False and native.get('running') is True
            and all(native.get(k) is not None for k in ('mount', 'scope', 'generation')),
            'Original rich editor missing or changed')
    if index:
        require(all(native.get(k) is True for k in ('sameNode', 'sameMount', 'sameBlocks')),
                'Metadata work remounted or changed remembered rich editor')
    if current:
        require(value.get('status') is None and value.get('retry') is None
                and value.get('body') == expected_file['name']
                and value.get('link') == dict(id=ID, root=D.ROOT,
                    name=expected_file['name'], url=expected_file['permalink']),
                'Exact canonical source name/permalink not rendered')
    else:
        require(value.get('link') is None and value.get('body') == D.roots()[0]['text']
                and value.get('status') == dict(phase=phase, text=LOADING if phase == 'loading' else notice),
                'Invalidated link or truthful fallback/guidance differs')
        retry = value.get('retry')
        if phase == 'unavailable':
            require(isinstance(retry, dict) and retry.get('disabled') is False
                    and retry.get('text') == 'Retry file details'
                    and retry.get('style') == dict(opacity=1, cursor='pointer'),
                    'Explicit retry unavailable or visually disabled')
            visible(retry, viewport)
        else:
            require(retry is None, 'Held work exposes competing retry')


def exercises(stage, case, index, previous_event):
    name = STAGES[case][index]
    value = stage.get('exercise')
    require(isinstance(value, dict), 'Original action proof absent')
    if name == 'draft':
        require(value == {}, 'Draft stage gained unrelated actions')
        return previous_event
    q = value.get('quiet')
    require(isinstance(q, dict) and type(q.get('observedMs')) is int and q['observedMs'] >= 500
            and same(q.get('before'), q.get('after')) and same(q['after'], observation(stage)),
            'Bounded quiet/source/native evidence changed or substituted')
    keys = {'quiet'}
    if name in ('loading', 'offline', 'superseded'):
        keys.add('event');event = value.get('event')
        require(isinstance(event, dict) and event.get('id') == ID and event.get('team') == 'T1'
                and type(event.get('before')) is int and type(event.get('after')) is int
                and event['before'] >= 0 and event['after'] > event['before']
                and (previous_event is None or event['before'] == previous_event),
                'Actual original file invalidation absent/foreign/reordered')
        previous_event = event['after']
    if name in ('malformed', 'rejected') or (name == 'recovered' and case == 'success'):
        keys.add('retry')
        require(value.get('retry') == dict(ok=True, before=dict(root=D.ROOT,
                text='Retry file details', disabled=False), sameNode=True, connected=True),
                'Retry did not activate the original enabled control')
    if name in ('renamed', 'late-refused') or (name == 'recovered' and case == 'retirement'):
        keys.add('release')
        require(value.get('release') == release(2 if name == 'recovered' else 1,
                                               name != 'late-refused'),
                'Original held result was not actually released')
    if name == 'late-refused':
        keys.update(('before', 'after'));a, b = value.get('before'), value.get('after')
        require(isinstance(a, dict) and isinstance(b, dict) and same(b, observation(stage))
                and same(stable(a), stable(b))
                and same(a['layout'], b['layout'])
                and same({k: v for k, v in a['proof'].items() if k != 'known'},
                         {k: v for k, v in b['proof'].items() if k != 'known'}),
                'Late response changed newer source or original custody')
        acquisition(a['acquisition'], case, index - 1)
        require(same(a['proof'].get('known'), a['acquisition']), 'Late original ledger substituted')
    require(set(value) == keys, 'Missing or unrecognized original action proof')
    return previous_event


def validate(receipt):
    case = receipt.get('case')
    require(case in STAGES, 'Unrecognized known freshness case')
    require(all(receipt.get(k) is True for k in ('ok', 'noSession', 'noTransport',
            'draftPreserved', 'originalReadRetained', 'noNavigation', 'noAutomaticLoop')),
            'Incomplete known freshness terminal')
    require(all(type(receipt.get(k)) is int and receipt[k] == n for k, n in
            dict(requestCount=2, acquisitionCount=COUNTS[case][-1], resolverCount=0).items()),
            'Terminal request counts differ')
    stages = receipt.get('stages');require(isinstance(stages, list) and len(stages) == len(STAGES[case]),
                                          'Known stage inventory differs')
    viewport = receipt.get('layout', {}).get('viewport')
    baseline = None;event = None;initial = None
    for i, (stage, name) in enumerate(zip(stages, STAGES[case])):
        require(stage.get('stage') == name and stage.get('case') == case
                and type(stage.get('ordinal')) is int and stage['ordinal'] == i
                and all(stage.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Stage case/order/isolation differs')
        event = exercises(stage, case, i, event)
        acquisition(stage.get('acquisition'), case, i)
        expected = source(stage, case, i, event)
        blocks = C.draft(stage.get('reply'), int(i > 0))
        reply = stage['reply'];runtime = reply['runtime'];draft = reply['journal']['drafts'][0]
        text = 'Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.' + (
            ' Canonical tail \u4e2d\U0001f98a' if i else '')
        require(runtime.get('error') is None and runtime.get('text') == draft.get('text') == text
                and runtime.get('capture') == draft.get('capture')
                and isinstance(draft.get('id'), str) and bool(draft['id'])
                and isinstance(draft.get('capture'), str) and bool(draft['capture'])
                and type(draft.get('revision')) is int and draft['revision'] > 0,
                'Exact durable rich draft/text/capture changed')
        geometry(stage['layout'], case, i, viewport, expected, blocks)
        if i == 0:
            initial = copy.deepcopy(draft)
        elif i == 1:
            require(draft['id'] == initial['id'] and draft['revision'] > initial['revision'],
                    'Actual native edit did not advance original journal')
            baseline = stable(stage)
        else:
            require(same(stable(stage), baseline), 'Metadata work changed original reply/SQLite/comment custody')
    require(all(same(receipt.get(k), stages[-1][k]) for k in ('layout', 'proof', 'acquisition', 'reply')),
            'Terminal replaced captured source or custody')
