"""Single-attempt caption budgets and post-response custody; no native APIs."""
import copy
from contextlib import nullcontext
import inspect
from pathlib import Path
import runpy
import unittest
from unittest.mock import MagicMock, patch

import visual_pointer as P


WINDOW = {"pid": 123, "hwnd": 456, "window_rect_physical": [100, 100, 1000, 700],
          "client_rect_physical": [110, 175, 980, 615],
          "virtual_desktop_rect": [0, 0, 2560, 1600],
          "foreground_owned": True, "taskbar_client_overlap": False}


class CaptionTests(unittest.TestCase):
    def probe(self, budget=None, returned=1, result=2, error=0, owners=(456, 456)):
        user = MagicMock()
        user.WindowFromPoint.side_effect = owners
        clock = [10.0]
        def send(*args):
            self.assertEqual(clear.call_args.args, (0,))
            args[-1]._obj.value = result
            clock[0] += .75
            return returned
        user.SendMessageTimeoutW.side_effect = send
        with patch.object(P, '_libraries', return_value=(user, None)):
            io = P.WindowsPointerIO(**({} if budget is None else {'caption_timeout_ms': budget}))
        with patch.object(P, '_physical_pixels', return_value=nullcontext()), \
                patch.object(P.ctypes, 'set_last_error', create=True) as clear, \
                patch.object(P.ctypes, 'get_last_error', return_value=error, create=True), \
                patch.object(P.time, 'monotonic', side_effect=lambda: clock[0]):
            try:
                value = io.caption(456, (600, 125))
            except RuntimeError as exc:
                value = exc
        return io, user, value

    def test_default_and_explicit_bounded_budget_are_one_original_message(self):
        for budget, expected in ((None, 250), (2000, 2000)):
            with self.subTest(budget=budget):
                io, user, result = self.probe(budget)
                self.assertIs(result, True)
                self.assertEqual(user.SendMessageTimeoutW.call_count, 1)
                args = user.SendMessageTimeoutW.call_args.args
                self.assertEqual(args[:-1], (456, 0x84, 0, 600 | (125 << 16), 0x22, expected))
                self.assertEqual(io.caption_probes, [{
                    'timeout_ms': expected, 'hwnd': 456, 'point': (600, 125),
                    'flags': 0x22, 'required_result': 2, 'send_called': True,
                    'return_value': 1, 'result': 2, 'last_error': 0,
                    'elapsed_seconds': .75, 'window_from_point': 456,
                    'window_from_point_after': 456}])

    def test_zero_return_never_accepted_or_retried_even_with_caption_output(self):
        # ERROR_SUCCESS is generic failure, not implicit successful delivery.
        for error in (0, 1460, 1400, 5):
            with self.subTest(error=error):
                io, user, result = self.probe(2000, returned=0, error=error)
                self.assertIsInstance(result, RuntimeError)
                self.assertEqual(user.SendMessageTimeoutW.call_count, 1)
                self.assertEqual(io.caption_probes[0]['last_error'], error)
                self.assertEqual(io.caption_probes[0]['result'], 2)
                self.assertEqual(io.caption_probes[0]['elapsed_seconds'], .75)

    def test_wrong_caption_or_point_owner_never_accepted(self):
        for result in (0, 1, 3, 20):
            io, user, accepted = self.probe(2000, result=result)
            self.assertIs(accepted, False)
            self.assertEqual(user.SendMessageTimeoutW.call_count, 1)
            self.assertEqual(io.caption_probes[0]['result'], result)
        io, user, accepted = self.probe(2000, owners=(999,))
        self.assertIs(accepted, False)
        user.SendMessageTimeoutW.assert_not_called()
        self.assertFalse(io.caption_probes[0]['send_called'])
        io, user, accepted = self.probe(2000, owners=(456, 999))
        self.assertIs(accepted, False)
        self.assertEqual(user.SendMessageTimeoutW.call_count, 1)
        self.assertEqual(io.caption_probes[0]['window_from_point_after'], 999)

    def test_invalid_budget_refused_before_any_api(self):
        with patch.object(P, '_libraries') as libraries:
            for budget in (0, -1, 2001, True, 250.0, '2000', None):
                with self.subTest(budget=budget), self.assertRaises(ValueError):
                    P.WindowsPointerIO(caption_timeout_ms=budget)
            libraries.assert_not_called()


