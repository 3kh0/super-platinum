"""Bounded Windows input for one owned fixture. Never accesses the clipboard.

SendInput targets the foreground input queue, not a specified HWND. Admission
checks and observation hooks reduce that race; they do not make targeting atomic.
Unicode packets test native text insertion, not an installed OS input method.
"""
import ctypes as C
from ctypes import wintypes as W
import os
import sys
import threading
import time

KEYUP, UNICODE, SCANCODE, EXTENDED = 2, 4, 8, 1
TEXT = "Native \u65e5\u672c\u8a9e \U0001f98a abc"
MODIFIERS = (0x10, 0x11, 0x12, 0x5B, 0x5C, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5)
# If Windows refuses hook cleanup, keep callback code alive until process exit.
# A daemon observer that fails to stop also retains its session through this list.
_RETAINED_OBSERVERS = []


def unicode_batches(text):
    if not isinstance(text, str) or not 0 < len(text) <= 64 or not text.isprintable():
        raise ValueError("Only short printable fixture text is admitted")
    batches = []
    for character in text:
        encoded = character.encode("utf-16-le", errors="strict")
        batch = []
        for index in range(0, len(encoded), 2):
            unit = int.from_bytes(encoded[index:index + 2], "little")
            batch.extend([(unit, UNICODE), (unit, UNICODE | KEYUP)])
        batches.append(batch)
    return batches


def input_batches(action):
    if action == "tab":
        return [[(0x0F, SCANCODE), (0x0F, SCANCODE | KEYUP)]]
    if action == "type":
        return unicode_batches(TEXT)
    if action == "replace":
        return unicode_batches("Z")
    if action == "backspace":
        return [[(0x0E, SCANCODE), (0x0E, SCANCODE | KEYUP)]]
    if action == "select-left":
        return [[(0x2A, SCANCODE), (0x4B, SCANCODE | EXTENDED),
                 (0x4B, SCANCODE | EXTENDED | KEYUP), (0x2A, SCANCODE | KEYUP)]]
    raise ValueError("Only fixture text, Backspace, Shift+Left and Tab are admitted")


def pending_releases(batch, inserted):
    """Release only unmatched injected downs after a partial insertion; no replay."""
    if type(inserted) is not int or not 0 <= inserted <= len(batch):
        raise ValueError("Invalid SendInput count")
    down = []
    for scan, flags in batch[:inserted]:
        key = (scan, flags & ~KEYUP)
        if flags & KEYUP:
            if key not in down:
                raise ValueError("Unmatched injected key up")
            down.remove(key)
        else:
            if key in down:
                raise ValueError("Repeated injected key down")
            down.append(key)
    return [(scan, flags | KEYUP) for scan, flags in reversed(down)]


def admit(snapshot, hwnd, pid, held):
    if (type(hwnd) is not int or hwnd <= 0 or type(pid) is not int or pid <= 0
            or snapshot.get("hwnd") != hwnd or snapshot.get("pid") != pid
            or snapshot.get("minimized") is not False):
        raise RuntimeError("Owned keyboard fixture is not the foreground window")
    if held:
        raise RuntimeError("A modifier is already held; no keyboard injection")


class KEYBDINPUT(C.Structure):
    _fields_ = [("wVk", W.WORD), ("wScan", W.WORD), ("dwFlags", W.DWORD),
                ("time", W.DWORD), ("dwExtraInfo", C.c_size_t)]


class MOUSEINPUT(C.Structure):
    _fields_ = [("dx", W.LONG), ("dy", W.LONG), ("mouseData", W.DWORD),
                ("dwFlags", W.DWORD), ("time", W.DWORD), ("dwExtraInfo", C.c_size_t)]


class HARDWAREINPUT(C.Structure):
    _fields_ = [("uMsg", W.DWORD), ("wParamL", W.WORD), ("wParamH", W.WORD)]


class INPUTUNION(C.Union):
    _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("hi", HARDWAREINPUT)]


class INPUT(C.Structure):
    _anonymous_ = ("value",)
    _fields_ = [("type", W.DWORD), ("value", INPUTUNION)]


