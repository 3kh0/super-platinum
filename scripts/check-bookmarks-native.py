#!/usr/bin/env python3
"""Run the disconnected bookmark DOM workflow at both sizes; never builds.

The existing Rust check emits a text marker, not per-step JSON. This runner
validates that exact marker and records capture/cleanup evidence separately.
PNG inspection is still required; synthetic DOM events do not prove OS input.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import sys
import time
import warnings

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FIXTURES = ("bookmarks", "bookmarks-narrow")
MARKER = "bookmarks native check"
SUCCESS = ("super-platinum: bookmarks native check passed: list, validation, "
           "draft retention, edit, folders, collapse, movement, cascade removal cancellation, layout")
MAX_LOG_BYTES = 4 * 1024 * 1024


def validate_marker(log, *, final=False):
    """Only one complete, exact success line grants completion authority."""
    records = []
    partial = False
    for line in log.splitlines(keepends=True):
        if MARKER not in line:
            continue
        if "bookmarks native check failed" in line:
            raise RuntimeError("Native bookmark assertion failed: " + line.strip())
        if not line.endswith("\n"):
            partial = True
            continue
        if line.rstrip("\r\n") != SUCCESS:
            raise RuntimeError("Unrecognized bookmark completion record")
        records.append(SUCCESS)
    if len(records) > 1 or (records and partial):
        raise RuntimeError("Duplicate or additional incomplete bookmark record")
    if final and (len(records) != 1 or partial):
        raise RuntimeError("Missing complete bookmark success marker")
    return records[0] if records else None


def read_marker(path, *, final=False):
    with path.open("rb") as source:
        raw = source.read(MAX_LOG_BYTES + 1)
    if len(raw) > MAX_LOG_BYTES:
        raise RuntimeError("Bookmark fixture log exceeds bounded evidence size")
    return validate_marker(raw.decode("utf-8", errors="strict"), final=final)


def validate_state(response):
    data = response.get("data") if isinstance(response, dict) else None
    if not isinstance(response, dict) or response.get("ok") is not True:
        raise RuntimeError("Invalid final agent state response")
    expected = {"active_team": "T1", "active_channel": "C2", "channel_tab": "Bookmarks",
                "main_view": "home", "allow_destructive": False, "thread_open": False}
    if not isinstance(data, dict) or any(
            type(data.get(key)) is not type(value) or data[key] != value
            for key, value in expected.items()):
        raise RuntimeError("Final agent state left the disconnected bookmark fixture scope")
    workspaces = data.get("workspaces")
    if (not isinstance(workspaces, list) or len(workspaces) != 2
            or {w.get("team_id") for w in workspaces if isinstance(w, dict)} != {"T1", "T2"}
            or any(not isinstance(w, dict) or w.get("rt_connected") is not False for w in workspaces)):
        raise RuntimeError("Unexpected workspace or connected realtime state")
    return {key: data[key] for key in expected}


def validate_capture(response, path, dimensions):
    data = response.get("data") if isinstance(response, dict) else None
    if (not isinstance(response, dict) or response.get("ok") is not True
            or not isinstance(data, dict) or not isinstance(data.get("path"), str)
            or Path(data["path"]).resolve() != path.resolve()
            or any(type(data.get(k)) is not int or data[k] <= 0 for k in ("width", "height"))
            or [data["width"], data["height"]] != list(dimensions)):
        raise RuntimeError("Screenshot response does not match the owned PNG path/dimensions")


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def run(fixture, binary, captures, timeout=45):
    if fixture not in FIXTURES or type(timeout) is not int or not 1 <= timeout <= 120:
        raise ValueError("Invalid bookmark fixture or bounded completion timeout")
    captures.mkdir(parents=True, exist_ok=True)
    log = captures / f"{fixture}.log"
    receipt_path = captures / f"{fixture}.json"
    screenshot = captures / f"{fixture}.png"
    native = NativeFixture(fixture, log, binary, {"SUPER_PLATINUM_BOOKMARKS_CHECK": "1"})
    evidence = {
        "fixture": fixture, "platform": sys.platform, "binary": str(binary), "ok": False,
        "scope": "disconnected synthetic DOM bookmark workflow",
        "nativeEvidenceKind": "exact text marker, not structured per-step receipt",
        "fixtureConnectionGuard": "bookmarks_check::run rejects session or Transport before DOM evaluation",
        "visuallyInspected": False,
        "limitations": ["No trusted keyboard, clipboard, Slack delivery or successful remote mutation proof.",
                        "Final PNG only; intermediate DOM assertions are attested by the native marker.",
                        "PNG includes native chrome; DOM viewport and occlusion are not measured.",
                        "Cleanup covers the owned process and NativeFixture scratch, not independently enumerated descendants."],
    }
    temporary = None
    caught = []
    try:
        evidence["binarySha256"] = sha256(binary)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            with native:
                evidence["pid"] = native.process.pid
                temporary = Path(native._temporary.name)
                evidence["privateTemporary"] = str(temporary)
                try:
                    deadline = time.monotonic() + timeout
                    next_focus = time.monotonic() + 2
                    while True:
                        native.require_running()
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            raise TimeoutError("Bookmark completion timed out; see " + str(log))
                        if read_marker(log):
                            break
                        if time.monotonic() >= next_focus:
                            native.control("focus-window", timeout=min(3, remaining))
                            next_focus = time.monotonic() + 2
                        time.sleep(min(.1, remaining))
                    evidence["nativeMarker"] = read_marker(log, final=True)
                    time.sleep(.25)
                    state = native.control("state", timeout=5)
                    evidence["finalState"] = validate_state(state)
                    (captures / f"{fixture}-state.json").write_text(
                        json.dumps(state, indent=2) + "\n", encoding="utf-8")
                    capture = native.capture(screenshot)
                    with screenshot.open("rb") as source:
                        header = source.read(24)
                    dimensions = struct.unpack(">II", header[16:24])
                    validate_capture(capture, screenshot, dimensions)
                    evidence.update(screenshot=str(screenshot), capture=capture,
                                    pngDimensions=list(dimensions))
                    read_marker(log, final=True)
                    native.require_running()
                except BaseException:
                    try:
                        failure = captures / f"{fixture}-failure.png"
                        native.capture(failure)
                        evidence["failureScreenshot"] = str(failure)
                    except Exception as error:
                        evidence["failureCaptureError"] = str(error)
                    raise
        evidence["nativeMarker"] = read_marker(log, final=True)
        if native.process.poll() is None or temporary.exists():
            raise RuntimeError("Owned bookmark process or private scratch did not clean up")
        evidence["binarySha256After"] = sha256(binary)
        if evidence["binarySha256After"] != evidence["binarySha256"]:
            raise RuntimeError("Native binary changed during bookmark verification")
        if caught:
            raise RuntimeError("Native fixture emitted cleanup/runtime warnings")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        if native.process is not None:
            evidence.update(pid=native.process.pid, ownedProcessExited=native.process.poll() is not None,
                            exitCode=native.process.poll())
        evidence["privateTemporaryRemoved"] = temporary is not None and not temporary.exists()
        evidence["warnings"] = [str(w.message) for w in caught]
        receipt_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(receipt_path), "ok": evidence["ok"]}), flush=True)
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui/bookmarks-native")))
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--timeout", type=int, default=45, choices=range(1, 121), metavar="1..120")
    args = parser.parse_args()
    for fixture in dict.fromkeys(args.fixture or FIXTURES):
        run(fixture, args.binary.resolve(), args.captures.resolve(), args.timeout)


if __name__ == "__main__":
    main()
