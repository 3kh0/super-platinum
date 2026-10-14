"""Mocked Win32 enumeration; no native windows or settings are touched."""
from contextlib import nullcontext
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import windows_capture as capture


class TaskbarTests(unittest.TestCase):
    def probe(self, rows, *, enumeration_ok=True, class_ok=True, rect_ok=True,
              monitor_ok=True, allow_empty=True):
        def classify(hwnd, name, _length):
            name.value = rows[int(hwnd)][0]
            return len(name.value) if class_ok else 0

        def rectangle(hwnd, result):
            rect = result._obj
            rect.left, rect.top, rect.right, rect.bottom = rows[int(hwnd)][2]
            return rect_ok

        def enumerate_windows(callback, context):
            for hwnd in rows:
                if not callback(hwnd, context):
                    return False
            return enumeration_ok

        user = SimpleNamespace(
            EnumWindows=Mock(side_effect=enumerate_windows),
            GetClassNameW=Mock(side_effect=classify),
            GetWindowRect=Mock(side_effect=rectangle),
            IsWindowVisible=Mock(side_effect=lambda hwnd: rows[int(hwnd)][1]),
            MonitorFromWindow=Mock(return_value=42 if monitor_ok else 0),
        )
        with patch.object(capture, "_libraries", return_value=(user, None)), \
                patch.object(capture, "_physical_pixels", return_value=nullcontext()), \
                patch.object(capture.ctypes, "WINFUNCTYPE", create=True,
                             side_effect=lambda *_: lambda fn: fn), \
                patch.object(capture.ctypes, "WinError", create=True,
                             return_value=OSError("enumeration failed")), \
                patch.object(capture.ctypes, "get_last_error", create=True, return_value=5):
            return capture.taskbar_rects(999, allow_empty=allow_empty)

    def test_successful_absence_requires_explicit_opt_in(self):
        rows = {1: ("Progman", True, (0, 0, 2560, 1600))}
        self.assertEqual(self.probe(rows), [])
        with self.assertRaises(RuntimeError):
            self.probe(rows, allow_empty=False)
        self.assertEqual(self.probe({}), [])
        self.assertEqual(self.probe({2: ("Shell_TrayWnd", False, (0, 0, 0, 0))}), [])

    def test_failed_enumeration_or_classification_never_means_absence(self):
        rows = {1: ("Progman", True, (0, 0, 2560, 1600))}
        with self.assertRaises(OSError): self.probe(rows, enumeration_ok=False)
        with self.assertRaises(RuntimeError): self.probe(rows, class_ok=False)
        with self.assertRaises(RuntimeError): self.probe(rows, monitor_ok=False)

    def test_visible_taskbar_geometry_is_required_and_retained(self):
        rows = {2: ("Shell_TrayWnd", True, (0, 1528, 2560, 1600)),
                3: ("Shell_SecondaryTrayWnd", True, (-1920, 0, -1880, 1080))}
        self.assertEqual(self.probe(rows), [(0, 1528, 2560, 72), (-1920, 0, 40, 1080)])
        with self.assertRaises(RuntimeError): self.probe(rows, rect_ok=False)
        with self.assertRaises(RuntimeError):
            self.probe({2: ("Shell_TrayWnd", True, (0, 0, 0, 0))})


if __name__ == "__main__":
    unittest.main()
