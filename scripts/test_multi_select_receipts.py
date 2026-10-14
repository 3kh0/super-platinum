"""Offline receipt checks: compact triggers must be proven by both native runners."""
import runpy
from pathlib import Path
import unittest

from native_check import expected_viewport, validate_receipt


class CompactTriggerReceipts(unittest.TestCase):
    def test_both_runners_require_compact_geometry_in_both_sizes(self):
        for kind in ("channel", "conversation"):
            runner = runpy.run_path(str(Path(__file__).with_name(f"check-multi-{kind}-native.py")))
            spec = runner["MESSAGE_SPEC"]
            self.assertIn("compactSingleRowControls", spec.required)
            for suffix in ("", "-narrow"):
                fixture = spec.fixture + suffix
                good = {name: True for name in spec.required}
                good.update(ok=True, preparedRequests=4, viewport=expected_viewport(fixture))
                validate_receipt(spec, fixture, good)
                for value in (None, False):
                    with self.subTest(kind=kind, suffix=suffix, compact=value):
                        bad = dict(good)
                        if value is None:
                            del bad["compactSingleRowControls"]
                        else:
                            bad["compactSingleRowControls"] = value
                        with self.assertRaises(RuntimeError):
                            validate_receipt(spec, fixture, bad)


if __name__ == "__main__":
    unittest.main()
