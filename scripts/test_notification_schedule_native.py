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

RUNNER = runpy.run_path(str(Path(__file__).with_name("check-notification-schedule-native.py")))
VALIDATE = RUNNER["validate_receipt"]


def receipt(fixture="notification-schedule", platform="win32"):
    viewport = expected_viewport(fixture, platform)
    layout = {"ok": True, "viewport": viewport,
              "dialog": [10, 8, viewport[0] - 10, viewport[1] - 8],
              "scroll": [20, 100, viewport[0] - 20, viewport[1] - 20], "tabTops": [50, 50, 50]}
    return {"ok": True, **{key: True for key in RUNNER["REQUIRED"]},
            "writes": 8, "reads": 10,
            "allSevenDays": [{"day": key, "ok": True} for key in RUNNER["DAYS"]],
            "fixedChrome": {"ok": True},
            "layout": copy.deepcopy(layout), "finalLayout": copy.deepcopy(layout)}


class NotificationScheduleReceipts(unittest.TestCase):
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
                    VALIDATE("notification-schedule", bad, "win32")
            bad = receipt()
            del bad[key]
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-schedule", bad, "win32")

    def test_counts_require_exact_integers(self):
        for key, expected in (("writes", 8), ("reads", 10)):
            for value in (None, True, str(expected), float(expected), expected - 1, expected + 1):
                bad = receipt()
                bad[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-schedule", bad, "win32")

    def test_days_cannot_be_duplicated_omitted_or_failed(self):
        good = receipt()["allSevenDays"]
        candidates = [None, {}, good[:-1], good + [good[0]],
                      [good[0]] * 7, list(reversed(good)),
                      [{"day": item["day"], "ok": 1} for item in good],
                      [None, *good[1:]]]
        for reached in candidates:
            bad = receipt()
            bad["allSevenDays"] = reached
            with self.subTest(reached=reached), self.assertRaises(RuntimeError):
                VALIDATE("notification-schedule", bad, "win32")

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
                    VALIDATE("notification-schedule", bad, "win32")
            bad = receipt()
            del bad[key]
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-schedule", bad, "win32")

    def test_fixed_chrome_and_settings_tabs_are_required(self):
        for value in (None, {}, {"ok": False}, {"ok": 1}):
            bad = receipt()
            bad["fixedChrome"] = value
            with self.assertRaises(RuntimeError):
                VALIDATE("notification-schedule", bad, "win32")
        for key in ("layout", "finalLayout"):
            for value in (None, [], [50, 50], [50, 51, 50], [float("nan")] * 3,
                          [True] * 3, [-1] * 3, [100] * 3):
                bad = receipt()
                bad[key]["tabTops"] = value
                with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                    VALIDATE("notification-schedule", bad, "win32")

    def test_single_receipt_cannot_hide_failure_or_duplicate(self):
        validate = RUNNER["validate_receipts"]
        good = receipt()
        failed = {"ok": False, "error": "native assertion failed"}
        for batch in ([], [good, good], [failed, good], [good, failed], [failed]):
            with self.assertRaises(RuntimeError):
                validate("notification-schedule", batch, "win32")
        self.assertIs(validate("notification-schedule", [good], "win32"), good)

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
                RUNNER["run"]("notification-schedule-narrow", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-schedule-narrow.json").read_text())
        self.assertTrue(evidence["ok"])
        self.assertTrue(evidence["processExited"])
        self.assertEqual(evidence["native"]["writes"], 8)
        self.assertEqual(evidence["native"]["reads"], 10)
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
                    RUNNER["run"]("notification-schedule", Path("unused"), Path(tmp))
            evidence = json.loads((Path(tmp) / "notification-schedule.json").read_text())
        self.assertFalse(evidence["ok"])
        self.assertEqual(len(evidence["nativeReceipts"]), 2)
        self.assertTrue(created[0].closed)
        self.assertEqual(created[0].checks, {RUNNER["FLAG"]: "1"})
        self.assertEqual(created[0].prefix, RUNNER["PREFIX"])


if __name__ == "__main__":
    unittest.main()
