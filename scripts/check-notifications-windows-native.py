#!/usr/bin/env python3
"""Isolated Windows toast submission/history/cleanup and OS corner capture.

Pixels need human/agent inspection: a successful HRESULT or history record does
not prove a visible banner. This runner changes no notification preferences.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


def programs_directory():
    import ctypes
    import uuid
    from ctypes import wintypes

    class Guid(ctypes.Structure):
        _fields_ = [("data1", wintypes.DWORD), ("data2", wintypes.WORD),
                    ("data3", wintypes.WORD), ("data4", ctypes.c_ubyte * 8)]

    folder = Guid.from_buffer_copy(uuid.UUID("a77f5d77-2e2b-44c3-a6a2-aba601054a51").bytes_le)
    shell = ctypes.WinDLL("shell32")
    ole = ctypes.WinDLL("ole32")
    shell.SHGetKnownFolderPath.argtypes = [ctypes.POINTER(Guid), wintypes.DWORD,
                                         wintypes.HANDLE, ctypes.POINTER(ctypes.c_void_p)]
    shell.SHGetKnownFolderPath.restype = ctypes.c_long
    ole.CoTaskMemFree.argtypes = [ctypes.c_void_p]
    ole.CoTaskMemFree.restype = None
    pointer = ctypes.c_void_p()
    result = shell.SHGetKnownFolderPath(ctypes.byref(folder), 0, None, ctypes.byref(pointer))
    if result < 0:
        raise OSError(f"SHGetKnownFolderPath failed: 0x{result & 0xffffffff:08x}")
    try:
        return Path(ctypes.wstring_at(pointer))
    finally:
        ole.CoTaskMemFree(pointer)


def registration_snapshot():
    """Hash only this application's named registrations, without logging values."""
    import winreg

    def key(path):
        try:
            handle = winreg.OpenKey(winreg.HKEY_CURRENT_USER, path)
        except FileNotFoundError:
            return None
        with handle:
            children, values, _ = winreg.QueryInfoKey(handle)
            entries = [winreg.EnumValue(handle, index) for index in range(values)]
            names = [winreg.EnumKey(handle, index) for index in range(children)]
        digest = hashlib.sha256(repr(sorted(entries)).encode("utf-8")).hexdigest()
        return {"values_sha256": digest,
                "children": {name: key(path + "\\" + name) for name in sorted(names)}}

    paths = [
        r"Software\Classes\AppUserModelId\com.echonet.superplatinum",
        r"Software\Classes\AppUserModelId\com.echonet.superplatinum.fixture.notifications",
        r"Software\Classes\CLSID\{4B3A4C96-9F72-4AFB-A549-84CC5DB5C071}",
        r"Software\Classes\CLSID\{910CE832-EE9F-4C71-832F-0F4BDB37A2B8}",
    ]
    programs = programs_directory()
    shortcuts = [programs / "Super Platinum.lnk",
                 programs / "Super Platinum Notification Fixture.lnk"]
    return {"registry": {path: key(path) for path in paths},
            "shortcuts": {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                          if path.is_file() else None for path in shortcuts}}


@contextmanager
def cleanup_grace(native):
    """Let the fixture retire its own OS state even if capture/validation fails."""
    try:
        yield
    finally:
        deadline = time.monotonic() + 35
        while native.process.poll() is None and not native.receipts("notification-check: "):
            if time.monotonic() >= deadline:
                break
            time.sleep(.1)


