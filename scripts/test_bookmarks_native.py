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

    def test_unsupported_fixture_and_unbounded_timeout_rejected_before_launch(self):
        for fixture, timeout in (("bookmarks-edit", 45), ("bookmarks", 0),
                                 ("bookmarks", 121), ("bookmarks", True)):
            with self.assertRaises(ValueError):
                runner.run(fixture, Path("unused"), Path("unused"), timeout)


if __name__ == "__main__":
    unittest.main()
