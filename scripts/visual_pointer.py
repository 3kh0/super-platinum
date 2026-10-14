"""Opt-in Windows fixture pointer parking; no clicks, keys, focus or settings.

Only the current owned native title bar is eligible. Polls are observations,
not a continuous input monitor: a move away and back can escape detection.
"""
import ctypes
from ctypes import wintypes as W
import time

from windows_capture import _libraries, _physical_pixels, contains_rect


def titlebar_point(observation):
    x, y, width, height = observation["window_rect_physical"]
    client = observation["client_rect_physical"]
    gap = client[1] - y
    if width < 300 or not 16 <= gap < height:
        raise RuntimeError("No usable native title bar bounds")
    point = (x + width // 2, y + gap // 3)
    if (not contains_rect(observation["virtual_desktop_rect"], (*point, 1, 1))
            or contains_rect(client, (*point, 1, 1))
            or any(not -32768 <= coordinate <= 32767 for coordinate in point)):
        raise RuntimeError("Title bar point is outside safe physical bounds")
    return point


class WindowsPointerIO:
    def __init__(self):
        self.user, _ = _libraries()
        # Primary Win32 contracts: HTCAPTION=2 proves title bar, not a button.
        # https://learn.microsoft.com/windows/win32/inputdev/wm-nchittest
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw
        # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setcursorpos
        for name, args, result in (
            ("GetCursorPos", [ctypes.POINTER(W.POINT)], W.BOOL),
            ("SetCursorPos", [ctypes.c_int, ctypes.c_int], W.BOOL),
            ("WindowFromPoint", [W.POINT], W.HWND),
            ("SendMessageTimeoutW", [W.HWND, W.UINT, ctypes.c_size_t,
                                    ctypes.c_ssize_t, W.UINT, W.UINT,
                                    ctypes.POINTER(ctypes.c_size_t)], ctypes.c_ssize_t),
        ):
            function = getattr(self.user, name)
            function.argtypes, function.restype = args, result

    def position(self):
        with _physical_pixels(self.user):
            point = W.POINT()
            if not self.user.GetCursorPos(ctypes.byref(point)):
                raise ctypes.WinError(ctypes.get_last_error())
            return point.x, point.y

    def move(self, point):
        with _physical_pixels(self.user):
            if not self.user.SetCursorPos(*point):
                raise ctypes.WinError(ctypes.get_last_error())

    def caption(self, hwnd, point):
        with _physical_pixels(self.user):
            if self.user.WindowFromPoint(W.POINT(*point)) != hwnd:
                return False
            result = ctypes.c_size_t()
            packed = (point[0] & 0xffff) | ((point[1] & 0xffff) << 16)
            # ABORTIFHUNG | ERRORONEXIT; never broadcast or send input messages.
            if not self.user.SendMessageTimeoutW(hwnd, 0x84, 0, packed, 0x22,
                                                 250, ctypes.byref(result)):
                raise RuntimeError("Owned title bar hit test failed or timed out")
            return result.value == 2


class FixturePointer:
    def __init__(self, observe, verify_identity, io=None):
        self.observe, self.verify_identity = observe, verify_identity
        self.io = io or WindowsPointerIO()
        self.original = self.placed = self.window = None
        self.data = {"enabled": True, "move_attempted": False, "move_observed": False,
                     "movement_observed": False, "restore_attempted": False,
                     "restore_observed": False, "checkpoints": [],
                     "continuous_input_monitoring": False,
                     "dom_tooltip_absence_asserted": False}

    @staticmethod
    def identity(window):
        return tuple(window[key] for key in ("pid", "hwnd", "window_rect_physical",
                                             "client_rect_physical"))

    def park(self):
        self.verify_identity()
        self.window = self.observe()
        point = titlebar_point(self.window)
        if not self.io.caption(self.window["hwnd"], point):
            raise RuntimeError("Parking point is not the owned native title bar")
        self.original = self.io.position()
        self.data.update(original=self.original, requested=point, window=self.window)
        # Recheck foreground/geometry and retained process immediately before move.
        self.verify_identity()
        if self.identity(self.observe()) != self.identity(self.window):
            raise RuntimeError("Owned foreground window changed before pointer move")
        if self.io.position() != self.original:
            self.data["movement_observed"] = True
            raise RuntimeError("Pointer moved before parking; user position preserved")
        self.data["move_attempted"] = True
        self.io.move(point)
        self.placed = point
        self.placed = self.io.position()
        self.data.update(placed=self.placed, move_observed=self.placed == point,
                         actual_position_changed=self.placed != self.original)
        if self.placed != point:
            # Do not later overwrite an unexpected position (clipping or user input).
            self.data["movement_observed"] = True
            raise RuntimeError("Pointer placement differed from requested title bar point")

    def check(self, stage, require_clear=False):
        position = self.io.position()
        self.data["movement_observed"] |= position != self.placed
        self.verify_identity()
        window = self.observe()
        self.data["checkpoints"].append({"stage": stage, "position": position,
                                          "window": window})
        if self.data["movement_observed"]:
            raise RuntimeError("Pointer movement observed; normalized capture rejected")
        if self.identity(window) != self.identity(self.window):
            raise RuntimeError("Owned foreground window changed while pointer parked")
        if not self.io.caption(window["hwnd"], self.placed):
            raise RuntimeError("Parked pointer no longer hits the owned native title bar")
        if require_clear and window.get("taskbar_client_overlap") is not False:
            raise RuntimeError("Taskbar overlap returned before normalized capture completed")
        return window

    def settle(self, native, fingerprint, timeout=5):
        started = time.monotonic()
        previous, consecutive = None, 0
        self.data["settlement"] = {"ok": False, "timeout_seconds": timeout}
        while time.monotonic() - started < timeout:
            response = native.control("state", timeout=min(1, max(.1, timeout - (time.monotonic() - started))))
            state = response.get("data", response)
            window = self.check("settlement")
            current = fingerprint(state)
            eligible = (window.get("taskbar_client_overlap") is False
                        and "profile_hover" in state and state["profile_hover"] is None)
            consecutive = (consecutive + 1 if current == previous else 1) if eligible else 0
            previous = current
            elapsed = time.monotonic() - started
            if consecutive >= 3 and elapsed >= .6:
                self.data["settlement"].update(ok=True, elapsed_seconds=elapsed,
                    stable_samples=consecutive, taskbar_client_overlap=False,
                    profile_hover_absent=True)
                return response
            time.sleep(min(.15, max(0, timeout - elapsed)))
        raise TimeoutError(f"Pointer/taskbar/profile-hover settlement was not observed within {timeout:g} seconds")

    def close(self):
        if self.placed is None:
            return
        try:
            current = self.io.position()
            self.data["position_before_restore"] = current
            self.data["movement_observed"] |= current != self.placed
            if self.data["movement_observed"]:
                self.data["restore_skipped"] = "Observed movement; do not overwrite user input"
                return
            self.data["restore_attempted"] = True
            self.io.move(self.original)
            actual = self.io.position()
            self.data.update(position_after_restore=actual, restore_observed=actual == self.original)
            if actual != self.original:
                raise RuntimeError("Pointer restoration was not observed")
        except (OSError, RuntimeError) as error:
            self.data["restore_error"] = str(error)
            raise
