#!/usr/bin/env python3
"""Capture disconnected static fixtures with bounded readiness and owned cleanup.

No build or workflow actions. Inspect the PNGs: stable native state and a first
paint receipt do not establish complete DOM/media readiness or workflow parity.
"""
import argparse
import ctypes
from ctypes import wintypes as W
from contextlib import ExitStack
import hashlib
import json
import math
import os
from pathlib import Path
import re
import struct
import sys
import time
import warnings

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

PRESETS = {
    "search-navigation": (
        "search", "search-narrow", "search-filters", "search-filters-narrow",
        "search-has-filters", "search-has-filters-narrow",
        "search-loading", "search-empty", "search-error", "search-more-error",
        "palette", "palette-people", "dms", "new-dm", "new-dm-narrow",
        "new-dm-selected", "new-dm-pending", "new-dm-error", "new-dm-directory",
        "new-dm-directory-loading", "new-dm-directory-empty",
        "new-dm-directory-error", "new-dm-directory-error-narrow",
    ),
    "resources": (
        "later", "later-files", "later-files-narrow", "later-empty", "later-error",
        "later-reminder", "later-new-reminder", "files", "files-narrow",
        "files-empty", "files-error", "files-density", "files-density-narrow",
        "channel-files", "channel-files-narrow", "channel-links", "channel-links-narrow",
        "channel-links-empty", "channel-links-error", "pins", "pins-narrow",
        "pins-empty", "pins-error", "channel-pinned-message", "pins-files", "pins-files-narrow", "bookmarks",
        "bookmarks-narrow", "bookmarks-emoji", "bookmarks-emoji-narrow", "bookmarks-edit", "bookmarks-remove", "bookmarks-error",
        "bookmarks-folder", "bookmarks-folder-remove", "bookmarks-move",
    ),
}


def known_fixtures():
    # The checked-in sweep is the explicit fixture allowlist, not a prefix match.
    # The bounded-channel hook additionally has this exact boot.rs match arm.
    catalog = (ROOT / "scripts/agent-ui-check.sh").read_text(encoding="utf-8")
    block = catalog.split("FIXTURES=(", 1)[1].split(")", 1)[0]
    names = block.split()
    if not names or any(not re.fullmatch(r"[a-z][a-z0-9-]*", name) for name in names):
        raise RuntimeError("Fixture catalog is not a literal list of known names")
    return set(names) | {"channel-window-10000"}


def selected_fixtures(presets, fixtures):
    names = [name for preset in presets for name in PRESETS[preset]] + fixtures
    unknown = set(names) - known_fixtures()
    if unknown:
        raise ValueError(f"Unknown fixture names: {', '.join(sorted(unknown))}")
    if not names:
        raise ValueError("Select --preset or repeat --fixture; no implicit full sweep")
    return list(dict.fromkeys(names))


def fingerprint(state):
    # Performance counters continue changing without a layout mutation.
    return json.dumps({key: value for key, value in state.items() if key != "performance"},
                      sort_keys=True, separators=(",", ":"))


def first_paint(state):
    value = state.get("performance", {}).get("startup", {}).get("first_paint_ms")
    return isinstance(value, (float, int)) and not isinstance(value, bool) and math.isfinite(value) and value >= 0


def wait_ready(native, timeout):
    started = time.monotonic()
    previous, consecutive, samples = None, 0, 0
    while time.monotonic() - started < timeout:
        response = native.control("state", timeout=min(3, max(.1, timeout - (time.monotonic() - started))))
        state = response.get("data", response)
        if not isinstance(state, dict):
            raise RuntimeError("Agent state is not an object")
        samples += 1
        current = fingerprint(state)
        consecutive = (consecutive + 1 if current == previous else 1) if first_paint(state) else 0
        previous = current
        if first_paint(state) and consecutive >= 3:
            return response, {"samples": samples, "stable_samples": consecutive,
                              "sample_interval_seconds": .15,
                              "elapsed_seconds": time.monotonic() - started,
                              "first_paint_reported": True, "dom_ready_asserted": False}
        time.sleep(min(.15, max(0, timeout - (time.monotonic() - started))))
    raise TimeoutError("First paint and three stable native state samples were not observed")


def png_dimensions(path, response):
    with path.open("rb") as source:
        header = source.read(24)
    if (len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n"
            or header[12:16] != b"IHDR"):
        raise RuntimeError("Capture is missing a PNG/IHDR header")
    dimensions = struct.unpack(">II", header[16:24])
    data = response.get("data", response)
    if min(dimensions) <= 0 or list(dimensions) != [data.get("width"), data.get("height")]:
        raise RuntimeError("PNG dimensions disagree with the native capture receipt")
    return list(dimensions)


def overlap(first, second):
    x, y, width, height = first
    left, top, w, h = second
    return max(x, left) < min(x + width, left + w) and max(y, top) < min(y + height, top + h)


