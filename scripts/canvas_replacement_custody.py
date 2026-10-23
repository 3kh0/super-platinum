"""Independent evidence checks for actual disconnected A -> B -> A replacement.

This checks original producer envelopes and separate durable inventories. It does
not authorize an old owner, infer a Slack account switch, or claim pixel review.
"""
import math
import canvas_canonical_native_destinations as D
from canvas_canonical_native_storage import ACCOUNT, canonical_uuid, text
from canvas_special_roots import require, same, request

PREFIX = 'canvas-replacement-custody'
FIXTURES = ('canvas-canonical-native-canvas', 'canvas-canonical-native-canvas-narrow')
STAGES = ('thread', 'edited-held', 'replacement-b', 'detached-held',
          'late-saved', 'returned-a', 'remounted-a')
B = '513bac6d-2702-4634-a2d9-f435a4c9ee62'
B_CAPTURE = 'fe3f849e-efb7-4e6b-a442-4d8e03806f60'
B_TEXT = 'Independent B reply stays here.'


def b_blocks():
    return [{'type': 'rich_text', 'elements': [{'type': 'rich_text_section',
             'elements': [{'type': 'text', 'text': B_TEXT, 'style': {'italic': True}}]}]}]


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= 9_007_199_254_740_991


def owner(value):
    require(isinstance(value, dict) and canonical_uuid(value.get('mount'))
            and isinstance(value.get('scope'), str) and integer(value.get('generation')),
            'Native owner missing')
    parts = value['scope'].split(':')
    require(len(parts) == 3 and canonical_uuid(parts[0]) and parts[1].isdigit()
            and parts[2] == str(value['generation']), 'Native scope/read differs')
    return {k: value[k] for k in ('mount', 'scope', 'generation')}


def journal(value, expected_blocks, expected_text):
    require(isinstance(value, dict) and set(value) == {'drafts', 'attempt', 'history'}
            and value['attempt'] is None and value['history'] == []
            and isinstance(value['drafts'], list) and len(value['drafts']) == 1,
            'Independent single-draft journal changed or sent')
    draft = value['drafts'][0]
    require(set(draft) == {'id', 'revision', 'capture', 'text', 'blocks'}
            and canonical_uuid(draft['id']) and canonical_uuid(draft['capture'])
            and integer(draft['revision'], 1) and draft['text'] == expected_text
            and same(draft['blocks'], expected_blocks), 'Exact durable rich draft differs')
    return draft


def successor(value):
    require(isinstance(value, dict), 'Independent B absent')
    draft = journal(value.get('journal'), b_blocks(), B_TEXT)
    expected = dict(id='replacement-b-entry', account=B, workspace='T1', user='UREPLACEB',
                    file='FreplacementB', root=D.D.ROOT, text=B_TEXT, blocks=b_blocks(),
                    capture=B_CAPTURE, serial=0, loading=False, saving=False, sending=False,
                    nativePresent=False, error=None)
    require(same(value.get('runtime'), expected) and draft['capture'] == B_CAPTURE,
            'Independent B original scope/body changed')


def source(value, index):
    count = 4 if index == 6 else 2
    require(isinstance(value, dict) and set(value) == {'requests', 'responses'}
            and isinstance(value['requests'], list) and len(value['requests']) == count,
            'Original comment read count differs')
    for n, wire in enumerate(value['requests']):
        request(wire, 'replies' if n % 2 else 'history', D.D.ROOT if n % 2 else None)
    require(same(value['responses'], [D.response(D.CANVAS, thread=n % 2 == 1)
                                     for n in range(count)]), 'Original HTTP responses rewritten')


def envelope(value, original, detached):
    require(isinstance(value, dict) and value.get('kind') == 'capture'
            and owner(value) == original and integer(value.get('sequence'), 1)
            and integer(value.get('revision')) and value.get('request') is None
            and 'request' in value and value.get('detached') is detached
            and value.get('finalCapture') is False and value.get('composing') is False
            and same(value.get('blocks'), D.C.edited()),
            'Held packet is not the original automatic rich capture')


