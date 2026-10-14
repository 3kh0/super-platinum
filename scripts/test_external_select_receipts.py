"""Offline evidence contract checks. No process, UI, or Slack interaction."""
from pathlib import Path
import runpy
import unittest
from native_check import expected_viewport, validate_receipt


class ExternalReceipts(unittest.TestCase):
    def test_each_required_fact_and_exact_request_count(self):
        runner = runpy.run_path(str(Path(__file__).with_name("check-external-select-native.py")))
        for name in ("MESSAGE_SPEC", "MODAL_SPEC"):
            spec = runner[name]
            for suffix in ("", "-narrow"):
                fixture = spec.fixture + suffix
                good = dict.fromkeys(spec.required, True)
                good.update(ok=True, viewport=expected_viewport(fixture), preparedRequests=2)
                validate_receipt(spec, fixture, good)
                for flag in spec.required:
                    for missing in (True, False):
                        bad = dict(good)
                        if missing:
                            del bad[flag]
                        else:
                            bad[flag] = False
                        with self.subTest(fixture=fixture, flag=flag, missing=missing):
                            with self.assertRaises(RuntimeError):
                                validate_receipt(spec, fixture, bad)
                for count in (0, 1, 3):
                    with self.assertRaises(RuntimeError):
                        validate_receipt(spec, fixture, {**good, "preparedRequests": count})


if __name__ == "__main__":
    unittest.main()
