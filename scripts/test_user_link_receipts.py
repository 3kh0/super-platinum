"""Pure user-link evidence validators; no native launch or real session/storage."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

from native_check import expected_viewport
from test_multi_external_select_receipts import png

R = runpy.run_path(str(Path(__file__).with_name("check-user-links-native.py")))


class Receipts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = R["FIXTURE"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture = fixture or self.fixture
        value = dict(noTransport=True, syntheticSession=True, checks=dict.fromkeys(R["CHECKS"], True))
        value.update(ok=True, fixture=fixture, nonce=self.nonce, pid=self.pid,
                     preparedRequests=6, viewport=expected_viewport(fixture), captures={}, attempts=[])
        url = ("https://user-links.slack.com/api/conversations.open?"
               "_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
               "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
               "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
               "&fp=2123456789ab4def8123456789abcdef&slack_route=default")
        for index, user in enumerate(R["USERS"]):
            token = "fake-user-links-rotated" if index == 5 else "fake-user-links"
            value["attempts"].append({"method": "POST", "url": url,
                                      "fields": [["token", token], ["users", user], ["return_im", "true"]]})
        rect = dict(x=8, y=8, width=200, height=100)
        value["geometry"] = [dict(ok=True, stage=stage, viewport=value["viewport"],
            documentWidth=value["viewport"][0], composer=rect.copy(), profileVisible=stage in ("cached", "created"),
            profile=rect.copy() if stage in ("cached", "created") else None,
            closeProfileHit=True if stage in ("cached", "created") else None) for stage in R["STAGES"]]
        for stage in R["STAGES"]:
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
            self.assertEqual(len(self.validate(value, edges)), 8)

    def test_every_required_flag_missing_or_false_rejected(self):
        value, edges = self.good()
        for flag in R["REQUIRED"]:
            for absent in (False, True):
                changed = copy.deepcopy(value)
                target, key = (changed["checks"], flag.split(".")[1]) if flag.startswith("checks.") else (changed, flag)
                if absent:
                    del target[key]
                else:
                    target[key] = False
                with self.subTest(flag=flag, absent=absent), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)

    def test_actual_request_ledger_cannot_be_replaced_by_flags_or_replay(self):
        value, edges = self.good()
        for mutation in ("missing", "extra", "empty", "method", "order", "credential", "recipient", "invented", "duplicate"):
            changed = copy.deepcopy(value)
            if mutation == "missing":
                del changed["attempts"]
            elif mutation == "extra":
                changed["attempts"].append(copy.deepcopy(changed["attempts"][0]))
            elif mutation == "empty":
                changed["attempts"] = []
            elif mutation == "method":
                changed["attempts"][0]["method"] = "GET"
            elif mutation == "order":
                changed["attempts"].reverse()
            elif mutation == "credential":
                changed["attempts"][-1]["fields"][0][1] = "fake-user-links"
            elif mutation == "recipient":
                changed["attempts"][0]["fields"][1][1] = "UCACHED"
            elif mutation == "invented":
                changed["attempts"][0]["fields"].append(["channel", "DCREATED"])
            else:
                changed["attempts"][0]["fields"].append(["users", "UCREATED"])
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_url_requires_exact_host_endpoint_default_parameters_and_uuid_types(self):
        value, edges = self.good()
        url = value["attempts"][0]["url"]
        for bad in (None, url.split("?")[0], url + "&_x_id=extra", url + "&token=secret", url + "#fragment",
                    url.replace("user-links.slack.com", "other.slack.com"),
                    url.replace("conversations.open", "conversations.info"),
                    url.replace("_x_num_retries=0", "_x_num_retries=1"),
                    url.replace("_x_foreground=true", "_x_foreground=false"),
                    url.replace("0123456789ab4def", "0123456789ab1def")):
            changed = copy.deepcopy(value)
            changed["attempts"][0]["url"] = bad
            with self.subTest(url=bad), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_geometry_null_booleans_nonfinite_and_impossible_rects_rejected(self):
        value, edges = self.good()
        changes = ((0, "composer", None), (0, "composer", True), (0, "profile", {}),
                   (0, "closeProfileHit", False), (1, "profile", None), (1, "profileVisible", 1),
                   (1, "closeProfileHit", 1), (0, "documentWidth", True), (0, "documentWidth", float("nan")),
                   (0, "documentWidth", 10000), (0, "composer", dict(x=0, y=0, width=0, height=100)),
                   (0, "composer", dict(x=False, y=0, width=100, height=100)),
                   (0, "composer", dict(x=0, y=float("inf"), width=100, height=100)))
        for index, key, bad in changes:
            changed = copy.deepcopy(value)
            changed["geometry"][index][key] = bad
            with self.subTest(index=index, key=key, bad=bad), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_capture_paths_cannot_escape_or_substitute_another_stage(self):
        value, edges = self.good()
        for path in (str(self.root / ".." / "initial.png"), str(self.root / "cached.png"), "initial.png", None):
            changed = copy.deepcopy(value)
            changed["captures"]["initial"] = path
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_geometry_and_stage_sequence_cannot_be_partial(self):
        value, edges = self.good()
        changed = copy.deepcopy(value)
        changed["geometry"][1]["closeProfileHit"] = False
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
