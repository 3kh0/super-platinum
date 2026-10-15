"""Fault-injected complete Canvas runner tests; no app, Win32, or image capture."""
from contextlib import ExitStack, redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = next(p for p in HERE.parents if (p / 'Cargo.toml').is_file())
sys.path.insert(0, str(ROOT / 'scripts'))

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module

e = load('poll_candidate_canvas_evidence', HERE / 'canvas_evidence.py')
c = load('poll_test_canvas_native', ROOT / 'scripts/canvas_native.py')


class Cleanup(unittest.TestCase):
    def exercise(self, *, poll_error=None, primary=None, identity_error=None,
                 close_result=True, close_error=None, running=False):
        events, closes, polls = [], [], []
        owned = []
        spec = c.CanvasCheck('canvas-structure-text', 'unused', (), {}, ())
        terminal = dict(ok=True, noSession=True, noTransport=True,
                        layout=dict(ok=True, viewport=c.expected_viewport(spec.fixture)))
        class Native:
            def __init__(self, fixture, log, binary, env):
                self.log, self.binary = log, binary
                self.cleanup_errors, self.exited = [], False
                def poll():
                    polls.append('poll')
                    if poll_error: raise poll_error
                    return None if running else 0
                self.process = types.SimpleNamespace(pid=71, poll=poll)
                owned.append(self)
            def __enter__(self):
                self.log.write_text(spec.fixture + '-check: ' + json.dumps(terminal) + '\n', encoding='utf8')
                events.append('enter')
                return self
            def __exit__(self, *args):
                events.append('exit'); self.exited = True
            def require_running(self):
                if self.exited: raise RuntimeError('already exited')
            def receipts(self, prefix):
                return [terminal] if prefix.endswith('-check: ') else []
            def capture(self, path):
                self.require_running(); events.append('capture:' + path.name)
        def times(handle):
            self.assertEqual(handle, 42)
            if owned[0].exited and identity_error: raise identity_error
            return dict(creation_filetime=23, exit_filetime=31 if owned[0].exited else 0)
        def close(handle):
            closes.append(handle); events.append('close')
            if close_error: raise close_error
            return close_result
        api = types.SimpleNamespace(times=times, kernel=types.SimpleNamespace(CloseHandle=close))
        def normalize(native, evidence, stack, *args):
            evidence['pointer'] = dict(movement_observed=False, restore_attempted=True, restore_observed=False)
            def restore():
                native.require_running(); events.append('restore')
                evidence['pointer']['restore_observed'] = True
            stack.callback(restore)
            return types.SimpleNamespace(check=lambda *a, **kw: None)
        original_validate = c.validate
        def validate(*args):
            if primary: raise primary
            return original_validate(*args)
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack, redirect_stdout(io.StringIO()):
            root = Path(directory)
            stack.enter_context(patch.object(e.sys, 'platform', 'win32'))
            for name, value in dict(open_process=lambda pid: (api, 42), binary_sha=lambda p: 'digest',
                                    normalize=normalize, png_signature=lambda *a: (100,80)).items():
                stack.enter_context(patch.object(e, name, value))
            for name, value in dict(NativeFixture=Native, audited_fixture=e.audited_fixture,
                                    validate=validate, png_signature=lambda *a: (100,80)).items():
                stack.enter_context(patch.object(c, name, value))
            caught = None
            try: c.run(spec, spec.fixture, root/'unused.exe', root)
            except BaseException as error: caught = error
            receipt = json.loads((root/(spec.fixture+'.json')).read_text(encoding='utf8'))
        self.assertEqual(closes, [42])
        self.assertEqual(polls, ['poll'])
        self.assertLess(events.index('restore'), events.index('exit'))
        self.assertLess(events.index('exit'), events.index('close'))
        self.assertEqual(receipt['closedTerminalReceipts'], [terminal])
        self.assertEqual(receipt['closedStageReceipts'], [])
        return caught, receipt, events

    def test_success_records_one_poll_and_exact_exit_identity(self):
        error, receipt, _ = self.exercise()
        self.assertIsNone(error); self.assertTrue(receipt['ok'])
        self.assertTrue(receipt['closedLogRevalidated'])
        self.assertEqual(receipt['auditErrors'], [])
        self.assertEqual(receipt['cleanupProcessIdentity'], dict(creation_filetime=23, exit_filetime=31))

    def test_poll_failure_writes_failed_receipt_and_still_queries_identity(self):
        error, receipt, _ = self.exercise(poll_error=OSError('poll failed'))
        self.assertIsInstance(error, RuntimeError); self.assertFalse(receipt['ok'])
        self.assertFalse(receipt['processExited']); self.assertIsNone(receipt['exitCode'])
        self.assertEqual([x['stage'] for x in receipt['auditErrors']], ['poll','process'])
        self.assertEqual(receipt['exitIdentity'], dict(creation_filetime=23, exit_filetime=31))
        self.assertIn('poll failed', receipt['error'])

    def test_primary_survives_poll_failure_and_live_failure_capture(self):
        primary = RuntimeError('original native validation failure')
        error, receipt, events = self.exercise(primary=primary, poll_error=OSError('poll failed'))
        self.assertIs(error, primary)
        self.assertEqual(receipt['error'], 'RuntimeError: original native validation failure')
        self.assertFalse(receipt['ok'])
        self.assertTrue(any('poll failed' in note for note in primary.__notes__))
        self.assertLess(events.index('capture:canvas-structure-text-failure.png'), events.index('restore'))

    def test_poll_and_identity_and_close_errors_all_retained(self):
        primary = RuntimeError('original error')
        error, receipt, _ = self.exercise(primary=primary, poll_error=OSError('poll failed'),
            identity_error=OSError('times failed'), close_error=OSError('close failed'))
        self.assertIs(error, primary)
        self.assertEqual([x['stage'] for x in receipt['auditErrors']], ['poll','process','identity','handle'])
        self.assertNotIn('exitIdentity', receipt)
        self.assertFalse(receipt['ok'])

    def test_false_close_bool_cannot_pass(self):
        error, receipt, _ = self.exercise(close_result=False)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(receipt['ok'])
        self.assertEqual([x['stage'] for x in receipt['auditErrors']], ['handle'])

    def test_identity_failure_closes_once_and_writes_receipt(self):
        error, receipt, _ = self.exercise(identity_error=OSError('times failed'))
        self.assertIsInstance(error, RuntimeError); self.assertFalse(receipt['ok'])
        self.assertEqual([x['stage'] for x in receipt['auditErrors']], ['identity'])

    def test_missing_exit_is_not_inferred_from_retained_identity(self):
        error, receipt, _ = self.exercise(running=True)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(receipt['ok'])
        self.assertFalse(receipt['processExited']); self.assertIsNone(receipt['exitCode'])
        self.assertEqual(receipt['exitIdentity']['exit_filetime'], 31)
        self.assertEqual([x['stage'] for x in receipt['auditErrors']], ['process'])

if __name__ == '__main__': unittest.main()
