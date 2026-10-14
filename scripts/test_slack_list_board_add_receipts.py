"""Pure Board Add evidence validators; no native launch or real session/storage."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))


class Receipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = R["FIXTURE"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture = fixture or self.fixture
        value = dict.fromkeys(R["REQUIRED"], True)
        value.update(ok=True, fixture=fixture, nonce=self.nonce, pid=self.pid, viewer="U00000001",
                     preparedRequests=4, viewport=expected_viewport(fixture), captures={}, attempts=[])
        for index in range(4):
            fields = [["token", "fixture-not-a-token"], ["list_id", "F_LIST_FIXTURE"],
                      ["position", ""], ["updated_timestamp", f"1800000000000-{index:04X}-0123456789abcdef"],
                      ["_x_reason", "create-record"], ["initial_fields", json.dumps(R["seed_fields"](index, "U00000001"))]]
            value["attempts"].append({"method": "POST", "url": "https://example.test/api/lists.records.create",
                                      "retrySafe": False, "fields": fields})
        value["geometry"] = [dict(ok=True, bounded=True, compact=True, hitTested=True, status=True,
                                  kind=kind, viewport=value["viewport"])
                             for kind in ["lane", "pending", "detail", "detail", "lane", "detail", "lane", "uncertain"]]
        for stage in R["STAGES"]:
            path = self.root / f"{fixture}-{stage}.png"
            path.write_bytes(png())
            value["captures"][stage] = str(path)
        edges = [{"stage": stage, "edge": edge} for stage in R["STAGES"] for edge in ("before", "after")]
        return value, edges

    def validate(self, value, edges):
        return R["validate_native"](value["fixture"], self.nonce, self.pid, self.root, value, edges)

    def test_normal_and_narrow_complete_evidence(self):
        for fixture in R["FIXTURES"]:
            value, edges = self.good(fixture)
            self.assertEqual(len(self.validate(value, edges)), 7)

    def test_every_required_flag_missing_or_false_rejected(self):
        value, edges = self.good()
        for flag in R["REQUIRED"]:
            for absent in (False, True):
                changed = copy.deepcopy(value)
                if absent:
                    del changed[flag]
                else:
                    changed[flag] = False
                with self.subTest(flag=flag, absent=absent), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)

    def test_wrong_order_viewer_type_or_invented_routing_rejected(self):
        value, edges = self.good()
        for mutation in ("reverse", "viewer", "empty", "checkbox", "view_id", "parent_record_id", "duplicated_record_id"):
            changed = copy.deepcopy(value)
            fields = changed["attempts"][0]["fields"]
            seeds = json.loads(fields[-1][1])
            if mutation == "reverse":
                seeds.reverse()
            elif mutation == "viewer":
                seeds[1]["user"] = ["U1"]
            elif mutation == "empty":
                seeds[-1]["select"] = []
            elif mutation == "checkbox":
                seeds[-1] = {"column_id": "C_SELECT", "checkbox": False}
            else:
                fields.append([mutation, "invented"])
            fields[5][1] = json.dumps(seeds)
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_no_ledger_or_replay_or_nonempty_position_rejected(self):
        value, edges = self.good()
        for mutation in ("missing", "replay", "retry", "position", "timestamp"):
            changed = copy.deepcopy(value)
            if mutation == "missing":
                del changed["attempts"]
            elif mutation == "replay":
                changed["attempts"].append(changed["attempts"][0])
            elif mutation == "retry":
                changed["attempts"][0]["retrySafe"] = True
            elif mutation == "position":
                changed["attempts"][0]["fields"][2][1] = "5000000000"
            else:
                changed["attempts"][1]["fields"][3][1] = changed["attempts"][0]["fields"][3][1]
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_geometry_and_stage_sequence_cannot_be_partial(self):
        value, edges = self.good()
        changed = copy.deepcopy(value)
        changed["geometry"][-1]["hitTested"] = False
        with self.assertRaises(RuntimeError):
            self.validate(changed, edges)
        for bad in (edges[:-1], edges + edges[-1:], list(reversed(edges))):
            with self.assertRaises(RuntimeError):
                self.validate(value, bad)

    def test_readiness_exact_and_cannot_overwrite(self):
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        for name, value in (("pid", 43), ("nonce", "b" * 32), ("fixture", R["FIXTURE"]), ("version", True)):
            changed = {**expected, name: value}
            with self.assertRaises(RuntimeError):
                R["validate_ready"](changed, expected)
        R["publish_ready"](self.root, expected)
        with self.assertRaises(FileExistsError):
            R["publish_ready"](self.root, expected)

    def test_closed_log_late_duplicate_and_pointer_skip_rejected(self):
        value, edges = self.good()
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        lines = [R["WAITING"] + json.dumps(expected)]
        lines.extend(R["STAGE"] + json.dumps(edge) for edge in edges)
        lines.append(R["PREFIX"] + json.dumps(value))
        path = self.root / "closed.log"
        path.write_text("\n".join(lines) + "\n")
        R["validate_closed_log"](path, expected, self.root, value)
        path.write_text(path.read_text() + lines[-1] + "\n")
        with self.assertRaises(RuntimeError):
            R["validate_closed_log"](path, expected, self.root, value)
        with self.assertRaises(RuntimeError):
            R["validate_pointer_cleanup"]({"movement_observed": True, "restore_observed": False,
                                            "restore_attempted": True})

    def test_png_truncation_wrong_dimensions_and_fresh_output(self):
        value, edges = self.good()
        path = Path(value["captures"]["initial"])
        with self.assertRaises(RuntimeError):
            R["png_signature"](path, [99, 200])
        path.write_bytes(path.read_bytes()[:-12])
        with self.assertRaises(RuntimeError):
            self.validate(value, edges)
        (self.root / self.fixture).mkdir()
        with self.assertRaises(FileExistsError):
            R["run"](self.fixture, self.root / "never-launched.exe", self.root)


if __name__ == "__main__":
    unittest.main()