def validate_armed(armed, pid, nonce):
    expected = {"ok": True, "noTransport": True, "noSession": True,
                "minimized": True, "pid": pid, "nonce": nonce,
                "app_id": "com.echonet.superplatinum.fixture.notifications",
                "appName": "Super Platinum Fixture", "title": "Super Platinum",
                "body": f"Super Platinum activation check {nonce}"}
    if any(armed.get(key) != value for key, value in expected.items()):
        raise RuntimeError("Missing exact isolated, minimized activation checkpoint")
    if (not re.fullmatch(r"[0-9a-f]{32}", nonce) or
            not isinstance(armed.get("hwnd"), int) or armed["hwnd"] <= 0 or
            armed.get("foregroundHwndBefore") == armed["hwnd"] or
            any(type(armed.get(key)) is not int or armed[key] < 0
                for key in ("activationCountBefore", "handledCountBefore"))):
        raise RuntimeError("Invalid activation checkpoint identity or baseline")


def invoke_owned_toast(armed, captures, evidence):
    """One bounded OS-provider action; never retry after ambiguous completion."""
    from windows_capture import capture_rect, primary_rect

    helper = ROOT / "scripts/windows_toast_activation.ps1"
    env = os.environ.copy()
    left, top, width, height = primary_rect()
    # Prior inspected 1100x1200 corner PNGs locate the banner at x=530..1075,
    # y=990..1180. Keep read-only point discovery within that exact region.
    observed_rect = [left + width - 570, top + height - 210, 545, 190]
    if width < 570 or height < 210:
        raise RuntimeError("Primary display cannot contain the observed banner probe region")
    evidence["uia_observed_banner_rect"] = observed_rect
    env["SP_TOAST_ACTIVATION_JSON"] = json.dumps({**armed, "observedBannerRect": observed_rect})
    output = captures / "notification-windows-activation-uia.log"
    evidence["uia_log"] = str(output)
    command = [str(Path(os.environ["SystemRoot"]) /
                   "System32/WindowsPowerShell/v1.0/powershell.exe"),
               "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
               "-File", str(helper)]
    with output.open("w", encoding="utf-8") as stream:
        process = subprocess.Popen(command, env=env, cwd=ROOT, stdout=stream,
                                   stderr=stream, creationflags=subprocess.CREATE_NO_WINDOW)
        started = time.monotonic()
        capture_delays = [.5, 1.5, 3, 5]
        try:
            while process.poll() is None:
                elapsed = time.monotonic() - started
                if elapsed >= 10:
                    raise subprocess.TimeoutExpired(command, 10)
                if capture_delays and elapsed >= capture_delays[0]:
                    delay = capture_delays.pop(0)
                    left, top, width, height = primary_rect()
                    crop_width, crop_height = min(1100, width), min(1200, height)
                    path = captures / f"notification-windows-uia-{int(delay * 1000)}ms.png"
                    capture_rect(path, left + width - crop_width,
                                 top + height - crop_height, crop_width, crop_height)
                    evidence["screenshots"].append({"path": str(path),
                        "elapsed_after_uia_start_ms": round(elapsed * 1000)})
                time.sleep(.05)
        except BaseException as error:
            process.kill()
            process.wait(timeout=5)
            if isinstance(error, subprocess.TimeoutExpired):
                raise RuntimeError("UI Automation helper timed out; invocation is not replayed") from error
            raise
    events = []
    for line in output.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith('{'):
            events.append(json.loads(line))
    evidence["uia"] = events
    if (process.returncode != 0 or [event.get("stage") for event in events] !=
            ["discovery", "inspection", "invoking", "invoked"] or
            any(event.get("nonce") != armed["nonce"] for event in events) or
            events[-1].get("count") != 1 or
            events[2].get("method") != "UIAutomation.Invoke"):
        raise RuntimeError(f"No confirmed unique fixture toast invocation; see {output}")
    return events[2]["actionTimestampMs"]


