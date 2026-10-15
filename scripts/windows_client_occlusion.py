"""Passive, sampled client HWND ownership; not complete pixel occlusion proof.

WindowFromPoint omits hidden/disabled windows and can skip static controls. Nine
points cannot exclude overlays between points or between observation times.
No input, focus, explicit window messages, process enumeration, or settings changes.
WindowFromPoint may internally perform hit testing; zero implicit message work
is not asserted. Compare only runs with this same observer enabled.
"""
import ctypes as C
from ctypes import wintypes as W

from windows_capture import _libraries, _physical_pixels


def client_points(rect):
    """Nine screen-physical points, two pixels inside each client edge."""
    if (not isinstance(rect, (list, tuple)) or len(rect) != 4
            or any(type(v) is not int for v in rect)):
        raise RuntimeError("Invalid physical client rectangle")
    x, y, width, height = rect
    if width < 7 or height < 7:
        raise RuntimeError("Client rectangle too small for distinct ownership samples")
    return [(px, py) for py in (y + 2, y + height // 2, y + height - 3)
            for px in (x + 2, x + width // 2, x + width - 3)]


class WindowsHitAPI:
    def __init__(self):
        self.user, _ = _libraries()
        for name, args, result in (
            ("WindowFromPoint", [W.POINT], W.HWND),
            ("GetAncestor", [W.HWND, W.UINT], W.HWND),
            ("GetWindowThreadProcessId", [W.HWND, C.POINTER(W.DWORD)], W.DWORD),
            ("GetClientRect", [W.HWND, C.POINTER(W.RECT)], W.BOOL),
            ("ClientToScreen", [W.HWND, C.POINTER(W.POINT)], W.BOOL),
        ):
            function = getattr(self.user, name)
            function.argtypes, function.restype = args, result

    def physical(self):
        # Same PER_MONITOR_AWARE_V2 thread context used by window_observation.
        # Geometry and WindowFromPoint run inside ONE physical-coordinate scope.
        return _physical_pixels(self.user)

    def client_rect(self, hwnd):
        rect, origin = W.RECT(), W.POINT()
        if (not self.user.GetClientRect(hwnd, C.byref(rect))
                or not self.user.ClientToScreen(hwnd, C.byref(origin))):
            raise C.WinError(C.get_last_error())
        return [origin.x, origin.y, rect.right - rect.left, rect.bottom - rect.top]

    def pid(self, hwnd):
        pid = W.DWORD()
        if not hwnd or not self.user.GetWindowThreadProcessId(hwnd, C.byref(pid)) or not pid.value:
            raise RuntimeError("Window ownership query failed")
        return int(pid.value)

    def root(self, hwnd):
        # GA_ROOT walks parents, NOT GA_ROOTOWNER: an owned popup is not client.
        return int(self.user.GetAncestor(hwnd, 2) or 0)

    def hit(self, point):
        return int(self.user.WindowFromPoint(W.POINT(*point)) or 0)


def sample_client_ownership(window, api=None):
    evidence = {"ok": False, "points": [], "coordinate_space": "physical-screen-pixels",
                "policy": "hit GA_ROOT equals retained fixture HWND; root PID equals fixture PID",
                "sampled_only": True}
    try:
        pid, hwnd = window["pid"], window["hwnd"]
        if type(pid) is not int or type(hwnd) is not int or pid <= 0 or hwnd <= 0:
            raise RuntimeError("Missing fixture window identity")
        expected = window["client_rect_physical"]
        points = client_points(expected)
        api = api or WindowsHitAPI()
        with api.physical():
            if api.pid(hwnd) != pid or api.root(hwnd) != hwnd:
                raise RuntimeError("Fixture root HWND identity changed")
            if api.client_rect(hwnd) != expected:
                raise RuntimeError("Client physical geometry changed before ownership samples")
            for point in points:
                record = {"position": list(point), "owned": False}
                evidence["points"].append(record)
                hit = api.hit(point)
                record["hwnd"] = hit
                if not hit:
                    raise RuntimeError("No HWND at sampled client point")
                record["pid"] = api.pid(hit)
                root = api.root(hit)
                record["root_hwnd"] = root
                if not root:
                    raise RuntimeError("Unknown root at sampled client point")
                record["root_pid"] = api.pid(root)
                # A WebView2 child can belong to a different process. Its actual
                # parent-root HWND, not its executable name or root-owner link,
                # establishes that this sampled surface is inside our window.
                record["owned"] = root == hwnd and record["root_pid"] == pid
                if not record["owned"]:
                    raise RuntimeError("Foreign HWND covers sampled client point")
            if (api.pid(hwnd) != pid or api.root(hwnd) != hwnd
                    or api.client_rect(hwnd) != expected):
                raise RuntimeError("Fixture identity/geometry changed during ownership samples")
        evidence["ok"] = True
    except (OSError, RuntimeError, KeyError, TypeError) as error:
        evidence["error"] = str(error)
    return evidence
