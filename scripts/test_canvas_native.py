"""Pure receipt and capture-ack tests; no application or Slack session is opened."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

from canvas_native import capture_stage, validate
from native_check import expected_viewport


def spec(name):
    source = Path(__file__).with_name(f"check-canvas-{name}-native.py")
    definition = importlib.util.spec_from_file_location(f"canvas_{name}_runner", source)
    module = importlib.util.module_from_spec(definition)
    definition.loader.exec_module(module)
    return module.SPEC


class CanvasRunnerTests(unittest.TestCase):
    def test_final_receipts_require_every_flag_count_and_current_platform_viewport(self):
        for name in ("text", "checklist"):
            config = spec(name)
            for fixture in (config.fixture, config.fixture + "-narrow"):
                receipt = {"ok": True, "noSession": True, "noTransport": True,
                           "layout": {"ok": True, "viewport": expected_viewport(fixture)},
                           **config.counts}
                for key in config.required:
                    if key.startswith("layout."):
                        receipt["layout"][key.split(".")[1]] = True
                    else:
                        receipt[key] = True
                validate(config, fixture, receipt)
                for key in (*config.required, "noSession", "noTransport", "layout.ok"):
                    bad = copy.deepcopy(receipt)
                    if key.startswith("layout."):
                        bad["layout"][key.split(".")[1]] = False
                    else:
                        bad[key] = False
                    with self.subTest(fixture=fixture, field=key), self.assertRaises(RuntimeError):
                        validate(config, fixture, bad)
                for key, count in config.counts.items():
                    with self.subTest(fixture=fixture, count=key), self.assertRaises(RuntimeError):
                        validate(config, fixture, {**receipt, key: count + 1})
                bad = copy.deepcopy(receipt)
                bad["layout"]["viewport"] = [1, 1]
                with self.assertRaises(RuntimeError):
                    validate(config, fixture, bad)

    def test_stage_ack_is_written_only_after_successful_capture(self):
        config = spec("text")
        stage = {"ok": True, "ordinal": 0, "stage": config.stages[0],
                 "noSession": True, "noTransport": True}
        with tempfile.TemporaryDirectory(prefix="sp-canvas-test-") as directory:
            directory = Path(directory)
            class Native:
                def capture(self, path):
                    if (directory / "0.ack").exists():
                        raise AssertionError("Acknowledged before capturing")
                    path.write_bytes(b"test-only pixels")
            result = capture_stage(Native(), config, config.fixture, stage, 0, directory, directory)
            self.assertTrue(Path(result["screenshot"]).is_file())
            self.assertEqual((directory / "0.ack").read_text(), "captured\n")

    def test_failed_capture_or_wrong_stage_never_releases_native_check(self):
        config = spec("checklist")
        stage = {"ok": True, "ordinal": 0, "stage": config.stages[0],
                 "noSession": True, "noTransport": True}
        class Native:
            calls = 0
            def capture(self, path):
                self.calls += 1
                raise RuntimeError("Capture failed")
        with tempfile.TemporaryDirectory(prefix="sp-canvas-test-") as directory:
            directory = Path(directory)
            native = Native()
            with self.assertRaises(RuntimeError):
                capture_stage(native, config, config.fixture, stage, 0, directory, directory)
            self.assertEqual(native.calls, 1)
            for key, value in (("ordinal", 1), ("ordinal", False), ("stage", "unknown"), ("ok", False),
                               ("noSession", False), ("noTransport", False)):
                with self.subTest(field=key), self.assertRaises(RuntimeError):
                    capture_stage(native, config, config.fixture, {**stage, key: value}, 0,
                                  directory, directory)
            self.assertEqual(native.calls, 1)
            self.assertFalse((directory / "0.ack").exists())


if __name__ == "__main__":
    unittest.main()
