"""Pure reading-preference receipt tests; no native launch, session, or network."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-read-preferences-native.py")))
QUERY = ("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
         "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
         "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
         "&fp=2123456789ab4def8123456789abcdef&slack_route=default")


class Receipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = R["FIXTURE"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture = fixture or self.fixture
        value = {"ok": True, "fixture": fixture, "nonce": self.nonce, "pid": self.pid,
                 "noSession": True, "noTransport": True, "syntheticInput": True, "actualPreparedRequests": True,
                 "checks": dict.fromkeys(R["CHECKS"], True), "preparedRequests": 16,
                 "viewport": expected_viewport(fixture), "captures": {}, "geometry": [],
                 "attempts": [], "readRequests": [], "finalRaw": R["raw_preferences"]((True, True))}
        def request(sequence, endpoint, fields):
            path = "/api/" + endpoint
            return dict(sequence=sequence, method="POST", url="https://fixture.slack.com" + path + "?" + QUERY,
                        path=path, rawQuery=QUERY, fields=fields, retrySafe=False)
        for sequence, values in zip((0, 1, 3, 4, 6, 7, 8, 10, 11, 13, 14, 15),
                                    ((True, True), (True, True), (True, False), (True, False), (True, False),
                                     (False, False), (False, False), (False, False), (False, False),
                                     (True, True), ("unsupported", True), (True, True))):
            entry = request(sequence, "users.prefs.get", [["prefs", ",".join(R["PREFS"])]])
            entry["response"] = {"ok": True, "prefs": R["raw_preferences"](values)}
            value["readRequests"].append(entry)
        outcomes = ({"kind": "ack", "response": {"ok": True}, "applied": True},
                    {"kind": "unknown", "applied": False},
                    {"kind": "rate-limited", "status": 429, "retryAfterSeconds": 2, "applied": False},
                    {"kind": "ack", "response": {"ok": True}, "applied": True})
        for sequence, values, outcome in zip((2, 5, 9, 12), ((True, False), (False, False), (True, True), (True, True)), outcomes):
            entry = request(sequence, "users.prefs.set", [["prefs", json.dumps(dict(zip(R["PREFS"], values)))],
                            ["_x_reason", "prefs-api/setMultipleUserPrefsByApi"]])
            entry["outcome"] = outcome
            value["attempts"].append(entry)
        for stage in R["STAGES"]:
            controls = []
            for index, name in enumerate(R["CONTROL_NAMES"]):
                disabled = stage not in ("initial", "newest-read", "confirmed", "restored") if index < 3 else (
                    stage in ("saving", "cooldown") if index == 4 else False)
                selected = {"initial": 0, "saving": 0, "newest-read": 1, "uncertain": 1,
                            "mismatch": 1, "confirmed": 2, "restored": 0}.get(stage)
                controls.append(dict(name=name, rect=dict(x=20, y=20 + index * 30, width=20, height=20),
                                     hit=True, disabled=disabled, checked=(selected == index) if index < 3 else None))
            focused = {"newest-read": 1, "mismatch": 4, "confirmed": 4, "unsupported": 4, "restored": 4}.get(stage)
            value["geometry"].append(dict(ok=True, stage=stage, viewport=value["viewport"],
                documentWidth=value["viewport"][0], dialog=dict(x=8, y=8, width=300, height=300), controls=controls,
                panel=dict(x=10, y=10, width=280, height=280), focusRingContained=True,
                focusRing=None if focused is None else controls[focused]["rect"].copy(),
                focusedControl=None if focused is None else controls[focused]["name"]))
            path = self.root / f"{stage}.png"
            path.write_bytes(png())
            value["captures"][stage] = str(path)
        edges = [{"stage": stage, "edge": edge} for stage in R["STAGES"] for edge in ("before", "after")]
        return value, edges

    def validate(self, value, edges):
        return R["validate_native"](value["fixture"], self.nonce, self.pid, self.root, value, edges)

    def test_normal_and_narrow_complete_evidence(self):
        for fixture in R["FIXTURES"]:
            value, edges = self.good(fixture)
            self.assertEqual(len(self.validate(value, edges)), 10)

    def test_required_claims_cannot_be_missing_false_or_truthy_numbers(self):
        value, edges = self.good()
        for flag in R["REQUIRED"]:
            for bad in (None, False, 1):
                changed = copy.deepcopy(value)
                target, key = (changed["checks"], flag.split(".")[1]) if flag.startswith("checks.") else (changed, flag)
                if bad is None:
                    del target[key]
                else:
                    target[key] = bad
                with self.subTest(flag=flag, bad=bad), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)
        for key, bad in (("preparedRequests", 16.0), ("preparedRequests", True), ("pid", True),
                         ("nonce", "b" * 32), ("viewport", [1, 1])):
            changed = {**value, key: bad}
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_actual_ledger_cannot_be_missing_replayed_or_reordered(self):
        value, edges = self.good()
        for key in ("attempts", "readRequests"):
            for mutation in ("missing", "empty", "extra", "order", "sequence", "boolean-sequence", "retry", "method"):
                changed = copy.deepcopy(value)
                ledger = changed[key]
                if mutation == "missing": del changed[key]
                elif mutation == "empty": changed[key] = []
                elif mutation == "extra": ledger.append(copy.deepcopy(ledger[-1]))
                elif mutation == "order": ledger.reverse()
                elif mutation == "sequence": ledger[1]["sequence"] = ledger[0]["sequence"]
                elif mutation == "boolean-sequence": ledger[0]["sequence"] = False
                elif mutation == "retry": ledger[0]["retrySafe"] = True
                else: ledger[0]["method"] = "GET"
                with self.subTest(key=key, mutation=mutation), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)

    def test_forms_route_and_both_exact_raw_boolean_patch_keys(self):
        value, edges = self.good()
        for mutation in ("numeric", "missing", "string", "extra", "duplicate-json", "duplicate-field", "order", "reason", "host", "path", "query"):
            changed = copy.deepcopy(value)
            entry = changed["attempts"][0]
            if mutation in ("numeric", "missing", "string", "extra"):
                patch = json.loads(entry["fields"][0][1])
                if mutation == "numeric": patch[R["PREFS"][0]] = 1
                elif mutation == "missing": del patch[R["PREFS"][1]]
                elif mutation == "string": patch[R["PREFS"][1]] = "false"
                else: patch["unknown_pref"] = False
                entry["fields"][0][1] = json.dumps(patch)
            elif mutation == "duplicate-json": entry["fields"][0][1] = '{"mark_msgs_read_immediately":false,"mark_msgs_read_immediately":true,"start_scroll_at_oldest":false}'
            elif mutation == "duplicate-field": entry["fields"].append(entry["fields"][0])
            elif mutation == "order": entry["fields"].reverse()
            elif mutation == "reason": entry["fields"][1][1] = "other"
            elif mutation == "host": entry["url"] = entry["url"].replace("fixture.slack.com", "other.slack.com")
            elif mutation == "path": entry["path"] = "/api/users.prefs.get"
            else: entry["rawQuery"] += "&token=unexpected"
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)
        changed = copy.deepcopy(value)
        changed["readRequests"][0]["fields"].append(["prefs", ",".join(R["PREFS"])])
        with self.assertRaises(RuntimeError): self.validate(changed, edges)

    def test_raw_baseline_readback_outcomes_and_unknown_inventory_are_independent(self):
        value, edges = self.good()
        for index in range(12):
            for bad in (1, None, "true", False):
                changed = copy.deepcopy(value)
                raw = changed["readRequests"][index]["response"]["prefs"]
                if type(raw[R["PREFS"][0]]) is type(bad) and raw[R["PREFS"][0]] == bad:
                    del raw[R["PREFS"][0]]
                else: raw[R["PREFS"][0]] = bad
                with self.subTest(index=index, bad=bad), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)
        for index, key, bad in ((0, "response", {"ok": 1}), (1, "applied", 0),
                                 (2, "retryAfterSeconds", 0), (2, "status", 200), (3, "applied", False)):
            changed = copy.deepcopy(value)
            changed["attempts"][index]["outcome"][key] = bad
            with self.subTest(index=index, key=key), self.assertRaises(RuntimeError):
                self.validate(changed, edges)
        for final in ({}, R["raw_preferences"]((1, True)), dict(zip(R["PREFS"], (True, True)))):
            changed = {**value, "finalRaw": final}
            with self.assertRaises(RuntimeError): self.validate(changed, edges)

    def test_geometry_controls_and_stage_sequence_are_bounded_and_complete(self):
        value, edges = self.good()
        for mutation in ("missing", "overflow", "nonfinite", "hit", "selection", "enabled", "boolean", "width", "order"):
            changed = copy.deepcopy(value)
            entry = changed["geometry"][0]
            if mutation == "missing": entry["controls"].pop()
            elif mutation == "overflow": entry["controls"][0]["rect"]["x"] = 10000
            elif mutation == "nonfinite": entry["dialog"]["y"] = float("nan")
            elif mutation == "hit": entry["controls"][0]["hit"] = False
            elif mutation == "selection": entry["controls"][0]["checked"] = False
            elif mutation == "enabled": entry["controls"][0]["disabled"] = True
            elif mutation == "boolean": entry["controls"][0]["checked"] = 1
            elif mutation == "width": entry["documentWidth"] = True
            else: changed["geometry"].reverse()
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)
        for bad in (edges[:-1], edges + edges[-1:], list(reversed(edges))):
            with self.assertRaises(RuntimeError): self.validate(value, bad)

    def test_pending_and_unavailable_stages_cannot_display_a_speculative_radio(self):
        value, edges = self.good()
        for index, entry in enumerate(value["geometry"]):
            if entry["stage"] not in ("saving", "uncertain", "mismatch", "cooldown", "rotation", "unsupported"):
                continue
            changed = copy.deepcopy(value)
            control = changed["geometry"][index]["controls"][0]
            control["checked"] = not control["checked"]
            with self.subTest(stage=entry["stage"]), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_focus_is_preserved_and_outline_containment_is_independent(self):
        value, edges = self.good()
        for mutation in ("missing", "wrong-control", "missing-ring", "clipped"):
            changed = copy.deepcopy(value)
            entry = changed["geometry"][2]
            if mutation == "missing": del entry["panel"]
            elif mutation == "wrong-control": entry["focusedControl"] = "reading-preference-left-off"
            elif mutation == "missing-ring": entry["focusRing"] = None
            else: entry["focusRing"]["y"] = 8
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_closed_log_revalidates_ownership_terminal_edges_and_png_bytes(self):
        value, edges = self.good()
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        lines = [R["WAITING"] + json.dumps(expected)]
        lines += [R["STAGE"] + json.dumps(edge) for edge in edges]
        lines += [R["PREFIX"] + json.dumps(value)]
        path = self.root / "closed.log"
        path.write_text("\n".join(lines) + "\n")
        R["validate_closed_log"](path, expected, self.root, value)
        for changed in (lines + lines[-1:], lines[:-1], lines + lines[:1], lines[:-2] + lines[-1:]):
            path.write_text("\n".join(changed) + "\n")
            with self.assertRaises(RuntimeError):
                R["validate_closed_log"](path, expected, self.root, value)
        path.write_text("\n".join(lines))
        with self.assertRaises(RuntimeError): R["validate_closed_log"](path, expected, self.root, value)
        for bad in (str(self.root / ".." / "initial.png"), str(self.root / "saving.png"), "initial.png"):
            changed = copy.deepcopy(value); changed["captures"]["initial"] = bad
            with self.assertRaises(RuntimeError): self.validate(changed, edges)
        image = Path(value["captures"]["initial"])
        with self.assertRaises(RuntimeError): R["png_signature"](image, [99, 200])
        image.write_bytes(image.read_bytes()[:-12])
        with self.assertRaises(RuntimeError): self.validate(value, edges)

    def test_raw_boolean_equality_does_not_coerce_missing_or_numeric_values(self):
        raw = dict(zip(R["PREFS"], (True, False)))
        self.assertTrue(R["strict_equal"](raw, copy.deepcopy(raw)))
        for bad in ({}, {**raw, R["PREFS"][0]: 1}, {**raw, R["PREFS"][1]: 0},
                    {**raw, R["PREFS"][0]: "true"}, {**raw, R["PREFS"][1]: None}):
            self.assertFalse(R["strict_equal"](raw, bad))
        for bad in ('{"a":true,"a":false}', '{"a":NaN}', '{"a":Infinity}', None):
            with self.subTest(bad=bad), self.assertRaises(RuntimeError):
                R["strict_json"](bad)

    def test_default_query_requires_unique_ordered_fields_and_v4_identifiers(self):
        R["validate_query"](QUERY)
        for bad in (None, "", QUERY + "&_x_id=extra", QUERY + "&token=secret",
                    QUERY.replace("_x_num_retries=0", "_x_num_retries=1"),
                    QUERY.replace("_x_foreground=true", "_x_foreground=false"),
                    QUERY.replace("0123456789ab4def", "0123456789ab1def"),
                    "&".join(reversed(QUERY.split("&")))):
            with self.subTest(bad=bad), self.assertRaises(RuntimeError):
                R["validate_query"](bad)

    def test_readiness_exact_types_unknown_fixtures_and_fresh_publication(self):
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        for name, value in (("pid", 43), ("nonce", "b" * 32), ("fixture", R["FIXTURE"]),
                            ("version", True), ("pid", True)):
            with self.subTest(name=name), self.assertRaises(RuntimeError):
                R["validate_ready"]({**expected, name: value}, expected)
        for fixture, nonce, pid in (("unowned", self.nonce, self.pid),
                                    (self.fixture, "A" * 32, self.pid),
                                    (self.fixture, self.nonce, True)):
            with self.assertRaises(RuntimeError):
                R["ready_tuple"](fixture, nonce, pid)
        R["publish_ready"](self.root, expected)
        with self.assertRaises(FileExistsError):
            R["publish_ready"](self.root, expected)
        with self.assertRaises(RuntimeError):
            R["run"]("unowned", self.root / "never-launched.exe", self.root)
        (self.root / self.fixture).mkdir()
        with self.assertRaises(FileExistsError):
            R["run"](self.fixture, self.root / "never-launched.exe", self.root)

    def test_retained_process_identity_and_pointer_cleanup_are_required(self):
        R["validate_exit"](123, {"creation_filetime": 123, "exit_filetime": 124})
        for bad in ({"creation_filetime": 124, "exit_filetime": 125},
                    {"creation_filetime": 123, "exit_filetime": 0},
                    {"creation_filetime": 123, "exit_filetime": True}):
            with self.assertRaises(RuntimeError):
                R["validate_exit"](123, bad)
        good = {"movement_observed": False, "restore_observed": True, "restore_attempted": True}
        R["validate_pointer_cleanup"](good)
        for key, bad in (("movement_observed", True), ("restore_observed", False),
                         ("restore_attempted", False), ("restore_error", "failed"),
                         ("restore_skipped", True)):
            with self.assertRaises(RuntimeError):
                R["validate_pointer_cleanup"]({**good, key: bad})


if __name__ == "__main__":
    unittest.main()
