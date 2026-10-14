"""Move only an owned fixture into its monitor work area; never resize it."""
import ctypes
from ctypes import wintypes as W
import time

from windows_capture import _libraries, _physical_pixels, _MonitorInfo, contains_rect


def capture_area(work, taskbars, *, taskbar_probe_completed=False):
    """Exclude only observed full edge strips, including auto-hide taskbars."""
    if (not isinstance(taskbars, (list, tuple))
            or (not taskbars and taskbar_probe_completed is not True)):
        raise RuntimeError("Taskbar bounds unavailable; cannot derive unobstructed capture area")
    left, top, width, height = work
    right, bottom = left + width, top + height
    for x, y, w, h in taskbars:
        if min(w, h) <= 0:
            raise RuntimeError("Invalid observed taskbar dimensions")
        x1, y1, x2, y2 = max(left, x), max(top, y), min(right, x + w), min(bottom, y + h)
        if x1 >= x2 or y1 >= y2:
            continue
        if x1 == left and x2 == right and y1 == top:
            top = y2
        elif x1 == left and x2 == right and y2 == bottom:
            bottom = y1
        elif y1 == top and y2 == bottom and x1 == left:
            left = x2
        elif y1 == top and y2 == bottom and x2 == right:
            right = x1
        else:
            raise RuntimeError("Observed taskbar is not a full edge strip; capture area is ambiguous")
        if left >= right or top >= bottom:
            raise RuntimeError("Observed taskbar leaves no positive capture area")
    return [left, top, right - left, bottom - top]


def clamp_window(window, work, *, position_policy="clamp"):
    if position_policy not in ("clamp", "top-left"):
        raise ValueError("Unknown fixture placement policy")
    x, y, width, height = window
    left, top, available_width, available_height = work
    if (min(width, height, available_width, available_height) <= 0
            or width > available_width or height > available_height):
        raise RuntimeError("Fixture does not fit monitor work area; resizing is not permitted")
    if position_policy == "top-left":
        return [left, top, width, height]
    return [min(max(x, left), left + available_width - width),
            min(max(y, top), top + available_height - height), width, height]


class WindowsPlacementIO:
    def __init__(self):
        self.user, _ = _libraries()
        self.user.SetWindowPos.argtypes = [W.HWND, W.HWND, ctypes.c_int, ctypes.c_int,
                                          ctypes.c_int, ctypes.c_int, W.UINT]
        self.user.SetWindowPos.restype = W.BOOL

    def work_area(self, hwnd):
        # rcWork is in virtual-screen coordinates, including negative monitors.
        # https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-monitorinfo
        with _physical_pixels(self.user):
            monitor = self.user.MonitorFromWindow(W.HWND(hwnd), 2)
            info = _MonitorInfo(size=ctypes.sizeof(_MonitorInfo))
            if not monitor or not self.user.GetMonitorInfoW(monitor, ctypes.byref(info)):
                raise ctypes.WinError(ctypes.get_last_error())
            rect = info.work
            return [rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top]

    def move(self, hwnd, target):
        # NOSIZE | NOZORDER | NOACTIVATE | NOOWNERZORDER | ASYNCWINDOWPOS.
        # Async prevents a hung window blocking this process; observe completion.
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowpos
        with _physical_pixels(self.user):
            if not self.user.SetWindowPos(hwnd, None, target[0], target[1], 0, 0, 0x4215):
                raise ctypes.WinError(ctypes.get_last_error())


def fit_work_area(observe, verify_identity, evidence, io=None, *, position_policy="clamp"):
    """Place an owned foreground fixture; top-left reduces, not rules out, overlays."""
    if position_policy not in ("clamp", "top-left"):
        raise ValueError("Unknown fixture placement policy")
    io = io or WindowsPlacementIO()
    evidence.update(enabled=True, ok=False, move_attempted=False, move_observed=False,
                    position_policy=position_policy,
                    resize_permitted=False, activation_requested=False,
                    z_order_change_requested=False, settings_changed=False,
                    position_restoration="Unneeded: owned fixture is terminated after capture")
    verify_identity()
    before = observe()
    evidence["before"] = before
    work = io.work_area(before["hwnd"])
    evidence["work_area_physical"] = work
    evidence["observed_taskbar_rects"] = before.get("taskbar_rects")
    available = capture_area(work, before.get("taskbar_rects"),
                             taskbar_probe_completed=before.get("taskbar_probe_completed"))
    evidence["available_capture_area_physical"] = available
    target = clamp_window(before["window_rect_physical"], available,
                          position_policy=position_policy)
    evidence["requested_window_rect_physical"] = target
    current = observe()
    verify_identity()
    keys = ("pid", "hwnd", "window_rect_physical", "client_rect_physical")
    if any(current[key] != before[key] for key in keys):
        raise RuntimeError("Owned foreground/geometry changed before work-area placement")
    if io.work_area(before["hwnd"]) != work:
        raise RuntimeError("Monitor work area changed before placement")
    if capture_area(work, current.get("taskbar_rects"),
                    taskbar_probe_completed=current.get("taskbar_probe_completed")) != available:
        raise RuntimeError("Observed taskbar capture area changed before placement")
    if target != before["window_rect_physical"]:
        evidence["move_attempted"] = True
        io.move(before["hwnd"], target)
    started = time.monotonic()
    while True:
        verify_identity()
        after = observe()
        evidence["after"] = after
        if (after["pid"] != before["pid"] or after["hwnd"] != before["hwnd"]
                or after["window_rect_physical"][2:] != before["window_rect_physical"][2:]
                or after["client_rect_physical"][2:] != before["client_rect_physical"][2:]):
            raise RuntimeError("Owned window identity or dimensions changed during placement")
        if io.work_area(after["hwnd"]) != work:
            raise RuntimeError("Monitor work area changed during placement")
        current_area = capture_area(work, after.get("taskbar_rects"),
                                    taskbar_probe_completed=after.get("taskbar_probe_completed"))
        evidence["available_capture_area_after_physical"] = current_area
        if not contains_rect(current_area, target):
            raise RuntimeError("Observed taskbar now obstructs requested placement")
        if after["window_rect_physical"] == target:
            evidence.update(ok=True, move_observed=evidence["move_attempted"],
                            size_unchanged=True, elapsed_seconds=time.monotonic() - started)
            return
        if time.monotonic() - started >= 2:
            raise TimeoutError("Owned fixture placement did not reach requested bounds within 2 seconds")
        time.sleep(.05)
