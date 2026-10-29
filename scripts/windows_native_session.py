"""Read-only preflight for native fixtures; no desktop switching or input."""
import ctypes
from ctypes import wintypes as W
import os


def observe():
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    user = ctypes.WinDLL("user32", use_last_error=True)
    kernel.ProcessIdToSessionId.argtypes = [W.DWORD, ctypes.POINTER(W.DWORD)]
    kernel.ProcessIdToSessionId.restype = W.BOOL
    kernel.WTSGetActiveConsoleSessionId.argtypes = []
    kernel.WTSGetActiveConsoleSessionId.restype = W.DWORD
    user.OpenInputDesktop.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    user.OpenInputDesktop.restype = W.HANDLE
    user.CloseDesktop.argtypes = [W.HANDLE]
    user.CloseDesktop.restype = W.BOOL
    session = W.DWORD()
    if not kernel.ProcessIdToSessionId(os.getpid(), ctypes.byref(session)):
        raise ctypes.WinError(ctypes.get_last_error())
    result = dict(processSession=session.value,
                  activeConsoleSession=kernel.WTSGetActiveConsoleSessionId())
    # DESKTOP_READOBJECTS only: never switch the desktop, synthesize input,
    # change its permissions, or activate another user's process.
    desktop = user.OpenInputDesktop(0, False, 0x0001)
    result["inputDesktopOpened"] = bool(desktop)
    if desktop:
        if not user.CloseDesktop(desktop):
            raise ctypes.WinError(ctypes.get_last_error())
    else:
        result["inputDesktopError"] = ctypes.get_last_error()
    return result


def require_interactive():
    result = observe()
    # RDP sessions need not equal the physical console session. A real input
    # desktop in any nonzero session is sufficient for this preflight; existing
    # ownership, placement, capture and currentness checks still apply later.
    if result["processSession"] == 0 or not result["inputDesktopOpened"]:
        raise RuntimeError(
            "Native fixture needs an interactive Windows execution session; "
            f"observed {result}. No desktop switch or security change was attempted.")
    return result
