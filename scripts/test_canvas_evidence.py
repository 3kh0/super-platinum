"""Pure fake lifecycle/receipt tests; never starts a process or invokes Win32."""
from contextlib import ExitStack
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
TARGETS = HERE / "targets/scripts" if (HERE / "targets/scripts").is_dir() else HERE
sys.path.insert(0, str(TARGETS))
import canvas_evidence as e
# Load exact staged runner even if active scripts was inserted by the helper.
spec = importlib.util.spec_from_file_location("candidate_canvas_native", TARGETS / "canvas_native.py")
c = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = c
spec.loader.exec_module(c)


class Native:
    def __init__(self, path, events):
        self.binary = path / "fixture.exe"
        self.log = path / "fixture.log"
        self.cleanup_errors = []
        self.events = events
        self.exited = False
        self.process = types.SimpleNamespace(pid=71, poll=lambda: 0 if self.exited else None)

    def __enter__(self):
        self.events.append("enter")
        return self

    def __exit__(self, *args):
        self.events.append("exit")
        self.exited = True

    def require_running(self):
        if self.exited:
            raise RuntimeError("exited")

    def capture(self, path):
        self.require_running()
        self.events.append("capture:" + Path(path).name)


class Tests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.events = []
        self.native = Native(Path(self.temp.name), self.events)
        self.evidence = {"ok": False}
        self.closed = []
        self.api = types.SimpleNamespace(
            times=lambda handle: {"creation_filetime": 23, "exit_filetime": 31 if self.native.exited else 0},
            kernel=types.SimpleNamespace(CloseHandle=lambda handle: self.closed.append(handle) or True))

    def mocks(self, digest=None):
        stack = ExitStack()
        self.addCleanup(stack.close)
        stack.enter_context(patch.object(e.sys, "platform", "win32"))
        stack.enter_context(patch.object(e, "open_process", return_value=(self.api, 42)))
        stack.enter_context(patch.object(e, "binary_sha", side_effect=digest or ["abc", "abc"]))
        stack.enter_context(patch.object(e, "png_signature", return_value=(100, 80)))

        def normalize(native, evidence, cleanup, *_):
            evidence["pointer"] = {"movement_observed": False, "restore_attempted": True,
                                   "restore_observed": False}
            def restore():
                native.require_running()
                self.events.append("restore")
                evidence["pointer"]["restore_observed"] = True
            cleanup.callback(restore)
            return types.SimpleNamespace(check=lambda label, **kw: self.events.append("check:" + label))
        stack.enter_context(patch.object(e, "normalize", side_effect=normalize))
        return stack

    def test_restores_before_exit_and_retains_exact_identity(self):
        self.mocks()
        with e.audited_fixture(self.native, self.evidence) as audit:
            audit.capture(Path(self.temp.name) / "stage.png", "stage")
        self.assertLess(self.events.index("restore"), self.events.index("exit"))
        self.assertEqual(self.closed, [42])
        self.assertEqual(self.evidence["exitIdentity"], {"creation_filetime": 23, "exit_filetime": 31})
        self.assertEqual(self.evidence["auditErrors"], [])
        self.assertEqual(self.evidence["captures"][0]["label"], "stage")

    def test_actual_failure_survives_cleanup_error_and_has_live_capture(self):
        self.mocks(digest=["abc", "changed"])
        original = RuntimeError("actual native failure")
        with self.assertRaisesRegex(RuntimeError, "actual native failure") as result:
            with e.audited_fixture(self.native, self.evidence):
                raise original
        self.assertIs(result.exception, original)
        self.assertLess(self.events.index("capture:fixture-failure.png"), self.events.index("restore"))
        self.assertEqual(self.evidence["auditErrors"][0]["stage"], "binary")
        self.assertTrue(self.evidence["processExited"])
        self.assertEqual(self.closed, [42])

    def test_binary_change_cannot_pass(self):
        self.mocks(digest=["abc", "changed"])
        with self.assertRaisesRegex(RuntimeError, "Binary changed"):
            with e.audited_fixture(self.native, self.evidence):
                pass

    def test_pointer_restore_failure_preserves_actual_body_error(self):
        self.mocks()
        original = RuntimeError("actual native failure")

        def normalize(native, evidence, cleanup, *_):
            # Even optimistic/misleading data cannot mask the actual close error.
            evidence["pointer"] = {"movement_observed": False, "restore_attempted": True,
                                   "restore_observed": True}
            def restore():
                native.require_running()
                self.events.append("restore-failed")
                raise OSError("pointer restore failed")
            cleanup.callback(restore)
            return types.SimpleNamespace(check=lambda *a, **kw: None)

        with patch.object(e, "normalize", side_effect=normalize):
            with self.assertRaisesRegex(RuntimeError, "actual native failure") as result:
                with e.audited_fixture(self.native, self.evidence):
                    raise original
        self.assertIs(result.exception, original)
        self.assertLess(self.events.index("restore-failed"), self.events.index("exit"))
        self.assertTrue(self.evidence["processExited"])
        self.assertIn("pointer restore failed", self.evidence["pointerCleanupError"])
        self.assertIn("pointer-cleanup", [x["stage"] for x in self.evidence["auditErrors"]])
        self.assertEqual(self.closed, [42])

    def test_replaced_process_refuses_capture(self):
        self.mocks()
        with self.assertRaisesRegex(RuntimeError, "identity changed"):
            with e.audited_fixture(self.native, self.evidence) as audit:
                self.api.times = lambda handle: {"creation_filetime": 99, "exit_filetime": 0}
                audit.capture(Path(self.temp.name) / "wrong.png", "wrong")
        self.assertNotIn("capture:wrong.png", self.events)
        self.assertTrue(self.evidence["auditErrors"])

    def test_failed_handle_close_is_not_silent(self):
        self.mocks()
        self.api.kernel.CloseHandle = lambda handle: False
        with self.assertRaisesRegex(RuntimeError, "process handle"):
            with e.audited_fixture(self.native, self.evidence):
                pass

    def test_cleanup_warning_cannot_be_success(self):
        self.mocks()
        with self.assertRaisesRegex(RuntimeError, "cleanup reported"):
            with e.audited_fixture(self.native, self.evidence):
                self.native.cleanup_errors.append({"stage": "temporary", "message": "locked"})
        self.assertEqual(len(self.evidence["cleanupErrors"]), 1)

    def test_closed_log_rejects_duplicate_late_terminal_and_truncation(self):
        spec = c.CanvasCheck("canvas-structure", "flag", ("done",), {"attempts": 3}, ("initial",))
        final = {"ok": True, "done": True, "attempts": 3, "noSession": True, "noTransport": True,
                 "layout": {"ok": True, "viewport": c.expected_viewport("canvas-structure")}}
        stage = {"ok": True, "ordinal": 0, "stage": "initial", "noSession": True, "noTransport": True}
        lines = "canvas-structure-stage: " + json.dumps(stage) + "\ncanvas-structure-check: " + json.dumps(final) + "\n"
        self.native.log.write_text(lines, encoding="utf8")
        c.validate_closed(spec, "canvas-structure", self.native.log, [{**stage, "screenshot": "x"}], final)
        for bad in [lines + "canvas-structure-check: " + json.dumps(final) + "\n", lines.rstrip("\n")]:
            self.native.log.write_text(bad, encoding="utf8")
            with self.assertRaises(RuntimeError):
                c.validate_closed(spec, "canvas-structure", self.native.log, [stage], final)

    def test_capture_failure_never_publishes_stage_ack(self):
        stage = {"ok": True, "ordinal": 0, "stage": "initial", "noSession": True, "noTransport": True}
        spec = c.CanvasCheck("canvas-structure", "flag", (), {}, ("initial",))
        audit = types.SimpleNamespace(capture=lambda *_: (_ for _ in ()).throw(RuntimeError("capture failed")))
        with self.assertRaisesRegex(RuntimeError, "capture failed"):
            c.capture_stage(self.native, spec, "canvas-structure", stage, 0,
                            Path(self.temp.name), Path(self.temp.name), audit=audit)
        self.assertFalse((Path(self.temp.name) / "0.ack").exists())


if __name__ == "__main__":
    unittest.main()
