"""Pure regressions: no native launch, no Win32 calls, no Slack session."""
from copy import deepcopy
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import tempfile
import unittest

from file_toast_contract import LABELS, Protocol, read_events, validate_layout
from windows_keyboard import KEYUP, SCANCODE, input_batches, pending_releases


def hit_points(bounds):
    l, t, r, b = bounds
    d = min(6, (r-l)/4, (b-t)/4)
    return [dict(point=p, hit=True) for p in [[(l+r)/2, (t+b)/2], [l+d, t+d],
            [r-d, t+d], [l+d, b-d], [r-d, b-d]]]


def layout(tabs=0, visible=True, focus=0):
    controls = []
    for n, label in enumerate(LABELS):
        bounds = [110, 140+n*29, 290, 164+n*29]
        controls.append(dict(label=label, bounds=bounds, visible=True, enabled=True,
                             focused=n == focus, hits=hit_points(bounds)))
    toast = dict(visible=True, positioned=True, text="File link copied.", gap=8,
                 panelClear=True, bounds=[100, 72, 300, 122], textBounds=[110, 82, 230, 102],
                 dismissBounds=[260, 82, 290, 112], textHits=hit_points([110, 82, 230, 102]),
                 dismissHits=hit_points([260, 82, 290, 112])) if visible else None
    return dict(ok=True, viewport=[668, 380], toastExpected=visible, noActivation=True,
                tabCount=tabs, events=[dict(key="Tab", trusted=True, repeat=False, shift=False,
                  ctrl=False, alt=False, meta=False) for _ in range(tabs)], panel=[100, 130, 300, 300],
                clip=[0, 0, 668, 380], controls=controls, toast=toast,
                focusedLabel=LABELS[focus] if focus is not None else None)


def records(start=0):
    common = dict(ok=True, noSession=True, noTransport=True)
    data = [("stage", dict(common, ordinal=0, stage="toast-natural", layout=layout(focus=start)))]
    for index in range(4-start):
        data.append(("input", dict(common, index=index, layout=layout(index, focus=start+index))))
    tabs = 4-start
    data.append(("stage", dict(common, ordinal=1, stage="toast-delete-focus", layout=layout(tabs, focus=4))))
    data.append(("stage", dict(common, ordinal=2, stage="toast-dismissed", layout=layout(tabs, False, None))))
    data.append(("final", dict(common, stages=3, tabs=tabs, toastPublications=1, noRequests=True,
                noActivation=True, trustedTabs=True, layout=layout(tabs, False, None))))
    return data


def accept_all(data):
    protocol = Protocol([668, 380])
    for kind, value in data:
        protocol.accept(kind, value)
    return protocol


class ToastContractTests(unittest.TestCase):
    def test_both_natural_focus_positions_reach_delete(self):
        for start in (0, 1):
            self.assertTrue(accept_all(records(start)).done)

    def test_absent_toast_cannot_be_natural_or_focused_pass(self):
        for ordinal in (0, 5):
            data = records()
            data[ordinal][1]["layout"]["toast"] = None
            with self.assertRaises(RuntimeError):
                accept_all(data)

    def test_independent_geometry_rejects_claimed_clear_overlap(self):
        value = layout()
        value["toast"]["bounds"][3] = 123
        with self.assertRaisesRegex(RuntimeError, "8px"):
            validate_layout(value, [668, 380], True, 0)

    def test_independent_geometry_rejects_corner_occlusion(self):
        value = layout()
        value["controls"][-1]["hits"][-1]["hit"] = False
        with self.assertRaisesRegex(RuntimeError, "corner"):
            validate_layout(value, [668, 380], True, 0)

    def test_duplicate_missing_or_out_of_order_records_fail(self):
        data = records()
        for broken in (data[:1]+data, data[:2]+data[3:], data[:1]+[data[2], data[1]]+data[3:]):
            with self.assertRaises(RuntimeError):
                accept_all(broken)

    def test_focus_cannot_jump_or_start_on_delete(self):
        for index in (0, 2, 5):
            data = records()
            data[index][1]["layout"]["focusedLabel"] = "Delete…" if index != 5 else "Copy link"
            with self.assertRaises(RuntimeError):
                accept_all(data)

    def test_synthetic_modified_or_wrong_key_is_rejected(self):
        for field, value in (("trusted", False), ("key", "Enter"), ("shift", True), ("repeat", True)):
            data = records()
            data[2][1]["layout"]["events"][0][field] = value
            with self.assertRaises(RuntimeError):
                accept_all(data)

    def test_activation_or_republished_toast_rejected(self):
        data = records()
        data[-1][1]["toastPublications"] = 2
        with self.assertRaises(RuntimeError):
            accept_all(data)
        data = records()
        data[5][1]["layout"]["noActivation"] = False
        with self.assertRaises(RuntimeError):
            accept_all(data)

    def test_dismissed_baseline_requires_absence_and_final_is_unique(self):
        data = records()
        data[-2][1]["layout"]["toast"] = layout()["toast"]
        with self.assertRaises(RuntimeError):
            accept_all(data)
        data = records()
        with self.assertRaises(RuntimeError):
            accept_all(data+[deepcopy(data[-1])])

    def test_tab_is_exact_scan_pair_and_partial_cleanup_releases_only_tab(self):
        self.assertEqual(input_batches("tab"), [[(0x0F, SCANCODE), (0x0F, SCANCODE | KEYUP)]])
        self.assertEqual(pending_releases(input_batches("tab")[0], 1), [(0x0F, SCANCODE | KEYUP)])
        for forbidden in ("enter", "space", "shift-tab", "delete"):
            with self.assertRaises(ValueError):
                input_batches(forbidden)

    def test_ack_is_exact_and_exclusive(self):
        spec = spec_from_file_location("toast_runner", Path(__file__).with_name("check-message-file-toast-native.py"))
        runner = module_from_spec(spec)
        spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            runner.publish_tab(directory, 0)
            self.assertEqual((directory/"0.tab.ack").read_bytes(), b"tabbed\n")
            with self.assertRaises(FileExistsError):
                runner.publish_tab(directory, 0)

    def test_log_reader_ignores_partial_record_retains_order(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"native.log"
            path.write_text('message-file-actions-stage: {"ordinal":0}\nmessage-file-toast-input: {"index":0}\nmessage-file-toast-check: {', encoding="utf-8")
            self.assertEqual(read_events(path), [("stage", {"ordinal":0}), ("input", {"index":0})])
            with self.assertRaisesRegex(RuntimeError, "Incomplete closed"):
                read_events(path, closed=True)

    def test_numeric_boolean_impostors_rejected(self):
        data = records()
        data[2][1]["layout"]["events"][0]["trusted"] = 1
        with self.assertRaises(RuntimeError):
            accept_all(data)
        data = records(1)
        data[-1][1]["toastPublications"] = True
        with self.assertRaises(RuntimeError):
            accept_all(data)


if __name__ == "__main__":
    unittest.main()
