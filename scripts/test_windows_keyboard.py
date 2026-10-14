"""Pure input-admission/protocol regressions. No Win32 calls or native launches."""
from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from windows_keyboard import (KEYUP, UNICODE, SCANCODE, EXTENDED, TEXT, InputObserver,
                              KeyboardSession, _RETAINED_OBSERVERS, admit,
                              input_batches, pending_releases, unicode_batches)

spec = spec_from_file_location("keyboard_runner", Path(__file__).with_name("check-keyboard-windows-native.py"))
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


def sample(stage, ready=False):
    event = dict(trusted=True, field=stage["field"], composing=False, key=None)
    if stage["action"] == "select-left":
        events = [dict(event, type="keydown", key="ArrowLeft", inputType=None)]
    else:
        kind = "deleteContentBackward" if stage["action"] == "backspace" else "insertText"
        events = [dict(event, type=name, inputType=kind) for name in ("beforeinput", "input")]
    return dict(ok=True, ordinal=stage["ordinal"], field=stage["field"], action=stage["action"],
                noSession=True, noTransport=True, noSend=True, exactRustState=True, events=events,
                dom=dict(focused=True, visible=True, viewport=[1280, 780],
                         text=stage["before" if ready else "after"],
                         selection=stage["before_selection" if ready else "selection"]))


