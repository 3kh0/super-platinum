"""Independent real-SQLite draft refusal/recovery and native destination oracle."""
import copy
import re
import uuid
import canvas_canonical_native_destinations as D
from canvas_special_roots import require, same

PREFIX = 'canvas-canonical-native-storage'
FIXTURES = D.FIXTURES
STAGES = ('thread', 'draft', 'save-refused', 'save-recovered', 'destination', 'reopened')
READS = (2, 2, 2, 2, 2, 4)
RESOLVES = (0, 0, 1, 1, 2, 2)
BATCHES = (0, 0, 0, 0, 1, 1)
ACCOUNT = '147b5480-9f1a-4443-89da-836472cbb209'
SCOPE = {'workspace': 'T1', 'user': 'U0', 'file': 'Ffixture'}
ERROR = 'The reply is retained here but could not be saved locally. Retry local save before leaving the app.'
TAIL = ' Locked tail \u4e2d\U0001f98a'


def blocks(index):
    value = D.C.blocks() if index == 0 else D.C.edited()
    if index >= 2:
        value[1]['elements'][0]['elements'][0]['text'] += TAIL
    return value


def text(index):
    return ('Keep exact \u4e2d\U0001f98a & draft link e\u0301.\nOpaque attributes retained.'
            + (' Canonical tail \u4e2d\U0001f98a' if index else '') + (TAIL if index >= 2 else ''))


def canonical_uuid(value):
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except (ValueError, AttributeError):
        return False


def resolver(value, id, index):
    n = RESOLVES[index]
    require(isinstance(value, dict) and len(value.get('requests', [])) == n,
            'Storage resolver dispatch inventory differs')
    for request in value['requests']:
        D.wire(request, '/api/files.info', [['file', id]])
    require(same(value.get('responses'), [{'ok': True, 'file': D.target_file(id)}] * n)
            and value.get('settlements') == [{'serial': i, 'delivered': True} for i in range(1, n + 1)]
            and value.get('held') is None and value.get('releases') == [],
            'Fresh resolver receipt/settlement substituted')


def journal(value, index):
    require(isinstance(value, dict) and set(value) == {'drafts', 'attempt', 'history'}
            and value['attempt'] is None and value['history'] == []
            and isinstance(value['drafts'], list) and len(value['drafts']) == 1,
            'Original single draft/attempt/history custody differs')
    item = value['drafts'][0]
    require(set(item) == {'id', 'revision', 'capture', 'text', 'blocks'}
            and canonical_uuid(item['id']) and canonical_uuid(item['capture'])
            and type(item['revision']) is int and 0 < item['revision'] <= 2**63 - 1
            and item['text'] == text(index) and same(item['blocks'], blocks(index)),
            'Exact rich journal draft identity/body differs')
    return item


def ack(value, sequence, fixture=None):
    action = 'hold' if sequence == 1 else 'release'
    fields = {'version', 'nonce', 'pid', 'fixture', 'sequence', 'action', 'ok', 'account',
              'scope', 'root', 'journal', 'revision', 'captureIds', 'contentSha256'}
    if sequence == 2:
        fields.add('unchanged')
    require(isinstance(value, dict) and set(value) == fields
            and type(value['version']) is int and value['version'] == 1
            and type(value['pid']) is int and value['pid'] > 0
            and isinstance(value['nonce'], str) and re.fullmatch('[0-9a-f]{32}', value['nonce'])
            and value['fixture'] in FIXTURES and (fixture is None or value['fixture'] == fixture)
            and type(value['sequence']) is int and value['sequence'] == sequence
            and value['action'] == action and value['ok'] is True
            and value['account'] == ACCOUNT and value['scope'] == SCOPE
            and value['root'] == D.D.ROOT, 'Storage acknowledgement owner/protocol differs')
    item = journal(value['journal'], 1)
    captures = value['captureIds']
    require(type(value['revision']) is int and item['revision'] <= value['revision'] <= 2**63 - 1
            and isinstance(captures, list) and 0 < len(captures) <= 128
            and all(canonical_uuid(c) for c in captures)
            and captures == sorted(set(captures)) and item['capture'] in captures
            and isinstance(value['contentSha256'], str)
            and re.fullmatch('[0-9a-f]{64}', value['contentSha256']),
            'Storage original clock/capture/hash proof differs')
    if sequence == 2:
        require(value['unchanged'] is True, 'Lock altered original SQL content')
    return item


