"""Runner lifecycle tests: no native process, window, or existing database."""
from contextlib import contextmanager, ExitStack, redirect_stdout
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import canvas_native as C


class LifecycleTests(unittest.TestCase):
    def exercise(self, failure=None, cleanup_error=False):
        events = []
        error = ValueError("original " + (failure or "none"))
        real_temp = tempfile.TemporaryDirectory
        with real_temp(prefix="canvas-lifecycle-test-") as output:
            captures = Path(output) / "captures"
            observed = {}

            @contextmanager
            def private_directory(*args, **kwargs):
                try:
                    with real_temp(*args, **kwargs) as name:
                        observed["directory"] = Path(name)
                        events.append("directory-created")
                        try:
                            yield name
                        finally:
                            events.append("directory-removing")
                            self.assertFalse(observed["controller"].evidence["held"])
                            self.assertIn("audit-exit", events)
                finally:
                    events.append("directory-removed")

            class Control:
                @classmethod
                def prepare(cls, directory, fixture, checks):
                    self = cls()
                    self.directory = directory
                    self.evidence = dict(held=False, finished=False, cleanupErrors=[])
                    observed["controller"] = self
                    events.append("prepare")
                    self.assert_private = lambda: self_test.assertTrue(directory.is_dir())
                    checks["TEST_STORAGE_NONCE"] = "owned"
                    return self

                def __enter__(self):
                    events.append("control-enter")
                    return self

                def bind(self, native):
                    self.assert_private()
                    self_test.assertEqual(native.process.pid, 456)
                    events.append("bind")

                def service(self, native):
                    self.assert_private()
                    events.append("service")
                    self.evidence["held"] = True
                    if failure == "service":
                        raise error

                def finish(self, native):
                    events.append("finish")
                    if failure == "finish":
                        raise error
                    self.evidence.update(held=False, finished=True)

                def close(self):
                    self.assert_private()
                    events.append("close")
                    self.evidence["held"] = False
                    if cleanup_error:
                        self.evidence["cleanupErrors"].append({"stage":"rollback","error":"synthetic"})

                def __exit__(self, kind, exc, tb):
                    self.close()
                    if kind is None and (not self.evidence["finished"] or self.evidence["cleanupErrors"]):
                        raise RuntimeError("storage cleanup incomplete")
                    return False

                def validate_closed(self, native):
                    self.assert_private()
                    events.append("storage-closed")
                    self_test.assertIn("audit-exit", events)
                    self_test.assertIn("close", events)
                    self_test.assertFalse(self.evidence["held"])
                    if failure == "closed":
                        raise error

            self_test = self

            class Native:
                def __init__(self, fixture, log, binary, checks):
                    events.append("native-created")
                    self.fixture, self.log, self.binary = fixture, log, binary
                    self.process = SimpleNamespace(pid=456)
                    self_test.assertEqual(checks["TEST_STORAGE_NONCE"], "owned")
                    log.write_text("closed synthetic log\n")

                def require_running(self):
                    events.append("running")

                def receipts(self, prefix):
                    return [] if prefix.endswith("-stage: ") else [{"ok":True}]

            @contextmanager
            def audit(native, evidence):
                events.append("audit-enter")
                evidence["captures"] = []

                def checkpoint(label):
                    events.append("checkpoint")
                    if failure == "checkpoint":
                        raise error

                def capture(path, label):
                    events.append("capture")
                    if failure == "capture":
                        raise error
                    path.write_bytes(b"mock pixels")

                try:
                    yield SimpleNamespace(checkpoint=checkpoint, capture=capture)
                finally:
                    events.append("audit-exit")
                    self.assertFalse(observed["controller"].evidence["held"])
                    self.assertIn("close", events)
                    self.assertTrue(observed["directory"].is_dir())

            def validate(*args):
                events.append("validate")
                if failure == "validate":
                    raise error

            def validate_closed(*args):
                events.append("native-closed")
                self.assertIn("audit-exit", events)

            spec = C.CanvasCheck("mock-canvas", "SUPER_PLATINUM_TEST", (), {}, ())
            raised = None
            with ExitStack() as stack:
                for name, value in (
                    ("NativeFixture", Native), ("audited_fixture", audit),
                    ("validate", validate), ("validate_closed", validate_closed),
                    ("closed_receipts", lambda *a: []),
                ):
                    stack.enter_context(patch.object(C, name, value))
                stack.enter_context(patch.object(C.tempfile, "TemporaryDirectory", private_directory))
                stack.enter_context(redirect_stdout(io.StringIO()))
                try:
                    C.run(spec, "mock-canvas", Path(output)/"not-launched.exe", captures,
                          storage_factory=Control)
                except BaseException as exc:
                    raised = exc
            evidence = json.loads((captures/"mock-canvas.json").read_text())
            self.assertFalse(observed["directory"].exists())
            self.assertLess(events.index("close"), events.index("audit-exit"))
            self.assertLess(events.index("audit-exit"), events.index("directory-removing"))
            self.assertLess(events.index("directory-removing"), events.index("directory-removed"))
            if failure:
                self.assertIs(raised, error)
                self.assertFalse(evidence["ok"])
                self.assertEqual(evidence["error"], "ValueError: original " + failure)
                self.assertIn("closedStageReceipts", evidence)
                self.assertIn("closedTerminalReceipts", evidence)
            else:
                self.assertIsNone(raised)
                self.assertTrue(evidence["ok"])
            self.assertFalse(evidence["storageControl"]["held"])
            return events, evidence

    def test_success_full_lifecycle_and_persisted_evidence(self):
        events, evidence = self.exercise()
        ordered = ["prepare","native-created","audit-enter","control-enter","bind",
                   "service","validate","finish","capture","close","audit-exit",
                   "native-closed","storage-closed","directory-removing","directory-removed"]
        self.assertEqual([e for e in events if e in ordered], ordered)
        self.assertTrue(evidence["storageControl"]["finished"])
        self.assertTrue(evidence["closedLogRevalidated"])

    def test_held_service_failure_releases_before_native_cleanup(self):
        events, _ = self.exercise("service")
        self.assertNotIn("finish", events)

    def test_held_capture_audit_failure_preserves_error(self):
        self.exercise("checkpoint")

    def test_native_validation_failure_releases_held_lock(self):
        self.exercise("validate")

    def test_finish_failure_releases_held_lock(self):
        self.exercise("finish")

    def test_final_capture_failure_preserves_completed_storage_evidence(self):
        _, evidence = self.exercise("capture")
        self.assertTrue(evidence["storageControl"]["finished"])

    def test_closed_validation_failure_persists_after_cleanup(self):
        self.exercise("closed")

    def test_cleanup_diagnostic_does_not_replace_original_error(self):
        _, evidence = self.exercise("validate", cleanup_error=True)
        self.assertEqual(evidence["storageControl"]["cleanupErrors"],
                         [{"stage":"rollback","error":"synthetic"}])


if __name__ == "__main__":
    unittest.main()