class KeyboardSafetyTests(unittest.TestCase):
    def test_observer_pumps_on_own_thread_and_retains_failed_cleanup(self):
        for clean in (True, False):
            installed_on = []
            def install(*args):
                installed_on.append(threading.get_ident())
                return len(installed_on)
            user = SimpleNamespace(PeekMessageW=lambda *args: False,
                                   SetWindowsHookExW=install, SetWinEventHook=install,
                                   UnhookWindowsHookEx=lambda hook: clean,
                                   UnhookWinEvent=lambda hook: True)
            session = SimpleNamespace(user=user, kernel=SimpleNamespace(GetModuleHandleW=lambda _: 1),
                                      low_callback_type=lambda callback: callback,
                                      event_callback_type=lambda callback: callback,
                                      tag=123, hwnd=456, interruption=None)
            observer = InputObserver(session)
            try:
                observer.start()
                first = observer.heartbeat
                # Simulated main-thread blocking I/O does not stop hook pumping.
                time.sleep(.03)
                observer.check()
                self.assertGreater(observer.heartbeat, first)
                self.assertEqual(len(installed_on), 3)
                self.assertNotIn(threading.get_ident(), installed_on)
                self.assertEqual(len(set(installed_on)), 1)
            finally:
                result = observer.close()
            self.assertEqual(result, clean)
            self.assertTrue(observer.data["stopped"])
            self.assertEqual(observer.data["hooksReleased"], clean)
            if not clean:
                self.assertIn(observer, _RETAINED_OBSERVERS)
                self.assertIsNotNone(session.interruption)
                _RETAINED_OBSERVERS.remove(observer)  # No real hooks in this fake.

    def test_observer_fault_or_stale_pump_blocks_next_input(self):
        keyboard = KeyboardSession.__new__(KeyboardSession)
        keyboard.interruption = None
        keyboard.observer = SimpleNamespace(check=Mock(side_effect=RuntimeError("observer fault")))
        keyboard._insert = Mock()
        with self.assertRaisesRegex(RuntimeError, "observer fault"):
            keyboard.send("type")
        keyboard._insert.assert_not_called()
        self.assertEqual(keyboard.interruption, "observer fault")
        observer = InputObserver(SimpleNamespace(user=None))
        observer.thread = SimpleNamespace(is_alive=lambda: True)
        observer.data["ready"] = True
        observer.heartbeat = time.monotonic() - 2
        with self.assertRaisesRegex(RuntimeError, "pump deadline"):
            observer.check()

    def test_observer_stop_timeout_retains_callback_owner_and_fails(self):
        session = SimpleNamespace(user=None, interruption=None)
        observer = InputObserver(session)
        observer.thread = SimpleNamespace(ident=123, is_alive=lambda: True, join=Mock())
        self.assertFalse(observer.close())
        observer.thread.join.assert_called_once_with(timeout=3)
        self.assertTrue(observer.stop_requested.is_set())
        self.assertIn(observer, _RETAINED_OBSERVERS)
        self.assertIsNotNone(session.interruption)
        _RETAINED_OBSERVERS.remove(observer)

    def test_observer_latches_resumed_gap_before_refreshing_heartbeat(self):
        for clock in ([12.0], [10.1, 12.0]):
            session = SimpleNamespace(user=None, interruption=None)
            observer = InputObserver(session)
            observer.heartbeat = 10.0
            observer._pump = Mock()
            with patch("windows_keyboard.time.monotonic", side_effect=clock):
                with self.assertRaisesRegex(RuntimeError, "pump deadline"):
                    observer._checked_pump()
            self.assertEqual(observer.heartbeat, 10.0)
            self.assertIsNotNone(observer.error)
            self.assertEqual(session.interruption, observer.error)
            # Even a later fresh heartbeat cannot clear the latched failure.
            observer.heartbeat = time.monotonic()
            observer.thread = SimpleNamespace(is_alive=lambda: True)
            observer.data["ready"] = True
            with self.assertRaisesRegex(RuntimeError, "pump deadline"):
                observer.check()

    def test_partial_input_never_replays_and_prohibits_restore(self):
        for inserted, cleanup in ((0, 0), (1, 0), (1, 1), (2, 1), (2, 2)):
            keyboard = KeyboardSession.__new__(KeyboardSession)
            keyboard.data = {"batches": []}
            keyboard.interruption = None
            keyboard.check = Mock()
            keyboard._insert = Mock(side_effect=[inserted, cleanup])
            with self.assertRaisesRegex(RuntimeError, "partial batch"):
                keyboard.send("select-left")
            receipt = keyboard.data["batches"][0]
            expected = pending_releases(input_batches("select-left")[0], inserted)
            self.assertEqual(receipt["ownedReleases"], expected)
            self.assertEqual(receipt["cleanupIncomplete"], cleanup != len(expected))
            self.assertEqual(keyboard._insert.call_count, 2 if expected else 1)
            self.assertIsNotNone(keyboard.interruption)
            keyboard.restoration_done = False
            keyboard.observer = SimpleNamespace(close=lambda: True)
            keyboard.snapshot = lambda: dict(hwnd=123, pid=456, minimized=False)
            keyboard.held = lambda: []
            keyboard.hwnd, keyboard.pid = 123, 456
            keyboard.user = SimpleNamespace(SetForegroundWindow=Mock())
            keyboard.restore()
            keyboard.user.SetForegroundWindow.assert_not_called()
            self.assertFalse(keyboard.data["foregroundRestoration"]["attempted"])

    def test_restore_refuses_held_modifier_or_observer_cleanup_failure(self):
        for held, clean in (([0x10], True), ([], False)):
            keyboard = KeyboardSession.__new__(KeyboardSession)
            keyboard.data = {}
            keyboard.interruption = None
            keyboard.restoration_done = False
            keyboard.observer = SimpleNamespace(close=lambda: clean)
            keyboard.snapshot = lambda: dict(hwnd=123, pid=456, minimized=False)
            keyboard.held = lambda: held
            keyboard.hwnd, keyboard.pid = 123, 456
            keyboard.user = SimpleNamespace(SetForegroundWindow=Mock())
            keyboard.restore()
            keyboard.user.SetForegroundWindow.assert_not_called()
            self.assertFalse(keyboard.data["foregroundRestoration"]["attempted"])
            self.assertEqual(keyboard.data["foregroundRestoration"]["heldModifiers"], held)
            self.assertEqual(keyboard.data["foregroundRestoration"]["observerClean"], clean)

    def test_target_and_held_modifier_admission(self):
        good = dict(hwnd=123, pid=456, minimized=False)
        admit(good, 123, 456, [])
        for bad in (dict(good, hwnd=124), dict(good, pid=457), dict(good, minimized=True),
                    dict(good, minimized=None), dict(good, hwnd=0)):
            with self.subTest(bad=bad), self.assertRaises(RuntimeError):
                admit(bad, 123, 456, [])
        for held in ([0x10], [0x11], [0x12], [0x5B], [0xA3, 0xA5]):
            with self.subTest(held=held), self.assertRaises(RuntimeError):
                admit(good, 123, 456, held)

    def test_unicode_packets_are_bounded_balanced_and_not_control_keys(self):
        batches = unicode_batches("A\U0001f98a\u65e5")
        self.assertEqual([len(batch) for batch in batches], [2, 4, 2])
        self.assertEqual(batches[1], [(0xD83E, UNICODE), (0xD83E, UNICODE | KEYUP),
                                     (0xDD8A, UNICODE), (0xDD8A, UNICODE | KEYUP)])
        for text in ("", "x\n", "x\r", "\t", "\x1b", "\ud800", "x" * 65):
            with self.subTest(text=repr(text)), self.assertRaises((ValueError, UnicodeError)):
                unicode_batches(text)
        for action in ("send", "Enter", "paste", "copy", "cut", "ctrl-v", "undo"):
            with self.subTest(action=action), self.assertRaises(ValueError):
                input_batches(action)
        for action in ("type", "backspace", "select-left", "replace"):
            for batch in input_batches(action):
                self.assertLessEqual(len(batch), 4)
                self.assertEqual(pending_releases(batch, len(batch)), [])
                for scan, flags in batch:
                    if not flags & UNICODE:
                        self.assertIn(scan, (0x0E, 0x2A, 0x4B))

    def test_partial_insertions_release_only_unmatched_owned_downs(self):
        batch = input_batches("select-left")[0]
        expected = [[], [(0x2A, SCANCODE | KEYUP)],
                    [(0x4B, SCANCODE | EXTENDED | KEYUP), (0x2A, SCANCODE | KEYUP)],
                    [(0x2A, SCANCODE | KEYUP)], []]
        for count in range(5):
            self.assertEqual(pending_releases(batch, count), expected[count])
        for count in (-1, 5, True, 1.5):
            with self.subTest(count=count), self.assertRaises(ValueError):
                pending_releases(batch, count)

    def test_stage_receipts_require_trusted_edits_and_exact_model(self):
        for stage in runner.plan():
            runner.validate(sample(stage), stage, "threads")
            runner.validate(sample(stage, True), stage, "threads", ready=True)
        stage = runner.plan()[0]
        for field, value in (("trusted", False), ("field", "wrong-field"), ("composing", True),
                             ("type", "paste"), ("key", "Enter"), ("inputType", "insertFromPaste")):
            bad = sample(stage)
            bad["events"][0][field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                runner.validate(bad, stage, "threads")
        for key in ("noSession", "noTransport", "noSend", "exactRustState"):
            bad = sample(stage)
            bad[key] = False
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                runner.validate(bad, stage, "threads")
        for key, value in (("viewport", [668, 400]), ("focused", False), ("visible", False),
                           ("text", TEXT[:-1]), ("selection", [0, 0])):
            bad = sample(stage)
            bad["dom"][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                runner.validate(bad, stage, "threads")
        bad = sample(stage)
        bad["events"] = bad["events"][1:]
        with self.assertRaises(RuntimeError):
            runner.validate(bad, stage, "threads")

    def test_protocol_rejects_unsolicited_duplicate_and_early_final(self):
        first = runner.plan()[0]
        ready = "trusted-keyboard-ready: " + json.dumps(sample(first, True)) + "\n"
        done = "trusted-keyboard-stage: " + json.dumps(sample(first)) + "\n"
        self.assertEqual(len(runner.protocol_receipts(ready, 1)), 1)
        self.assertEqual(len(runner.protocol_receipts(ready + done, 2)), 2)
        self.assertEqual(runner.protocol_receipts(ready[:-1], 1), [])
        for log, expected in ((done, 1), (ready + ready, 2), (ready + done, 1),
                              ("trusted-keyboard-check: {\"ok\":true}\n", 1),
                              ("trusted-keyboard-ready: {\"ok\":false}\n", 1)):
            with self.subTest(log=log[:30]), self.assertRaises(RuntimeError):
                runner.protocol_receipts(log, expected)

    def test_acknowledgments_publish_exclusively(self):
        with tempfile.TemporaryDirectory(prefix="keyboard-ack-test-") as folder:
            directory = Path(folder)
            runner.acknowledge(directory, 0, "input")
            self.assertEqual((directory / "0.input.ack").read_bytes(), b"complete\n")
            self.assertFalse((directory / "0.input.pending").exists())
            with self.assertRaises(FileExistsError):
                runner.acknowledge(directory, 0, "input")
            self.assertEqual((directory / "0.input.ack").read_bytes(), b"complete\n")


if __name__ == "__main__":
    unittest.main()