def storage(value, index, saved_a, fixture=None):
    require(isinstance(value, dict) and set(value) == {'hold', 'release'},
            'Storage lock evidence absent')
    if index < 2:
        require(value == {'hold': None, 'release': None}, 'Lock issued before durable A')
        return
    hold = value['hold']
    require(same(ack(hold, 1, fixture), saved_a), 'Exclusive lock baseline is not durable A')
    if index == 2:
        require(value['release'] is None, 'Failure captured after lock release')
        return
    release = value['release']
    ack(release, 2, fixture)
    require(same({k: v for k, v in hold.items() if k not in ('sequence', 'action')},
                 {k: v for k, v in release.items() if k not in ('sequence', 'action', 'unchanged')}),
            'Release did not preserve original A SQL snapshot')


def runtime(value, index):
    require(isinstance(value, dict) and all(value.get(k) is False for k in ('loading', 'saving', 'sending'))
            and same(value.get('blocks'), blocks(index)) and value.get('text') == text(index)
            and canonical_uuid(value.get('capture'))
            and value.get('nativePresent') is (index != 4)
            and value.get('error') == (ERROR if index == 2 else None),
            'Original runtime rich draft/save state differs')


def alert_inventory(layout, expected):
    """Assert DOM ownership and layout visibility, independently of viewport scrolling."""
    entries = layout.get('panelAlerts')
    # The shared editor's hidden link-error element exists even without an error.
    inventory = [{'text': '', 'visible': False, 'owner': 'composer'}, *expected]
    require(isinstance(entries, list)
            and all(isinstance(entry, dict) and type(entry.get('visible')) is bool
                    for entry in entries)
            and entries == inventory, 'Panel/composer alert ownership or visibility differs')
    require(layout.get('alerts') == [entry['text'] for entry in inventory],
            'Raw alert lifecycle changed while suppressing duplicate guidance')
    controls = layout.get('controls', {}).get('alerts')
    require(isinstance(controls, list)
            and all(isinstance(entry, dict) for entry in controls)
            and [entry.get('text') for entry in controls]
            == [entry['text'] for entry in inventory if entry['visible']],
            'Visible refusal guidance duplicated, hidden, or substituted')


def source_geometry(value, index, viewport):
    alert_inventory(value, [{'text': ERROR, 'visible': True, 'owner': 'composer'}]
                    if index == 2 else [])
    require(value.get('ok') is True and value.get('viewport') == viewport
            and value.get('inScroll') is True and value.get('viewer') is None
            and value.get('targetKind') == ('retry' if index == 2 else 'editor-line'),
            'Original source/draft replaced on local failure')
    D.visible(value.get('target'), viewport, value.get('scroll'))
    native = value.get('native')
    require(isinstance(native, dict) and same(native.get('blocks'), blocks(index))
            and native.get('running') is True and native.get('composing') is False,
            'Exact original rich native B lost')
    if index == 1:
        require(native.get('sameNode') is True and native.get('sameMount') is True,
                'Original A editor replaced before lock')
    if index in (2, 3, 5):
        require(native.get('sameNode') is False and native.get('sameMount') is False
                and native.get('sameBlocks') is False,
                'B remount/reopened identity or A comparison differs')
    controls = value.get('controls')
    require(isinstance(controls, dict) and controls.get('ok') is True
            and isinstance(controls.get('send'), dict)
            and controls['send'].get('text') == 'Send reply'
            and controls['send'].get('disabled') is (index == 2),
            'Send state did not reflect durable-save failure')
    if index == 2:
        retry = controls.get('retry')
        require(isinstance(retry, dict) and retry.get('text') == 'Retry local save'
                and retry.get('disabled') is False, 'Actual enabled Retry unavailable')
        D.visible(retry, viewport, value.get('scroll'))
        alerts = controls.get('alerts')
        require(isinstance(alerts, list), 'Save refusal guidance absent')
        errors = [a for a in alerts if a.get('text') == ERROR]
        require(len(errors) == 1 and ERROR in value.get('alerts', []),
                'Exact save-error guidance missing')
        D.visible(errors[0], viewport, value.get('scroll'))
    else:
        require(controls.get('retry') is None and controls.get('alerts') == [],
                'Local save failure not recovered')


