"""Independent target-read retirement oracle; no native or network execution."""
import copy
import canvas_canonical_native_destinations as D
from canvas_special_roots import require, same

PREFIX = 'canvas-canonical-native-retirement'
FIXTURES = D.FIXTURES
STAGES = ('thread', 'draft', 'target-pending', 'source-reopened',
          'late-refused', 'destination', 'reopened')
READS = (2, 2, 2, 4, 4, 4, 6)
RESOLVES = (0, 0, 1, 1, 1, 2, 2)
BATCHES = (0, 0, 1, 1, 1, 2, 2)


def responses(id):
    return [D.response(id, thread=i % 2 == 1) for i in range(6)]


def resolver(value, id, index):
    count = RESOLVES[index]
    require(isinstance(value, dict) and len(value.get('requests', [])) == count,
            'Fresh resolver dispatch inventory differs')
    for request in value['requests']:
        D.wire(request, '/api/files.info', [['file', id]])
    require(same(value.get('responses'), [{'ok': True, 'file': D.target_file(id)}] * count)
            and value.get('settlements') == [{'serial': n, 'delivered': True}
                                             for n in range(1, count + 1)]
            and value.get('held') is None and value.get('releases') == [],
            'Retirement held the resolver or substituted its response')


def targets(value, id, index):
    require(isinstance(value, dict) and set(value) == {'canvas', 'list'},
            'Target reader inventory missing')
    count = BATCHES[index]
    active = 'canvas' if id == D.CANVAS else 'list'
    for kind in ('canvas', 'list'):
        v = value[kind]
        require(v.get('networkCalls') == 0 and v.get('httpCapability') is False,
                'Target reader gained HTTP capability')
        n = count if kind == active else 0
        released = n > 0 and index >= 4
        require(v.get('held') == (1 if n and not released else None)
                and v.get('releases') == ([{'serial': 1, 'delivered': kind == 'list'}]
                                         if released else []), 'Target release/custody differs')
        if kind == 'canvas':
            expected = []
            for batch in range(1, n + 1):
                for raw in D.canvas_requests():
                    row = copy.deepcopy(raw)
                    row.update(pair=batch, serial=len(expected) + 1)
                    expected.append(row)
            settlements = ([{'serial': 1, 'status': 'cancelled'}] if n and index >= 3 else [])
            if n == 2:
                settlements.append({'serial': 2, 'status': 'ready'})
            require(same(v.get('requests'), expected)
                    and v.get('pairs') == [{'serial': j, 'held': j == 1} for j in range(1, n + 1)]
                    and v.get('settlements') == settlements,
                    'Canvas original pair/cancellation/ready evidence differs')
        else:
            requests = v.get('requests')
            require(isinstance(requests, list) and len(requests) == n,
                    'List batch count differs')
            for serial, batch in enumerate(requests, 1):
                require(batch.get('serial') == serial and batch.get('id') == D.LIST
                        and all(type(batch.get(k)) is int and batch[k] >= 0
                                for k in ('generation', 'read', 'revision')),
                        'List original read identity missing')
                rows = batch.get('requests')
                require(isinstance(rows, list) and len(rows) == 5, 'List factory count differs')
                platform = dict(rows[-1].get('fields', [])).get('ssb_platform')
                require(platform in ('windows', 'linux', 'darwin'), 'List platform invalid')
                for actual, (path, fields) in zip(rows, D.list_wires(platform)):
                    D.wire(actual, path, fields)
            if n == 2:
                require(requests[1]['generation'] > requests[0]['generation'],
                        'Fresh List inherited retired generation')
            settled = ([{'serial': 1, 'delivered': True, 'current': False,
                         'parsed': False, 'refreshAgain': False}] if released else [])
            if n == 2:
                settled.append({'serial': 2, 'delivered': True, 'current': True,
                                'parsed': True, 'refreshAgain': False})
            require(same(v.get('responses'), [{'serial': j, 'responses': D.list_responses()}
                                             for j in range(1, n + 1)])
                    and v.get('settlements') == settled,
                    'List late result parsed/adopted or raw response changed')