def geometry(layout, index, viewport, original):
    require(isinstance(layout, dict) and layout.get('ok') is True
            and layout.get('viewport') == viewport
            and layout.get('canvasPresent') is (index in (0, 1, 6)),
            'Original replacement surface/viewport differs')
    editor = layout.get('editor')
    if index not in (0, 1, 6):
        require(editor is None, 'Original editor survived shell replacement')
    else:
        require(isinstance(editor, dict) and editor.get('running') is True
                and editor.get('composing') is False
                and same(editor.get('blocks'), D.C.blocks() if index == 0 else D.C.edited())
                and editor.get('sameOriginalNode') is (index == 1)
                and editor.get('sameOriginalMount') is (index == 1),
                'Original/fresh native editor identity or content differs')
        owner(editor)
        rect = editor.get('rect')
        require(isinstance(rect, dict)
                and all(type(rect.get(k)) in (int, float) and math.isfinite(rect[k])
                        for k in ('x', 'y', 'width', 'height'))
                and rect['width'] > 0 and rect['height'] > 0
                and rect['x'] < viewport[0] and rect['x'] + rect['width'] > 0
                and rect['y'] < viewport[1] and rect['y'] + rect['height'] > 0,
                'Native editor has no viewport intersection')
    old = layout.get('original')
    if index == 0:
        require(old is None and layout.get('bridge') is None,
                'Original capture was armed before baseline')
    else:
        require(isinstance(old, dict) and owner(old) == original
                and old.get('connected') is (index == 1)
                and old.get('holderSame') is (index < 4)
                and old.get('running') is (index < 4)
                and same(old.get('blocks'), D.C.edited()),
                'Detached original node/holder was replaced or revived')


