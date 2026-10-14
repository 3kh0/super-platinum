"""Offline receipt regressions only; never launches an application."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "thread_window_runner", Path(__file__).with_name("check-thread-window-native.py"))
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


def receipt(fixture):
    total = RUNNER.TOTALS[fixture]
    count = 200 if fixture.endswith("-200") else 2000 if fixture.endswith("-2000") else 10000
    return {
        "ok": True, "total": total, "initial": 102, "initialBounds": {"atBottom": True},
        "jumps": [{"fraction": fraction, "rows": 101,
                   "bounds": {"first": "1719800400.000000",
                              "last": f"{1719800400 + count}.000000", "atBottom": True}}
                  for fraction in (0, .25, .5, .75, 1)],
        "native_state": {"start": total - 100, "end": total},
    }


class ThreadReceiptTests(unittest.TestCase):
    def test_exact_fixture_totals_include_filtered_history_gap(self):
        for fixture in RUNNER.FIXTURES:
            with self.subTest(fixture=fixture):
                RUNNER.validate(fixture, receipt(fixture))
        bad = receipt("thread-window-gap")
        bad["total"] = 10001
        with self.assertRaises(RuntimeError):
            RUNNER.validate("thread-window-gap", bad)

    def test_missing_wrong_totals_and_native_ranges_fail(self):
        for fixture in RUNNER.FIXTURES:
            for mutate in (
                lambda r: r.pop("total"),
                lambda r: r.update(total=r["total"] - 1),
                lambda r: r["native_state"].update(start=True),
                lambda r: r["native_state"].update(start=-1),
                lambda r: r["native_state"].update(start=r["total"]),
                lambda r: r["native_state"].update(end=r["total"] + 1),
                lambda r: r["native_state"].update(start=r["total"] - 101),
                lambda r: r["jumps"][-1].update(rows=100),
            ):
                bad = copy.deepcopy(receipt(fixture))
                mutate(bad)
                with self.subTest(fixture=fixture, receipt=bad), self.assertRaises(RuntimeError):
                    RUNNER.validate(fixture, bad)

    def test_forced_root_and_pending_are_deduplicated_against_window(self):
        fixture = "thread-window-200"
        for start, end, mounted in ((0, 100, 101), (20, 100, 82), (101, 201, 101)):
            valid = receipt(fixture)
            valid["native_state"].update(start=start, end=end)
            valid["jumps"][-1]["rows"] = mounted
            RUNNER.validate(fixture, valid)


if __name__ == "__main__":
    unittest.main()
