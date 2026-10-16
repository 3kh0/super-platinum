"""Actual runner startup/report regressions; no process, Win32 or observer work."""
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = next(p for p in Path(__file__).resolve().parents
            if (p/'Cargo.toml').is_file() and (p/'scripts/native_fixture.py').is_file())
sys.path.insert(0, str(ROOT/'scripts'))
R = runpy.run_path(str(Path(__file__).with_name('check-emoji-retirement-native.py')))
G = R['main'].__globals__


class Startup(unittest.TestCase):
    def test_terminal_precedes_waiting_deadline_and_process_poll(self):
        terminal = [{'ok': False, 'error': 'Invalid private directory'}]
        seen = []
        def receipts(prefix):
            seen.append(prefix)
            return terminal if prefix == R['FAILED'] else [{'waiting': True}]
        native = types.SimpleNamespace(receipts=receipts,
            require_running=lambda: self.fail('failure was not read before polling'))
        with patch.object(R['time'], 'sleep', side_effect=AssertionError('must fail immediately')):
            with self.assertRaisesRegex(ValueError, 'Invalid private directory'):
                R['wait_for_startup'](native, 0)
        self.assertEqual(seen, [R['FAILED']])

    def test_waiting_remains_exact_and_does_not_poll_after_receipt(self):
        expected = [{'nonce': 'exact', 'pid': 47}]
        native = types.SimpleNamespace(
            receipts=lambda prefix: expected if prefix == R['WAITING'] else [],
            require_running=lambda: self.fail('unexpected poll'))
        self.assertIs(R['wait_for_startup'](native, 0), expected)

    def test_failure_after_initial_empty_poll_still_stops_without_second_sleep(self):
        checks = 0
        def receipts(prefix):
            nonlocal checks
            if prefix == R['FAILED']:
                checks += 1
                return [] if checks == 1 else [{'ok': False, 'error': 'late startup failure'}]
            return []
        native = types.SimpleNamespace(receipts=receipts, require_running=lambda: None)
        with patch.object(R['time'], 'sleep') as sleep, patch.object(R['time'], 'monotonic', return_value=1):
            with self.assertRaisesRegex(ValueError, 'late startup failure'):
                R['wait_for_startup'](native, 2)
        sleep.assert_called_once_with(.025)

    def test_no_receipt_retains_deadline_and_process_failure(self):
        native = types.SimpleNamespace(receipts=lambda prefix: [], require_running=lambda: None)
        with patch.object(R['time'], 'monotonic', return_value=2), patch.object(R['time'], 'sleep') as sleep:
            with self.assertRaisesRegex(ValueError, 'Startup deadline'):
                R['wait_for_startup'](native, 1)
            sleep.assert_not_called()
        def exited(): raise RuntimeError('process exited')
        native.require_running = exited
        with self.assertRaisesRegex(RuntimeError, 'process exited'):
            R['wait_for_startup'](native, 100)

    def test_direct_one_exports_absolute_private_directory_before_native_constructor(self):
        sentinel = RuntimeError('mock constructor stops before native')
        captured = []
        def native(fixture, log, binary, env):
            private = Path(env['SUPER_PLATINUM_EMOJI_RETIREMENT_DIR'])
            self.assertTrue(private.is_absolute()); self.assertTrue(private.is_dir())
            self.assertTrue(log.is_absolute())
            self.assertEqual(private.parent, log.parent)
            captured.append(env)
            raise sentinel
        with tempfile.TemporaryDirectory() as temp, patch.dict(G, NativeFixture=native,
                Capabilities=lambda *args: self.fail('must not construct native capabilities')):
            relative = Path(os.path.relpath(Path(temp)/'W1', Path.cwd()))
            self.assertFalse(relative.is_absolute())
            with self.assertRaises(RuntimeError) as caught:
                R['one']('W1', Path('unused.exe'), relative, {})
            self.assertIs(caught.exception, sentinel)
        self.assertEqual(captured[0]['SUPER_PLATINUM_EMOJI_RETIREMENT_PROCESS'], 'W1')


class Series(unittest.TestCase):
    def exercise(self, fail_role=None, returned_failure=False, preflight_failure=False):
        calls = []; original = RuntimeError('exact role failure')
        provenance = {'manifest_sha256': 'frozen'}; gate = {'fixed': True}
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)/'series'
            relative = Path(os.path.relpath(output, Path.cwd()))
            def one(role, binary, folder, build):
                self.assertTrue(folder.is_absolute())
                self.assertEqual(folder, output/role)
                calls.append(role)
                if role == fail_role and not returned_failure: raise original
                return {'process': role, 'ok': role != fail_role}
            seals = 0
            def sealed(*args):
                nonlocal seals
                seals += 1
                if preflight_failure and seals == 2: raise original
                return provenance
            argv = ['runner', '--binary', 'unused.exe', '--build-receipt', 'unused.json',
                    '--overhead-gate', 'unused-gate.json', '--output', str(relative)]
            with patch.object(R['sys'], 'argv', argv), patch.object(R['sys'], 'platform', 'win32'), \
                 patch.dict(G, sealed=sealed, overhead_gate=lambda *args: gate, one=one,
                            aggregate=lambda receipts: {'count': len(receipts)}):
                error = None
                try: R['main']()
                except BaseException as caught: error = caught
            report = json.loads((output/'report.json').read_text(encoding='utf-8'))
        return calls, error, report, original

    def test_failed_first_attempt_remains_listed_no_next_role(self):
        calls, error, report, primary = self.exercise('W1')
        self.assertIs(error, primary); self.assertEqual(calls, ['W1'])
        self.assertFalse(report['ok'])
        self.assertEqual(report['processes'], [{'role': 'W1', 'ok': False}])
        self.assertIn('exact role failure', report['error'])
        self.assertTrue(report['terminalProvenanceCurrent'])

    def test_later_failed_attempt_preserves_prior_success_without_replacement(self):
        calls, error, report, primary = self.exercise('B1')
        self.assertIs(error, primary); self.assertEqual(calls, ['W1', 'B1'])
        self.assertEqual(report['processes'], [{'role': 'W1', 'ok': True}, {'role': 'B1', 'ok': False}])

    def test_returned_failed_receipt_also_stops(self):
        calls, error, report, _ = self.exercise('W1', returned_failure=True)
        self.assertEqual(calls, ['W1']); self.assertIsInstance(error, ValueError)
        self.assertEqual(report['processes'], [{'role': 'W1', 'ok': False}])

    def test_pre_attempt_gate_failure_does_not_invent_attempt(self):
        calls, error, report, primary = self.exercise(preflight_failure=True)
        self.assertIs(error, primary); self.assertEqual(calls, [])
        self.assertEqual(report['processes'], [])

    def test_successful_mock_series_keeps_fixed_order_and_aggregate(self):
        calls, error, report, _ = self.exercise()
        self.assertIsNone(error); self.assertEqual(calls, list(R['ROLES']))
        self.assertTrue(report['ok']); self.assertEqual(report['count'], 6)
        self.assertEqual(report['processes'], [{'role': role, 'ok': True} for role in R['ROLES']])


if __name__ == '__main__': unittest.main()