class RevalidationTests(unittest.TestCase):
    def fixture(self):
        window = copy.deepcopy(WINDOW)
        io = MagicMock()
        # No fake WindowsPointerIO evidence; separate IO tests execute that path.
        del io.caption_probes
        io.point = (40, 50)
        io.position.side_effect = lambda: io.point
        io.move.side_effect = lambda value: setattr(io, 'point', value)
        io.caption.return_value = True
        verify = MagicMock()
        pointer = P.FixturePointer(lambda: copy.deepcopy(window), verify, io)
        return pointer, io, window, verify

    def test_actual_process_is_revalidated_after_successful_response(self):
        pointer, io, _, verify = self.fixture()
        events = []
        verify.side_effect = lambda: events.append('identity')
        io.caption.side_effect = lambda *_: events.append('caption') or True
        pointer.park()
        events.clear()
        pointer.check('edge', require_clear=True)
        self.assertEqual(events, ['identity', 'caption', 'identity'])
        self.assertEqual(pointer.data['caption_revalidations'][-1]['stage'], 'edge')

    def test_exit_during_response_refused_before_acceptance(self):
        pointer, io, _, verify = self.fixture()
        pointer.park()
        def caption(*_):
            verify.side_effect = RuntimeError('retained process exited')
            return True
        io.caption.side_effect = caption
        with self.assertRaisesRegex(RuntimeError, 'process exited'):
            pointer.check('edge', require_clear=True)
        self.assertEqual(io.caption.call_count, 2)  # park + one check, no retry

    def test_response_cannot_outlive_pointer_owner_geometry_or_clearance(self):
        changes = {
            'pointer': lambda io, w: setattr(io, 'point', (44, 55)),
            'pid': lambda io, w: w.update(pid=999),
            'hwnd': lambda io, w: w.update(hwnd=999),
            'foreground': lambda io, w: w.update(foreground_owned=False),
            'foreground-missing': lambda io, w: w.pop('foreground_owned') and None,
            'window': lambda io, w: w['window_rect_physical'].__setitem__(0, 200),
            'client': lambda io, w: w['client_rect_physical'].__setitem__(0, 200),
            'taskbar': lambda io, w: w.update(taskbar_client_overlap=True),
            'taskbar-unavailable': lambda io, w: w.update(taskbar_client_overlap=None),
        }
        for name, change in changes.items():
            with self.subTest(name=name):
                pointer, io, window, _ = self.fixture()
                pointer.park()
                io.caption.side_effect = lambda *_: change(io, window) or True
                with self.assertRaises(RuntimeError):
                    pointer.check('edge', require_clear=True)
                self.assertEqual(io.caption.call_count, 2)
                self.assertEqual(io.move.call_count, 1)
                if name == 'pointer':
                    pointer.close()
                    self.assertTrue(pointer.data['movement_observed'])
                    self.assertEqual(io.move.call_count, 1)

    def test_park_does_not_move_if_pointer_changes_during_caption(self):
        pointer, io, _, _ = self.fixture()
        io.caption.side_effect = lambda *_: setattr(io, 'point', (90, 90)) or True
        with self.assertRaisesRegex(RuntimeError, 'movement'):
            pointer.park()
        io.move.assert_not_called()

    def test_fixture_passes_budget_and_records_actual_io_diagnostics(self):
        with patch.object(P, 'WindowsPointerIO') as factory:
            factory.return_value.caption_timeout_ms = 2000
            factory.return_value.caption_probes = []
            p = P.FixturePointer(lambda: WINDOW, lambda: None, caption_timeout_ms=2000)
            factory.assert_called_once_with(caption_timeout_ms=2000)
            self.assertEqual(p.data['caption_timeout_ms'], 2000)
            self.assertIs(p.data['caption_probes'], factory.return_value.caption_probes)


class RankingOptInTests(unittest.TestCase):
    def test_only_exact_ranking_modes_opt_in_existing_normalize_default_unchanged(self):
        root = Path(__file__).parent
        runner = runpy.run_path(str(root / 'check-workflow-management-native.py'))
        from workflow_management_share_search import RANKING_CASES
        for case in RANKING_CASES:
            self.assertEqual(runner['pointer_options']('ranking:' + case),
                             {'caption_timeout_ms': 2000})
        for case in (None, 'cold-mixed', 'ranking:bogus', 'person-success-other', 'channel-success-other', 'prefixes', 'offline'):
            self.assertEqual(runner['pointer_options'](case), {})
        for case in ('person-success', 'channel-success'):
            self.assertEqual(runner['pointer_options'](case), {'caption_timeout_ms': 2000})
        self.assertEqual(inspect.signature(runner['H']['normalize']).parameters['caption_timeout_ms'].default, 250)


if __name__ == '__main__':
    unittest.main()