def validate(receipt, fixture=None):
    stages, id = receipt.get('stages'), receipt.get('destinationId')
    require(id in (D.CANVAS, D.LIST) and isinstance(stages, list) and len(stages) == len(STAGES),
            'Storage stage/target inventory differs')
    viewport = receipt.get('layout', {}).get('viewport')
    saved_a = saved_b = None
    for i, (stage, name, count) in enumerate(zip(stages, STAGES, READS)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int and stage['ordinal'] == i
                and all(stage.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Storage stage order/isolation differs')
        proof = stage['proof']
        rows = [D.roots(id)[0], D.D.REPLY]
        responses = [D.response(id, thread=j % 2 == 1) for j in range(4)]
        retained = D.D.wire_proof(proof, responses[:count], {1, 3}, rows, True, visible=i != 4)
        resolver(proof.get('resolver'), id, i)
        require(retained.get('pendingFile') is (i == 4), 'File continuation marker differs')
        catalog = retained.get('catalog')
        require(isinstance(catalog, dict) and catalog.get('serial') == retained['serial'],
                'Original display source missing')
        origin = next((r for r in catalog.get('origins', []) if r.get('ts') == D.D.ROOT), None)
        require(origin == {'ts': D.D.ROOT, 'firstId': id, 'blockId': 'original-' + D.D.ROOT},
                'Original root/first-file source changed')
        row = next((r for r in catalog.get('rows', []) if r.get('id') == id), None)
        raw = rows[0]['files'][-1]
        require(isinstance(row, dict) and same(row.get('source'), raw)
                and row.get('effective', {}).get('name') == raw['name']
                and row.get('effective', {}).get('permalink') == raw['permalink'],
                'Canonical source replaced by resolver data')
        D.targets(stage.get('targetReads'), id, i >= 4)
        reply = stage['reply']
        runtime(reply.get('runtime'), i)
        if i == 2:
            require(reply.get('journal') is None, 'Fresh journal claimed during exclusive lock')
            require(reply['runtime']['capture'] != saved_a['capture'], 'Unsaved B reused capture A')
        else:
            item = journal(reply.get('journal'), i)
            require(reply['runtime']['capture'] == item['capture'], 'Native/disk capture differs')
            if i == 1:
                saved_a = copy.deepcopy(item)
                initial = stages[0]['reply']['journal']['drafts'][0]
                require(item['id'] == initial['id'] and item['revision'] > initial['revision'],
                        'A edit replaced original draft or did not persist')
            if i == 3:
                saved_b = copy.deepcopy(item)
                require(item['id'] == saved_a['id'] and item['revision'] > saved_a['revision']
                        and item['capture'] == stages[2]['reply']['runtime']['capture'],
                        'Retry did not persist the retained exact B capture')
            if i >= 4:
                require(same(item, saved_b), 'Destination/reopen changed durable B')
        storage(stage.get('storage'), i, saved_a, fixture)
        if i >= 3:
            require(same(stage['storage']['hold'], stages[2]['storage']['hold']),
                    'Original hold acknowledgement rewritten')
        if i >= 4:
            require(same(stage['storage']['release'], stages[3]['storage']['release']),
                    'Original release acknowledgement rewritten')
        if i == 4:
            D.destination_geometry(stage['layout'], id, viewport)
        else:
            source_geometry(stage['layout'], i, viewport)
    require(receipt.get('lockedEdit', {}).get('ok') is True
            and receipt['lockedEdit'].get('text') == TAIL
            and same(receipt['lockedEdit'].get('blocks'), blocks(2)),
            'Distinct actual native B append proof missing')
    require(receipt.get('refusedClick') == {'ok': True, 'prevented': [True]},
            'Original refused file click missing')
    departure = receipt.get('departure')
    require(isinstance(departure, dict)
            and same(departure.get('reply'), stages[3]['reply'])
            and same(departure.get('native', {}).get('native'), stages[3]['layout']['native']),
            'Successful departure is not original recovered B')
    require(same(receipt.get('storage'), stages[-1]['storage']),
            'Terminal replaced original storage protocol')
    require(same(receipt.get('proof'), stages[-1]['proof'])
            and same(receipt.get('layout'), stages[-1]['layout']), 'Terminal original proof changed')
