#!/usr/bin/env python3
"""Run the disconnected bookmark DOM workflow at both sizes; never builds.

The Rust check retains the existing exact completion marker and additionally
attests every picker stage against canonical Movement, with paused native captures.
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


PICKER_PREFIX = "bookmarks-picker-stage: "
PICKER_STAGES = (
    ("prepopulated", "Bk1", "Bk2", "Bk3"),
    ("child-initial", "Bk3", "Bk2", ""),
    ("child-reopened", "Bk3", "Bk2", ""),
    ("child-before", "Bk3", "", "Bk1"),
    ("child-reset", "Bk3", "Bk2", ""),
    ("changed", "Bk1", "Bk2", "Bk3"),
    ("offline-retained", "Bk1", "Bk2", "Bk3"),
    ("reset", "Bk1", "", ""),
)
LABELS = {"": "At the end", "Bk1": "Before Design system",
          "Bk3": "Before Release checklist \u00b7 \u65e5\u672c\u8a9e"}


def validate_pickers(records):
    if not isinstance(records, list) or len(records) != len(PICKER_STAGES):
        raise RuntimeError("Missing or extra picker stages")
    for record, (stage, base, parent, before) in zip(records, PICKER_STAGES):
        if not isinstance(record, dict):
            raise RuntimeError("Malformed picker record")
        dom = record.get("dom")
        if (not isinstance(dom, dict) or dom.get("stage") != stage
                or record.get("canonical") != {"base": base, "parent": parent, "before": before}):
            raise RuntimeError("Picker stage/canonical movement mismatch")
        for key, value, label in (("parent", parent, "Launch references" if parent else "Top level"),
                                  ("before", before, LABELS[before])):
            pick = dom.get(key)
            if (not isinstance(pick, dict) or pick.get("id") != value or pick.get("label") != label
                    or type(pick.get("selectedCount")) is not int or pick["selectedCount"] != 1
                    or pick.get("visible") is not True or pick.get("disabled") is not False):
                raise RuntimeError("Picker visible label/ID/selection/availability mismatch")
    return records


def read_pickers(path):
    with path.open("rb") as source:
        raw = source.read(MAX_LOG_BYTES + 1)
    if len(raw) > MAX_LOG_BYTES:
        raise RuntimeError("Bookmark fixture log exceeds bounded evidence size")
    records = []
    for line in raw.decode("utf-8", errors="strict").splitlines(keepends=True):
        if PICKER_PREFIX not in line:
            continue
        if not line.startswith(PICKER_PREFIX) or not line.endswith("\n"):
            raise RuntimeError("Malformed or truncated picker receipt")
        records.append(json.loads(line[len(PICKER_PREFIX):]))
    return validate_pickers(records)


def validate_picker_captures(records, fixture, captures):
    for record, (stage, *_rest) in zip(records, PICKER_STAGES):
        path = captures / f"{fixture}-{stage}.png"
        with path.open("rb") as source:
            header = source.read(24)
        if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
            raise RuntimeError("Picker capture is not a PNG")
        dimensions = struct.unpack(">II", header[16:24])
        validate_capture({"ok": True, "data": record.get("capture")}, path, dimensions)


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
    native = NativeFixture(fixture, log, binary, {"SUPER_PLATINUM_BOOKMARKS_CHECK": "1",
                    "SUPER_PLATINUM_BOOKMARKS_CAPTURE_DIR": str(captures.resolve())})
    evidence = {
        "fixture": fixture, "platform": sys.platform, "binary": str(binary), "ok": False,
        "scope": "disconnected synthetic DOM bookmark workflow",
        "nativeEvidenceKind": "exact text marker and eight canonical picker receipts/captures",
        "fixtureConnectionGuard": "bookmarks_check::run rejects session or Transport before DOM evaluation",
        "visuallyInspected": False,
        "limitations": ["No trusted keyboard, clipboard, Slack delivery or successful remote mutation proof.",
                        "Eight paused picker PNGs and final PNG; remaining legacy steps use native assertions.",
                        "Picker DOM containment is checked; occlusion and trusted OS input are not measured.",
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
                    evidence["pickers"] = read_pickers(log)
                    validate_picker_captures(evidence["pickers"], fixture, captures)
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
        if read_pickers(log) != evidence["pickers"]:
            raise RuntimeError("Picker receipts changed after owned process closed")
        validate_picker_captures(evidence["pickers"], fixture, captures)
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
        primary = sys.exc_info()[1]
        evidence["cleanupErrors"] = list(native.cleanup_errors)

        def cleanup_error(stage, error):
            detail = f"{stage}: {type(error).__name__}: {error}"
            evidence["cleanupErrors"].append(detail)
            evidence["ok"] = False
            evidence.setdefault("error", detail)
            if primary is not None and hasattr(primary, "add_note"):
                primary.add_note(detail)

        evidence["ownedProcessExited"], evidence["exitCode"] = False, None
        try:
            if native.process is not None:
                evidence["pid"] = native.process.pid
                evidence["exitCode"] = native.process.poll()
                evidence["ownedProcessExited"] = evidence["exitCode"] is not None
            if not evidence["ownedProcessExited"]:
                raise RuntimeError("Owned bookmark process exit was not observed")
        except BaseException as cleanup:
            cleanup_error("process poll cleanup", cleanup)
        evidence["privateTemporaryRemoved"] = False
        try:
            evidence["privateTemporaryRemoved"] = temporary is not None and not temporary.exists()
            if temporary is not None and not evidence["privateTemporaryRemoved"]:
                raise RuntimeError("Owned bookmark temporary directory remains")
        except BaseException as cleanup:
            cleanup_error("temporary cleanup", cleanup)
        evidence["warnings"] = [str(w.message) for w in caught]
        if evidence["cleanupErrors"] or evidence["warnings"]:
            evidence["ok"] = False
            evidence.setdefault("error", "Native cleanup/runtime errors were reported")
        try:
            receipt_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
            print(json.dumps({"receipt": str(receipt_path), "ok": evidence["ok"]}), flush=True)
        except BaseException as cleanup:
            cleanup_error("receipt finalization", cleanup)
            if primary is None:
                raise
    if not evidence["ok"]:
        raise RuntimeError(evidence.get("error", "Bookmark phase did not complete"))
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
