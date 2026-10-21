"""Original final-capture timeout, one late delivery and explicit native navigation."""
import math
import canvas_canonical_native_destinations as D
from canvas_canonical_native_storage import alert_inventory, canonical_uuid, journal, text
from canvas_special_roots import require, same

PREFIX = 'canvas-canonical-native-late-capture'
FIXTURES = D.FIXTURES
STAGES = ('thread', 'draft', 'timed-out', 'duplicate-refused', 'capture-recovered',
          'retry-recovered', 'destination', 'reopened')
READS = (2, 2, 2, 2, 2, 2, 2, 4)
RESOLVES = (0, 0, 1, 2, 2, 2, 3, 3)
BATCHES = (0, 0, 0, 0, 0, 0, 1, 1)
ERROR = 'This file could not be opened. Your reply draft is retained here.'


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= 9_007_199_254_740_991


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def source(proof, id, index):
    rows = [D.roots(id)[0], D.D.REPLY]
    responses = [D.response(id, thread=j % 2 == 1) for j in range(4)]
    retained = D.D.wire_proof(proof, responses[:READS[index]], {1, 3}, rows, True,
                              visible=index != 6)
    require(retained.get('pendingFile') is (index == 6), 'Original file continuation differs')
    require(retained.get('navigationError') == (ERROR if index in (2, 3, 4, 5) else None),
            'Original navigation error lifecycle differs')
    catalog = retained.get('catalog')
    require(isinstance(catalog, dict) and catalog.get('serial') == retained['serial'],
            'Canonical source serial missing')
    origin = next((r for r in catalog.get('origins', []) if r.get('ts') == D.D.ROOT), None)
    require(origin == {'ts': D.D.ROOT, 'firstId': id, 'blockId': 'original-' + D.D.ROOT},
            'Original first-file/control identity changed')
    row = next((r for r in catalog.get('rows', []) if r.get('id') == id), None)
    raw = rows[0]['files'][-1]
    require(isinstance(row, dict) and same(row.get('source'), raw)
            and row.get('effective', {}).get('name') == raw['name']
            and row.get('effective', {}).get('permalink') == raw['permalink'],
            'Fresh navigation response replaced display source')
    resolver = proof.get('resolver')
    n = RESOLVES[index]
    require(isinstance(resolver, dict) and len(resolver.get('requests', [])) == n,
            'Exact explicit resolver count differs')
    for request in resolver['requests']:
        D.wire(request, '/api/files.info', [['file', id]])
    require(same(resolver.get('responses'), [{'ok': True, 'file': D.target_file(id)}] * n)
            and resolver.get('settlements') == [{'serial': j, 'delivered': True}
                                                for j in range(1, n + 1)]
            and resolver.get('held') is None and resolver.get('releases') == [],
            'Original resolver receipt/settlement differs')


def owner(value):
    require(isinstance(value, dict) and canonical_uuid(value.get('mount'))
            and isinstance(value.get('scope'), str) and value['scope']
            and integer(value.get('generation')),
            'Original native owner is not explicit')
    parts = value['scope'].split(':')
    require(len(parts) == 3 and canonical_uuid(parts[0])
            and parts[1].isdigit() and parts[2] == str(value['generation']),
            'Native field scope/read generation differs')
    return {k: value[k] for k in ('scope', 'mount', 'generation')}


def bridge(value, index, original, held):
    require(isinstance(value, dict) and value.get('failure') is None
            and integer(value.get('sends')) and integer(value.get('finalSends'))
            and integer(value.get('releaseCount')), 'Original bridge evidence invalid')
    require(owner(value) == original, 'Late capture adopted another native owner')
    if index < 2:
        require(value.get('phase') == 'idle' and value.get('closed') is False
                and value.get('heldPayload') is None and value.get('finishRequests') == []
                and value.get('finalSends') == 0 and value.get('releaseCount') == 0
                and value.get('heldAt') is None and value.get('releasedAt') is None,
                'Final packet dispatched before explicit finish')
        return
    packet = value.get('heldPayload')
    require(isinstance(packet, dict) and packet.get('kind') == 'capture'
            and owner(packet) == original and canonical_uuid(packet.get('request'))
            and integer(packet.get('sequence'), 1) and integer(packet.get('revision'))
            and packet.get('composing') is False and packet.get('detached') is False
            and packet.get('finalCapture') is True and same(packet.get('blocks'), D.C.edited()),
            'Held payload is not the exact original final rich capture')
    require(value.get('finishRequests') == [{'kind': 'finish', 'scope': original['scope'],
                 'mount': original['mount'], 'request': packet['request']}],
            'Competing or foreign finish reached original bridge')
    if held is not None:
        require(same(packet, held['heldPayload']) and value['heldAt'] == held['heldAt'],
                'Original held payload/timestamp rewritten')
    require(finite(value.get('heldAt')), 'Original hold time missing')
    if index in (2, 3):
        require(value.get('phase') == 'held' and value.get('closed') is False
                and value.get('releasedAt') is None and value['finalSends'] == 0
                and value['releaseCount'] == 0,
                'Timed-out original packet was delivered or uncharged early')
    else:
        require(value.get('phase') == 'delivered' and value.get('closed') is True
                and finite(value.get('releasedAt'))
                and value['releasedAt'] - value['heldAt'] >= 6000
                and value['finalSends'] == 1 and value['releaseCount'] == 1,
                'Original capture did not complete once after real timeout')
        require(held is not None and value['sends'] == held['sends'] + 1,
                'Late original bridge delivered extra packets')


