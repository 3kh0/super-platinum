"""Offline regression tests; no native APIs, app launches or counter collection."""
from contextlib import contextmanager, ExitStack
import importlib
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SCRIPTS = next(p / "scripts" for p in Path(__file__).resolve().parents if (p / "Cargo.toml").is_file())
sys.path.append(str(SCRIPTS))
import windows_client_occlusion as guard

WINDOW = {"pid": 2920, "hwnd": 100, "client_rect_physical": [391, 419, 1920, 1170],
          "taskbar_rects": [], "taskbar_client_overlap": False}


class FakeAPI:
    def __init__(self, choose=None):
        self.choose = choose or (lambda point: 200)
        self.inside = False
        self.restored = False
        self.queries = []
        self.geometry_reads = 0
    @contextmanager
    def physical(self):
        self.inside = True
        try: yield
        finally: self.inside = False; self.restored = True
    def client_rect(self, hwnd):
        assert self.inside
        self.geometry_reads += 1
        return list(WINDOW["client_rect_physical"])
    def pid(self, hwnd):
        assert self.inside
        return {100: 2920, 200: 1248, 300: 555, 400: 2920}[hwnd]
    def root(self, hwnd):
        assert self.inside
        return {100: 100, 200: 100, 300: 300, 400: 400}[hwnd]
    def hit(self, point):
        assert self.inside
        self.queries.append(point)
        return self.choose(point)


