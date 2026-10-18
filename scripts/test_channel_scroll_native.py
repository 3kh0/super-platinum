"""Mocked scroll harness lifecycle checks; no native launch, OS input or sleeps."""
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

_spec = importlib.util.spec_from_file_location(
    "scroll_receipt_runner", Path(__file__).with_name("measure-channel-scroll-native.py"))
scroll = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(scroll)


class ScrollReceiptTests(unittest.TestCase):
    def run_case(self, *, cleanup=False, running=False, poll_error=False,
                 capture_error=False, startup_error=False, cursor_error=False,
                 write_error=False, print_error=False):
        events = []
        primary = RuntimeError("primary sample failure")
        startup = RuntimeError("primary startup failure")
        metrics = {"scroll_frame_ms": [16, 17], "summaries": {"frames": 2}}
        instances = []

        class Fixture:
            def __init__(self, fixture, log, binary, checks):
                self.cleanup_errors = []
                self.process = None
                self.captures = 0
                self.constructor = (fixture, log.name, checks)
                instances.append(self)
            def poll(self):
                events.append("poll")
                if poll_error:
                    raise OSError("poll unavailable")
                return None if running else 0
            def __enter__(self):
                events.append("launch")
                if startup_error:
                    self.close()  # NativeFixture.__enter__ closes on startup error.
                    raise startup
                self.process = SimpleNamespace(pid=123, poll=self.poll)
                return self
            def close(self):
                events.append("fixture_exit")
                if cleanup:
                    self.cleanup_errors.append({"stage": "temporary", "message": "locked"})
            def __exit__(self, *_):
                self.close()
            def control(self, command):
                events.append(("control", command))
                return {"data": {"performance": metrics, "timeline": {"total": 10001},
                                 "profile_hover": None, "screen": "signed_in"}}
            def capture(self, path):
                self.captures += 1
                events.append(("capture", path.name))
                if capture_error:
                    raise primary if self.captures == 1 else RuntimeError("secondary failure capture")

        class Cursor:
            def __init__(self):
                self.data = {"mock": True}
            def __enter__(self):
                events.append("cursor_enter")
                return self
            def check(self, pid, stage):
                self.assert_pid = pid
                events.append(("cursor", stage))
            def __exit__(self, *_):
                events.append("cursor_exit")
                if cursor_error:
                    raise RuntimeError("cursor exit failure")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "fake.exe"
            binary.write_bytes(b"not executable; must never be launched")
            argv = ["scroll", "--binary", str(binary), "--captures", str(root),
                    "--label", "fake", "--runs", "1"]
            raised = None
            original_write = Path.write_text
            def write(path, *args, **kwargs):
                if write_error and path.name == "channel-scroll-fake-1.json":
                    raise OSError("result write unavailable")
                return original_write(path, *args, **kwargs)
            def printed(*_args, **_kwargs):
                if print_error:
                    raise BrokenPipeError("summary output unavailable")
                events.append("summary")
                self.assertTrue(json.loads((root / "channel-scroll-fake-1.json").read_text())["ok"])
            with patch.object(scroll.sys, "argv", argv), patch.object(scroll, "NativeFixture", Fixture), \
                 patch.object(scroll, "ScrollCursor", Cursor), \
                 patch.object(scroll.time, "sleep", lambda n: events.append(("sleep", n))), \
                 patch.object(scroll.time, "monotonic", return_value=0), \
                 patch("builtins.print", printed), patch.object(Path, "write_text", write):
                try:
                    scroll.main()
                except BaseException as error:
                    raised = error
            path = root / "channel-scroll-fake-1.json"
            receipt = json.loads(path.read_text()) if path.exists() else None
        return receipt, events, raised, primary, startup, instances[0]

    def test_success_is_after_closed_contexts_and_preserves_sampling(self):
        receipt, events, error, _, _, fixture = self.run_case()
        self.assertIsNone(error)
        self.assertTrue(receipt["ok"])
        self.assertTrue(receipt["process_exited"])
        self.assertEqual(receipt["cleanup_errors"], [])
        self.assertLess(events.index("fixture_exit"), events.index("cursor_exit"))
        self.assertLess(events.index("cursor_exit"), events.index("poll"))
        self.assertLess(events.index("poll"), events.index("summary"))
        self.assertEqual(events.count("poll"), 1)
        self.assertEqual(fixture.constructor, ("channel-window-10000", "channel-scroll-fake-1.log",
                         {"SUPER_PLATINUM_PERF_SCROLL": "1", "SUPER_PLATINUM_STARTUP_TRACE": "1"}))
        self.assertEqual([e for e in events if isinstance(e, tuple) and e[0] == "sleep"],
                         [("sleep", 4), ("sleep", 1), ("sleep", 3)])
        self.assertEqual([e[1] for e in events if isinstance(e, tuple) and e[0] == "capture"],
                         ["channel-scroll-fake-1.png", "channel-scroll-fake-1-delayed-1s.png",
                          "channel-scroll-fake-1-delayed-3s.png"])
        self.assertEqual(receipt["performance"], {"scroll_frame_ms": [16, 17], "summaries": {"frames": 2}})
        self.assertEqual(receipt["post_sample_state"]["timeline"], {"total": 10001})
        self.assertEqual([r["requested_delay_seconds"] for r in receipt["delayed_diagnostics"]], [1, 3])

    def test_cleanup_running_and_poll_failure_cannot_publish_success(self):
        for options in ({"cleanup": True}, {"running": True}, {"poll_error": True}):
            with self.subTest(options=options):
                receipt, events, error, *_ = self.run_case(**options)
                self.assertIsInstance(error, RuntimeError)
                self.assertIn("did not close cleanly", str(error))
                self.assertFalse(receipt["ok"])
                self.assertNotIn("summary", events)
                self.assertEqual(bool(receipt["cleanup_errors"]), options.get("cleanup", False))
                self.assertEqual(receipt["process_exited"], options.get("cleanup", False))
                if options.get("poll_error"):
                    self.assertEqual(receipt["closure_observation_error"], "OSError: poll unavailable")

    def test_primary_failure_survives_failure_capture_cleanup_and_poll_observation(self):
        receipt, events, error, primary, _, _ = self.run_case(
            capture_error=True, cleanup=True, poll_error=True)
        self.assertIs(error, primary)
        self.assertEqual(receipt["error"], "RuntimeError: primary sample failure")
        self.assertFalse(receipt["ok"])
        self.assertFalse(receipt["process_exited"])
        self.assertTrue(receipt["cleanup_errors"])
        self.assertIn("closure_observation_error", receipt)
        self.assertIn(("capture", "channel-scroll-fake-1-failure.png"), events)
        self.assertNotIn("summary", events)

    def test_startup_and_cursor_exit_failures_keep_failed_receipts(self):
        for options in ({"startup_error": True, "cleanup": True}, {"cursor_error": True}):
            with self.subTest(options=options):
                receipt, events, error, _, startup, _ = self.run_case(**options)
                self.assertFalse(receipt["ok"])
                self.assertNotIn("summary", events)
                if options.get("startup_error"):
                    self.assertIs(error, startup)
                    self.assertFalse(receipt["process_exited"])
                    self.assertTrue(receipt["cleanup_errors"])
                else:
                    self.assertIn("cursor exit failure", str(error))
                    self.assertTrue(receipt["process_exited"])

    def test_result_write_failure_keeps_primary_exception_identity(self):
        receipt, events, error, primary, *_ = self.run_case(capture_error=True, write_error=True)
        self.assertIsNone(receipt)
        self.assertIs(error, primary)
        self.assertTrue(any("Result could not be written" in note for note in error.__notes__))
        self.assertIn("fixture_exit", events)
        self.assertIn("cursor_exit", events)
        self.assertNotIn("summary", events)

    def test_output_failure_without_primary_still_fails(self):
        for options in ({"write_error": True}, {"print_error": True}):
            with self.subTest(options=options):
                _, events, error, *_ = self.run_case(**options)
                self.assertIsInstance(error, OSError)
                self.assertIn("fixture_exit", events)
                self.assertIn("cursor_exit", events)

    def test_real_cursor_cleanup_preserves_primary_and_fails_without_one(self):
        cursor = scroll.ScrollCursor()
        primary = RuntimeError("sample failed")
        cleanup = OSError("cursor restore failed")
        with patch.object(cursor, "close", side_effect=cleanup):
            cursor.__exit__(type(primary), primary, None)
            self.assertIn("cursor restore failed", primary.__notes__[0])
            with self.assertRaises(OSError) as caught:
                cursor.__exit__(None, None, None)
            self.assertIs(caught.exception, cleanup)


if __name__ == "__main__":
    unittest.main()
