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
    def test_finalization_failures_preserve_primary_and_fail_without_one(self):
        for stage in ("cleanup observation", "process closure observation", "receipt write", "summary output"):
            for prior in (False, True):
                with self.subTest(stage=stage, prior=prior), tempfile.TemporaryDirectory() as temp:
                    original = ValueError("original capture failure")
                    secondary = OSError("synthetic " + stage)
                    events = []

                    class Fixture:
                        def __init__(self, *_args):
                            self.polls = self.cleanup_reads = 0
                            self.process = SimpleNamespace(pid=123, poll=self.poll)
                        def poll(self):
                            self.polls += 1
                            if stage == "process closure observation" and (prior or self.polls == 2):
                                raise secondary
                            return 0
                        @property
                        def cleanup_errors(self):
                            self.cleanup_reads += 1
                            if stage == "cleanup observation" and (prior or self.cleanup_reads == 2):
                                raise secondary
                            return []
                        def __enter__(self): return self
                        def __exit__(self, *_args): events.append("exit")
                        def require_running(self): pass
                        def control(self, *_args):
                            return {"data": {"performance": {"startup": {"first_paint_ms": 10}}}}
                        def capture(self, *_args):
                            if prior: raise original

                    class Tree:
                        def __init__(self, *_args): pass
                        def __enter__(self): return self
                        def __exit__(self, *_args): events.append("close_handles")
                        def snapshot(self): return {"processes": [{"group": "webview2_descendants"}]}

                    directory = Path(temp)
                    binary = directory / "synthetic.exe"
                    binary.write_bytes(b"not an executable")
                    args = SimpleNamespace(binary=binary, output=directory, label="finality", fixture="channel",
                                           seconds=15, settle_seconds=5, normalize_pointer=False, fit_work_area=False)
                    real_write = Path.write_text

                    def write(path, *args, **kwargs):
                        events.append("write")
                        if stage == "receipt write": raise secondary
                        return real_write(path, *args, **kwargs)

                    def output(value, **kwargs):
                        if "receipt" in json.loads(value):
                            events.append("summary")
                            if stage == "summary output": raise secondary

                    with patch.object(idle, "NativeFixture", Fixture), patch.object(idle, "OwnedProcessTree", Tree), \
                         patch.object(idle, "quiet_wait"), patch.object(idle, "summarize", return_value={"observed_coverage_complete": True}), \
                         patch.object(Path, "write_text", write), patch("builtins.print", output):
                        with self.assertRaises((ValueError, OSError)) as caught:
                            idle.measure(args)
                    self.assertIs(caught.exception, original if prior else secondary)
                    self.assertEqual(events, ["close_handles", "exit", "write", "summary"])
                    if prior:
                        self.assertTrue(any(stage in note for note in original.__notes__))
                    receipt = directory / "idle-windows-finality.json"
                    if stage == "receipt write":
                        self.assertFalse(receipt.exists())
                    elif stage != "summary output":
                        self.assertFalse(json.loads(receipt.read_text())["ok"])

    def test_normalization_precedes_settle_and_restores_before_exit(self):
        cases = [(False, False, False, False, False), (True, False, False, False, False),
                 (False, True, False, False, False), (True, True, False, False, False),
                 (True, True, True, False, False), (True, True, False, True, False),
                 (True, True, False, False, True), (True, True, True, True, False)]
        for normalize, fit, fail_capture, fail_cleanup, still_running in cases:
            with self.subTest(normalize=normalize, fit=fit, fail_capture=fail_capture,
                              fail_cleanup=fail_cleanup, still_running=still_running), tempfile.TemporaryDirectory() as temp:
                events = []
                class Fixture:
                    process = SimpleNamespace(pid=123, poll=lambda: None if still_running else 0)
                    cleanup_errors = []
                    def __init__(self, *_args): pass
                    def __enter__(self): events.append("launch"); return self
                    def __exit__(self, *_args):
                        events.append("exit")
                        if fail_cleanup:
                            self.cleanup_errors.append({"stage": "temporary", "message": "synthetic lock"})
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
                    elif fail_cleanup or still_running:
                        with self.assertRaisesRegex(RuntimeError, "did not close cleanly"):
                            idle.measure(args)
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
                receipt = json.loads((directory / "idle-windows-fake.json").read_text())
                self.assertEqual(receipt["ok"], not (fail_capture or fail_cleanup or still_running))
                self.assertEqual(receipt["process_exited"], not still_running)
                self.assertEqual(bool(receipt["cleanup_errors"]), fail_cleanup)

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

    def test_pointer_restoration_preserves_primary_or_fails_alone(self):
        import visual_pointer
        for failure_stage in (None, "prepare", "sample"):
            with self.subTest(failure_stage=failure_stage):
                observer, _, _ = self.observation()
                observer.check = lambda _stage: None
                observer.visual.fingerprint = repr
                original = ValueError("original " + str(failure_stage) + " failure")
                secondary = OSError("restoration failed")
                restored = []

                class Pointer:
                    identity = staticmethod(lambda _window: ())
                    def __init__(self, *_args): self.data = {}
                    def park(self): pass
                    def settle(self, *_args):
                        if failure_stage == "prepare": raise original
                    def close(self):
                        restored.append(True)
                        raise secondary

                with patch.object(visual_pointer, "FixturePointer", Pointer):
                    with self.assertRaises((ValueError, OSError)) as caught:
                        with ExitStack() as stack:
                            observer.prepare(SimpleNamespace(fit_work_area=False, normalize_pointer=True), stack)
                            if failure_stage == "sample": raise original
                self.assertIs(caught.exception, original if failure_stage else secondary)
                self.assertEqual(restored, [True])
                if failure_stage:
                    self.assertTrue(any("Pointer restoration also failed" in note for note in original.__notes__))

    def test_process_handle_cleanup_preserves_primary_or_fails_alone(self):
        for prior in (False, True):
            with self.subTest(prior=prior):
                # Avoid __init__, which opens real process handles.
                tree = idle.OwnedProcessTree.__new__(idle.OwnedProcessTree)
                original = ValueError("sample failed")
                secondary = OSError("handle cleanup failed")
                with patch.object(tree, "close", side_effect=secondary) as close:
                    with self.assertRaises((ValueError, OSError)) as caught:
                        with tree:
                            if prior: raise original
                self.assertIs(caught.exception, original if prior else secondary)
                close.assert_called_once_with()
                if prior:
                    self.assertTrue(any("Process handle cleanup also failed" in note for note in original.__notes__))

    def test_quiet_wait_only_passive_callback_and_bounded_poll_interval(self):
        now, samples = [0.0], []
        def sleep(seconds): now[0] += seconds
        with patch.object(idle.time, "monotonic", lambda: now[0]), patch.object(idle.time, "sleep", sleep):
            idle.quiet_wait(1, lambda: samples.append(now[0]))
        self.assertEqual(samples, [0, .25, .5, .75, 1])


if __name__ == "__main__":
    unittest.main()
