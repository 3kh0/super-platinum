"""Offline orchestration/observer regression tests; no OS calls or native launch."""
from contextlib import ExitStack
import importlib
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

idle = importlib.import_module("measure-idle-windows-native")


class IdleTests(unittest.TestCase):
    def test_normalization_precedes_settle_and_restores_before_exit(self):
        for normalize, fit, fail_capture in ((False, False, False), (True, False, False),
                                            (False, True, False), (True, True, False), (True, True, True)):
            with self.subTest(normalize=normalize, fit=fit, fail_capture=fail_capture), tempfile.TemporaryDirectory() as temp:
                events = []
                class Fixture:
                    process = SimpleNamespace(pid=123)
                    def __init__(self, *_args): pass
                    def __enter__(self): events.append("launch"); return self
                    def __exit__(self, *_args): events.append("exit")
                    def require_running(self): pass
                    def control(self, *_args):
                        events.append("state")
                        return {"data": {"performance": {"startup": {"first_paint_ms": 10}}}}
                    def capture(self, *_args):
                        events.append("capture")
                        if fail_capture: raise RuntimeError("capture failed")
                class Tree:
                    def __init__(self, *_args): pass
                    def __enter__(self): return self
                    def __exit__(self, *_args): events.append("close_handles")
                    def snapshot(self):
                        events.append("counters")
                        return {"processes": [{"group": "webview2_descendants"}]}
                class Normalization:
                    def __init__(self, _fixture, _tree, data): self.data = data
                    def prepare(self, args, stack):
                        events.append(("prepare", args.normalize_pointer, args.fit_work_area))
                        if args.normalize_pointer:
                            self.data["pointer"].update(movement_observed=False, restore_observed=False)
                            def restore():
                                events.append("restore")
                                self.data["pointer"]["restore_observed"] = True
                            stack.callback(restore)
                    def check(self, stage): events.append(stage)
                def wait(seconds, observe=None):
                    events.append(("wait", seconds))
                    if observe: observe()
                directory = Path(temp)
                binary = directory / "binary.exe"
                binary.write_bytes(b"offline test binary; never executed")
                args = SimpleNamespace(binary=binary, output=directory, label="fake", fixture="channel",
                                       seconds=15, settle_seconds=5, normalize_pointer=normalize, fit_work_area=fit)
                with patch.object(idle, "NativeFixture", Fixture), patch.object(idle, "OwnedProcessTree", Tree), \
                     patch.object(idle, "NormalizedObservation", Normalization), patch.object(idle, "quiet_wait", wait), \
                     patch.object(idle, "summarize", return_value={"observed_coverage_complete": True}), \
                     patch("builtins.print"):
                    if fail_capture:
                        with self.assertRaisesRegex(RuntimeError, "capture failed"): idle.measure(args)
                    else:
                        idle.measure(args)
                start, end = events.index(("wait", 15)), events.index("counters", events.index("counters") + 1)
                self.assertEqual(events[start + 1:end], ["quiet"] if normalize or fit else [])
                self.assertLess(end, events.index("capture"))
                if normalize or fit:
                    self.assertLess(events.index(("prepare", normalize, fit)), events.index(("wait", 5)))
                    if not fail_capture:
                        self.assertLess(events.index("capture"), events.index("after_capture"))
                if normalize:
                    self.assertLess(events.index("capture"), events.index("restore"))
                    self.assertLess(events.index("restore"), events.index("close_handles"))
                self.assertLess(events.index("close_handles"), events.index("exit"))
                self.assertEqual(json.loads((directory / "idle-windows-fake.json").read_text())["ok"], not fail_capture)

    def observation(self):
        window = {"pid": 123, "hwnd": 456, "window_rect_physical": [0, 0, 1000, 700],
                  "client_rect_physical": [10, 75, 980, 615], "taskbar_client_overlap": False}
        times = {"creation_filetime": 42, "exit_filetime": 0}
        fixture = SimpleNamespace(process=SimpleNamespace(pid=123), require_running=lambda: None)
        tree = SimpleNamespace(owned={123: {"pid": 123, "creation_filetime": 42, "handle": 1}},
                               api=SimpleNamespace(times=lambda _handle: times))
        visual = SimpleNamespace(window_observation=lambda _pid: dict(window))
        with patch.object(idle.importlib, "import_module", return_value=visual):
            observer = idle.NormalizedObservation(fixture, tree, {})
        from visual_pointer import FixturePointer
        observer.window_identity = FixturePointer.identity
        observer.window = dict(window)
        observer.hit_probe = lambda _window: {"ok": True, "points": []}
        # No caption/check/settle methods: passive quiet observation must never call them.
        observer.pointer = SimpleNamespace(io=SimpleNamespace(position=lambda: (400, 20)),
                                           placed=(400, 20), data={"movement_observed": False})
        return observer, window, times

    def test_quiet_observer_latches_movement_and_rejects_identity_or_occlusion(self):
        observer, _, _ = self.observation()
        observer.check("quiet")
        observer.pointer.io.position = lambda: (401, 20)
        with self.assertRaisesRegex(RuntimeError, "Pointer movement"):
            observer.check("quiet")
        observer.pointer.io.position = lambda: (400, 20)
        with self.assertRaisesRegex(RuntimeError, "Pointer movement"):
            observer.check("quiet")
        self.assertTrue(observer.data["inconclusive"])
        for changed in ("identity", "exit", "geometry", "taskbar"):
            with self.subTest(changed=changed):
                observer, window, times = self.observation()
                if changed == "identity": times["creation_filetime"] = 43
                if changed == "exit": times["exit_filetime"] = 100
                if changed == "geometry": window["hwnd"] = 789
                if changed == "taskbar": window["taskbar_client_overlap"] = True
                with self.assertRaises(RuntimeError): observer.check("quiet")
                self.assertTrue(observer.data["inconclusive"])

    def test_failed_preparation_restores_before_unwinding(self):
        observer, _, _ = self.observation()
        events = []
        class Pointer:
            identity = staticmethod(lambda _window: ())
            def __init__(self, *_args): self.data = {}
            def park(self): events.append("park")
            def settle(self, *_args): raise RuntimeError("settlement failed")
            def close(self): events.append("restore")
        import visual_pointer
        observer.visual.fingerprint = repr
        with patch.object(visual_pointer, "FixturePointer", Pointer):
            with self.assertRaisesRegex(RuntimeError, "settlement failed"):
                with ExitStack() as stack:
                    observer.prepare(SimpleNamespace(fit_work_area=False, normalize_pointer=True), stack)
        self.assertEqual(events, ["park", "restore"])

    def test_quiet_wait_only_passive_callback_and_bounded_poll_interval(self):
        now, samples = [0.0], []
        def sleep(seconds): now[0] += seconds
        with patch.object(idle.time, "monotonic", lambda: now[0]), patch.object(idle.time, "sleep", sleep):
            idle.quiet_wait(1, lambda: samples.append(now[0]))
        self.assertEqual(samples, [0, .25, .5, .75, 1])


if __name__ == "__main__":
    unittest.main()
