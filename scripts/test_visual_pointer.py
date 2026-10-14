"""Pure pointer ownership/failure tests; no Win32 calls or native launches."""
import copy
import unittest
from unittest.mock import patch

import visual_pointer as pointer


WINDOW = {"pid": 123, "hwnd": 456, "window_rect_physical": [100, 100, 1000, 700],
          "client_rect_physical": [110, 175, 980, 615],
          "virtual_desktop_rect": [0, 0, 2560, 1600], "taskbar_client_overlap": False}


class FakeIO:
    def __init__(self):
        self.point = (40, 50)
        self.moves = []
        self.is_caption = True
        self.clip = False

    def position(self): return self.point
    def caption(self, hwnd, point): return hwnd == 456 and self.is_caption
    def move(self, point):
        self.moves.append(point)
        self.point = (0, 0) if self.clip else point


class PointerTests(unittest.TestCase):
    def test_empty_taskbar_sample_does_not_exempt_later_obstructions_or_failures(self):
        parked, io, window = self.fixture()
        window.update(taskbar_rects=[], taskbar_probe_completed=True)
        parked.park()
        parked.check("before_capture", require_clear=True)
        window.update(taskbar_rects=[[0, 700, 2560, 72]], taskbar_client_overlap=True)
        with self.assertRaises(RuntimeError): parked.check("after_capture", require_clear=True)
        window.update(taskbar_probe_completed=False, taskbar_client_overlap=None)
        with self.assertRaises(RuntimeError): parked.check("after_capture", require_clear=True)
        parked.close()

    def fixture(self):
        io = FakeIO()
        window = copy.deepcopy(WINDOW)
        parked = pointer.FixturePointer(lambda: copy.deepcopy(window), lambda: None, io)
        return parked, io, window

    def test_caption_identity_guards_precede_any_move(self):
        parked, io, _ = self.fixture()
        io.is_caption = False
        with self.assertRaises(RuntimeError): parked.park()
        self.assertEqual(io.moves, [])
        parked, io, window = self.fixture()
        def change_owner(): window["hwnd"] += 1
        parked.verify_identity = change_owner
        with self.assertRaises(RuntimeError): parked.park()
        self.assertEqual(io.moves, [])
        parked, io, _ = self.fixture()
        def failed_identity(): raise RuntimeError("exited/reused process")
        parked.verify_identity = failed_identity
        with self.assertRaises(RuntimeError): parked.park()
        self.assertEqual(io.moves, [])

    def test_restore_only_unchanged_parked_cursor_and_record_actual(self):
        parked, io, _ = self.fixture()
        parked.park()
        self.assertEqual(io.point, (600, 125))
        parked.check("capture")
        parked.close()
        self.assertEqual(io.point, (40, 50))
        self.assertTrue(parked.data["move_observed"])
        self.assertTrue(parked.data["restore_attempted"])
        self.assertTrue(parked.data["restore_observed"])
        parked, io, _ = self.fixture()
        parked.park()
        io.point = (77, 88)
        with self.assertRaises(RuntimeError): parked.check("capture")
        io.point = parked.placed  # Movement remains latched, even if user returns.
        parked.close()
        self.assertEqual(len(io.moves), 1)
        self.assertFalse(parked.data["restore_attempted"])

    def test_geometry_changes_and_clipped_moves_are_not_accepted(self):
        parked, io, window = self.fixture()
        parked.park()
        window["window_rect_physical"][0] += 10
        with self.assertRaises(RuntimeError): parked.check("capture")
        parked.close()
        parked, io, _ = self.fixture()
        io.clip = True
        with self.assertRaises(RuntimeError): parked.park()
        parked.close()
        self.assertEqual(io.moves, [(600, 125)])
        self.assertFalse(parked.data["move_observed"])

    def test_settlement_waits_for_taskbar_profile_hover_and_stable_state(self):
        parked, io, window = self.fixture()
        parked.park()
        class Clock:
            now = 0
            def monotonic(self): return self.now
            def sleep(self, seconds): self.now += seconds
        class Agent:
            calls = 0
            def control(self, *_args, **_kwargs):
                self.calls += 1
                window["taskbar_client_overlap"] = self.calls < 3
                return {"data": {"profile_hover": "card" if self.calls < 4 else None}}
        clock, agent = Clock(), Agent()
        with patch.object(pointer.time, "monotonic", clock.monotonic), patch.object(pointer.time, "sleep", clock.sleep):
            parked.settle(agent, repr)
        self.assertGreaterEqual(agent.calls, 6)
        self.assertTrue(parked.data["settlement"]["ok"])
        self.assertFalse(parked.data["dom_tooltip_absence_asserted"])
        class Obstructed:
            def control(self, *_args, **_kwargs):
                window["taskbar_client_overlap"] = True
                return {"data": {"profile_hover": None}}
        clock = Clock()
        with patch.object(pointer.time, "monotonic", clock.monotonic), patch.object(pointer.time, "sleep", clock.sleep):
            with self.assertRaises(TimeoutError): parked.settle(Obstructed(), repr, .7)
        self.assertFalse(parked.data["settlement"]["ok"])
        with self.assertRaises(RuntimeError): parked.check("capture", require_clear=True)
        parked.close()


if __name__ == "__main__":
    unittest.main()
