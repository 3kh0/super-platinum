"""Pure receipt and orchestration tests; no native processes or settings access."""
from contextlib import redirect_stdout
import copy
import io
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

from native_check import expected_viewport

RUNNER = runpy.run_path(str(Path(__file__).with_name("check-notification-presentation-native.py")))
VALIDATE = RUNNER["validate_receipt"]


def receipt(fixture="notification-presentation", platform="win32"):
    viewport = expected_viewport(fixture, platform)
    layout = {"ok": True, "viewport": viewport,
              "dialog": [10, 8, viewport[0] - 10, viewport[1] - 8],
              "scroll": [20, 100, viewport[0] - 20, viewport[1] - 20], "tabTops": [50, 50, 50]}
    macos = platform == "darwin"
    return {"ok": True, **{key: True for key in RUNNER["REQUIRED"]},
            "writes": 10 if macos else 6, "reads": 16 if macos else 11,
            "dockBounce": macos,
            "reached": [{"id": key, "ok": True} for key in
                        RUNNER["CONTROLS"] + (RUNNER["MAC_CONTROLS"] if macos else ())],
            "preview": {"ok": True, "preview": {"name": "been_tree.mp3",
                        "state": "ended", "duration": 1.3, "currentTime": 1.3}},
            "layout": copy.deepcopy(layout), "finalLayout": copy.deepcopy(layout)}