def native_owner(runtime, original, index):
    busy = index in (2, 3)
    require(runtime.get('nativeBusy') is busy, 'Original timeout charge differs')
    value = runtime.get('owner')
    if index == 6:
        require(value is None, 'Original native holder survived successful departure')
        return
    identity = owner(value)
    require(integer(value.get('sequence')) and value.get('failed') is False,
            'Native owner failed or lacks capture sequence')
    if index <= 3:
        require(identity == original and value.get('originalCompletion') is True,
                'Original timeout spent another completion notifier')
    else:
        require(identity['mount'] != original['mount']
                and identity['scope'].split(':')[0] == original['scope'].split(':')[0]
                and value.get('originalCompletion') is False,
                'Late completion did not remount a fresh native holder')


def geometry(value, index, viewport, expected):
    alerts = []
    if index in (2, 3, 4):
        alerts = [{'text': ERROR, 'visible': True, 'owner': 'composer'},
                  {'text': ERROR, 'visible': False, 'owner': 'panel'}]
    elif index == 5:
        alerts = [{'text': ERROR, 'visible': True, 'owner': 'panel'}]
    alert_inventory(value, alerts)
    require(value.get('ok') is True and value.get('viewport') == viewport
            and value.get('inScroll') is True and value.get('viewer') is None,
            'Original source surface replaced before fresh activation')
    D.visible(value.get('target'), viewport, value.get('scroll'))
    require(value.get('targetKind') == ('retry' if index in (2, 3, 4) else 'editor-line'),
            'Stage did not paint the intended recovery control or draft')
    n = value.get('native')
    require(isinstance(n, dict) and same(n.get('blocks'), expected)
            and n.get('composing') is False and n.get('running') is True,
            'Exact rich native draft lost')
    if index in (1, 2, 3):
        require(all(n.get(k) is True for k in ('sameNode', 'sameMount', 'sameBlocks')),
                'Original held editor replaced before terminal delivery')
    if index in (4, 5, 7):
        require(n.get('sameNode') is False and n.get('sameMount') is False
                and n.get('sameBlocks') is True, 'Late/reopened original draft not freshly remounted')
    if index in (2, 3, 4):
        retry = value.get('retry')
        require(isinstance(retry, dict) and retry.get('disabled') is (index in (2, 3))
                and retry.get('text') == 'Retry local save',
                'Timeout charge/recovery Retry state differs')
        D.visible(retry, viewport, value.get('scroll'))
        require(ERROR in value.get('alerts', []), 'Original navigation refusal guidance missing')
    controls, editor = value.get('controls'), value.get('editor')
    require(isinstance(controls, dict) and isinstance(editor, dict)
            and editor.get('connected') is True and editor.get('running') is True
            and editor.get('contentEditable') == ('false' if index in (2, 3) else 'true')
            and editor.get('finishing') is (index in (2, 3)),
            'Original editor lock/remount state differs')
    require(isinstance(controls.get('send'), dict)
            and controls['send'].get('text') == 'Send reply'
            and controls['send'].get('disabled') is (index in (2, 3, 4)),
            'Send was enabled during unresolved capture or local error')
    if index in (2, 3, 4):
        retry = controls.get('retry')
        require(isinstance(retry, dict) and retry.get('disabled') is (index in (2, 3))
                and retry.get('text') == 'Retry local save' and retry.get('inScroll') is True,
                'Actual enabled/disabled recovery control differs')
        D.visible(retry, viewport, value.get('scroll'))
        errors = [a for a in controls.get('alerts', []) if a.get('text') == ERROR
                  and a.get('inScroll') is True and a.get('inView') is True and a.get('hit') is True]
        require(errors, 'Composer-local failure guidance not readable')
        for error in errors:
            D.visible(error, viewport, value.get('scroll'))
    else:
        require(controls.get('retry') is None, 'Retry persisted after local recovery')


