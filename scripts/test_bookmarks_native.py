"""Pure bookmark evidence admission tests; no app, network or real data access."""
import copy
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import tempfile
import unittest

spec = spec_from_file_location("bookmarks_native", Path(__file__).with_name("check-bookmarks-native.py"))
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


def state():
    return {"ok": True, "data": {"active_team": "T1", "active_channel": "C2",
            "channel_tab": "Bookmarks", "main_view": "home", "allow_destructive": False,
            "thread_open": False, "workspaces": [{"team_id": team, "rt_connected": False}
                                                  for team in ("T1", "T2")]}}


class BookmarkEvidenceTests(unittest.TestCase):
    def test_only_complete_exact_marker_grants_completion(self):
        self.assertIsNone(runner.validate_marker("ordinary startup\n"))
        self.assertIsNone(runner.validate_marker(runner.SUCCESS))
        self.assertEqual(runner.validate_marker("startup\n" + runner.SUCCESS + "\r\n", final=True),
                         runner.SUCCESS)
        for raw in ("", runner.SUCCESS, runner.SUCCESS.replace("layout", "partial layout") + "\n",
                    "prefix " + runner.SUCCESS + "\n", runner.SUCCESS + " extra\n",
                    "super-platinum: bookmarks native check passed: {}\n"):
            with self.subTest(raw=raw), self.assertRaises(RuntimeError):
                runner.validate_marker(raw, final=True)

    def test_duplicate_late_failure_and_extra_partial_never_pass(self):
        good = runner.SUCCESS + "\n"
        failure = "super-platinum: bookmarks native check failed: connected fixture"
        for raw in (good + good, failure, failure + "\n" + good, good + failure,
                    good + runner.SUCCESS, good + "bookmarks native check unexpected\n"):
            with self.subTest(raw=raw), self.assertRaises(RuntimeError):
                runner.validate_marker(raw)

    def test_log_is_bounded_and_truncated_marker_is_not_final(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "fixture.log"
            log.write_bytes((runner.SUCCESS + "\n").encode())
            self.assertEqual(runner.read_marker(log, final=True), runner.SUCCESS)
            log.write_bytes(runner.SUCCESS.encode())
            self.assertIsNone(runner.read_marker(log))
            with self.assertRaises(RuntimeError):
                runner.read_marker(log, final=True)
            log.write_bytes(b"x" * (runner.MAX_LOG_BYTES + 1))
            with self.assertRaises(RuntimeError):
                runner.read_marker(log)

    def test_final_scope_rejects_changed_owner_surface_and_unsafe_state(self):
        runner.validate_state(state())
        for key, wrong in (("active_team", "T2"), ("active_channel", "C1"),
                           ("channel_tab", "Messages"), ("main_view", "dms"),
                           ("allow_destructive", True), ("allow_destructive", 0),
                           ("thread_open", True), ("workspaces", [])):
            bad = state()
            bad["data"][key] = wrong
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                runner.validate_state(bad)
        for bad in (None, [], {"ok": True}, {"ok": 1, "data": state()["data"]}):
            with self.assertRaises(RuntimeError):
                runner.validate_state(bad)
        for key, value in (("team_id", "T3"), ("rt_connected", True), ("rt_connected", 0)):
            bad = state()
            bad["data"]["workspaces"][1][key] = value
            with self.assertRaises(RuntimeError):
                runner.validate_state(bad)

    def test_capture_must_name_owned_file_and_match_physical_png(self):
        path = Path("bookmark-final.png").resolve()
        receipt = {"ok": True, "data": {"path": str(path), "width": 1024, "height": 656}}
        runner.validate_capture(receipt, path, (1024, 656))
        for key, value in (("path", str(path.with_name("other.png"))),
                           ("width", 668), ("height", 380), ("width", True),
                           ("height", 0), ("path", None)):
            bad = copy.deepcopy(receipt)
            bad["data"][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                runner.validate_capture(bad, path, (1024, 656))
        for bad in (None, [], {"ok": False, "data": receipt["data"]}, {"ok": True}):
            with self.assertRaises(RuntimeError):
                runner.validate_capture(bad, path, (1024, 656))

    def picker_records(self):
        return [{"dom": {"stage": stage,
                          "parent": {"id": parent, "label": "Launch references" if parent else "Top level", "selectedCount": 1, "visible": True, "disabled": False},
                          "before": {"id": before, "label": runner.LABELS[before], "selectedCount": 1, "visible": True, "disabled": False}},
                 "canonical": {"base": base, "parent": parent, "before": before}, "capture": None}
                for stage, base, parent, before in runner.PICKER_STAGES]

    def test_every_initial_changed_reopened_reset_picker_requires_exact_id_and_visible_label(self):
        records = self.picker_records()
        runner.validate_pickers(records)
        for index in range(len(records)):
            for which in ("parent", "before"):
                for key, wrong in (("id", "foreign"), ("label", "Top level stale label"),
                                   ("selectedCount", True), ("selectedCount", 0),
                                   ("visible", False), ("disabled", True)):
                    bad = copy.deepcopy(records)
                    bad[index]["dom"][which][key] = wrong
                    with self.subTest(index=index, which=which, key=key), self.assertRaises(RuntimeError):
                        runner.validate_pickers(bad)
            bad = copy.deepcopy(records)
            bad[index]["canonical"]["before"] = "foreign"
            with self.assertRaises(RuntimeError):
                runner.validate_pickers(bad)

    def test_picker_receipts_require_complete_ordered_stages_and_closed_lines(self):
        records = self.picker_records()
        for bad in (records[:-1], records + records[:1], records[::-1]):
            with self.assertRaises(RuntimeError):
                runner.validate_pickers(bad)
        import json
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "fixture.log"
            raw = "".join(runner.PICKER_PREFIX + json.dumps(r) + "\n" for r in records)
            log.write_text(raw, encoding="utf-8")
            self.assertEqual(runner.read_pickers(log), records)
            for wrong in (raw.rstrip("\n"), raw + raw, raw.replace(runner.PICKER_PREFIX, "prefix " + runner.PICKER_PREFIX, 1)):
                log.write_text(wrong, encoding="utf-8")
                with self.assertRaises(RuntimeError):
                    runner.read_pickers(log)

    def test_each_picker_png_must_match_exact_owned_stage_and_dimensions(self):
        import struct
        records = self.picker_records()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for record, (stage, *_rest) in zip(records, runner.PICKER_STAGES):
                path = root / f"bookmarks-{stage}.png"
                path.write_bytes(b"\x89PNG\r\n\x1a\n" + b"\0" * 8 + struct.pack(">II", 1280, 780))
                record["capture"] = {"path": str(path), "width": 1280, "height": 780}
            runner.validate_picker_captures(records, "bookmarks", root)
            for index in range(len(records)):
                bad = copy.deepcopy(records)
                bad[index]["capture"]["path"] = str(root / "another.png")
                with self.assertRaises(RuntimeError):
                    runner.validate_picker_captures(bad, "bookmarks", root)

    def test_unsupported_fixture_and_unbounded_timeout_rejected_before_launch(self):
        for fixture, timeout in (("bookmarks-edit", 45), ("bookmarks", 0),
                                 ("bookmarks", 121), ("bookmarks", True)):
            with self.assertRaises(ValueError):
                runner.run(fixture, Path("unused"), Path("unused"), timeout)


if __name__ == "__main__":
    unittest.main()