def window_observation(pid):
    if sys.platform != "win32":
        return {"available": False, "reason": "Physical window/taskbar probe is Windows-only"}
    from windows_capture import _libraries, _physical_pixels, contains_rect, taskbar_rects
    user, _ = _libraries()
    # Existing capture helper provides physical DPI context and taskbar bounds.
    # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclientrect
    # https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-clienttoscreen
    for name, args, result in (
        ("GetForegroundWindow", [], W.HWND),
        ("GetWindowThreadProcessId", [W.HWND, ctypes.POINTER(W.DWORD)], W.DWORD),
        ("GetClientRect", [W.HWND, ctypes.POINTER(W.RECT)], W.BOOL),
        ("ClientToScreen", [W.HWND, ctypes.POINTER(W.POINT)], W.BOOL),
    ):
        function = getattr(user, name)
        function.argtypes, function.restype = args, result
    with _physical_pixels(user):
        hwnd, owner = user.GetForegroundWindow(), W.DWORD()
        user.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value != pid:
            raise RuntimeError("Owned fixture is not foreground; visual capture rejected")
        window, client, origin = W.RECT(), W.RECT(), W.POINT()
        if (not user.GetWindowRect(hwnd, ctypes.byref(window))
                or not user.GetClientRect(hwnd, ctypes.byref(client))
                or not user.ClientToScreen(hwnd, ctypes.byref(origin))):
            raise ctypes.WinError(ctypes.get_last_error())
        bounds = [window.left, window.top, window.right - window.left, window.bottom - window.top]
        client_bounds = [origin.x, origin.y, client.right, client.bottom]
        virtual = [user.GetSystemMetrics(index) for index in (76, 77, 78, 79)]
        data = {"available": True, "pid": pid, "hwnd": int(hwnd), "foreground_owned": True,
                "window_rect_physical": bounds, "client_rect_physical": client_bounds,
                "virtual_desktop_rect": virtual, "window_within_virtual_bounds": contains_rect(virtual, bounds),
                "webview_viewport": None, "other_window_occlusion_checked": False}
        try:
            bars = taskbar_rects(hwnd, allow_empty=True)
            data.update(taskbar_probe_completed=True, taskbar_rects=bars,
                        taskbar_client_overlap=any(overlap(client_bounds, bar) for bar in bars))
        except (OSError, RuntimeError) as error:
            data.update(taskbar_probe_completed=False, taskbar_client_overlap=None,
                        taskbar_probe_error=str(error))
        return data


