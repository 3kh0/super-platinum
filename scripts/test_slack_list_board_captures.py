"""Capture-contract tests only; placeholder files do not prove native rendering."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from test_slack_list_board_geometry import sample

SPEC = importlib.util.spec_from_file_location("slack_list_native_capture_check", Path(__file__).with_name("check-slack-list-native.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class BoardCaptures(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.paths = {}
        for kind, stage in (("header", "board-header"), ("addGroup", "board-add-group")):
            p = self.folder / f"slack-list-{stage}.png"
            p.write_bytes(b"test placeholder, not an image")
            self.paths[kind] = str(p)
        self.receipt = sample(CHECK.expected_viewport("slack-list"))
        board = self.receipt["boardViews"]
        board["headerCaptures"] = self.paths
        lane = board["headerGeometry"]["lanes"][0]
        add = board["headerGeometry"]["addGroup"]
        common = dict(ok=True, owner=dict(list="F_LIST_FIXTURE", view="V_BOARD_MULTI", generation="1", layout="2"),
                      viewport=CHECK.expected_viewport("slack-list"), scroll=dict(board=[0,0], layout=[0,0], dialog=[0,0]))
        board["headerCaptureGeometry"] = {
            "header": dict(**copy.deepcopy(common), target="header", **lane["header"], text="Engineering 2 Name Color Delete Left Right", title=dict(**lane["heading"], text="Engineering"), actions=copy.deepcopy(lane["actions"]), icon=None),
            "addGroup": dict(**copy.deepcopy(common), target="addGroup", rect=add["rect"], inView=True, hit=True, text="Add group", title=None, actions=[], icon=dict(rect=add["icon"], text="", inView=True, hit=False))}


    def check(self):
        CHECK.validate_header_captures(self.receipt, "slack-list", self.folder)

    def test_exact_two_stage_files(self):
        self.check()
        self.assertEqual(len(CHECK.STAGES), 29)
        self.assertEqual(CHECK.STAGES.count("board-header"), 1)
        self.assertEqual(CHECK.STAGES.count("board-add-group"), 1)

    def test_focused_wrong_owner_target_and_viewport(self):
        for key, value in (("owner", {}), ("target", "other"), ("viewport", [1, 1])):
            original = copy.deepcopy(self.receipt)
            self.receipt["boardViews"]["headerCaptureGeometry"]["header"][key] = value
            with self.assertRaises(RuntimeError): self.check()
            self.receipt = original

    def test_offscreen_or_unreachable_focused_header(self):
        for key in ("inView", "hit"):
            original = copy.deepcopy(self.receipt)
            self.receipt["boardViews"]["headerCaptureGeometry"]["header"][key] = False
            with self.assertRaises(RuntimeError): self.check()
            self.receipt = original

    def test_focused_foreign_action_or_missing_icon(self):
        original = copy.deepcopy(self.receipt)
        self.receipt["boardViews"]["headerCaptureGeometry"]["header"]["actions"][0]["choice"] = "foreign"
        with self.assertRaises(RuntimeError): self.check()
        self.receipt = original
        self.receipt["boardViews"]["headerCaptureGeometry"]["addGroup"]["icon"] = None
        with self.assertRaises(RuntimeError): self.check()

    def test_each_file_required(self):
        for key in self.paths:
            p = Path(self.paths[key]); p.unlink()
            with self.assertRaises(RuntimeError): self.check()
            p.write_bytes(b"placeholder")

    def test_swapped_original_paths_refused(self):
        self.paths["header"], self.paths["addGroup"] = self.paths["addGroup"], self.paths["header"]
        with self.assertRaises(RuntimeError): self.check()

    def test_foreign_fixture_path_refused(self):
        p = self.folder / "slack-list-narrow-board-header.png"; p.touch()
        self.paths["header"] = str(p)
        with self.assertRaises(RuntimeError): self.check()

    def test_missing_receipt_field_refused(self):
        del self.paths["addGroup"]
        with self.assertRaises(RuntimeError): self.check()

    def test_directory_cannot_satisfy_capture(self):
        p = Path(self.paths["header"]); p.unlink(); p.mkdir()
        with self.assertRaises(RuntimeError): self.check()


if __name__ == "__main__":
    unittest.main()
