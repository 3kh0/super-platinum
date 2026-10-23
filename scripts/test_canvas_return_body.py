"""Portable return-body protocol mutations, never native execution."""
import copy
import importlib.util
from html.parser import HTMLParser
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import canvas_return_body as O
from test_canvas_replacement_custody import sample as replacement, set_path, ENTRY, FRESH
from test_canvas_special_roots import read

REQUEST = '88888888-8888-4888-8888-888888888888'
RESTORED = '99999999-9999-4999-8999-999999999999'


def control(node, text):
    return dict(node=node, text=text, disabled=False, connected=True, inView=True, hit=True,
                rect=dict(x=20, y=20, width=100, height=20))


def record(owner, closed=False):
    return dict(**owner, commands=[], captures=[], acks=[], closed=closed)


def sample(case='success', viewport=(1280, 780)):
    receipt = replacement(viewport)
    original = O.R.owner(receipt['stages'][0]['retainedEntry']['runtime']['owner'])
    returned = O.R.owner(receipt['stages'][6]['retainedEntry']['runtime']['owner'])
    controls = dict(back=control(1, 'Back to canvas'), refresh=control(2, 'Refresh'),
                    threads=control(3, 'Show all threads'), retry=None)
    for i, s in enumerate(receipt['stages']):
        s['returnBody'] = dict(source=dict(case=case, armed=False, current=False, owner=None,
            pairs=[], requests=[], presentation=None, networkCalls=0, httpCapability=False),
            native=dict(ok=True, viewport=list(viewport), title='Project handbook', commentsOpen=i in (0, 1, 6),
            body=None, status=[], error=None, controls=copy.deepcopy(controls), finishOwner=None,
            currentEditor=None, actions=[], captures=[record(original, i >= 4)] +
                       ([record(returned)] if i == 6 else [])))
    baseline = copy.deepcopy(receipt['stages'][6])
    runtime = baseline['retainedEntry']['runtime']
    blocks = runtime['blocks']
    owner = {k: baseline['shell'][k] for k in ('account', 'epoch', 'team', 'user', 'visit')}
    owner['file'] = O.FILE
    finished = record(returned, True)
    finished['commands'] = [dict(kind='finish', scope=returned['scope'], mount=returned['mount'], request=REQUEST)]
    finished['captures'] = [dict(**returned, kind='capture', sequence=2, request=REQUEST,
                                blocks=copy.deepcopy(blocks), revision=0, composing=False,
                                detached=False, finalCapture=True)]
    finished['acks'] = [dict(kind='ack', scope=returned['scope'], mount=returned['mount'], sequence=2)]
    fresh = dict(scope=ENTRY+':3:6', mount=RESTORED, generation=6)
    actions = []
    for i, name in enumerate(O.stages(case)[7:], 7):
        s = copy.deepcopy(baseline)
        s.update(stage=name, ordinal=i)
        ready = name in ('body-ready', 'reply-restored')
        restored = name == 'reply-restored'
        failed = name == 'body-failed'
        pairs = (2 if case == 'failure' else 1) if ready else int(failed)
        action = {'back-to-canvas': 'back', 'body-failed': 'refresh',
                  'body-ready': 'retry' if case == 'failure' else 'refresh',
                  'reply-restored': 'threads'}[name]
        clicked = control(4, 'Retry') if action == 'retry' else controls[action]
        actions.append(dict(action=action, control=copy.deepcopy(clicked), event=dict(type='click', isTrusted=False)))
        s['retainedEntry']['runtime'].update(nativePresent=restored, owner=(dict(**fresh, sequence=1,
                                                    failed=False, originalCompletion=False) if restored else None))
        s['layout']['editor'] = (dict(**fresh, blocks=copy.deepcopy(blocks), composing=False,
            running=True, sameOriginalNode=False, sameOriginalMount=False,
            rect=dict(x=20, y=50, width=500, height=200)) if restored else None)
        if restored:
            s['reads']['requests'] += [read(), read('replies', O.R.D.D.ROOT)]
            s['reads']['responses'] += [O.R.D.response(O.R.D.CANVAS), O.R.D.response(O.R.D.CANVAS, thread=True)]
        c = copy.deepcopy(controls)
        c['back'] = control(6, 'Back to canvas') if restored else None
        c['retry'] = control(4, 'Retry') if failed else None
        c['refresh']['disabled'] = restored
        c['threads']['disabled'] = restored
        s['returnBody'] = dict(source=dict(case=case, armed=True, current=True, owner=owner,
            pairs=[dict(serial=n, owner=owner, read=n, status='failed' if case == 'failure' and n == 1 else 'ready')
                   for n in range(1, pairs+1)],
            requests=[r for n in range(1, pairs+1) for r in O.requests(n, case == 'failure' and n == 1)],
            presentation=dict(visit=1, read=pairs, loading=False, stale=pairs == 0 or failed,
                error=O.ERROR if failed else None,
                snapshot=dict(file=O.FILE, thread=O.THREAD, document=O.DOCUMENT, sections=0) if ready else None),
            networkCalls=0, httpCapability=False), native=dict(ok=True, viewport=list(viewport),
            title=O.TITLE if ready else 'Project handbook', commentsOpen=restored,
            body=O.TITLE+O.CONTENT+'Read-only acquired body & \u4e2d\U0001f98a' if name == 'body-ready'
                 else None if restored else O.STATIC_BODY,
            status=[] if failed or ready else ['This canvas changed. Refresh to continue editing.'],
            error=O.ERROR if failed else None, controls=c,
            finishOwner=dict(**returned, blocks=copy.deepcopy(blocks), connected=False, running=False, holderSame=False),
            currentEditor=dict(node=7, mount=RESTORED, sameFinishedNode=False, blocks=copy.deepcopy(blocks)) if restored else None,
            actions=copy.deepcopy(actions), captures=[record(original, True), copy.deepcopy(finished)]+
                       ([record(fresh)] if restored else [])))
        receipt['stages'].append(s)
    receipt['layout'] = copy.deepcopy(receipt['stages'][-1]['layout'])
    receipt['requestCount'] = 6
    return receipt


