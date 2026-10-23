"""Portable adversarial receipts; these tests never launch a native process."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import canvas_replacement_custody as O
from test_canvas_special_roots import read

ENTRY = '11111111-1111-4111-8111-111111111111'
DRAFT = '22222222-2222-4222-8222-222222222222'
CAPTURE = '33333333-3333-4333-8333-333333333333'
EDITED = '44444444-4444-4444-8444-444444444444'
MOUNT = '55555555-5555-4555-8555-555555555555'
FRESH = '66666666-6666-4666-8666-666666666666'
B_DRAFT = '77777777-7777-4777-8777-777777777777'
ORIGINAL = dict(scope=ENTRY + ':1:2', mount=MOUNT, generation=2)


def draft(blocks, text, id=DRAFT, capture=CAPTURE, revision=1):
    return dict(drafts=[dict(id=id, capture=capture, revision=revision, blocks=blocks, text=text)],
                attempt=None, history=[])


def sample(viewport=(1280, 780)):
    b = dict(runtime=dict(id='replacement-b-entry', account=O.B, workspace='T1', user='UREPLACEB',
             file='FreplacementB', root=O.D.D.ROOT, text=O.B_TEXT, blocks=O.b_blocks(),
             capture=O.B_CAPTURE, serial=0, loading=False, saving=False, sending=False,
             nativePresent=False, error=None),
             journal=draft(O.b_blocks(), O.B_TEXT, B_DRAFT, O.B_CAPTURE))
    packets = [dict(**ORIGINAL, kind='capture', sequence=2+j, revision=2,
                    blocks=O.D.C.edited(), request=None, composing=False,
                    detached=bool(j), finalCapture=False) for j in range(2)]
    stages = []
    for i, name in enumerate(O.STAGES):
        edited = i >= 3
        blocks = O.D.C.edited() if edited else O.D.C.blocks()
        body = O.text(int(edited))
        capture = EDITED if edited else CAPTURE
        native = dict(**ORIGINAL, sequence=1 if i < 3 else 2,
                      failed=False, originalCompletion=True)
        if i in (4, 5):
            native = None
        if i == 6:
            native = dict(scope=ENTRY + ':3:4', mount=FRESH, generation=4, sequence=1,
                          failed=False, originalCompletion=False)
        runtime = dict(id=ENTRY, account=O.ACCOUNT, workspace='T1', user='U0', file='Ffixture',
                       root=O.D.D.ROOT, text=body, blocks=blocks, capture=capture, serial=1+int(edited),
                       saving=False, loading=False, sending=False, error=None,
                       nativePresent=i not in (4, 5), nativeBusy=False,
                       originalOwnerCurrent=i < 2, owner=native)
        editor = None
        if i in (0, 1, 6):
            editor = {k: native[k] for k in ('mount', 'scope', 'generation')}
            editor.update(blocks=O.D.C.blocks() if i == 0 else O.D.C.edited(), composing=False,
                          running=True, sameOriginalNode=i == 1, sameOriginalMount=i == 1,
                          rect=dict(x=20, y=50, width=500, height=200))
        original = None if i == 0 else dict(**ORIGINAL, connected=i == 1,
                                           holderSame=i < 4, running=i < 4, blocks=O.D.C.edited())
        bridge = None if i == 0 else dict(**ORIGINAL, failure=None, closed=i >= 4,
                   phase='edited-held' if i < 3 else 'detached-held' if i == 3 else 'detached-delivered',
                   held=copy.deepcopy(packets[:1 if i < 3 else 2]),
                   delivered=copy.deepcopy(packets[:0 if i < 3 else 1 if i == 3 else 2]))
        count = 4 if i == 6 else 2
        stages.append(dict(ok=True, stage=name, ordinal=i, noSession=True, noTransport=True,
            shell=dict(account=O.B if 2 <= i <= 4 else O.ACCOUNT,
                       epoch=8+(1 if 2 <= i <= 4 else 2 if i >= 5 else 0),
                       team='T1', user='UREPLACEB' if 2 <= i <= 4 else 'U0', visit=1 if i < 2 else 2 if i < 6 else 3,
                       canvas='Ffixture' if i in (0, 1, 6) else None,
                       noSession=True, noTransport=True, replyLedger=dict(requests=[], prepared=[])),
            retainedEntry=dict(runtime=runtime, journal=draft(blocks, body, capture=capture, revision=1+int(edited))),
            successor=copy.deepcopy(b), originalDestinationCurrent=i < 2,
            layout=dict(ok=True, viewport=list(viewport), canvasPresent=i in (0, 1, 6),
                        editor=editor, original=original, bridge=bridge),
            reads=dict(requests=[read('replies', O.D.D.ROOT) if n % 2 else read() for n in range(count)],
                       responses=[O.D.response(O.D.CANVAS, thread=n % 2 == 1) for n in range(count)])))
    return dict(ok=True, noSession=True, noTransport=True, replacement=True,
                originalEntryRetained=True, noForeignSend=True, requestCount=4,
                layout=copy.deepcopy(stages[-1]['layout']), stages=stages)


def set_path(value, path, replacement):
    target = value
    for k in path[:-1]:
        target = target[k]
    target[path[-1]] = replacement


def wrapper():
    path = Path(__file__).with_name('check-canvas-replacement-custody-native.py')
    spec = importlib.util.spec_from_file_location('replacement_native_wrapper', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Replacement(unittest.TestCase):
    def reject(self, path, value):
        receipt = sample()
        set_path(receipt, path, value)
        with self.assertRaises(RuntimeError):
            O.validate(receipt, O.FIXTURES[0])

    def test_both_widths(self):
        for fixture, viewport in zip(O.FIXTURES, ((1280, 780), (668, 380))):
            O.validate(sample(viewport), fixture)

    def test_loaded_shell_not_authority_revival(self):
        for index, key, value in ((2, 'account', O.ACCOUNT), (2, 'epoch', 8),
                                  (5, 'epoch', 8), (6, 'user', 'UREPLACEB'),
                                  (3, 'canvas', 'Ffixture'), (4, 'noSession', False),
                                  (4, 'noTransport', False), (6, 'visit', None)):
            with self.subTest(index=index, key=key):
                self.reject(('stages', index, 'shell', key), value)
        for i in range(2, 7):
            with self.subTest(revival=i):
                self.reject(('stages', i, 'originalDestinationCurrent'), True)
                self.reject(('stages', i, 'retainedEntry', 'runtime', 'originalOwnerCurrent'), True)

    def test_independent_b_and_original_scope(self):
        for key, value in (('account', O.B), ('user', 'UREPLACEB'), ('file', 'FreplacementB'),
                           ('id', B_DRAFT), ('root', 'foreign')):
            with self.subTest(key=key):
                self.reject(('stages', 4, 'retainedEntry', 'runtime', key), value)
        self.reject(('stages', 3, 'successor', 'runtime', 'text'), 'A content leaked')
        self.reject(('stages', 3, 'successor', 'journal', 'drafts', 0, 'revision'), 2)
        self.reject(('stages', 5, 'successor', 'runtime', 'nativePresent'), True)

    def test_rich_and_durable_custody(self):
        self.reject(('stages', 4, 'retainedEntry', 'journal', 'drafts', 0, 'blocks'), [])
        self.reject(('stages', 4, 'retainedEntry', 'journal', 'drafts', 0, 'blocks', 1, 'opaque'), {})
        self.reject(('stages', 3, 'retainedEntry', 'journal', 'drafts', 0, 'id'), B_DRAFT)
        self.reject(('stages', 3, 'retainedEntry', 'journal', 'drafts', 0, 'revision'), 1)
        self.reject(('stages', 5, 'retainedEntry', 'journal', 'drafts', 0, 'revision'), 3)
        self.reject(('stages', 2, 'retainedEntry', 'journal'), sample()['stages'][3]['retainedEntry']['journal'])
        self.reject(('stages', 4, 'retainedEntry', 'runtime', 'serial'), 3)
        self.reject(('stages', 3, 'retainedEntry', 'runtime', 'capture'), CAPTURE)

    def test_original_envelopes_not_manufactured(self):
        for key, value in (('mount', FRESH), ('scope', ENTRY+':8:2'), ('generation', 3),
                           ('kind', 'replace-result'), ('request', CAPTURE), ('finalCapture', True),
                           ('detached', True), ('composing', True), ('sequence', 1), ('revision', -1),
                           ('blocks', O.D.C.blocks())):
            with self.subTest(key=key):
                self.reject(('stages', 1, 'layout', 'bridge', 'held', 0, key), value)
        self.reject(('stages', 3, 'layout', 'bridge', 'held', 1, 'detached'), False)
        self.reject(('stages', 3, 'layout', 'bridge', 'held', 1, 'sequence'), 2)
        self.reject(('stages', 5, 'layout', 'bridge', 'held', 1, 'revision'), 999)
        self.reject(('stages', 2, 'layout', 'bridge', 'delivered'), sample()['stages'][3]['layout']['bridge']['delivered'])
        self.reject(('stages', 4, 'layout', 'bridge', 'delivered'), [])

    def test_detached_holder_and_fresh_remount(self):
        for index, key, value in ((2, 'connected', True), (3, 'holderSame', False),
                                  (4, 'running', True), (5, 'holderSame', True)):
            with self.subTest(index=index, key=key):
                self.reject(('stages', index, 'layout', 'original', key), value)
        self.reject(('stages', 6, 'layout', 'editor', 'sameOriginalNode'), True)
        self.reject(('stages', 6, 'layout', 'editor', 'mount'), MOUNT)
        self.reject(('stages', 6, 'retainedEntry', 'runtime', 'owner', 'originalCompletion'), True)
        self.reject(('stages', 4, 'retainedEntry', 'runtime', 'owner'), sample()['stages'][0]['retainedEntry']['runtime']['owner'])
        self.reject(('stages', 1, 'retainedEntry', 'runtime', 'owner', 'sequence'), 2)

    def test_no_send_no_foreign_read_no_receipt_rewrite(self):
        self.reject(('stages', 3, 'shell', 'replyLedger', 'prepared'), [{'method': 'chat.postMessage'}])
        self.reject(('stages', 4, 'retainedEntry', 'journal', 'attempt'), {'status': 'confirmed'})
        self.reject(('stages', 5, 'retainedEntry', 'journal', 'history'), [{'receipt': {'ok': True}}])
        self.reject(('stages', 2, 'reads', 'requests'), [])
        self.reject(('stages', 6, 'reads', 'responses', 0, 'messages', 0, 'text'), 'rewritten')
        self.reject(('stages', 6, 'reads', 'requests', 2, 'url'), 'https://fixture.slack.com/api/chat.postMessage')

    def test_geometry_and_stage_completeness(self):
        self.reject(('stages', 6, 'layout', 'editor', 'rect', 'y'), 900)
        self.reject(('stages', 1, 'layout', 'editor', 'rect', 'width'), float('nan'))
        self.reject(('stages', 3, 'ordinal'), 2)
        self.reject(('stages', 5, 'layout', 'canvasPresent'), True)
        self.reject(('layout', 'editor', 'blocks'), O.D.C.blocks())

    def test_wrapper_private_paths_and_exact_fixture(self):
        w = wrapper()
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder).resolve()
            checks = w.private_checks(w.SPEC, base)
            self.assertEqual(checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
            self.assertEqual(checks[w.SPEC.flag], '1')
            self.assertEqual(checks['SUPER_PLATINUM_CANVAS_REPLY_DB'], str(base/'replies.sqlite'))
            self.assertEqual(checks['SUPER_PLATINUM_CANVAS_REPLACEMENT_REPLY_DB'], str(base/'replacement-b.sqlite'))
            self.assertFalse(w.SPEC.synthetic_context)
            with self.assertRaises(RuntimeError):
                w.private_checks(object(), base)
            (base/'replacement-b.sqlite').touch()
            with self.assertRaises(RuntimeError):
                w.private_checks(w.SPEC, base)
        with self.assertRaises(RuntimeError):
            O.validate(sample(), 'canvas-canonical-native-list')

    def test_closed_stage_inventory_and_no_context_bypass(self):
        w = wrapper()
        receipt = sample()
        captured = [{**s, 'screenshot': f'{i}.png'} for i, s in enumerate(receipt['stages'])]
        log = '\n'.join(O.PREFIX+'-stage: '+json.dumps(s) for s in receipt['stages'])
        log += '\n'+O.PREFIX+'-check: '+json.dumps(receipt)
        with tempfile.TemporaryDirectory() as folder, patch(
                'canvas_native.expected_viewport', return_value=[1280, 780]):
            path = Path(folder)/'native.log'
            path.write_text(log+'\n', encoding='utf-8')
            w.validate_closed(w.SPEC, O.FIXTURES[0], path, captured, receipt)
            altered = copy.deepcopy(captured)
            altered[2]['shell']['epoch'] = 50
            with self.assertRaises(RuntimeError):
                w.validate_closed(w.SPEC, O.FIXTURES[0], path, altered, receipt)
            path.write_text(log+'\n'+log+'\n', encoding='utf-8')
            with self.assertRaises(RuntimeError):
                w.validate_closed(w.SPEC, O.FIXTURES[0], path, captured, receipt)


if __name__ == '__main__':
    unittest.main()