def validate(receipt, fixture=None):
    require(fixture is None or fixture in FIXTURES, 'Replacement fixture differs')
    require(all(receipt.get(k) is True for k in
                ('ok', 'noSession', 'noTransport', 'replacement', 'originalEntryRetained', 'noForeignSend'))
            and type(receipt.get('requestCount')) is int and receipt['requestCount'] == 4,
            'Replacement terminal scope differs')
    stages = receipt.get('stages')
    require(isinstance(stages, list) and len(stages) == 7, 'Replacement stage inventory differs')
    viewport = receipt.get('layout', {}).get('viewport')
    require(isinstance(viewport, list) and len(viewport) == 2
            and all(integer(v, 1) for v in viewport), 'Viewport absent')
    first = stages[0]
    original = owner(first['retainedEntry']['runtime']['owner'])
    entry_id = first['retainedEntry']['runtime']['id']
    require(canonical_uuid(entry_id) and original['scope'].split(':')[0] == entry_id,
            'Native owner differs from retained entry')
    epoch = first['shell'].get('epoch')
    require(integer(epoch), 'Original epoch absent')
    first_draft = last_draft = None
    held = None
    for i, (stage, name) in enumerate(zip(stages, STAGES)):
        require(stage.get('stage') == name and type(stage.get('ordinal')) is int
                and stage['ordinal'] == i and all(stage.get(k) is True for k in
                ('ok', 'noSession', 'noTransport')), 'Stage order/isolation differs')
        shell = stage.get('shell', {})
        b = 2 <= i <= 4
        require(shell.get('account') == (B if b else ACCOUNT)
                and shell.get('epoch') == epoch + (1 if b else 2 if i >= 5 else 0)
                and shell.get('team') == 'T1' and shell.get('user') == ('UREPLACEB' if b else 'U0')
                and integer(shell.get('visit'))
                and shell.get('canvas') == ('Ffixture' if i in (0, 1, 6) else None)
                and shell.get('noSession') is True and shell.get('noTransport') is True
                and same(shell.get('replyLedger'), {'requests': [], 'prepared': []}),
                'Actual loaded-shell identity/isolation or mutation ledger differs')
        require(stage.get('originalDestinationCurrent') is (i < 2),
                'Original destination authority survived replacement')
        successor(stage.get('successor'))
        require(same(stage['successor'], first['successor']), 'Independent B changed across A capture')
        source(stage.get('reads'), i)
        if i and i < 6:
            require(same(stage['reads'], first['reads']), 'Replacement issued an extra read')
        if i == 6:
            require(same(stage['reads']['requests'][:2], first['reads']['requests']),
                    'Original read request prefix changed')
        retained = stage.get('retainedEntry', {})
        runtime = retained.get('runtime', {})
        edited = i >= 3
        draft = journal(retained.get('journal'), D.C.edited() if edited else D.C.blocks(), text(int(edited)))
        require(runtime.get('id') == entry_id and runtime.get('account') == ACCOUNT
                and runtime.get('workspace') == 'T1' and runtime.get('user') == 'U0'
                and runtime.get('file') == 'Ffixture' and runtime.get('root') == D.D.ROOT
                and runtime.get('text') == draft['text'] and same(runtime.get('blocks'), draft['blocks'])
                and runtime.get('capture') == draft['capture'] and integer(runtime.get('serial'))
                and all(runtime.get(k) is False for k in ('saving', 'loading', 'sending', 'nativeBusy'))
                and runtime.get('error') is None
                and runtime.get('originalOwnerCurrent') is (i < 2)
                and runtime.get('nativePresent') is (i not in (4, 5)),
                'Retained original rich runtime/journal custody differs')
        if i == 0:
            first_draft = draft
        elif i < 3:
            require(same(draft, first_draft), 'Held original packet was persisted before delivery')
        else:
            require(draft['id'] == first_draft['id'] and draft['revision'] > first_draft['revision']
                    and draft['capture'] != first_draft['capture'], 'Original draft edit identity/revision lost')
            if last_draft is not None:
                require(same(draft, last_draft), 'Unchanged detached capture rewrote durable draft')
            last_draft = draft
        require(runtime['serial'] == first['retainedEntry']['runtime']['serial'] + int(edited),
                'Original body serial changed without a content edit')
        native = runtime.get('owner')
        if i in (4, 5):
            require(native is None, 'Terminal original native holder still charged')
        else:
            identity = owner(native)
            require(integer(native.get('sequence')) and native.get('failed') is False
                    and native.get('originalCompletion') is (i < 4), 'Native completion identity differs')
            if i < 4:
                require(identity == original, 'Held native owner changed')
            else:
                require(identity['mount'] != original['mount'] and identity['scope'] != original['scope']
                        and identity['scope'].split(':')[0] == entry_id,
                        'Fresh A remount reused original lease')
            require(owner(stage['layout']['editor']) == identity if i in (0, 1, 6) else True,
                    'Painted editor differs from runtime owner')
        geometry(stage.get('layout'), i, viewport, original)
        if i == 0:
            continue
        bridge = stage['layout'].get('bridge')
        require(isinstance(bridge, dict) and owner(bridge) == original and bridge.get('failure') is None
                and bridge.get('closed') is (i >= 4)
                and bridge.get('phase') == ('edited-held' if i < 3 else 'detached-held' if i == 3 else 'detached-delivered'),
                'Original held bridge lifecycle differs')
        packets = bridge.get('held')
        require(isinstance(packets, list) and len(packets) == (1 if i < 3 else 2), 'Capture envelope count differs')
        envelope(packets[0], original, False)
        if held is None:
            held = packets[0]
        require(same(packets[0], held), 'Original held envelope rewritten')
        require(held['sequence'] > first['retainedEntry']['runtime']['owner']['sequence'],
                'Edited envelope predates the baseline capture')
        if i < 3:
            require(native['sequence'] < held['sequence'], 'Held packet already adopted')
        else:
            envelope(packets[1], original, True)
            require(packets[1]['sequence'] > packets[0]['sequence'], 'Detached capture sequence did not advance')
        require(same(bridge.get('delivered'), packets[:0 if i < 3 else 1 if i == 3 else 2]),
                'Original packet was omitted, rewritten, or delivered more than once')
        if i > 3:
            require(same(packets, stages[3]['layout']['bridge']['held']), 'Detached original envelope rewritten')
        if i > 4:
            require(same(bridge, stages[4]['layout']['bridge']), 'Closed original bridge changed on A return')
    require(same(receipt.get('layout'), stages[-1]['layout']), 'Terminal layout differs from captured remount')
