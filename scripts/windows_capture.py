"""Read-only physical screen-region PNG captures using Windows GDI and stdlib.

These are desktop pixels, not PrintWindow/rendered content. A covered, hidden,
auto-hidden, or suppressed OS surface stays that way; no OS setting is changed.
Rectangles use (left, top, width, height) in physical pixels, including negatives.
"""
import ctypes
from ctypes import wintypes as W
from contextlib import contextmanager
from pathlib import Path
import struct
import sys
import zlib


def _libraries():
    if sys.platform != "win32":
        raise RuntimeError("Windows screen capture requires Windows")
    user = ctypes.WinDLL("user32", use_last_error=True)
    gdi = ctypes.WinDLL("gdi32", use_last_error=True)
    signatures = [
        (user, "SetThreadDpiAwarenessContext", [W.HANDLE], W.HANDLE),
        (user, "GetSystemMetrics", [ctypes.c_int], ctypes.c_int),
        (user, "GetDC", [W.HWND], W.HDC),
        (user, "ReleaseDC", [W.HWND, W.HDC], ctypes.c_int),
        (user, "GetWindowRect", [W.HWND, ctypes.POINTER(W.RECT)], W.BOOL),
        (user, "GetClassNameW", [W.HWND, W.LPWSTR, ctypes.c_int], ctypes.c_int),
        (user, "IsWindowVisible", [W.HWND], W.BOOL),
        (user, "MonitorFromWindow", [W.HWND, W.DWORD], W.HANDLE),
        (user, "GetMonitorInfoW", [W.HANDLE, ctypes.c_void_p], W.BOOL),
        (gdi, "CreateCompatibleDC", [W.HDC], W.HDC),
        (gdi, "CreateCompatibleBitmap", [W.HDC, ctypes.c_int, ctypes.c_int], W.HBITMAP),
        (gdi, "SelectObject", [W.HDC, W.HANDLE], W.HANDLE),
        (gdi, "DeleteObject", [W.HANDLE], W.BOOL),
        (gdi, "DeleteDC", [W.HDC], W.BOOL),
        (gdi, "BitBlt", [W.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                          ctypes.c_int, W.HDC, ctypes.c_int, ctypes.c_int, W.DWORD], W.BOOL),
        (gdi, "GetDIBits", [W.HDC, W.HBITMAP, W.UINT, W.UINT,
                             ctypes.c_void_p, ctypes.c_void_p, W.UINT], ctypes.c_int),
    ]
    for library, name, args, result in signatures:
        function = getattr(library, name)
        function.argtypes, function.restype = args, result
    return user, gdi


@contextmanager
def _physical_pixels(user):
    prior = user.SetThreadDpiAwarenessContext(W.HANDLE(-4))
    if not prior:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        yield
    finally:
        user.SetThreadDpiAwarenessContext(prior)


def primary_rect():
    """Return primary screen (left, top, width, height), in physical pixels."""
    user, _ = _libraries()
    with _physical_pixels(user):
        return 0, 0, user.GetSystemMetrics(0), user.GetSystemMetrics(1)


class _MonitorInfo(ctypes.Structure):
    _fields_ = [("size", W.DWORD), ("monitor", W.RECT), ("work", W.RECT), ("flags", W.DWORD)]


def monitor_rect(hwnd):
    """Return the physical monitor bounds associated with an observed HWND."""
    user, _ = _libraries()
    with _physical_pixels(user):
        monitor = user.MonitorFromWindow(W.HWND(hwnd), 2)
        info = _MonitorInfo(size=ctypes.sizeof(_MonitorInfo))
        if not monitor or not user.GetMonitorInfoW(monitor, ctypes.byref(info)):
            raise ctypes.WinError(ctypes.get_last_error())
        rect = info.monitor
        return rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top


def contains_rect(outer, inner):
    """Whether a positive-area rectangle is fully within another rectangle."""
    left, top, width, height = inner
    x, y, w, h = outer
    return (width > 0 and height > 0 and left >= x and top >= y
            and left + width <= x + w and top + height <= y + h)