class NotificationPresentationReceipts(unittest.TestCase):
    def test_both_sizes_and_platform_menu_adjustment(self):
        for fixture in RUNNER["FIXTURES"]:
            for platform in ("win32", "darwin", "linux"):
                with self.subTest(fixture=fixture, platform=platform):
                    VALIDATE(fixture, receipt(fixture, platform), platform)
            with self.assertRaises(RuntimeError):
                VALIDATE(fixture, receipt(fixture, "darwin"), "win32")

    def test_every_behavior_flag_must_be_boolean_true(self):
        for key in ("ok", *RUNNER["REQUIRED"]):
            for value in (None, False, 1, "true"):
                bad = receipt()
                bad[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-presentation", bad, "win32")
            bad = receipt()
            del bad[key]
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-presentation", bad, "win32")

    def test_counts_require_exact_integers(self):
        for key, expected in (("writes", 6), ("reads", 11)):
            for value in (None, True, str(expected), float(expected), expected - 1, expected + 1):
                bad = receipt()
                bad[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-presentation", bad, "win32")

    def test_controls_cannot_be_duplicated_omitted_or_failed(self):
        good = receipt()["reached"]
        candidates = [None, {}, good[:-1], good + [good[0]],
                      [good[0]] * 5, list(reversed(good)),
                      [{"id": item["id"], "ok": 1} for item in good],
                      [None, *good[1:]]]
        for reached in candidates:
            bad = receipt()
            bad["reached"] = reached
            with self.subTest(reached=reached), self.assertRaises(RuntimeError):
                VALIDATE("notification-presentation", bad, "win32")

    def test_platform_dock_contract_is_exact_boolean(self):
        for platform in ("win32", "darwin", "linux"):
            for value in (None, 0, 1, "false", platform != "darwin"):
                bad = receipt(platform=platform)
                bad["dockBounce"] = value
                with self.subTest(platform=platform, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-presentation", bad, platform)

    def test_audio_must_be_exact_bundled_preview_with_finite_progress(self):
        for value in (None, {}, {"ok": False}, {"ok": True, "preview": None}):
            bad = receipt()
            bad["preview"] = value
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-presentation", bad, "win32")
        for key, values in (("state", [None, "playing", "error"]),
                            ("name", [None, "b2.mp3"]),
                            ("duration", [None, 0, -1, True, "1", float("nan"), float("inf")]),
                            ("currentTime", [None, 0, -1, True, "1", float("nan"), float("inf")])):
            for value in values:
                bad = receipt()
                bad["preview"]["preview"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-presentation", bad, "win32")

    def test_both_layouts_need_valid_visible_finite_bounds(self):
        for key in ("layout", "finalLayout"):
            for field, value in (("ok", False), ("viewport", [668, 380]),
                                 ("dialog", [-1, 0, 1270, 770]),
                                 ("dialog", [0, 0, 1280, 801]),
                                 ("dialog", [0, 0, float("nan"), 770]),
                                 ("scroll", [20, 100, 1260, 139]),
                                 ("scroll", [0, 100, 1260, 760]),
                                 ("scroll", [20, 100, 1260, float("inf")]),
                                 ("scroll", [True, 100, 1260, 760])):
                bad = receipt()
                bad[key][field] = value
                with self.subTest(key=key, field=field, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-presentation", bad, "win32")
            bad = receipt()
            del bad[key]
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-presentation", bad, "win32")

    def test_single_receipt_cannot_hide_failure_or_duplicate(self):
        validate = RUNNER["validate_receipts"]
        good = receipt()
        failed = {"ok": False, "error": "native assertion failed"}
        for batch in ([], [good, good], [failed, good], [good, failed], [failed]):
            with self.assertRaises(RuntimeError):
                validate("notification-presentation", batch, "win32")
        self.assertIs(validate("notification-presentation", [good], "win32"), good)

    def test_unknown_fixture_is_rejected_before_launch(self):
        with patch.dict(RUNNER["run"].__globals__, {"NativeFixture": lambda *a: self.fail("launched")}):
            with self.assertRaises(ValueError):
                RUNNER["run"]("channel", Path("unused"), Path("unused"))

    def test_success_captures_once_then_records_owned_process_exit(self):
        created = []

        class FakeNative:
            def __init__(self, fixture, log, binary, checks):
                self.fixture, self.checks = fixture, checks
                self.closed, self.captures, self.reads = False, [], 0
                self.process, self.pid = self, 123
                created.append(self)

            def __enter__(self):
                return self

            def __exit__(self, *_):
                self.closed = True

            def require_running(self):
                if self.closed:
                    raise RuntimeError("already closed")

            def receipts(self, prefix):
                self.reads += 1
                return [receipt(self.fixture, RUNNER["sys"].platform)]

            def capture(self, path):
                self.captures.append(path)

            def poll(self):
                return 0 if self.closed else None

        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(RUNNER["run"].__globals__, {"NativeFixture": FakeNative}), redirect_stdout(io.StringIO()):
                RUNNER["run"]("notification-presentation-narrow", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-presentation-narrow.json").read_text())
        self.assertTrue(evidence["ok"])
        self.assertTrue(evidence["processExited"])
        self.assertEqual(evidence["native"]["writes"], 10 if RUNNER["sys"].platform == "darwin" else 6)
        self.assertEqual(evidence["native"]["reads"], 16 if RUNNER["sys"].platform == "darwin" else 11)
        self.assertIs(evidence["localAudioPreviewEnabled"], True)
        self.assertIn("local audio", evidence["localAudioEffect"])
        self.assertEqual(created[0].reads, 2)
        self.assertEqual(len(created[0].captures), 1)
        self.assertEqual(created[0].checks, {RUNNER["FLAG"]: "1"})

    def test_late_receipt_failure_records_failure_and_closes_owned_fixture(self):
        created = []

        class FakeNative:
            def __init__(self, fixture, log, binary, checks):
                self.fixture, self.checks = fixture, checks
                self.closed, self.captured = False, False
                self.process = self
                self.pid = 123
                created.append(self)

            def __enter__(self):
                return self

            def __exit__(self, *_):
                self.closed = True

            def require_running(self):
                pass

            def receipts(self, prefix):
                self.prefix = prefix
                good = receipt(self.fixture, RUNNER["sys"].platform)
                return [good, {"ok": False}] if self.captured else [good]

            def capture(self, _):
                self.captured = True

        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(RUNNER["run"].__globals__, {"NativeFixture": FakeNative}), redirect_stdout(io.StringIO()):
                with self.assertRaises(RuntimeError):
                    RUNNER["run"]("notification-presentation", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-presentation.json").read_text())
        self.assertFalse(evidence["ok"])
        self.assertEqual(len(evidence["nativeReceipts"]), 2)
        self.assertTrue(created[0].closed)
        self.assertEqual(created[0].checks, {RUNNER["FLAG"]: "1"})
        self.assertEqual(created[0].prefix, RUNNER["PREFIX"])

    def test_capture_failure_keeps_original_error_and_closes_owned_fixture(self):
        created = []

        class FakeNative:
            def __init__(self, fixture, *args):
                self.fixture, self.closed, self.captures = fixture, False, 0
                self.process, self.pid = self, 321
                created.append(self)

            def __enter__(self):
                return self

            def __exit__(self, *_):
                self.closed = True

            def require_running(self):
                pass

            def receipts(self, _):
                return [receipt(self.fixture, RUNNER["sys"].platform)]

            def capture(self, _):
                self.captures += 1
                raise RuntimeError("original capture failed" if self.captures == 1
                                   else "failure capture also failed")

        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(RUNNER["run"].__globals__, {"NativeFixture": FakeNative}), redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "original capture failed"):
                    RUNNER["run"]("notification-presentation", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-presentation.json").read_text())
        self.assertFalse(evidence["ok"])
        self.assertEqual(evidence["error"], "original capture failed")
        self.assertTrue(created[0].closed)
        self.assertEqual(created[0].captures, 2)

    def test_startup_failure_is_recorded_without_attempting_global_cleanup(self):
        class FakeNative:
            def __init__(self, *args):
                pass

            def __enter__(self):
                # NativeFixture owns its startup cleanup before raising.
                raise RuntimeError("startup failed")

            def __exit__(self, *_):
                raise AssertionError("context never entered")

        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(RUNNER["run"].__globals__, {"NativeFixture": FakeNative}), redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "startup failed"):
                    RUNNER["run"]("notification-presentation", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-presentation.json").read_text())
        self.assertFalse(evidence["ok"])
        self.assertNotIn("processExited", evidence)
        self.assertEqual(evidence["error"], "startup failed")


if __name__ == "__main__":
    unittest.main()