def pending_geometry(value, id, viewport):
    require(value.get('ok') is True and value.get('pending') is True
            and value.get('viewport') == viewport and value.get('id') == id
            and value.get('kind') == ('canvas' if id == D.CANVAS else 'list')
            and value.get('native') is None and value.get('contentPresent') is False,
            'Pending target is ready, wrong, or retains original editor')
    require(isinstance(value.get('generation'), str) and value['generation'],
            'Pending target generation absent')
    for key in ('title', 'loading', 'close'):
        # Loading labels are outside the body scroller; a cold List has no table/scroller yet.
        D.visible(value.get(key), viewport)
    require(value['title'].get('text') == D.TITLE[id]
            and value['close'].get('disabled') is False
            and value.get('busy') is True
            and value.get('busySource') == ('scroll-aria-busy' if id == D.CANVAS else 'initial-list-loading-status')
            and value.get('scrollMounted') is (id == D.CANVAS)
            and (isinstance(value.get('scroll'), dict) if id == D.CANVAS else value.get('scroll') is None)
            and value.get('skeletonPresent') is (id == D.CANVAS)
            and value['loading'].get('text') == ('Loading\u2026' if id == D.CANVAS else 'Loading list\u2026'),
            'Pending title/Close differs')


def source_geometry(value, index, viewport, expected):
    require(value.get('ok') is True and value.get('viewport') == viewport
            and value.get('inScroll') is True and value.get('viewer') is None
            and value.get('targetKind') == 'editor-line', 'Original source editor not visible')
    D.visible(value.get('target'), viewport, value.get('scroll'))
    native = value.get('native')
    require(isinstance(native, dict) and same(native.get('blocks'), expected)
            and native.get('composing') is False and native.get('running') is True,
            'Original native blocks/capture changed')
    if index == 1:
        require(all(native.get(k) is True for k in ('sameNode', 'sameMount', 'sameBlocks')),
                'Original predeparture native holder changed')
    if index in (3, 4, 6):
        require(native.get('sameNode') is False and native.get('sameMount') is False
                and native.get('sameBlocks') is True, 'Reopened editor is not fresh with original blocks')


def draft(reply, index):
    expected = D.C.draft(reply, 0 if index == 0 else 1)
    runtime, item = reply['runtime'], reply['journal']['drafts'][0]
    text = ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.'
            + (' Canonical tail \u4e2d\U0001f98a' if index else ''))
    require(runtime.get('error') is None and runtime.get('text') == text and item.get('text') == text
            and isinstance(item.get('id'), str) and item['id']
            and isinstance(item.get('capture'), str) and item['capture']
            and runtime.get('capture') == item['capture']
            and type(item.get('revision')) is int and item['revision'] > 0,
            'Original journal text/capture/revision differs')
    require(runtime.get('nativePresent') is (index not in (2, 5)),
            'Original native holder custody differs')
    return expected, item



def route(value, id, index, layout, original):
    require(isinstance(value, dict) and value.get('account') == '147b5480-9f1a-4443-89da-836472cbb209'
            and value.get('team') == 'T1' and value.get('user') == 'U0'
            and type(value.get('epoch')) is int and value['epoch'] >= 0
            and type(value.get('shortcutGeneration')) is int and value['shortcutGeneration'] >= 0,
            'Original account/team/user route missing')
    require(all(value.get(k) == original.get(k) for k in ('account', 'epoch', 'team', 'user')),
            'Destination crossed original owner')
    target = index in (2, 5)
    kind = 'canvas' if id == D.CANVAS else 'list'
    require(value.get('overlay') == (kind if target else 'canvas'), 'Unexpected active overlay')
    if target and id == D.LIST:
        current = value.get('list')
        require(isinstance(current, dict) and current.get('id') == id
                and current.get('loading') is (index == 2)
                and type(current.get('generation')) is int
                and str(current['generation']) == layout.get('generation'),
                'List pending/current visit differs from DOM')
    else:
        require(value.get('canvas') == {'id': id if target else 'Ffixture',
                                       'title': D.TITLE[id] if target else 'Project handbook'}
                and value.get('list') is None, 'Original Canvas identity replaced')
        if target:
            current = value.get('canvasRead')
            require(isinstance(current, dict) and current.get('loading') is (index == 2)
                    and current.get('error') is None
                    and current.get('file') == (None if index == 2 else id)
                    and type(current.get('visit')) is int and type(current.get('read')) is int
                    and str(value['shortcutGeneration']) == layout.get('generation'),
                    'Canvas pending/current reader differs from DOM')