class KBDLLHOOKSTRUCT(C.Structure):
    _fields_ = [("vkCode", W.DWORD), ("scanCode", W.DWORD), ("flags", W.DWORD),
                ("time", W.DWORD), ("dwExtraInfo", C.c_size_t)]


class InputObserver:
    """Hooks and message pump live on this thread, never on blocking capture I/O."""
    def __init__(self, session):
        self.session = session
        self.user = session.user
        self.ready = threading.Event()
        self.stop_requested = threading.Event()
        self.thread = threading.Thread(target=self._run, name="fixture-input-observer", daemon=True)
        self.heartbeat = 0.0
        self.error = None
        self.callbacks = []
        self.hooks = []
        self.event_hook = None
        self.data = {"ready": False, "stopped": False, "hooksReleased": False}

    def start(self):
        self.thread.start()
        if not self.ready.wait(3):
            raise RuntimeError("Input observer did not start")
        self.check()

    def check(self):
        if (self.error or not self.thread.is_alive() or not self.data["ready"]
                or time.monotonic() - self.heartbeat > 1):
            raise RuntimeError(self.error or "Input observer stopped or missed its pump deadline")

    def _pump(self):
        message = W.MSG()
        while self.user.PeekMessageW(C.byref(message), None, 0, 0, 1):
            if message.message == 0x12:
                raise RuntimeError("Input observer message loop ended")
            self.user.TranslateMessage(C.byref(message))
            self.user.DispatchMessageW(C.byref(message))

    def _checked_pump(self):
        # A resumed thread must not erase a gap during which Windows could
        # silently remove a low-level hook. Check before updating the heartbeat,
        # including time spent inside the pump and the final shutdown drain.
        for pumping in (True, False):
            checked_at = time.monotonic()
            if checked_at - self.heartbeat > 1:
                self.error = "Input observer missed its pump deadline"
                self.session.interruption = self.session.interruption or self.error
                raise RuntimeError(self.error)
            if pumping:
                self._pump()
        self.heartbeat = checked_at

    def _run(self):
        session = self.session

        def keyboard(code, message, payload):
            if code >= 0 and C.cast(payload, C.POINTER(KBDLLHOOKSTRUCT)).contents.dwExtraInfo != session.tag:
                session.interruption = session.interruption or "Other keyboard input observed"
            return self.user.CallNextHookEx(None, code, message, payload)

        def mouse(code, message, payload):
            if code >= 0:
                session.interruption = session.interruption or "Pointer input observed"
            return self.user.CallNextHookEx(None, code, message, payload)

        def foreground(hook, event, hwnd, object_id, child_id, thread, timestamp):
            if int(hwnd or 0) != session.hwnd:
                session.interruption = session.interruption or "Foreground changed"

        try:
            self.callbacks = [session.low_callback_type(keyboard), session.low_callback_type(mouse), session.event_callback_type(foreground)]
            self.heartbeat = time.monotonic()
            self._pump()  # Establish the installing thread's message queue.
            module = session.kernel.GetModuleHandleW(None)
            for kind, callback in zip((13, 14), self.callbacks[:2]):
                hook = self.user.SetWindowsHookExW(kind, callback, module, 0)
                if not hook:
                    raise C.WinError(C.get_last_error())
                self.hooks.append(hook)
            self.event_hook = self.user.SetWinEventHook(3, 3, None, self.callbacks[2], 0, 0, 0)
            if not self.event_hook:
                raise C.WinError(C.get_last_error())
            self._checked_pump()
            self.data["ready"] = True
            self.ready.set()
            while not self.stop_requested.is_set():
                self._checked_pump()
                self.stop_requested.wait(.005)
            self._checked_pump()
        except BaseException as error:
            self.error = f"Input observer failed: {type(error).__name__}: {error}"
            session.interruption = session.interruption or self.error
        finally:
            results = []
            cleanup = [(self.user.UnhookWindowsHookEx, hook) for hook in self.hooks]
            if self.event_hook:
                cleanup.append((self.user.UnhookWinEvent, self.event_hook))
            for release, hook in cleanup:
                try:
                    results.append(bool(release(hook)))
                except BaseException:
                    results.append(False)
            self.data["hooksReleased"] = all(results)
            self.data["unhookResults"] = results
            if not all(results):
                self.error = self.error or "Windows refused input observer hook cleanup"
                session.interruption = session.interruption or self.error
                _RETAINED_OBSERVERS.append(self)
            self.data["error"] = self.error
            self.data["stopped"] = True
            self.ready.set()

    def close(self):
        self.stop_requested.set()
        if self.thread.ident is not None:
            self.thread.join(timeout=3)
        if self.thread.is_alive():
            self.error = "Input observer did not stop within its deadline"
            self.data["error"] = self.error
            self.session.interruption = self.session.interruption or self.error
            _RETAINED_OBSERVERS.append(self)
        return not self.thread.is_alive() and self.data["hooksReleased"] and not self.error


