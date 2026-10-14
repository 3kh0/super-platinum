"""Pure placement geometry/ownership tests; no native calls."""
import copy
import unittest

from visual_placement import capture_area, clamp_window, fit_work_area


class PlacementTests(unittest.TestCase):
    def test_empty_taskbars_require_completed_probe_on_every_sample(self):
        work = [0, 0, 2560, 1600]
        self.assertEqual(capture_area(work, [], taskbar_probe_completed=True), work)
        for bars, completed in ((None, True), ([], False), ([], None), ([], 1)):
            with self.assertRaises(RuntimeError):
                capture_area(work, bars, taskbar_probe_completed=completed)
        window, io = self.fixture()
        window.update(taskbar_rects=[], taskbar_probe_completed=True)
        evidence = {}
        fit_work_area(lambda: copy.deepcopy(window), lambda: None, evidence, io)
        self.assertEqual(evidence["available_capture_area_physical"], work)
        self.assertEqual(io.moves, [])
        for stage in (2, 3):
            for failure in ("probe", "reappearing_bar"):
                with self.subTest(stage=stage, failure=failure):
                    calls = 0
                    def observe():
                        nonlocal calls
                        calls += 1
                        result = copy.deepcopy(window)
                        if calls >= stage:
                            if failure == "probe": result["taskbar_probe_completed"] = False
                            else: result["taskbar_rects"] = [[0, 1528, 2560, 72]]
                        return result
                    with self.assertRaises(RuntimeError):
                        fit_work_area(observe, lambda: None, {}, io)

    def test_capture_area_excludes_auto_hide_strip_on_all_edges(self):
        self.assertEqual(capture_area([0, 0, 2560, 1600], [[0, 1528, 2560, 72]]),
                         [0, 0, 2560, 1528])
        self.assertEqual(capture_area([0, 0, 2560, 1528], [[0, 1528, 2560, 72]]),
                         [0, 0, 2560, 1528])
        self.assertEqual(capture_area([-1920, -100, 1920, 1080], [[-1920, -100, 1920, 40]]),
                         [-1920, -60, 1920, 1040])
        self.assertEqual(capture_area([-1920, 0, 1920, 1080], [[-1920, 0, 60, 1080]]),
                         [-1860, 0, 1860, 1080])
        self.assertEqual(capture_area([-1920, 0, 1920, 1080], [[-60, 0, 60, 1080]]),
                         [-1920, 0, 1860, 1080])
        area = capture_area([0, 0, 2560, 1600], [[0, 1528, 2560, 72]])
        with self.assertRaises(RuntimeError): clamp_window([0, 0, 1000, 1540], area)
        for bars in (None, [], [[0, 1528, 1280, 72]], [[50, 50, 100, 100]], [[0, 0, 2560, 1600]]):
            with self.assertRaises(RuntimeError): capture_area([0, 0, 2560, 1600], bars)

    def test_clamp_preserves_size_and_supports_negative_monitors(self):
        self.assertEqual(clamp_window([304, 304, 1942, 1256], [0, 0, 2560, 1528]),
                         [304, 272, 1942, 1256])
        self.assertEqual(clamp_window([152, 152, 1942, 1256], [0, 0, 2560, 1528]),
                         [152, 152, 1942, 1256])
        self.assertEqual(clamp_window([-2300, -50, 1000, 700], [-1920, 20, 1920, 1000]),
                         [-1920, 20, 1000, 700])
        for window in ([0, 0, 2561, 700], [0, 0, 1000, 1529], [0, 0, 0, 700]):
            with self.assertRaises(RuntimeError): clamp_window(window, [0, 0, 2560, 1528])

    def test_top_left_uses_validated_area_including_negative_monitors(self):
        area = capture_area([-1920, -100, 1920, 1080], [[-1920, -100, 1920, 40]])
        self.assertEqual(clamp_window([-1500, 50, 1000, 700], area,
                                      position_policy="top-left"),
                         [-1920, -60, 1000, 700])
        with self.assertRaises(RuntimeError):
            clamp_window([0, 0, 1942, 1256], area, position_policy="top-left")
        with self.assertRaises(ValueError):
            clamp_window([0, 0, 1000, 700], area, position_policy="unknown")

    def fixture(self):
        window = {"pid": 123, "hwnd": 456, "window_rect_physical": [304, 304, 1942, 1256],
                  "client_rect_physical": [315, 379, 1920, 1170],
                  "taskbar_rects": [[0, 1528, 2560, 72]]}
        class IO:
            moves = []
            def work_area(self, _hwnd): return [0, 0, 2560, 1600]
            def move(self, hwnd, target):
                self.moves.append((hwnd, target))
                old = window["window_rect_physical"]
                window["client_rect_physical"][0] += target[0] - old[0]
                window["client_rect_physical"][1] += target[1] - old[1]
                window["window_rect_physical"] = target
        io = IO()
        io.moves = []
        return window, io

    def test_exact_move_observed_and_already_fit_is_noop(self):
        window, io = self.fixture()
        evidence = {}
        fit_work_area(lambda: copy.deepcopy(window), lambda: None, evidence, io)
        self.assertEqual(io.moves, [(456, [304, 272, 1942, 1256])])
        self.assertTrue(evidence["size_unchanged"])
        self.assertTrue(evidence["move_observed"])
        self.assertEqual(evidence["work_area_physical"], [0, 0, 2560, 1600])
        self.assertEqual(evidence["available_capture_area_physical"], [0, 0, 2560, 1528])
        fit_work_area(lambda: copy.deepcopy(window), lambda: None, evidence, io)
        self.assertEqual(len(io.moves), 1)
        self.assertFalse(evidence["move_attempted"])

    def test_identity_change_and_oversize_refuse_before_move(self):
        window, io = self.fixture()
        calls = 0
        def observe():
            nonlocal calls
            calls += 1
            result = copy.deepcopy(window)
            if calls > 1: result["hwnd"] = 999
            return result
        with self.assertRaises(RuntimeError): fit_work_area(observe, lambda: None, {}, io)
        self.assertEqual(io.moves, [])
        window["window_rect_physical"][2] = 3000
        with self.assertRaises(RuntimeError):
            fit_work_area(lambda: copy.deepcopy(window), lambda: None, {}, io)
        self.assertEqual(io.moves, [])
        def exited(): raise RuntimeError("retained process no longer live")
        with self.assertRaises(RuntimeError):
            fit_work_area(lambda: copy.deepcopy(window), exited, {}, io)
        self.assertEqual(io.moves, [])

    def test_top_left_move_and_noop_preserve_size_and_flags(self):
        window, io = self.fixture()
        evidence = {}
        fit_work_area(lambda: copy.deepcopy(window), lambda: None, evidence, io,
                      position_policy="top-left")
        self.assertEqual(io.moves, [(456, [0, 0, 1942, 1256])])
        self.assertEqual(window["client_rect_physical"], [11, 75, 1920, 1170])
        self.assertEqual(evidence["position_policy"], "top-left")
        for key in ("resize_permitted", "activation_requested", "z_order_change_requested",
                    "settings_changed"):
            self.assertFalse(evidence[key])
        fit_work_area(lambda: copy.deepcopy(window), lambda: None, evidence, io,
                      position_policy="top-left")
        self.assertEqual(len(io.moves), 1)
        self.assertFalse(evidence["move_attempted"])

    def test_top_left_refuses_identity_foreground_and_taskbar_changes(self):
        for change in ("pid", "hwnd", "foreground", "taskbar", "work", "retained"):
            with self.subTest(change=change):
                window, io = self.fixture()
                calls = 0
                def observe():
                    nonlocal calls
                    calls += 1
                    result = copy.deepcopy(window)
                    if calls > 1:
                        if change in ("pid", "hwnd"): result[change] += 1
                        if change == "foreground": raise RuntimeError("Owned window lost foreground")
                        if change == "taskbar": result["taskbar_rects"] = [[0, 0, 2560, 72]]
                        if change == "work": io.work_area = lambda _: [0, 0, 2500, 1600]
                    return result
                def verify():
                    if change == "retained" and calls:
                        raise RuntimeError("Retained process identity changed")
                with self.assertRaises(RuntimeError):
                    fit_work_area(observe, verify, {}, io, position_policy="top-left")
                self.assertEqual(io.moves, [])

    def test_unknown_policy_refuses_before_observation(self):
        def unexpected(): raise AssertionError("must not inspect or move windows")
        with self.assertRaises(ValueError):
            fit_work_area(unexpected, unexpected, {}, position_policy="unknown")


if __name__ == "__main__":
    unittest.main()