def validate(receipt):
    stages, id = receipt.get('stages'), receipt.get('destinationId')
    require(id in (D.CANVAS, D.LIST) and isinstance(stages, list) and len(stages) == len(STAGES),
            'Retirement stage inventory differs')
    viewport = receipt.get('layout', {}).get('viewport')
    saved = None
    for i, (stage, name, count) in enumerate(zip(stages, STAGES, READS)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int
                and stage['ordinal'] == i and all(stage.get(k) is True
                for k in ('ok', 'noSession', 'noTransport')), 'Stage order/isolation differs')
        proof = stage['proof']
        rows = [D.roots(id)[0], D.D.REPLY]
        retained = D.D.wire_proof(proof, responses(id)[:count], {1, 3, 5}, rows,
                                  True, visible=i not in (2, 5))
        resolver(proof.get('resolver'), id, i)
        require(retained.get('pendingFile') is (i in (2, 5)), 'Original departure marker differs')
        catalog = retained.get('catalog')
        require(isinstance(catalog, dict) and catalog.get('serial') == retained['serial'],
                'Canonical display provenance absent')
        origin = next((r for r in catalog.get('origins', []) if r.get('ts') == D.D.ROOT), None)
        require(origin == {'ts': D.D.ROOT, 'firstId': id, 'blockId': 'original-' + D.D.ROOT},
                'Original first-file identity changed')
        row = next((r for r in catalog.get('rows', []) if r.get('id') == id), None)
        raw = rows[0]['files'][-1]
        require(isinstance(row, dict) and same(row.get('source'), raw)
                and row.get('effective', {}).get('name') == raw['name']
                and row.get('effective', {}).get('permalink') == raw['permalink'],
                'Original source replaced by target reader facts')
        targets(stage.get('targetReads'), id, i)
        route(stage.get('route'), id, i, stage['layout'], stages[0].get('route', {}))
        if i in (3, 6):
            require(stage['route']['shortcutGeneration'] > stages[i - 1]['route']['shortcutGeneration'],
                    'Source reopen did not advance original visit')
        if i >= 3:
            kind = 'canvas' if id == D.CANVAS else 'list'
            first = stages[2]['targetReads'][kind]
            current = stage['targetReads'][kind]
            require(same(current['requests'][:len(first['requests'])], first['requests']),
                    'Original held request identity changed later')
            if kind == 'list':
                require(same(current['responses'][:1], first['responses']),
                        'Original held List response changed later')
        if i in (2, 5) and id == D.LIST:
            require(stage['route']['list']['generation'] == stage['targetReads']['list']['requests'][-1]['generation'],
                    'List DOM route does not own selected read batch')
        expected, item = draft(stage.get('reply'), i)
        if i == 1:
            saved = copy.deepcopy(item)
            initial = stages[0]['reply']['journal']['drafts'][0]
            require(item['id'] == initial['id'] and item['revision'] > initial['revision'],
                    'Native edit replaced scope or did not advance revision')
        if i >= 2:
            require(same(item, saved), 'Durable draft changed after departure')
        if i == 2:
            pending_geometry(stage['layout'], id, viewport)
        elif i == 5:
            D.destination_geometry(stage['layout'], id, viewport)
        else:
            source_geometry(stage['layout'], i, viewport, expected)
    departure = receipt.get('departure')
    require(isinstance(departure, dict), 'Original predeparture witness missing')
    D.C.draft(departure.get('reply'), 1)
    require(same(departure['reply']['journal']['drafts'][0], saved), 'Predeparture disk changed')
    require(same(departure.get('route'), stages[1].get('route')), 'Departure owner changed')
    native = departure.get('native', {}).get('native', {})
    require(same(native.get('blocks'), D.C.edited()) and all(native.get(k) is True
            for k in ('sameNode', 'sameMount', 'sameBlocks', 'running')),
            'Predeparture witness borrowed a replacement holder')
    require(stages[5]['layout']['generation'] != stages[2]['layout']['generation'],
            'Fresh target reused retired DOM generation')
    late = receipt.get('late')
    require(isinstance(late, dict) and set(late) == {'before', 'after', 'release'}
            and same(late.get('before'), late.get('after')),
            'Late target result changed original source/DOM/disk')
    require(late['release'] == {'serial': 1, 'delivered': id == D.LIST},
            'Cancellation versus stale-delivery distinction lost')
    for key in ('route', 'layout', 'proof', 'reply'):
        require(same(late['before'].get(key), stages[3].get(key))
                and same(late['after'].get(key), stages[4].get(key)),
                'Release equality witness not bound to captured original source')
    require(same(receipt.get('proof'), stages[-1]['proof'])
            and same(receipt.get('layout'), stages[-1]['layout']),
            'Terminal source/layout substitution')