class OwnershipTests(unittest.TestCase):
    def test_ci1_empty_taskbar_rects_foreign_bottom_hit_refuses(self):
        # Actual ci1 client ends at screen y1589; taskbar pixels cover bottom.
        api = FakeAPI(lambda point: 300 if point[1] >= 1530 else 200)
        result = guard.sample_client_ownership(WINDOW, api)
        self.assertFalse(result["ok"])
        self.assertEqual(result["points"][-1]["position"], [393, 1586])
        self.assertEqual(result["points"][-1]["root_hwnd"], 300)
        self.assertFalse(result["points"][-1]["owned"])
        self.assertTrue(api.restored)

    def test_cross_process_webview_child_with_exact_fixture_parent_root_passes(self):
        api = FakeAPI()
        result = guard.sample_client_ownership(WINDOW, api)
        self.assertTrue(result["ok"])
        self.assertEqual(len(result["points"]), 9)
        self.assertTrue(all(p["pid"] == 1248 and p["root_pid"] == 2920 and p["owned"]
                            for p in result["points"]))
        self.assertEqual(api.geometry_reads, 2)
        self.assertTrue(api.restored)

    def test_direct_host_window_hits_pass(self):
        self.assertTrue(guard.sample_client_ownership(WINDOW, FakeAPI(lambda _: 100))["ok"])

    def test_same_process_other_top_level_window_does_not_pass(self):
        result = guard.sample_client_ownership(WINDOW, FakeAPI(lambda _: 400))
        self.assertFalse(result["ok"])
        self.assertEqual(result["points"][0]["pid"], WINDOW["pid"])

    def test_unknown_hit_or_ancestor_or_pid_refuses(self):
        for kind in ("hit", "root", "pid"):
            with self.subTest(kind=kind):
                api = FakeAPI(lambda _: 0 if kind == "hit" else 200)
                if kind == "root": api.root = lambda hwnd: 100 if hwnd == 100 else 0
                if kind == "pid":
                    old = api.pid
                    def pid(hwnd):
                        if hwnd == 200: raise RuntimeError("ownership unavailable")
                        return old(hwnd)
                    api.pid = pid
                self.assertFalse(guard.sample_client_ownership(WINDOW, api)["ok"])
                self.assertTrue(api.restored)

    def test_initial_or_final_geometry_change_refuses(self):
        for final in (False, True):
            with self.subTest(final=final):
                api = FakeAPI()
                original = api.client_rect
                def rectangle(hwnd):
                    value = original(hwnd)
                    if not final or api.geometry_reads == 2: value[0] += 1
                    return value
                api.client_rect = rectangle
                self.assertFalse(guard.sample_client_ownership(WINDOW, api)["ok"])

    def test_foreign_root_identity_refuses_before_hits(self):
        api = FakeAPI(); api.pid = lambda _: 999
        self.assertFalse(guard.sample_client_ownership(WINDOW, api)["ok"])
        self.assertEqual(api.queries, [])

    def test_coordinates_are_physical_include_edges_and_support_negative_monitor(self):
        points = guard.client_points([-1910, -400, 1920, 1170])
        self.assertEqual(points[0], (-1908, -398))
        self.assertEqual(points[4], (-950, 185))
        self.assertEqual(points[-1], (7, 767))
        self.assertEqual(len(set(points)), 9)

    def test_malformed_or_tiny_rectangle_refuses(self):
        for rect in ([0, 0, 6, 10], [0, 0, 10, 0], [0., 0, 10, 10], [0, 0, 10], None):
            with self.subTest(rect=rect):
                with self.assertRaises(RuntimeError): guard.client_points(rect)

    def test_actual_api_uses_client_screen_geometry_under_physical_context(self):
        events = []
        api = object.__new__(guard.WindowsHitAPI)
        def client(hwnd, rect):
            events.append("client"); rect._obj.right = 1920; rect._obj.bottom = 1170; return True
        def screen(hwnd, point):
            events.append("screen"); point._obj.x = 391; point._obj.y = 419; return True
        def point(value):
            events.append((value.x, value.y)); return 200
        api.user = SimpleNamespace(GetClientRect=client, ClientToScreen=screen, WindowFromPoint=point)
        @contextmanager
        def physical(user):
            self.assertIs(user, api.user); events.append("physical")
            try: yield
            finally: events.append("restore")
        with patch.object(guard, "_physical_pixels", physical):
            with api.physical():
                self.assertEqual(api.client_rect(100), WINDOW["client_rect_physical"])
                self.assertEqual(api.hit((393, 1586)), 200)
        self.assertEqual(events, ["physical", "client", "screen", (393, 1586), "restore"])

    def test_idle_observer_latches_refusal_without_pointer_or_agent_actions(self):
        old = importlib.import_module("test_idle_windows_native")
        observer, window, _ = old.IdleTests().observation()
        window.update(WINDOW)
        # Existing observation retains its initial identity; use same fixture ID.
        window.update(pid=123, hwnd=456)
        observer.window = dict(window)
        observer.hit_probe = lambda current: {"ok": False, "points": [{"owned": False}]}
        with self.assertRaisesRegex(RuntimeError, "Client ownership hit"):
            observer.check("quiet")
        self.assertTrue(observer.data["inconclusive"])
        self.assertFalse(observer.data["observations"][-1]["client_ownership"]["ok"])
        self.assertFalse(observer.pointer.data["movement_observed"])

    def test_existing_taskbar_guard_still_refuses_before_hit_probe(self):
        old = importlib.import_module("test_idle_windows_native")
        observer, window, _ = old.IdleTests().observation()
        window["taskbar_client_overlap"] = True
        observer.hit_probe = lambda _: self.fail("Known overlap must refuse first")
        with self.assertRaisesRegex(RuntimeError, "Taskbar overlaps"):
            observer.check("quiet")
        self.assertTrue(observer.data["inconclusive"])

    def test_failed_prepared_hit_restores_pointer_through_existing_stack(self):
        old = importlib.import_module("test_idle_windows_native")
        observer, _, _ = old.IdleTests().observation()
        observer.hit_probe = lambda _: {"ok": False}
        events = []
        class Pointer:
            identity = staticmethod(lambda window: window["hwnd"])
            def __init__(self, *_):
                self.data = {"movement_observed": False}
                self.io = SimpleNamespace(position=lambda: (400, 20))
                self.placed = (400, 20)
            def park(self): events.append("park")
            def settle(self, *_): events.append("settle")
            def close(self): events.append("restore")
        import visual_pointer
        observer.visual.fingerprint = repr
        with patch.object(visual_pointer, "FixturePointer", Pointer):
            with self.assertRaisesRegex(RuntimeError, "Client ownership hit"):
                with ExitStack() as stack:
                    observer.prepare(SimpleNamespace(fit_work_area=False, normalize_pointer=True), stack)
        self.assertEqual(events, ["park", "settle", "restore"])


if __name__ == "__main__": unittest.main()