def wrapper():
    path = Path(__file__).with_name('check-canvas-return-body-native.py')
    spec = importlib.util.spec_from_file_location('return_body_native_wrapper', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReturnBody(unittest.TestCase):
    def reject(self, path, value, case='success'):
        receipt = sample(case)
        set_path(receipt, path, value)
        with self.assertRaises(RuntimeError):
            O.validate(receipt, O.FIXTURES[0], case)

    def test_both_cases_both_widths(self):
        for case in O.CASES:
            for fixture, viewport in zip(O.FIXTURES, ((1280, 780), (668, 380))):
                with self.subTest(case=case, fixture=fixture):
                    O.validate(sample(case, viewport), fixture, case)

    def test_actual_loaded_prefix_without_manufactured_final(self):
        receipt = sample(); receipt['stages'] = receipt['stages'][:9]
        del receipt['layout']; del receipt['requestCount']
        O.validate_loaded_prefix(receipt, O.FIXTURES[0])
        with self.assertRaises(RuntimeError):
            O.validate(receipt, O.FIXTURES[0])
        for mutation in ('missing', 'reordered', 'incomplete'):
            bad = copy.deepcopy(receipt)
            if mutation == 'missing':
                bad['stages'].pop()
            elif mutation == 'reordered':
                bad['stages'][7], bad['stages'][8] = bad['stages'][8], bad['stages'][7]
            else:
                bad['stages'][8]['returnBody']['source']['presentation']['stale'] = True
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                O.validate_loaded_prefix(bad, O.FIXTURES[0])

    def test_static_preview_exact_source_and_not_current_readiness(self):
        class Content(HTMLParser):
            def __init__(self):
                super().__init__(); self.depth = 0; self.text = []
            def handle_starttag(self, tag, attrs):
                if self.depth or (tag == 'div' and dict(attrs).get('class') == 'quip-canvas-content'):
                    self.depth += 1
            def handle_endtag(self, tag):
                if self.depth:
                    self.depth -= 1
            def handle_data(self, text):
                if self.depth:
                    self.text.append(text)
        source = Path(__file__).resolve().parents[1]/'src/desktop/canvas_fixture.html'
        parser = Content(); parser.feed(source.read_text(encoding='utf-8'))
        self.assertEqual(''.join(parser.text), O.STATIC_BODY)
        self.reject(('stages', 7, 'returnBody', 'native', 'body'), None)
        self.reject(('stages', 7, 'returnBody', 'native', 'body'), O.TITLE+O.CONTENT)
        self.reject(('stages', 8, 'returnBody', 'native', 'body'), None, 'failure')
        self.reject(('stages', 8, 'returnBody', 'source', 'presentation', 'stale'), False, 'failure')
        self.reject(('stages', 8, 'returnBody', 'native', 'body'), O.STATIC_BODY)
        self.reject(('stages', 9, 'returnBody', 'native', 'body'), O.STATIC_BODY)

    def test_original_replacement_guard_reused(self):
        self.reject(('stages', 2, 'originalDestinationCurrent'), True)
        self.reject(('stages', 3, 'layout', 'bridge', 'held', 1, 'detached'), False)
        self.reject(('stages', 0, 'returnBody', 'source', 'requests'), O.requests(1))
        self.reject(('stages', 5, 'returnBody', 'native', 'actions'), [{'action': 'refresh'}])

    def test_current_pair_owner_and_no_auto_acquisition(self):
        for key, value in (('account', O.R.B), ('epoch', 0), ('visit', 0), ('file', 'FCANON002'), ('user', 'UREPLACEB')):
            with self.subTest(key=key):
                self.reject(('stages', 8, 'returnBody', 'source', 'owner', key), value)
        self.reject(('stages', 8, 'returnBody', 'source', 'current'), False)
        self.reject(('stages', 7, 'returnBody', 'source', 'requests'), O.requests(1))
        self.reject(('stages', 8, 'returnBody', 'source', 'pairs', 0, 'read'), 0)
        self.reject(('stages', 8, 'returnBody', 'source', 'pairs', 0, 'status'), 'retired')
        self.reject(('stages', 8, 'returnBody', 'source', 'httpCapability'), True)

    def test_exact_factories_and_original_payloads(self):
        for field, value in (('target', 'foreign'), ('method', 'PUT'), ('path', '/api/chat.postMessage'),
                             ('args', {'file': O.FILE, 'token': 'invented'})):
            with self.subTest(field=field):
                self.reject(('stages', 8, 'returnBody', 'source', 'requests', 0, field), value)
        self.reject(('stages', 8, 'returnBody', 'source', 'requests', 2, 'response', 'base64'), 'AA==')
        self.reject(('stages', 8, 'returnBody', 'source', 'requests', 4, 'response', 'html'), '<p>fake</p>')
        self.reject(('stages', 8, 'returnBody', 'source', 'requests', 0, 'response', 'file', 'editable'), True)
        self.reject(('stages', 9, 'returnBody', 'source', 'requests', 4, 'response'), {'html': O.HTML}, 'failure')

    def test_failure_cannot_adopt_canonical_half_or_retry_automatically(self):
        self.reject(('stages', 8, 'returnBody', 'source', 'presentation', 'snapshot'),
                    dict(file=O.FILE, thread=O.THREAD, document=O.DOCUMENT, sections=0), 'failure')
        self.reject(('stages', 8, 'returnBody', 'source', 'presentation', 'stale'), False, 'failure')
        self.reject(('stages', 8, 'returnBody', 'native', 'error'), None, 'failure')
        self.reject(('stages', 8, 'returnBody', 'native', 'status'), [O.ERROR], 'failure')
        self.reject(('stages', 9, 'returnBody', 'native', 'error'), O.ERROR, 'failure')
        self.reject(('stages', 9, 'returnBody', 'native', 'actions', 2, 'action'), 'refresh', 'failure')
        self.reject(('stages', 9, 'returnBody', 'native', 'actions', 2, 'event', 'isTrusted'), True, 'failure')

    def test_final_native_capture_request_and_ack_exact(self):
        path = ('stages', 7, 'returnBody', 'native', 'captures', 1)
        for field, value in (('request', None), ('detached', True), ('finalCapture', False),
                             ('blocks', O.R.D.C.blocks()), ('mount', RESTORED), ('sequence', 1)):
            with self.subTest(field=field):
                self.reject((*path, 'captures', 0, field), value)
        self.reject((*path, 'commands', 0, 'request'), O.R.B_CAPTURE)
        self.reject((*path, 'acks', 0, 'sequence'), 3)
        self.reject((*path, 'closed'), False)
        self.reject(('stages', 8, 'returnBody', 'native', 'captures', 1, 'captures', 0, 'revision'), 7)
        self.reject(('stages', 9, 'returnBody', 'native', 'finishOwner', 'connected'), True)

    def test_unchanged_journals_and_no_send(self):
        self.reject(('stages', 8, 'retainedEntry', 'journal', 'drafts', 0, 'revision'), 3)
        self.reject(('stages', 8, 'retainedEntry', 'runtime', 'capture'), REQUEST)
        self.reject(('stages', 8, 'retainedEntry', 'runtime', 'sending'), True)
        self.reject(('stages', 8, 'successor', 'runtime', 'text'), 'wrong account')
        self.reject(('stages', 8, 'shell', 'replyLedger', 'requests'), [{'method': 'chat.postMessage'}])
        self.reject(('stages', 9, 'reads', 'responses', 4, 'messages', 0, 'text'), 'rewritten source')

    def test_actual_controls_and_restored_original_rich_content(self):
        self.reject(('stages', 8, 'returnBody', 'native', 'actions', 1, 'control', 'node'), 123)
        self.reject(('stages', 8, 'returnBody', 'native', 'actions', 1, 'control', 'hit'), False)
        self.reject(('stages', 8, 'returnBody', 'native', 'body'), O.TITLE)
        self.reject(('stages', 9, 'returnBody', 'native', 'currentEditor', 'blocks'), [])
        self.reject(('stages', 9, 'returnBody', 'native', 'currentEditor', 'sameFinishedNode'), True)
        self.reject(('stages', 9, 'retainedEntry', 'runtime', 'owner', 'mount'), FRESH)
        self.reject(('stages', 9, 'originalDestinationCurrent'), True)
        self.reject(('stages', 9, 'returnBody', 'native', 'controls', 'retry'), control(4, 'Retry'))
        self.reject(('stages', 9, 'returnBody', 'native', 'controls', 'refresh', 'disabled'), False)
        self.reject(('stages', 9, 'returnBody', 'native', 'controls', 'threads', 'disabled'), False)
        self.reject(('stages', 8, 'returnBody', 'native', 'controls', 'refresh', 'disabled'), True)
        self.reject(('stages', 8, 'returnBody', 'native', 'controls', 'threads', 'disabled'), True)

    def test_spec_private_flags_and_closed_capture_inventory(self):
        w = wrapper()
        for case, spec in w.SPECS.items():
            receipt = sample(case)
            with tempfile.TemporaryDirectory() as folder, patch(
                    'canvas_native.expected_viewport', return_value=[1280, 780]):
                base = Path(folder)
                flags = w.private_checks(spec, base)
                self.assertEqual(flags[spec.flag], case)
                self.assertEqual(flags['SUPER_PLATINUM_CANVAS_REPLACEMENT_CUSTODY_CHECK'], '1')
                self.assertEqual(flags['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'], '1')
                self.assertEqual(flags['SUPER_PLATINUM_CANVAS_REPLACEMENT_REPLY_DB'], str(base/'replacement-b.sqlite'))
                self.assertFalse(spec.synthetic_context)
                w.validate(spec, O.FIXTURES[0], receipt)
                log = '\n'.join(O.PREFIX+'-stage: '+json.dumps(s) for s in receipt['stages'])
                log += '\n'+O.PREFIX+'-check: '+json.dumps(receipt)+'\n'
                path = base/'native.log';path.write_text(log, encoding='utf-8')
                captured = [{**s, 'screenshot': f'{i}.png'} for i, s in enumerate(receipt['stages'])]
                w.validate_closed(spec, O.FIXTURES[0], path, captured, receipt)
                bad = copy.deepcopy(captured);bad[-1]['returnBody']['source']['current'] = False
                with self.assertRaises(RuntimeError):
                    w.validate_closed(spec, O.FIXTURES[0], path, bad, receipt)
        with self.assertRaises(RuntimeError):
            w.case_for(object())
        with self.assertRaises(RuntimeError):
            O.validate(sample(), O.FIXTURES[0], 'automatic')


if __name__ == '__main__':
    unittest.main()