def terminal_late(value, stages):
    require(isinstance(value, dict) and integer(value.get('timeoutElapsedMs'), 6000)
            and value['timeoutElapsedMs'] < 60000, 'Actual six-second request timeout unproven')
    for key in ('firstClick', 'duplicateClick'):
        require(value.get(key) == {'ok': True, 'prevented': [True]},
                'Actual original/competing activation missing')
    arm = value.get('arm')
    require(isinstance(arm, dict) and arm.get('ok') is True
            and same(arm.get('bridge'), {**stages[1]['bridge'], 'phase': 'armed'}),
            'Original running holder was not explicitly armed')
    before, after = value.get('beforeRelease'), value.get('afterRelease')
    for action, index in ((before, 3), (after, 4)):
        require(isinstance(action, dict) and action.get('ok') is True
                and same(action.get('bridge'), stages[index]['bridge'])
                and same(action.get('editor'), stages[index]['layout']['editor']),
                'Late release action adopted a different bridge/editor')
        require(action.get('controls', {}).get('retry', {}).get('disabled') is (index == 3)
                and action.get('controls', {}).get('send', {}).get('disabled') is True,
                'Release action charge/recovery state differs')
    release = value.get('release')
    require(isinstance(release, dict) and release.get('ok') is True,
            'Explicit original final release missing')
    expected = {**stages[3]['bridge'], 'phase': 'released', 'releaseCount': 1,
                'releasedAt': stages[4]['bridge']['releasedAt']}
    require(same(release.get('bridge'), expected), 'Release changed original held packet or delivered early')
    require(stages[2]['lateRuntime']['owner'] == stages[3]['lateRuntime']['owner'],
            'Competing activation advanced original native capture')
    held = stages[2]['bridge']['heldPayload']
    require(held['sequence'] > stages[2]['lateRuntime']['owner']['sequence'],
            'Held final packet was already adopted before release')


def validate(receipt, fixture=None):
    stages, id = receipt.get('stages'), receipt.get('destinationId')
    require(id in (D.CANVAS, D.LIST) and isinstance(stages, list) and len(stages) == len(STAGES),
            'Late capture stage/target inventory differs')
    if fixture is not None:
        require(fixture in FIXTURES and id == (D.CANVAS if '-canvas' in fixture else D.LIST),
                'Named fixture target differs')
    viewport = receipt.get('layout', {}).get('viewport')
    original = owner(stages[1]['lateRuntime']['owner'])
    saved = held = None
    for i, (stage, name) in enumerate(zip(stages, STAGES)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int
                and stage['ordinal'] == i
                and all(stage.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Stage order/isolation differs')
        source(stage['proof'], id, i)
        D.targets(stage.get('targetReads'), id, i >= 6)
        item = journal(stage['reply'].get('journal'), min(i, 1))
        runtime = stage['reply'].get('runtime')
        require(isinstance(runtime, dict)
                and all(runtime.get(k) is False for k in ('saving', 'loading', 'sending'))
                and same(runtime.get('blocks'), D.C.blocks() if i == 0 else D.C.edited())
                and runtime.get('text') == text(min(i, 1))
                and runtime.get('capture') == item['capture']
                and runtime.get('nativePresent') is (i != 6)
                and runtime.get('error') == (ERROR if i in (2, 3, 4) else None),
                'Original rich runtime/journal/error differs')
        late = stage.get('lateRuntime')
        require(isinstance(late, dict)
                and same({k: v for k, v in late.items() if k not in ('nativeBusy', 'owner')}, runtime),
                'Late witness is not original runtime')
        native_owner(late, original, i)
        if i == 1:
            saved = item
            previous = stages[0]['reply']['journal']['drafts'][0]
            require(item['id'] == previous['id'] and item['revision'] > previous['revision'],
                    'Actual edit did not preserve draft identity and advance revision')
        if i >= 2:
            require(same(item, saved), 'Unchanged late capture rewrote original durable draft')
        bridge(stage.get('bridge'), i, original, held)
        if i == 2:
            held = stage['bridge']
        if i == 3:
            require(same(stage['bridge'], held), 'Competing activation changed original bridge')
        if i >= 5:
            require(same(stage['bridge'], stages[4]['bridge']),
                    'Closed original bridge changed during retry/navigation')
        if i == 6:
            D.destination_geometry(stage['layout'], id, viewport)
            require(stage['layout'].get('editor') is None,
                    'Original editor remained in destination')
        else:
            geometry(stage['layout'], i, viewport, runtime['blocks'])
            require(owner(stage['layout']['native']) == owner(late['owner']),
                    'Painted native owner differs from original runtime witness')
            require(late['owner']['generation'] == stage['proof']['retained']['serial'],
                    'Painted native generation differs from original comment read')
    terminal_late(receipt.get('late'), stages)
    departure = receipt.get('departure')
    require(isinstance(departure, dict) and same(departure.get('reply'), stages[5]['reply'])
            and same(departure.get('native', {}).get('native'), stages[5]['layout']['native']),
            'Fresh explicit departure did not retain recovered original draft')
    require(same(receipt.get('proof'), stages[-1]['proof'])
            and same(receipt.get('layout'), stages[-1]['layout']),
            'Terminal source/layout changed')
