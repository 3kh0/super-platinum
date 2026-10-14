"""Offline ownership regressions; no process, registry, or notification API runs."""
import unittest
from unittest.mock import Mock, patch

import windows_cold_fixture as fixture


class OwnedProcessTests(unittest.TestCase):
    def setUp(self):
        self.times = {'creation_filetime': 100, 'exit_filetime': 0}
        self.kernel = Mock()
        self.kernel.OpenProcess.return_value = 123
        self.kernel.WaitForSingleObject.return_value = 258
        self.kernel.QueryFullProcessImageNameW.side_effect = self.query
        self.kernel.TerminateProcess.side_effect = self.terminate
        self.api = Mock(kernel=self.kernel)
        self.api.times.side_effect = lambda handle: dict(self.times)
        self.factory = patch.object(fixture, 'WindowsAPI', return_value=self.api)
        self.factory.start()
        self.addCleanup(self.factory.stop)

    def query(self, handle, flags, buffer, size):
        self.assertEqual(handle, 123)
        buffer.value = 'fixture.exe'
        return True

    def terminate(self, handle, code):
        self.assertEqual(handle, 123)
        self.times['exit_filetime'] = 200
        self.kernel.WaitForSingleObject.return_value = 0
        # Models the actual Windows failure: image query unavailable after exit.
        self.kernel.QueryFullProcessImageNameW.side_effect = OSError('WinError 31')
        return True

    def owned(self):
        value = fixture.OwnedProcess(7, 'fixture.exe', {'pid': 7, 'created': 100})
        self.addCleanup(value.close)
        return value

    def test_termination_verifies_exited_retained_handle_without_image_query(self):
        value = self.owned()
        result = value.terminate()
        self.assertEqual(result['exit_filetime'], 200)
        self.assertEqual(result['executable'], 'fixture.exe')
        self.assertTrue(result['process_signaled'])
        self.assertEqual(result['executable_verification'],
                         'verified-live-path-retained-handle-after-exit')
        self.assertTrue(value.exited())
        value.terminate()  # Idempotent cleanup must not query or terminate again.
        self.kernel.QueryFullProcessImageNameW.assert_called_once()
        self.kernel.TerminateProcess.assert_called_once_with(123, 0)
        self.kernel.OpenProcess.assert_called_once()

    def test_already_exited_cannot_establish_ownership(self):
        self.terminate(123, 0)
        with self.assertRaisesRegex(RuntimeError, 'before its executable was verified'):
            self.owned()
        self.kernel.QueryFullProcessImageNameW.assert_not_called()
        self.kernel.CloseHandle.assert_called_once_with(123)

    def test_failed_initial_query_does_not_cache_path(self):
        self.kernel.QueryFullProcessImageNameW.side_effect = None
        self.kernel.QueryFullProcessImageNameW.return_value = False
        with patch.object(fixture.C, 'get_last_error', return_value=31, create=True), \
                patch.object(fixture.C, 'WinError', return_value=OSError('WinError 31'), create=True):
            with self.assertRaisesRegex(OSError, 'Initial live process executable query failed') as caught:
                self.owned()
        self.assertIn('31', str(caught.exception.__cause__))
        self.kernel.CloseHandle.assert_called_once_with(123)

    def test_wrong_executable_never_authorizes_termination(self):
        with self.assertRaisesRegex(RuntimeError, 'executable path does not match'):
            fixture.OwnedProcess(7, 'different.exe')
        self.kernel.TerminateProcess.assert_not_called()

    def test_exit_during_first_query_does_not_cache_path(self):
        def exit_during_query(*args):
            self.query(*args)
            self.terminate(123, 0)
            return True
        self.kernel.QueryFullProcessImageNameW.side_effect = exit_during_query
        with self.assertRaisesRegex(RuntimeError, 'remain alive through initial'):
            self.owned()
        self.kernel.CloseHandle.assert_called_once_with(123)

    def test_creation_mismatch_after_exit_is_rejected(self):
        value = self.owned()
        self.terminate(123, 0)
        self.times['creation_filetime'] = 101
        with self.assertRaisesRegex(RuntimeError, 'creation identity changed'):
            value.verify()

    def test_signaled_without_exit_time_is_rejected(self):
        value = self.owned()
        self.kernel.WaitForSingleObject.return_value = 0
        with self.assertRaisesRegex(RuntimeError, 'matching creation and exit times'):
            value.verify()

    def test_failed_wait_is_not_exit_proof(self):
        value = self.owned()
        self.kernel.WaitForSingleObject.return_value = 0xffffffff
        with patch.object(fixture.C, 'get_last_error', return_value=6, create=True), \
                patch.object(fixture.C, 'WinError', return_value=OSError('invalid handle'), create=True):
            with self.assertRaisesRegex(OSError, 'wait failed'):
                value.verify()


if __name__ == '__main__':
    unittest.main()