def validate_activation(activated, armed, action_ms):
    expected = {"ok": True, "noTransport": True, "noSession": True,
                "minimized": False, "pid": armed["pid"], "nonce": armed["nonce"],
                "hwnd": armed["hwnd"], "foregroundHwnd": armed["hwnd"],
                "activationCount": armed["activationCountBefore"] + 1,
                "handledCount": armed["handledCountBefore"] + 1}
    if any(activated.get(key) != value for key, value in expected.items()):
        raise RuntimeError("Warm activation did not restore exactly one owned fixture window")
    stamps = [activated.get(key) for key in
              ("callbackTimestampMs", "handledTimestampMs", "foregroundTimestampMs")]
    if (any(type(stamp) is not int for stamp in stamps) or
            not action_ms <= stamps[0] <= stamps[1] <= stamps[2] or
            stamps[2] - action_ms > 10000):
        raise RuntimeError("Activation timestamps do not follow the actual OS invocation")


def foreground_snapshot(hwnd):
    """Independent read of the owned HWND after the UI-thread receipt."""
    import ctypes
    from ctypes import wintypes

    user = ctypes.WinDLL("user32", use_last_error=True)
    user.GetForegroundWindow.argtypes = []
    user.GetForegroundWindow.restype = wintypes.HWND
    user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user.GetWindowThreadProcessId.restype = wintypes.DWORD
    user.IsIconic.argtypes = [wintypes.HWND]
    user.IsIconic.restype = wintypes.BOOL
    pid = wintypes.DWORD()
    if not user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid)):
        raise ctypes.WinError(ctypes.get_last_error())
    return {"hwnd": hwnd, "pid": pid.value,
            "foregroundHwnd": user.GetForegroundWindow() or 0,
            "minimized": bool(user.IsIconic(hwnd)), "observedTimestampMs": time.time_ns() // 1000000}


def run(binary, captures, warm_activation=False):
    from windows_capture import capture_rect, primary_rect

    captures.mkdir(parents=True, exist_ok=True)
    receipt_path = captures / "notification-windows.json"
    evidence = {"ok": False, "platform": sys.platform,
                "binary": str(binary.resolve()),
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "visible_banner_verified": False, "warm_activation_requested": warm_activation}
    checks = {"SUPER_PLATINUM_NOTIFICATION_CHECK": "1"}
    nonce = uuid.uuid4().hex if warm_activation else None
    if warm_activation:
        checks.update(SUPER_PLATINUM_NOTIFICATION_CLICK_CHECK="1",
                      SUPER_PLATINUM_NOTIFICATION_CLICK_NONCE=nonce)
    before_registration = registration_snapshot()
    native = None
    try:
        with NativeFixture("channel", captures / "notification-windows.log", binary,
                           checks) as native, cleanup_grace(native):
            evidence["pid"] = native.process.pid
            captured = False
            submitted_at = None
            capture_delays = [0] if warm_activation else [0, .5, 1.5, 3, 5]
            action_ms = None
            evidence["screenshots"] = []
            deadline = time.monotonic() + 45
            while True:
                native.require_running()
                submitted = native.receipts("notification-check-submitted: ")
                if submitted and submitted_at is None:
                    evidence["submission"] = submitted[-1]
                    submitted_at = time.monotonic()
                if (submitted_at is not None and capture_delays
                        and time.monotonic() - submitted_at >= capture_delays[0]):
                    delay = capture_delays.pop(0)
                    # Read only the OS notification corner; do not dismiss or
                    # click notifications belonging to another application.
                    left, top, width, height = primary_rect()
                    crop_width, crop_height = min(1100, width), min(1200, height)
                    name = ("notification-windows-corner.png" if delay == 0 else
                            f"notification-windows-corner-{int(delay * 1000)}ms.png")
                    path = captures / name
                    capture_rect(path, left + width - crop_width,
                                 top + height - crop_height, crop_width, crop_height)
                    if delay == 0:
                        evidence["screenshot"] = str(path)
                    evidence["screenshots"].append({"path": str(path),
                        "elapsed_after_receipt_ms": round((time.monotonic() - submitted_at) * 1000)})
                    captured = True
                if warm_activation and captured and action_ms is None:
                    armed = native.receipts("notification-click-armed: ")
                    if len(armed) != 1:
                        raise RuntimeError("Expected exactly one activation checkpoint")
                    evidence["armed"] = armed[0]
                    validate_armed(armed[0], native.process.pid, nonce)
                    action_ms = invoke_owned_toast(armed[0], captures, evidence)
                if warm_activation and action_ms is not None and "activation" not in evidence:
                    activated = native.receipts("notification-click-activated: ")
                    if activated:
                        if len(activated) != 1:
                            raise RuntimeError("Multiple activation receipts; no replay is permitted")
                        evidence["activation"] = activated[0]
                        validate_activation(activated[0], evidence["armed"], action_ms)
                        foreground = foreground_snapshot(evidence["armed"]["hwnd"])
                        evidence["foreground"] = foreground
                        if (foreground["pid"] != native.process.pid or foreground["minimized"] or
                                foreground["foregroundHwnd"] != foreground["hwnd"]):
                            raise RuntimeError("Owned fixture is not the restored foreground window")
                        path = captures / "notification-windows-activated.png"
                        native.capture(path)
                        evidence["activation_screenshot"] = str(path)
                receipts = native.receipts("notification-check: ")
                if receipts:
                    evidence["native"] = receipts[-1]
                    required = ("ok", "noTransport", "submissionAccepted",
                                "historyConfirmed", "previewRemoved",
                                "fixtureRegistrationCleanup")
                    if any(receipts[-1].get(key) is not True for key in required):
                        raise RuntimeError(f"Notification fixture failed: {receipts[-1]}")
                    if not captured:
                        raise RuntimeError("No toast submission stage was captured")
                    if capture_delays:
                        raise RuntimeError("Toast observation ended before timed captures finished")
                    submission = evidence["submission"]
                    if (not submission.get("app_id", "").startswith(
                            "com.echonet.superplatinum.fixture.notifications")
                            or submission.get("silent") is not True
                            or submission.get("preview") is not True
                            or submission.get("submitted") is not True
                            or submission.get("history_present") is not True):
                        raise RuntimeError("Missing isolated silent preview receipt")
                    if receipts[-1].get("visibleBannerVerified") is not False:
                        raise RuntimeError("OS banner visibility needs separate pixel inspection")
                    if warm_activation:
                        if (receipts[-1].get("warmActivationVerified") is not True or
                                "activation" not in evidence or
                                len(native.receipts("notification-click-activated: ")) != 1):
                            raise RuntimeError("Missing final warm activation proof")
                        evidence["warm_activation_verified"] = True
                    after_registration = registration_snapshot()
                    evidence["registration_restored"] = after_registration == before_registration
                    if not evidence["registration_restored"]:
                        evidence["registration_before"] = before_registration
                        evidence["registration_after"] = after_registration
                        raise RuntimeError("Notification check changed retained application registration")
                    evidence["ok"] = True
                    return evidence
                if time.monotonic() >= deadline:
                    raise RuntimeError("Notification fixture receipt timed out")
                time.sleep(.1)
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        # cleanup_grace can produce the final receipt after the runner has
        # rejected a UIA target. Preserve that cleanup evidence on failures too.
        if native is not None and native.log.is_file():
            final_receipts = native.receipts("notification-check: ")
            if final_receipts:
                evidence["native"] = final_receipts[-1]
        after_registration = registration_snapshot()
        evidence["registration_restored"] = after_registration == before_registration
        if not evidence["registration_restored"]:
            evidence["ok"] = False
            evidence["registration_before"] = before_registration
            evidence["registration_after"] = after_registration
        receipt_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(receipt_path), **evidence}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path,
                        default=ROOT / "tmp/agent-ui/windows-notifications")
    parser.add_argument("--warm-activation", action="store_true",
                        help="Invoke only the unique isolated fixture toast once via UI Automation")
    args = parser.parse_args()
    if sys.platform != "win32":
        raise SystemExit("This check requires native Windows")
    run(args.binary, args.captures.resolve(), args.warm_activation)


if __name__ == "__main__":
    main()
