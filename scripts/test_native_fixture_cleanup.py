"""Cleanup regressions: mocked processes/deletion plus one owned empty temp; no native launches."""
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import native_fixture as module


def windows_error(code):
    value = OSError(f"Windows cleanup error {code}")
    value.winerror = code
    return value


class CleanupTests(unittest.TestCase):
    def test_real_stdlib_explicit_cleanup_after_finalizer_detach(self):
        value = module.NativeFixture("test", "unused.log")
        temporary = tempfile.TemporaryDirectory(prefix="sp-fixture-cleanup-test-")
        path = Path(temporary.name).resolve(strict=True)
        info = path.stat()
        value._temporary = value._temporary_owner = temporary
        value._temporary_path = path
        value._temporary_name = temporary.name
        value._temporary_identity = (info.st_dev, info.st_ino)
        temporary._finalizer.detach()
        self.assertFalse(temporary._finalizer.alive)
        value.close()  # Actual deletion is guarded by the captured identity.
        self.assertFalse(path.exists())
        value.close()

    def fixture(self, errors=()):
        value = module.NativeFixture("test", "unused.log")
        # Never create or delete this path. All recursive cleanup is mocked.
        path = (Path(tempfile.gettempdir()) / "sp-fixture-mocked-owned").resolve()
        self.assertFalse(path.exists())
        temporary = Mock(name=str(path))
        temporary.name = str(path)
        temporary.cleanup.side_effect = list(errors) + [None]
        value._temporary = value._temporary_owner = temporary
        value._temporary_path = path
        value._temporary_name = str(path)
        value._temporary_identity = (1, 2)
        value._output = io.StringIO()
        value.process = Mock()
        value.process.poll.return_value = 0
        return value, temporary

    def test_windows_145_retries_owned_path_then_succeeds(self):
        value, temporary = self.fixture([windows_error(145)])
        with patch.object(module.sys, "platform", "win32"), patch.object(module.time, "sleep") as sleep:
            value.close()
        self.assertEqual(temporary.cleanup.call_count, 2)
        sleep.assert_called_once_with(.1)
        self.assertEqual(value.cleanup_errors, [])
        self.assertIsNone(value._temporary)
        value.process.terminate.assert_not_called()
        value.process.kill.assert_not_called()
        value.close()  # No repeated deletion.
        self.assertEqual(temporary.cleanup.call_count, 2)

    def test_exhaustion_retains_exact_error_record_and_existing_warning_policy(self):
        value, temporary = self.fixture([windows_error(145)])
        with patch.object(module.sys, "platform", "win32"), patch.object(module.time, "monotonic", side_effect=[0, 3]), self.assertWarns(RuntimeWarning):
            value.close()
        self.assertEqual(temporary.cleanup.call_count, 1)
        self.assertEqual(value.cleanup_errors, [{"stage": "temporary", "type": "OSError", "message": "Windows cleanup error 145", "path": str(value._temporary_path), "attempts": 1}])
        self.assertIsNone(value._temporary)

    def test_exit_preserves_exact_primary_exception_with_secondary_details(self):
        value, _ = self.fixture([windows_error(123)])
        original = RuntimeError("Owned title bar hit test failed or timed out")
        with self.assertRaises(RuntimeError) as caught:
            try:
                raise original
            except RuntimeError as error:
                value.__exit__(type(error), error, error.__traceback__)
                raise
        self.assertIs(caught.exception, original)
        self.assertIn("Windows cleanup error 123", original.__notes__[0])
        self.assertEqual(value.cleanup_errors[0]["stage"], "temporary")

    def test_cleanup_only_unexpected_error_remains_a_failure(self):
        value, _ = self.fixture([windows_error(123)])
        with self.assertRaises(OSError):
            value.__exit__(None, None, None)
        self.assertEqual(len(value.cleanup_errors), 1)

    def test_non_windows_directory_not_empty_is_not_retried(self):
        value, temporary = self.fixture([windows_error(145)])
        with patch.object(module.sys, "platform", "linux"), patch.object(module.time, "sleep") as sleep, self.assertRaises(OSError):
            value.close()
        sleep.assert_not_called()
        self.assertEqual(temporary.cleanup.call_count, 1)

    def test_changed_path_or_owner_never_invokes_recursive_cleanup(self):
        for change in ("name", "owner", "resolution", "identity"):
            with self.subTest(change=change):
                value, temporary = self.fixture()
                if change == "name":
                    temporary.name = str(Path(tempfile.gettempdir()))
                if change == "owner":
                    value._temporary_owner = object()
                resolve = patch.object(Path, "resolve", return_value=Path(tempfile.gettempdir())) if change == "resolution" else patch.object(value, "fixture", "test")
                stat = patch.object(Path, "stat", return_value=Mock(st_dev=999, st_ino=999)) if change == "identity" else patch.object(value, "fixture", "test")
                with resolve, stat, self.assertRaises(RuntimeError):
                    value.close()
                temporary.cleanup.assert_not_called()

    def test_path_is_verified_again_before_each_retry(self):
        value, temporary = self.fixture()
        def changed():
            temporary.name = str(Path(tempfile.gettempdir()))
            raise windows_error(145)
        temporary.cleanup.side_effect = changed
        with patch.object(module.sys, "platform", "win32"), patch.object(module.time, "sleep"), self.assertRaises(RuntimeError):
            value.close()
        self.assertEqual(temporary.cleanup.call_count, 1)

    def test_owned_process_failure_preserves_primary_and_does_not_delete_live_scratch(self):
        value, temporary = self.fixture()
        value.process.poll.return_value = None
        value.process.terminate.side_effect = OSError("owned termination failed")
        primary = TimeoutError("primary timeout")
        value.__exit__(TimeoutError, primary, None)
        self.assertEqual(value.cleanup_errors[0]["stage"], "process")
        temporary.cleanup.assert_not_called()
        value.process.kill.assert_not_called()

    def test_failed_enter_preserves_primary_and_detaches_implicit_cleanup(self):
        value, temporary = self.fixture([windows_error(145)])
        temporary._finalizer = Mock()
        primary = RuntimeError("agent endpoint startup failed")
        with patch.object(Path, "is_file", return_value=True), patch.object(Path, "mkdir"), patch.object(Path, "resolve", return_value=value._temporary_path), patch.object(Path, "stat", return_value=Mock(st_dev=1, st_ino=2)), patch.object(Path, "open", return_value=io.StringIO()), patch.object(module.tempfile, "TemporaryDirectory", return_value=temporary), patch.object(module.subprocess, "Popen", return_value=value.process), patch.object(value, "require_running", side_effect=primary), patch.object(module.sys, "platform", "linux"):
            with self.assertRaises(RuntimeError) as caught:
                value.__enter__()
        self.assertIs(caught.exception, primary)
        temporary._finalizer.detach.assert_called_once()
        self.assertEqual(value.cleanup_errors[0]["stage"], "temporary")


if __name__ == "__main__":
    unittest.main()
