"""Closed attribution context admission; no process, credentials or network."""
import copy
from dataclasses import replace
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from canvas_native import (CanvasCheck, capture_stage, context_valid, private_checks,
                           validate, validate_closed)
from native_check import expected_viewport


SPEC = CanvasCheck(
    fixture="canvas-root-attribution", flag="SUPER_PLATINUM_CANVAS_ROOT_ATTRIBUTION_CHECK",
    required=(), counts={}, stages=("editor",), synthetic_context=True,
)


def receipt(fixture=SPEC.fixture):
    return {"ok": True, "ordinal": 0, "stage": "editor", "noSession": False,
            "noTransport": False, "layout": {"ok": True, "viewport": expected_viewport(fixture)},
            "isolation": {"syntheticSession": True, "noHttp": True,
                          "fixture": SPEC.fixture,
                          "account": "147b5480-9f1a-4443-89da-836472cbb209",
                          "team": "T1", "self": "UATTRSELF"}}


class SyntheticContextTests(unittest.TestCase):
    def test_recovery_context_cannot_borrow_attribution_receipts_or_flags(self):
        recovery = replace(SPEC, fixture="canvas-editor-recovery",
                           flag="SUPER_PLATINUM_CANVAS_EDITOR_RECOVERY_CHECK")
        for fixture in (recovery.fixture, recovery.fixture + "-narrow"):
            value = receipt(fixture)
            self.assertFalse(context_valid(recovery, fixture, value))
            value["isolation"]["fixture"] = recovery.fixture
            validate(recovery, fixture, value)
            with tempfile.TemporaryDirectory() as temporary:
                self.assertEqual(private_checks(recovery, Path(temporary))[recovery.flag], "1")
            self.assertFalse(context_valid(SPEC, SPEC.fixture, value))
            foreign = replace(recovery, flag=SPEC.flag)
            self.assertFalse(context_valid(foreign, fixture, value))
            with self.assertRaises(RuntimeError):
                private_checks(foreign, Path("unused"))

    def test_exact_context_at_both_widths(self):
        for fixture in (SPEC.fixture, SPEC.fixture + "-narrow"):
            value = receipt(fixture)
            self.assertTrue(context_valid(SPEC, fixture, value))
            validate(SPEC, fixture, value)

    def test_default_stays_session_and_transport_free(self):
        normal = replace(SPEC, synthetic_context=False)
        self.assertFalse(context_valid(normal, normal.fixture, receipt()))
        self.assertTrue(context_valid(normal, normal.fixture,
                                     {"noSession": True, "noTransport": True}))
        for key in ("noSession", "noTransport"):
            for bad in (False, None, 1, "true"):
                value = {"noSession": True, "noTransport": True, key: bad}
                self.assertFalse(context_valid(normal, normal.fixture, value))

    def test_foreign_fixture_or_flag_rejected_before_launch(self):
        for config in (replace(SPEC, fixture="canvas-text"), replace(SPEC, flag="OTHER"),
                       replace(SPEC, fixture="unknown", flag=None)):
            self.assertFalse(context_valid(config, config.fixture, receipt()))
            with self.assertRaises(RuntimeError):
                private_checks(config, Path("unused"))
        self.assertFalse(context_valid(SPEC, "canvas-text", receipt()))

    def test_every_isolation_field_exact_and_no_credential_extras(self):
        for key in receipt()["isolation"]:
            for bad in (None, False, 1, "foreign"):
                value = receipt()
                value["isolation"][key] = bad
                self.assertFalse(context_valid(SPEC, SPEC.fixture, value), (key, bad))
            value = receipt()
            del value["isolation"][key]
            self.assertFalse(context_valid(SPEC, SPEC.fixture, value), key)
        value = receipt()
        value["isolation"]["credentials"] = "must-not-be-emitted"
        self.assertFalse(context_valid(SPEC, SPEC.fixture, value))
        for key in ("noSession", "noTransport"):
            for bad in (True, None, 0, "false"):
                value = receipt()
                value[key] = bad
                self.assertFalse(context_valid(SPEC, SPEC.fixture, value))

    def test_invalid_context_never_captures_or_acknowledges(self):
        class Native:
            def capture(self, path):
                raise AssertionError("Invalid context reached capture")
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            value = receipt()
            value["isolation"]["noHttp"] = False
            with self.assertRaises(RuntimeError):
                capture_stage(Native(), SPEC, SPEC.fixture, value, 0, folder, folder)
            self.assertFalse((folder / "0.ack").exists())

    def test_closed_log_revalidates_isolation(self):
        value = receipt()
        for mutate in (False, True):
            stage = copy.deepcopy(value)
            if mutate:
                stage["isolation"]["team"] = "TFOREIGN"
            with patch("canvas_native.closed_receipts", side_effect=[[stage], [value]]):
                if mutate:
                    with self.assertRaises(RuntimeError):
                        validate_closed(SPEC, SPEC.fixture, Path("unused"), [stage], value)
                else:
                    validate_closed(SPEC, SPEC.fixture, Path("unused"), [stage], value)


if __name__ == "__main__":
    unittest.main()
