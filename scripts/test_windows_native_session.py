"""Mocked session checks; no GUI, filesystem mutation, input or cleanup."""
import unittest
from unittest.mock import patch
from pathlib import Path

import native_fixture
import windows_native_session as module


class SessionTests(unittest.TestCase):
    def test_fixture_refuses_unusable_session_before_creating_runtime_or_process(self):
        fixture = native_fixture.NativeFixture("channel", "unused.log")
        with patch.object(native_fixture.sys, "platform", "win32"), \
                patch.object(Path, "is_file", return_value=True), \
                patch.object(Path, "mkdir") as mkdir, \
                patch.object(native_fixture.tempfile, "TemporaryDirectory") as temporary, \
                patch.object(native_fixture.subprocess, "Popen") as process, \
                patch.object(module, "require_interactive", side_effect=RuntimeError("session 0")):
            with self.assertRaisesRegex(RuntimeError, "session 0"):
                fixture.__enter__()
        mkdir.assert_not_called()
        temporary.assert_not_called()
        process.assert_not_called()

    def test_session_zero_refused_even_when_a_desktop_handle_opens(self):
        with patch.object(module, "observe", return_value=dict(
                processSession=0, activeConsoleSession=1, inputDesktopOpened=True)):
            with self.assertRaisesRegex(RuntimeError, "interactive Windows"):
                module.require_interactive()

    def test_inaccessible_input_desktop_refused(self):
        with patch.object(module, "observe", return_value=dict(
                processSession=1, activeConsoleSession=1, inputDesktopOpened=False,
                inputDesktopError=5)):
            with self.assertRaisesRegex(RuntimeError, "inputDesktopError"):
                module.require_interactive()

    def test_usable_remote_session_allowed_without_requiring_physical_console(self):
        result = dict(processSession=3, activeConsoleSession=1, inputDesktopOpened=True)
        with patch.object(module, "observe", return_value=result):
            self.assertEqual(module.require_interactive(), result)

    def test_failed_session_probe_does_not_become_an_interactive_pass(self):
        with patch.object(module, "observe", side_effect=OSError("session query failed")):
            with self.assertRaisesRegex(OSError, "session query failed"):
                module.require_interactive()


if __name__ == "__main__":
    unittest.main()
