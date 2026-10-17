"""Pure readiness/ordering tests; never launches a window or touches user storage."""
import sys
sys.dont_write_bytecode = True
from pathlib import Path
import contextlib
import json
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = next(p for p in HERE.parents if (p / "scripts/native_fixture.py").is_file())
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(HERE))
import canvas_native as runner
import canvas_readiness as ready

SPEC = runner.CanvasCheck("canvas-comments", "SUPER_PLATINUM_CANVAS_COMMENTS_CHECK", (), {}, (), readiness=True)
OWNER = dict(version=1, nonce="a" * 32, fixture="canvas-comments")
EXPECTED = dict(OWNER, pid=321)


class Handshake(unittest.TestCase):
    def test_default_off_has_no_files_or_env_change(self):
        with tempfile.TemporaryDirectory() as d:
            checks = {}
            spec = runner.CanvasCheck("canvas-other", "OTHER", (), {}, ())
            self.assertIsNone(ready.prepare(spec, "canvas-other", Path(d), checks))
            self.assertEqual(checks, {})
            self.assertEqual(list(Path(d).iterdir()), [])

    def test_owner_is_fresh_and_nonce_exact(self):
        with tempfile.TemporaryDirectory() as d:
            checks = {}
            owner = ready.prepare(SPEC, SPEC.fixture, Path(d), checks)
            self.assertEqual(json.loads((Path(d) / "reader-owner.json").read_text()), owner)
            self.assertEqual(checks[ready.NONCE], owner["nonce"])
            self.assertRegex(owner["nonce"], r"^[0-9a-f]{32}$")
            with self.assertRaises(RuntimeError):
                ready.prepare(SPEC, SPEC.fixture, Path(d), {})

    def test_exact_tuple_and_all_identity_mutants(self):
        ready.validate_waiting([EXPECTED], EXPECTED)
        for key, value in (("pid", 322), ("pid", True), ("version", True), ("version", 2),
                           ("nonce", "b" * 32), ("fixture", "canvas-comments-narrow"), ("extra", 1)):
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                ready.validate_waiting([{**EXPECTED, key: value}], EXPECTED)
        for values in ([], [EXPECTED, EXPECTED], [{k:v for k,v in EXPECTED.items() if k != "nonce"}]):
            with self.assertRaises(RuntimeError):
                ready.validate_waiting(values, EXPECTED)

    def test_publish_is_atomic_and_exclusive(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            ready.publish(d, EXPECTED)
            self.assertEqual(json.loads((d / "reader-ready.json").read_text()), EXPECTED)
            self.assertFalse((d / "reader-ready.pending").exists())
            with self.assertRaises(FileExistsError):
                ready.publish(d, dict(EXPECTED, pid=322))
            self.assertEqual(json.loads((d / "reader-ready.json").read_text()), EXPECTED)

    def test_early_stage_or_terminal_refuses_release(self):
        for prefix in (SPEC.fixture + "-stage: ", SPEC.fixture + "-check: "):
            with tempfile.TemporaryDirectory() as d:
                native = SimpleNamespace(process=SimpleNamespace(pid=321), require_running=lambda: None,
                    receipts=lambda p: [EXPECTED] if p == ready.PREFIX else ([{}] if p == prefix else []))
                with self.assertRaises(RuntimeError):
                    ready.release(native, SPEC, Path(d), OWNER)
                self.assertFalse((Path(d) / "reader-ready.json").exists())

    def test_missing_request_times_out_without_release(self):
        with tempfile.TemporaryDirectory() as d, patch.object(ready.time, "monotonic", side_effect=[0, 26]):
            native = SimpleNamespace(process=SimpleNamespace(pid=321), require_running=lambda: None, receipts=lambda p: [])
            with self.assertRaises(TimeoutError):
                ready.release(native, SPEC, Path(d), OWNER)
            self.assertFalse((Path(d) / "reader-ready.json").exists())

    def test_closed_log_rejects_late_duplicate_and_early_stage(self):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "app.log"
            waiting = ready.PREFIX + json.dumps(EXPECTED) + "\n"
            stage = SPEC.fixture + '-stage: {}\n'
            log.write_text(waiting + stage)
            ready.validate_closed(log, SPEC, EXPECTED)
            for bad in (waiting + stage + waiting, stage + waiting):
                log.write_text(bad)
                with self.assertRaises(RuntimeError):
                    ready.validate_closed(log, SPEC, EXPECTED)

    def test_actual_run_releases_only_inside_normalized_context(self):
        events = []
        class Native:
            def __init__(self, fixture, log, binary, checks):
                self.log, self.binary = log, binary
                self.process = SimpleNamespace(pid=321)
                self.directory = Path(checks["SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR"])
                self.expected = dict(json.loads((self.directory / "reader-owner.json").read_text()), pid=321)
                self.log.write_text(ready.PREFIX + json.dumps(self.expected) + "\n")
                self.finished = False
            def require_running(self):
                pass
            def receipts(self, prefix):
                if prefix == ready.PREFIX:
                    return [self.expected]
                if prefix.endswith("-check: ") and (self.directory / "reader-ready.json").exists():
                    if not self.finished:
                        with self.log.open("a") as output:
                            output.write(prefix + '{"ok":true}\n')
                        self.finished = True
                    return [{"ok": True}]
                return []
        @contextlib.contextmanager
        def audited(native, evidence):
            events.append("normalized")
            evidence["captures"] = []
            yield SimpleNamespace(checkpoint=lambda _: None, capture=lambda *_: None)
            events.append("cleanup")
        original = ready.release
        def release(*args):
            self.assertEqual(events, ["normalized"])
            events.append("release")
            return original(*args)
        with tempfile.TemporaryDirectory() as d, patch.object(runner, "NativeFixture", Native), \
             patch.object(runner, "audited_fixture", audited), patch.object(runner, "validate", lambda *_: None), \
             patch.object(runner, "validate_closed", lambda *_: None), patch.object(ready, "release", release):
            runner.run(SPEC, SPEC.fixture, Path(d) / "unused-binary", Path(d) / "captures")
            receipt = json.loads((Path(d) / "captures/canvas-comments.json").read_text())
            self.assertTrue(receipt["ok"])
            self.assertEqual(receipt["readiness"]["pid"], 321)
        self.assertEqual(events, ["normalized", "release", "cleanup"])


if __name__ == "__main__":
    unittest.main()
