"""Full-run cleanup fault injection; no subprocess, Win32, network or captures."""
from contextlib import ExitStack, redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import struct
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
    spec.loader.exec_module(module)
    return module


P = load('cleanup_protected', HERE / 'check-protected-drafts-native.py')
B = load('cleanup_bookmarks', HERE / 'check-bookmarks-native.py')


class Cleanup(unittest.TestCase):
    def exercise(self, kind, *, primary=None, poll_error=False, close_result=True,
                 close_error=False, identity_error=False, receipt_error=False,
                 cleanup_errors=(), running=False):
        m = P if kind == 'protected' else B
        closes, polls, identities, events = [], [], [], []
        owned = []
        terminal = {'ok': True}
        nonce = 'a' * 32
        expected = dict(version=1, nonce=nonce, pid=71, scenario='consume-channel', phase='exercise')
        class Native:
            def __init__(self, fixture, log, binary, env):
                self.log, self.exited = log, False
                self.cleanup_errors = list(cleanup_errors)
                self._temporary = types.SimpleNamespace(name=str(log.parent / 'absent-scratch'))
                def poll():
                    polls.append('poll')
                    # Healthy path first polls after native exit, then finalizes.
                    # With a primary error, the first poll is already finalization.
                    if poll_error and (primary is not None or len(polls) >= 2):
                        raise OSError('poll injected')
                    return None if running else 0
                self.process = types.SimpleNamespace(pid=71, poll=poll)
                owned.append(self)
            def __enter__(self):
                events.append('enter'); return self
            def __exit__(self, *args):
                events.append('exit'); self.exited = True
            def require_running(self):
                if self.exited: raise RuntimeError('used exited process')
            def receipts(self, prefix):
                return [expected] if prefix == P.WAITING else [terminal] if prefix == P.PREFIX else []
            def capture(self, path):
                events.append('capture')
                path.write_bytes(b'\x89PNG\r\n\x1a\n' + b'\0\0\0\rIHDR' + struct.pack('>II', 100, 80))
                return {'ok': True, 'data': {'path': str(path), 'width': 100, 'height': 80}}
            def control(self, *args, **kwargs):
                return {'ok': True, 'data': {}}
        def times(handle):
            self.assertEqual(handle, 42)
            identities.append(owned[0].exited)
            if identity_error and owned[0].exited: raise OSError('identity injected')
            return {'creation_filetime': 23, 'exit_filetime': 31 if owned[0].exited else 0}
        def close(handle):
            closes.append(handle); events.append('close')
            if close_error: raise OSError('close injected')
            return close_result
        api = types.SimpleNamespace(open=lambda pid: 42, times=times,
                                    kernel=types.SimpleNamespace(CloseHandle=close))
        def artifacts(*args):
            if primary: raise primary
            return {}
        def marker(*args, **kwargs):
            if primary: raise primary
            return B.SUCCESS
        def normalize(native, evidence, stack, *args):
            evidence['pointer'] = {}
            stack.callback(lambda: events.append('restore'))
            return types.SimpleNamespace(check=lambda *args, **kwargs: None)
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack, redirect_stdout(io.StringIO()):
            directory = Path(temp)
            receipt = directory / ('protected-drafts-consume-channel-exercise.json' if kind == 'protected' else 'bookmarks.json')
            stack.enter_context(patch.object(m, 'NativeFixture', Native))
            stack.enter_context(patch.object(m.time, 'sleep', lambda _: None))
            if kind == 'protected':
                stack.enter_context(patch.object(m.sys, 'platform', 'win32'))
                stack.enter_context(patch.dict(sys.modules, {'windows_process_metrics': types.SimpleNamespace(WindowsAPI=lambda: api)}))
                changes = dict(require_contract=lambda: None, sha=lambda _: 'digest',
                               validate_artifacts=artifacts, normalize=normalize,
                               validate_pointer_cleanup=lambda _: None,
                               closed_receipts=lambda path, prefix: [expected] if prefix == P.WAITING else [terminal] if prefix == P.PREFIX else [])
            else:
                changes = dict(sha256=lambda _: 'digest', read_marker=marker,
                               read_pickers=lambda _: [], validate_picker_captures=lambda *args: None,
                               validate_state=lambda _: {})
            for name, value in changes.items():
                stack.enter_context(patch.object(m, name, value))
            original_write = Path.write_text
            def write(path, *args, **kwargs):
                if receipt_error and path == receipt: raise OSError('receipt injected')
                return original_write(path, *args, **kwargs)
            stack.enter_context(patch.object(Path, 'write_text', write))
            caught = None
            try:
                if kind == 'protected':
                    m.run_phase('protected-drafts', 'consume-channel', 'exercise', nonce,
                                directory, directory / 'unused.exe', directory, 'digest')
                else:
                    m.run('bookmarks', directory / 'unused.exe', directory)
            except BaseException as error:
                caught = error
            data = json.loads(receipt.read_text(encoding='utf-8')) if receipt.exists() else None
        self.assertEqual(closes, [42] if kind == 'protected' else [])
        if kind == 'protected':
            self.assertLess(events.index('restore'), events.index('exit'))
            self.assertLess(events.index('exit'), events.index('close'))
        return caught, data, polls, identities

    def test_success_preserves_both_receipts(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                error, data, polls, _ = self.exercise(kind)
                self.assertIsNone(error); self.assertTrue(data['ok'])
                self.assertEqual(data['cleanupErrors'], [])
                self.assertEqual(len(polls), 2)
                self.assertEqual(data['exitCode'], 0)

    def test_final_poll_failure_forces_failure_and_receipt(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                error, data, _, identities = self.exercise(kind, poll_error=True)
                self.assertIsInstance(error, RuntimeError); self.assertFalse(data['ok'])
                self.assertIsNone(data['exitCode'])
                self.assertFalse(data['processExited' if kind == 'protected' else 'ownedProcessExited'])
                self.assertIn('poll injected', data['error'])
                if kind == 'protected': self.assertTrue(identities[-1])

    def test_primary_survives_poll_failure(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                primary = ValueError('primary validation')
                error, data, polls, _ = self.exercise(kind, primary=primary, poll_error=True)
                self.assertIs(error, primary); self.assertFalse(data['ok'])
                self.assertIn('primary validation', data['error'])
                self.assertTrue(any('poll injected' in x for x in primary.__notes__))
                self.assertEqual(len(polls), 1)

    def test_false_close_bool_forces_failed_receipt(self):
        error, data, _, _ = self.exercise('protected', close_result=False)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(data['ok'])
        self.assertIn('CloseHandle returned false', data['error'])

    def test_combined_faults_keep_primary_and_close_once(self):
        primary = ValueError('primary validation')
        error, data, _, _ = self.exercise('protected', primary=primary, poll_error=True,
                                         identity_error=True, close_error=True)
        self.assertIs(error, primary); self.assertFalse(data['ok'])
        self.assertEqual(len(data['cleanupErrors']), 3)
        self.assertNotIn('cleanupProcessIdentity', data)

    def test_identity_failure_alone_still_closes(self):
        error, data, _, _ = self.exercise('protected', identity_error=True)
        self.assertIsInstance(error, OSError); self.assertFalse(data['ok'])
        self.assertIn('identity injected', data['error'])

    def test_receipt_failure_keeps_primary(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                primary = ValueError('primary validation')
                error, data, _, _ = self.exercise(kind, primary=primary, receipt_error=True)
                self.assertIs(error, primary); self.assertIsNone(data)
                self.assertTrue(any('receipt injected' in x for x in primary.__notes__))

    def test_receipt_failure_without_primary_raises(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                error, data, _, _ = self.exercise(kind, receipt_error=True)
                self.assertIsInstance(error, OSError); self.assertIsNone(data)
                self.assertIn('receipt injected', str(error))

    def test_existing_cleanup_errors_cannot_pass(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                error, data, _, _ = self.exercise(kind, cleanup_errors=['retained cleanup error'])
                self.assertIsInstance(error, RuntimeError); self.assertFalse(data['ok'])
                self.assertIn('retained cleanup error', data['cleanupErrors'])

    def test_running_process_does_not_gain_exit_evidence(self):
        for kind in ('protected', 'bookmarks'):
            with self.subTest(kind=kind):
                error, data, _, _ = self.exercise(kind, running=True)
                self.assertIsInstance(error, RuntimeError); self.assertFalse(data['ok'])
                self.assertIsNone(data['exitCode'])
                self.assertFalse(data['processExited' if kind == 'protected' else 'ownedProcessExited'])


if __name__ == '__main__':
    unittest.main()