def run(fixture, args, digest):
    destination = args.captures.resolve() / fixture
    # Refuse reuse so old captures cannot pass as new evidence or be overwritten.
    destination.mkdir(parents=True, exist_ok=False)
    native = NativeFixture(fixture, destination / "native.log", args.binary,
                           {"SUPER_PLATINUM_STARTUP_TRACE": "1"})
    evidence = {"fixture": fixture, "ok": False, "platform": sys.platform,
                "binary": str(args.binary.resolve()), "binary_sha256": digest,
                "scope": "disconnected static fixture; no workflow actions",
                "inspected": False, "workflow_verified": False,
                "limitations": ["State stability/first paint do not prove DOM or media completion.",
                                "Native client/window pixels include chrome; DOM viewport is not measured.",
                                "Screen captures can include occlusion; optional owned placement is recorded; no settings changes.",
                                "No OS clipboard, live session or dedicated workflow check is invoked."]}
    process_api = process_handle = scratch = None
    started = time.monotonic()
    try:
        with warnings.catch_warnings(record=True) as cleanup_warnings:
            try:
                with native, ExitStack() as stack:
                    evidence.update(pid=native.process.pid, startup_to_agent_seconds=time.monotonic() - started)
                    scratch = Path(native._temporary.name)
                    if sys.platform == "win32":
                        from windows_process_metrics import WindowsAPI
                        process_api = WindowsAPI()
                        process_handle = process_api.open(native.process.pid)
                        evidence["process_creation_filetime"] = process_api.times(process_handle)["creation_filetime"]
                    response, evidence["readiness"] = wait_ready(native, args.ready_timeout)
                    pointer = None
                    evidence["pointer"] = {"enabled": args.normalize_pointer}
                    evidence["placement"] = {"enabled": args.fit_work_area}
                    if args.normalize_pointer or args.fit_work_area:
                        def verify_identity():
                            native.require_running()
                            identity = process_api.times(process_handle)
                            if (identity["creation_filetime"] != evidence["process_creation_filetime"]
                                    or identity["exit_filetime"] != 0):
                                raise RuntimeError("Retained owned process identity is no longer live")
                    if args.fit_work_area:
                        from visual_placement import fit_work_area
                        fit_work_area(lambda: window_observation(native.process.pid),
                                      verify_identity, evidence["placement"])
                        response, evidence["readiness_after_placement"] = wait_ready(native, args.ready_timeout)
                    if args.normalize_pointer:
                        from visual_pointer import FixturePointer
                        pointer = FixturePointer(lambda: window_observation(native.process.pid), verify_identity)
                        evidence["pointer"] = pointer.data
                        stack.callback(pointer.close)
                        pointer.park()
                        response = pointer.settle(native, fingerprint)
                    state = response.get("data", response)
                    evidence["startup"] = state.get("performance", {}).get("startup", {})
                    state_path = destination / "state.json"
                    state_path.write_text(json.dumps(response, indent=2) + "\n", encoding="utf-8")
                    evidence["state"] = str(state_path)
                    evidence["window_before"] = window_observation(native.process.pid)
                    if pointer:
                        pointer.check("before_capture", require_clear=True)
                    path = destination / "window.png"
                    capture = native.capture(path)
                    evidence.update(screenshot=str(path), capture=capture,
                                    png_dimensions=png_dimensions(path, capture))
                    evidence["window_after"] = window_observation(native.process.pid)
                    if pointer:
                        pointer.check("after_capture", require_clear=True)
                    if evidence["window_before"] != evidence["window_after"]:
                        raise RuntimeError("Window/foreground/taskbar geometry changed during capture")
                    after = native.control("state", timeout=3)
                    after_state = after.get("data", after)
                    (destination / "state-after.json").write_text(json.dumps(after, indent=2) + "\n", encoding="utf-8")
                    evidence["state_stable_through_capture"] = fingerprint(state) == fingerprint(after_state)
                    if not evidence["state_stable_through_capture"]:
                        raise RuntimeError("Native fixture state changed during capture; retained evidence is inconclusive")
            finally:
                evidence["cleanup_warnings"] = [str(item.message) for item in cleanup_warnings]
        evidence["owned_process_exited"] = native.process is not None and native.process.poll() is not None
        evidence["exit_code"] = native.process.returncode if native.process else None
        if not evidence["owned_process_exited"]:
            raise RuntimeError("Owned process exit was not confirmed")
        if process_handle:
            departed = process_api.times(process_handle)
            evidence["process_exit_filetime"] = departed["exit_filetime"]
            if (departed["creation_filetime"] != evidence["process_creation_filetime"]
                    or departed["exit_filetime"] == 0):
                raise RuntimeError("Owned retained process identity/exit did not validate")
        with args.binary.open("rb") as source:
            evidence["binary_sha256_after"] = hashlib.file_digest(source, "sha256").hexdigest()
        if evidence["binary_sha256_after"] != digest:
            raise RuntimeError("Binary changed during sweep; capture cannot identify one build")
        if scratch.exists():
            raise RuntimeError("Owned fixture scratch cleanup is incomplete; capture retained")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        if process_handle:
            try:
                departed = process_api.times(process_handle)
                evidence["cleanup_process_identity"] = {
                    "pid": native.process.pid,
                    "creation_filetime": departed["creation_filetime"],
                    "exit_filetime": departed["exit_filetime"],
                    "same_creation": departed["creation_filetime"] == evidence["process_creation_filetime"],
                }
            except OSError as error:
                evidence["cleanup_identity_error"] = str(error)
                evidence["ok"] = False
            process_api.kernel.CloseHandle(process_handle)
        evidence["owned_process_exited"] = native.process is not None and native.process.poll() is not None
        evidence["exit_code"] = native.process.returncode if native.process else None
        evidence["private_temp_removed"] = not scratch.exists() if scratch else None
        evidence["elapsed_seconds"] = time.monotonic() - started
        evidence["descendant_profile_cleanup"] = "Not independently asserted; only NativeFixture-owned scratch cleanup observed"
        receipt = destination / "receipt.json"
        receipt.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"fixture": fixture, "ok": evidence["ok"], "receipt": str(receipt)}), flush=True)
    if not evidence["ok"]:
        # A final retained-handle probe can fail after the capture passed. Do
        # not return a successful shell status for that incomplete evidence.
        raise RuntimeError(f"Final cleanup evidence failed; see {receipt}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preset", action="append", choices=PRESETS, default=[])
    parser.add_argument("--fixture", action="append", default=[], help="Exact catalog/boot fixture name; repeatable")
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-visual")
    parser.add_argument("--ready-timeout", type=float, default=15, help="Bound first-paint/state settling, 1-30 seconds")
    parser.add_argument("--normalize-pointer", action="store_true",
                        help="Windows: park on owned title bar, wait for taskbar/profile-hover settlement, restore safely")
    parser.add_argument("--fit-work-area", action="store_true",
                        help="Windows: move owned fixture into monitor work area without resizing or activating")
    parser.add_argument("--list", action="store_true", help="List known fixtures/presets without launching")
    args = parser.parse_args()
    if args.list:
        print(json.dumps({"presets": PRESETS, "fixtures": sorted(known_fixtures())}, indent=2))
        return
    if not math.isfinite(args.ready_timeout) or not 1 <= args.ready_timeout <= 30:
        parser.error("Readiness timeout must be between 1 and 30 seconds")
    if (args.normalize_pointer or args.fit_work_area) and sys.platform != "win32":
        parser.error("Pointer normalization and work-area placement are Windows-only")
    try:
        fixtures = selected_fixtures(args.preset, args.fixture)
    except ValueError as error:
        parser.error(str(error))
    args.binary = args.binary.resolve()
    with args.binary.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    for fixture in fixtures:
        run(fixture, args, digest)


if __name__ == "__main__":
    main()
