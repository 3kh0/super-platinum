"""Offline evidence contract checks. No process, UI, or Slack interaction."""
from pathlib import Path
import copy
import json
import runpy
import unittest
from native_check import expected_viewport, validate_receipt


class ExternalReceipts(unittest.TestCase):
    def test_raw_attempts_keep_encoded_state_options_and_siblings(self):
        runner = runpy.run_path(str(Path(__file__).with_name("check-external-select-native.py")))
        validate = runner["validate_attempts"]
        for modal in (False, True):
            fixture = "app-modal-external-select" if modal else "message-external-select"
            attempts = []
            for index in range(2):
                option = None if modal and index == 0 else {
                    "text": {"type": "plain_text", "text": "Release Beta" if modal else "Release Alpha"},
                    "value": "beta" if modal else "alpha,desktop",
                    "description": {"type": "plain_text", "text": "App-provided release option"},
                }
                values = {"choice": {"pick": {"selected_option": option}},
                          "note": {"value": {"value": "Keep this sibling"}}} if modal else {
                    "choice": {"pick": {"selected_option": option}},
                    "review": {"environment": {"selected_option": {"value": "prod"}}},
                }
                attempt = {"state": {"values": values} if modal else values,
                           "fields": {"state": json.dumps({"values": values})}}
                if not modal:
                    action = {"type": "external_select", "selected_option": option} if index == 0 else {"type": "button"}
                    attempt.update(action=action)
                    attempt["fields"]["actions"] = json.dumps([action])
                attempts.append(attempt)
            good = {"attempts": attempts}
            validate(fixture, good)
            for invalid in ({}, {"attempts": attempts[:1]}, {"attempts": attempts + attempts[:1]}):
                with self.assertRaises(RuntimeError):
                    validate(fixture, invalid)
            bad = copy.deepcopy(good)
            bad["attempts"][0]["fields"]["state"] = "{}"
            with self.assertRaises(RuntimeError):
                validate(fixture, bad)
            bad = copy.deepcopy(good)
            state = bad["attempts"][1]["state"]
            values = state["values"] if modal else state
            values["choice"]["pick"]["selected_option"]["description"]["text"] = "Reconstructed"
            bad["attempts"][1]["fields"]["state"] = json.dumps(state if modal else {"values": state})
            with self.assertRaises(RuntimeError):
                validate(fixture, bad)
            bad = copy.deepcopy(good)
            state = bad["attempts"][1]["state"]
            values = state["values"] if modal else state
            if modal:
                values["note"]["value"]["value"] = "Lost sibling"
            else:
                values["review"]["environment"]["selected_option"]["value"] = "staging"
            bad["attempts"][1]["fields"]["state"] = json.dumps(state if modal else {"values": state})
            with self.assertRaises(RuntimeError):
                validate(fixture, bad)

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