def taskbar_rects(hwnd=None):
    """Visible taskbar rectangles; optionally only the fixture window's monitor."""
    user, _ = _libraries()
    callback_type = ctypes.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback_type, W.LPARAM]
    user.EnumWindows.restype = W.BOOL
    result = []
    with _physical_pixels(user):
        monitor = user.MonitorFromWindow(W.HWND(hwnd), 2) if hwnd else None

        @callback_type
        def visit(window, _):
            name = ctypes.create_unicode_buffer(128)
            user.GetClassNameW(window, name, len(name))
            if (name.value in ("Shell_TrayWnd", "Shell_SecondaryTrayWnd")
                    and user.IsWindowVisible(window)
                    and (not monitor or user.MonitorFromWindow(window, 2) == monitor)):
                rect = W.RECT()
                if user.GetWindowRect(window, ctypes.byref(rect)):
                    width, height = rect.right - rect.left, rect.bottom - rect.top
                    if width > 0 and height > 0:
                        result.append((rect.left, rect.top, width, height))
            return True

        if not user.EnumWindows(visit, 0):
            raise ctypes.WinError(ctypes.get_last_error())
    if not result:
        raise RuntimeError("No visible taskbar found for fixture monitor; OS settings unchanged")
    return result


class _BitmapInfo(ctypes.Structure):
    _fields_ = [("size", W.DWORD), ("width", W.LONG), ("height", W.LONG),
                ("planes", W.WORD), ("bits", W.WORD), ("compression", W.DWORD),
                ("image_size", W.DWORD), ("x_ppm", W.LONG), ("y_ppm", W.LONG),
                ("used", W.DWORD), ("important", W.DWORD), ("colors", W.DWORD * 3)]


def _chunk(kind, data):
    return (struct.pack(">I", len(data)) + kind + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff))


def capture_rect(path, left, top, width, height):
    """Save exact current screen pixels; return path/rect/dimensions metadata."""
    left, top, width, height = map(int, (left, top, width, height))
    if not (0 < width <= 16384 and 0 < height <= 16384 and width * height <= 40_000_000):
        raise ValueError("Screen capture dimensions out of bounds")
    user, gdi = _libraries()
    with _physical_pixels(user):
        virtual = tuple(user.GetSystemMetrics(index) for index in (76, 77, 78, 79))
        if not contains_rect(virtual, (left, top, width, height)):
            raise RuntimeError(f"Capture region {(left, top, width, height)} is offscreen "
                               f"relative to desktop {virtual}; visual evidence is insufficient")
        screen = user.GetDC(None)
        if not screen:
            raise ctypes.WinError(ctypes.get_last_error())
        memory = bitmap = previous = None
        try:
            memory = gdi.CreateCompatibleDC(screen)
            bitmap = gdi.CreateCompatibleBitmap(screen, width, height)
            if not memory or not bitmap:
                raise ctypes.WinError(ctypes.get_last_error())
            previous = gdi.SelectObject(memory, bitmap)
            if not previous or previous == ctypes.c_void_p(-1).value:
                raise ctypes.WinError(ctypes.get_last_error())
            if not gdi.BitBlt(memory, 0, 0, width, height, screen, left, top, 0x40CC0020):
                raise ctypes.WinError(ctypes.get_last_error())
            gdi.SelectObject(memory, previous)
            previous = None
            info = _BitmapInfo(size=40, width=width, height=-height, planes=1, bits=32)
            data = ctypes.create_string_buffer(width * height * 4)
            if gdi.GetDIBits(memory, bitmap, 0, height, data, ctypes.byref(info), 0) != height:
                raise ctypes.WinError(ctypes.get_last_error())
        finally:
            if previous and memory:
                gdi.SelectObject(memory, previous)
            if bitmap:
                gdi.DeleteObject(bitmap)
            if memory:
                gdi.DeleteDC(memory)
            user.ReleaseDC(None, screen)
    # GDI screen alpha is undefined. Encode its BGR color channels as opaque RGB.
    bgra = data.raw
    rows = bytearray()
    for y in range(height):
        row = bgra[y * width * 4:(y + 1) * width * 4]
        rgb = bytearray(width * 3)
        rgb[0::3], rgb[1::3], rgb[2::3] = row[2::4], row[1::4], row[0::4]
        rows.append(0)
        rows.extend(rgb)
    png = (b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
           + _chunk(b"IDAT", zlib.compress(rows)) + _chunk(b"IEND", b""))
    path = Path(path).resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)
    return {"path": str(path), "rect": [left, top, width, height], "width": width, "height": height}
