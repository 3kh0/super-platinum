"""Mocked full-run cleanup regressions; no processes, Win32 calls or screenshots."""
import contextlib
import io
import json
from pathlib import Path
import runpy
import tempfile
import types
import unittest
from unittest.mock import patch

R = runpy.run_path(str(Path(__file__).with_name('check-emoji-native.py')))


class Cleanup(unittest.TestCase):
    def exercise(self, *, cleanup=None, close_result=True, close_error=None,
                 identity_error=None, identity_change=None, body_error=None,
                 remains_running=False, poll_error=None):
        events, closed, owned = [], [], []
        class Native:
            def __init__(self, fixture, log, binary, env):
                self.log, self.cleanup_errors, self.exited = log, [], False
                self.poll_calls = 0
                def poll():
                    self.poll_calls += 1
                    if poll_error and (body_error or self.poll_calls >= 2): raise poll_error
                    return 0 if self.exited else None
                self.process = types.SimpleNamespace(pid=123, poll=poll)
                self.expected = R['ready_tuple'](fixture, env['SUPER_PLATINUM_EMOJI_NATIVE_NONCE'], 123)
                owned.append(self)
            def __enter__(self):
                events.append('enter'); return self
            def __exit__(self, *args):
                events.append('exit'); self.exited = not remains_running
                self.cleanup_errors = list(cleanup or [])
            def require_running(self):
                if self.exited: raise RuntimeError('already exited')
            def receipts(self, prefix):
                if prefix == R['WAITING']: return [self.expected]
                if prefix == R['PREFIX']: return [{'ok': True}]
                return []
            def capture(self, path):
                self.require_running(); events.append('failure-capture')
        def normalize(native, evidence, stack, *args):
            evidence['pointer'] = {}
            def restore():
                native.require_running(); events.append('restore')
            stack.callback(restore)
            return types.SimpleNamespace(check=lambda *args, **kwargs: None)
        calls = 0
        def times(handle):
            nonlocal calls
            self.assertEqual(handle, 42)
            calls += 1
            final = calls == (2 if body_error or remains_running else 3)
            if final and identity_error: raise identity_error
            value = dict(creation_filetime=10, exit_filetime=20 if owned[0].exited else 0)
            if final and identity_change: value.update(identity_change)
            events.append('identity-final' if final else 'identity')
            return value
        def close(handle):
            closed.append(handle); events.append('close')
            if close_error: raise close_error
            return close_result
        api = types.SimpleNamespace(open=lambda pid: 42, times=times,
                                    kernel=types.SimpleNamespace(CloseHandle=close))
        def validate(*args, **kwargs):
            if body_error: raise body_error
            return {}
        mocked = dict(contract=lambda fixture: None, NativeFixture=Native,
                      binary_sha=lambda path: 'digest', normalize=normalize,
                      publish_ready=lambda *args: None, validate_native=validate,
                      validate_closed_log=lambda *args: {}, validate_pointer_cleanup=lambda p: None)
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()), \
             patch.object(R['sys'], 'platform', 'win32'), \
             patch.dict(sys_modules(), windows_process_metrics=types.SimpleNamespace(WindowsAPI=lambda: api)), \
             patch.dict(R['run'].__globals__, mocked):
            root = Path(temp); caught = None
            try: R['run']('emoji-native', root/'unused.exe', root)
            except BaseException as error: caught = error
            receipt = json.loads((root/'emoji-native/emoji-native.json').read_text())
        self.assertEqual(closed, [42], 'retained handle must close exactly once')
        self.assertLess(events.index('restore'), events.index('exit'))
        self.assertLess(events.index('exit'), events.index('close'))
        return caught, receipt

    def test_clean_exit_retains_exact_identity_and_one_successful_close(self):
        error, r = self.exercise()
        self.assertIsNone(error); self.assertTrue(r['ok']); self.assertEqual(r['cleanupErrors'], [])
        self.assertEqual(r['cleanupProcessIdentity'], r['exitIdentity'])
        self.assertEqual(r['cleanupProcessIdentity'], dict(creation_filetime=10, exit_filetime=20))
        self.assertTrue(r['closedLogRevalidated']); self.assertTrue(r['processExited'])

    def test_recorded_cleanup_error_cannot_leave_success(self):
        details = [{'stage': 'temporary', 'message': 'retained cleanup failure'}]
        error, r = self.exercise(cleanup=details)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertEqual(r['cleanupErrors'], details); self.assertIn('cleanup', r['error'])

    def test_false_bool_close_blocks_success_and_retains_identity(self):
        error, r = self.exercise(close_result=0)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertIn('CloseHandle returned false', r['error'])
        self.assertEqual(r['cleanupProcessIdentity'], r['exitIdentity'])

    def test_close_exception_records_failure_in_written_receipt(self):
        error, r = self.exercise(close_error=OSError('close failed'))
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertTrue(any('close failed' in str(v) for v in r['cleanupErrors']))

    def test_identity_query_failure_still_closes_once(self):
        error, r = self.exercise(identity_error=OSError('times failed'))
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertEqual(r['cleanupIdentityError'], 'times failed')

    def test_changed_creation_or_exit_identity_cannot_leave_success(self):
        for change in [dict(creation_filetime=11), dict(exit_filetime=0), dict(exit_filetime=21)]:
            with self.subTest(change=change):
                error, r = self.exercise(identity_change=change)
                self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
                self.assertTrue(any('identity cleanup' in str(v) for v in r['cleanupErrors']))

    def test_primary_failure_survives_cleanup_and_close_failure(self):
        primary = RuntimeError('original native validator failure')
        error, r = self.exercise(body_error=primary, cleanup=[{'stage': 'temporary'}],
                                 close_result=False, identity_error=OSError('late times failed'))
        self.assertIs(error, primary); self.assertFalse(r['ok'])
        self.assertEqual(r['error'], 'RuntimeError: original native validator failure')
        self.assertEqual(len(r['cleanupErrors']), 3)

    def test_nonexited_owned_process_cannot_leave_success(self):
        error, r = self.exercise(remains_running=True)
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertFalse(r['processExited'])

    def test_final_poll_failure_still_writes_receipt_and_closes_once(self):
        error, r = self.exercise(poll_error=OSError('poll failed'))
        self.assertIsInstance(error, RuntimeError); self.assertFalse(r['ok'])
        self.assertFalse(r['processExited']); self.assertIn('poll failed', r['error'])
        self.assertEqual(r['cleanupProcessIdentity'], r['exitIdentity'])

    def test_final_poll_failure_preserves_primary_and_still_closes_once(self):
        primary = RuntimeError('original native failure')
        error, r = self.exercise(body_error=primary, poll_error=OSError('poll failed'))
        self.assertIs(error, primary); self.assertFalse(r['ok'])
        self.assertEqual(r['error'], 'RuntimeError: original native failure')
        self.assertTrue(any('poll failed' in str(v) for v in r['cleanupErrors']))
        self.assertEqual(r['cleanupProcessIdentity'], dict(creation_filetime=10, exit_filetime=20))

    def test_staged_contract_still_refuses_before_any_native_construction(self):
        if Path(__file__).resolve().parent == R['SCRIPTS']:
            return
        with tempfile.TemporaryDirectory() as temp, patch.dict(R['run'].__globals__,
                NativeFixture=lambda *args: self.fail('staged runner launched')):
            with self.assertRaisesRegex(RuntimeError, 'before promotion'):
                R['run']('emoji-native', Path('unused.exe'), Path(temp))
            self.assertEqual(list(Path(temp).iterdir()), [])


def sys_modules():
    import sys
    return sys.modules


if __name__ == '__main__': unittest.main()
