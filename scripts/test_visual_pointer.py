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
    def test_physical_probe_uses_and_restores_actual_context_helper(self):
        # Stub only Win32 exports; execute the real existing context manager.
        for outcome in ("success", "getter_error", "admission_error"):
            with self.subTest(outcome=outcome):
                class User:
                    current = 123
                    events = []
                    def SetThreadDpiAwarenessContext(self, value):
                        raw = value.value if hasattr(value, "value") else value
                        self.events.append(("context", raw))
                        if outcome == "admission_error":
                            return 0
                        prior, self.current = self.current, raw
                        return prior
                    def GetPhysicalCursorPos(self, out):
                        self.events.append(("getter", self.current))
                        self_expected = pointer.W.HANDLE(-4).value
                        if self.current != self_expected:
                            raise AssertionError("getter escaped PMv2")
                        if outcome == "getter_error":
                            raise OSError("physical probe unavailable")
                        out._obj.x, out._obj.y = 971, 25
                        return True
                user = User()
                probe = pointer.WindowsPointerIO.__new__(pointer.WindowsPointerIO)
                probe.user = user
                if outcome == "success":
                    self.assertEqual(probe.physical_position(), (971, 25))
                else:
                    with patch.object(pointer.ctypes, "get_last_error", return_value=5), patch.object(pointer.ctypes, "WinError", return_value=OSError("stub failure")):
                        with self.assertRaises(OSError):
                            probe.physical_position()
                self.assertEqual(user.current, 123)
                if outcome == "admission_error":
                    self.assertEqual(user.events, [("context", pointer.W.HANDLE(-4).value)])
                else:
                    self.assertEqual(user.events, [("context", pointer.W.HANDLE(-4).value),
                                                    ("getter", pointer.W.HANDLE(-4).value),
                                                    ("context", 123)])

    def test_failure_diagnostics_label_raw_probe_apis_and_contexts(self):
        parked, io, _ = self.fixture()
        io.clip = True
        io.physical_position = lambda: (647, 17)
        io.clip_rect = lambda: (0, 0, 2560, 1600)
        io.input_desktop = lambda: True
        with patch.object(pointer.time, "sleep", lambda _: None):
            with self.assertRaisesRegex(RuntimeError, "Pointer placement differed"):
                parked.park()
        diagnostic = parked.data["placement_diagnostic"]
        self.assertEqual(diagnostic["probes"], {
            "position": {"api": "GetCursorPos", "thread_dpi_context": "PER_MONITOR_AWARE_V2"},
            "physical_position": {"api": "GetPhysicalCursorPos", "thread_dpi_context": "PER_MONITOR_AWARE_V2"},
            "clip_rect_ltrb": {"api": "GetClipCursor", "thread_dpi_context": "PER_MONITOR_AWARE_V2"},
            "thread_desktop_receives_input": {"api": "GetThreadDesktop/GetUserObjectInformationW(UOI_IO)",
                                              "thread_dpi_context": "ambient"}})
        self.assertTrue(all(s["physical_position"]["value"] == (647, 17) for s in diagnostic["samples"]))
        self.assertTrue(all(s["position"]["value"] == (0, 0) for s in diagnostic["samples"]))
        self.assertEqual(len(io.moves), 1)
        self.assertFalse(parked.data["move_observed"])

    def test_failed_move_diagnostics_never_accept_delayed_target_or_retry(self):
        class Clock:
            now = 0
            def monotonic(self): return self.now
            def sleep(self, seconds): self.now += seconds
        for observed in ("unchanged", "clipped", "delayed"):
            with self.subTest(observed=observed):
                parked, io, _ = self.fixture()
                clock = Clock()
                def move(point):
                    io.moves.append(point)
                    io.point = (0, 0) if observed == "clipped" else (40, 50)
                io.move = move
                io.position = lambda: (600, 125) if observed == "delayed" and clock.now >= .02 else io.point
                io.physical_position = io.position
                io.clip_rect = lambda: (0, 0, 2560, 1600)
                io.input_desktop = lambda: True
                with patch.object(pointer.time, "monotonic", clock.monotonic), patch.object(pointer.time, "sleep", clock.sleep):
                    with self.assertRaisesRegex(RuntimeError, "Pointer placement differed"):
                        parked.park()
                samples = parked.data["placement_diagnostic"]["samples"]
                self.assertEqual([s["elapsed_seconds"] for s in samples], [0, .02, .1, .25])
                self.assertEqual(len(io.moves), 1)
                self.assertFalse(parked.data["move_observed"])
                self.assertTrue(parked.data["movement_observed"])
                self.assertTrue(all(s["thread_desktop_receives_input"] == {"ok": True, "value": True} for s in samples))
                if observed == "delayed":
                    self.assertEqual(samples[-1]["position"]["value"], (600, 125))
                parked.close()
                self.assertEqual(len(io.moves), 1)
                self.assertFalse(parked.data["restore_attempted"])

    def test_diagnostic_errors_preserve_original_failure_and_other_probes(self):
        parked, io, _ = self.fixture()
        io.clip = True
        io.physical_position = lambda: (0, 0)
        def failed(): raise OSError("probe unavailable")
        io.clip_rect = failed
        io.input_desktop = lambda: False
        with patch.object(pointer.time, "sleep", lambda _: None):
            with self.assertRaisesRegex(RuntimeError, "Pointer placement differed"):
                parked.park()
        samples = parked.data["placement_diagnostic"]["samples"]
        self.assertEqual(len(samples), 4)
        self.assertTrue(all(s["clip_rect_ltrb"]["ok"] is False for s in samples))
        self.assertTrue(all(s["physical_position"]["ok"] is True for s in samples))
        self.assertTrue(all(s["thread_desktop_receives_input"]["value"] is False for s in samples))
        self.assertEqual(len(io.moves), 1)
        parked, io, _ = self.fixture()
        io.clip = True
        with patch.object(pointer.time, "sleep", side_effect=OSError("clock unavailable")):
            with self.assertRaisesRegex(RuntimeError, "Pointer placement differed"):
                parked.park()
        self.assertEqual(parked.data["placement_diagnostic"]["error_type"], "OSError")
        self.assertEqual(len(io.moves), 1)

    def test_successful_move_does_not_run_failure_diagnostics(self):
        parked, io, _ = self.fixture()
        def forbidden(): raise AssertionError("failure-only probe on successful park")
        io.physical_position = io.clip_rect = io.input_desktop = forbidden
        parked.park()
        self.assertNotIn("placement_diagnostic", parked.data)
        parked.close()
        self.assertEqual(len(io.moves), 2)

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