class KeyboardSession:
    def __init__(self):
        if sys.platform != "win32":
            raise RuntimeError("Trusted keyboard runner requires Windows")
        if C.sizeof(INPUT) != (40 if C.sizeof(C.c_void_p) == 8 else 28):
            raise RuntimeError("Unexpected Win32 INPUT layout")
        self.user = C.WinDLL("user32", use_last_error=True)
        self.kernel = C.WinDLL("kernel32", use_last_error=True)
        self.low_callback_type = C.WINFUNCTYPE(C.c_ssize_t, C.c_int, W.WPARAM, W.LPARAM)
        self.event_callback_type = C.WINFUNCTYPE(None, W.HANDLE, W.DWORD, W.HWND, W.LONG, W.LONG, W.DWORD, W.DWORD)
        for name, args, result in [
            ("GetForegroundWindow", [], W.HWND),
            ("GetWindowThreadProcessId", [W.HWND, C.POINTER(W.DWORD)], W.DWORD),
            ("IsIconic", [W.HWND], W.BOOL), ("IsWindow", [W.HWND], W.BOOL),
            ("SetForegroundWindow", [W.HWND], W.BOOL),
            ("GetAsyncKeyState", [C.c_int], C.c_short),
            ("SendInput", [W.UINT, C.POINTER(INPUT), C.c_int], W.UINT),
            ("SetWindowsHookExW", [C.c_int, self.low_callback_type, W.HINSTANCE, W.DWORD], W.HANDLE),
            ("CallNextHookEx", [W.HANDLE, C.c_int, W.WPARAM, W.LPARAM], C.c_ssize_t),
            ("UnhookWindowsHookEx", [W.HANDLE], W.BOOL),
            ("SetWinEventHook", [W.DWORD, W.DWORD, W.HMODULE, self.event_callback_type, W.DWORD, W.DWORD, W.DWORD], W.HANDLE),
            ("UnhookWinEvent", [W.HANDLE], W.BOOL),
            ("PeekMessageW", [C.POINTER(W.MSG), W.HWND, W.UINT, W.UINT, W.UINT], W.BOOL),
            ("TranslateMessage", [C.POINTER(W.MSG)], W.BOOL),
            ("DispatchMessageW", [C.POINTER(W.MSG)], C.c_ssize_t),
        ]:
            function = getattr(self.user, name)
            function.argtypes, function.restype = args, result
        self.kernel.GetModuleHandleW.argtypes = [W.LPCWSTR]
        self.kernel.GetModuleHandleW.restype = W.HMODULE
        self.tag = (time.monotonic_ns() ^ os.getpid()) & ((1 << (8 * C.sizeof(C.c_size_t))) - 1)
        self.observer = None
        self.hwnd = self.pid = None
        self.interruption = None
        self.restoration_done = False
        self.data = {"batches": [], "checkpoints": [], "clipboardAccessed": False,
                     "monitoring": "Dedicated WinEvent/low-level input observer thread; targeting is not atomic"}

    def snapshot(self):
        hwnd = int(self.user.GetForegroundWindow() or 0)
        owner = W.DWORD()
        if hwnd:
            self.user.GetWindowThreadProcessId(hwnd, C.byref(owner))
        return {"hwnd": hwnd, "pid": owner.value, "minimized": bool(self.user.IsIconic(hwnd))}

    def held(self):
        return [key for key in MODIFIERS if self.user.GetAsyncKeyState(key) & 0x8000]

    def __enter__(self):
        self.original = self.snapshot()
        self.data["originalForeground"] = self.original
        if self.held():
            raise RuntimeError("A modifier is held before fixture launch")
        return self

    def bind(self, pid):
        current = self.snapshot()
        admit(current, current["hwnd"], pid, self.held())
        self.hwnd, self.pid = current["hwnd"], pid
        self.data["fixtureHwnd"] = self.hwnd
        self.data["fixturePid"] = pid

        self.observer = InputObserver(self)
        self.data["observer"] = self.observer.data
        self.observer.start()
        self.check("bound")

    def observe(self):
        try:
            if self.observer is None:
                raise RuntimeError("No input observer")
            self.observer.check()
        except RuntimeError as error:
            self.interruption = self.interruption or str(error)
            raise
        if self.interruption:
            raise RuntimeError(self.interruption)

    def check(self, stage, modifiers=True):
        self.observe()
        current = self.snapshot()
        self.data["checkpoints"].append({"stage": stage, **current})
        try:
            admit(current, self.hwnd, self.pid, self.held() if modifiers else [])
        except RuntimeError as error:
            self.interruption = self.interruption or str(error)
            raise

    def _insert(self, batch):
        inputs = (INPUT * len(batch))()
        for target, (scan, flags) in zip(inputs, batch):
            target.type = 1
            target.ki = KEYBDINPUT(0, scan, flags, 0, self.tag)
        return self.user.SendInput(len(inputs), inputs, C.sizeof(INPUT))

    def send(self, action):
        for index, batch in enumerate(input_batches(action)):
            self.check(f"{action}-{index}-before")
            inserted = int(self._insert(batch))
            receipt = {"action": action, "batch": index, "requested": len(batch), "inserted": inserted}
            self.data["batches"].append(receipt)
            if inserted != len(batch):
                # Do not retry text or shortcuts after partial success. Only
                # release keys this call actually left down.
                self.interruption = "Partial SendInput batch; no further input or foreground restore"
                releases = pending_releases(batch, inserted)
                receipt["cleanupRequested"] = len(releases)
                receipt["ownedReleases"] = releases
                receipt["cleanupIncomplete"] = bool(releases)
                receipt["cleanupInserted"] = int(self._insert(releases)) if releases else 0
                receipt["cleanupIncomplete"] = receipt["cleanupInserted"] != len(releases)
                raise RuntimeError("SendInput inserted a partial batch; input was not replayed")
            self.check(f"{action}-{index}-after", modifiers=False)
            for attempt in range(30):
                self.observe()
                if not self.held():
                    break
                time.sleep(.01)
            else:
                self.interruption = "Injected modifier release was not observed"
                raise RuntimeError("Injected modifier release was not observed")
        self.check(f"{action}-complete")

    def restore(self):
        if self.restoration_done:
            return
        self.restoration_done = True
        observer_clean = self.observer.close() if self.observer is not None else True
        current = self.snapshot()
        held = self.held()
        if held:
            self.interruption = self.interruption or "A modifier remains held; no foreground restore"
        evidence = {"attempted": False, "restored": False, "interruption": self.interruption,
                    "observerClean": observer_clean, "heldModifiers": held}
        self.data["foregroundRestoration"] = evidence
        if not observer_clean or self.interruption or self.hwnd is None or current["hwnd"] != self.hwnd or current["pid"] != self.pid:
            evidence["reason"] = "Respect intervening focus/input or an unbound fixture"
            return
        hwnd = self.original["hwnd"]
        owner = W.DWORD()
        if hwnd:
            self.user.GetWindowThreadProcessId(hwnd, C.byref(owner))
        if not hwnd or not self.user.IsWindow(hwnd) or owner.value != self.original["pid"] or self.user.IsIconic(hwnd):
            evidence["reason"] = "Original window disappeared, changed identity, or was minimized"
            return
        evidence["attempted"] = True
        evidence["apiSucceeded"] = bool(self.user.SetForegroundWindow(hwnd))
        evidence["restored"] = self.snapshot()["hwnd"] == hwnd
        evidence["reason"] = "Restored" if evidence["restored"] else "Original foreground was not observed after the restore request"

    def __exit__(self, *_):
        self.restore()
