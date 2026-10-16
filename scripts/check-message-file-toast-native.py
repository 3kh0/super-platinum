#!/usr/bin/env python3
"""Windows fixture-only simultaneous toast/menu evidence; trusted Tab, no activation."""
import argparse
import json
import os
from pathlib import Path
import sys
import tempfile
import time

from canvas_evidence import audited_fixture, png_signature
from canvas_native import CanvasCheck, capture_stage, private_checks
from file_toast_contract import Protocol, STAGES, read_events, require
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from windows_keyboard import KeyboardSession

SPEC = CanvasCheck("message-file-actions", "SUPER_PLATINUM_MESSAGE_FILE_ACTIONS_CHECK", (), {},
                   STAGES, (("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB", "saved-file-recovery.sqlite"),))


def publish_tab(directory, index):
    pending = directory / f"{index}.tab.pending"
    with pending.open("xb") as output:
        output.write(b"tabbed\n")
    os.link(pending, directory / f"{index}.tab.ack")
    pending.unlink()


def run(fixture, binary, captures):
    require(sys.platform == "win32", "Supplement requires Windows")
    captures.mkdir(parents=True, exist_ok=True)
    label = fixture.replace("message-file-actions", "message-file-toast")
    names = [f"{label}.log", f"{label}.json", f"{label}.png", f"{label}-failure.png",
             *[f"{label}-{stage}.png" for stage in STAGES]]
    require(not any((captures/name).exists() for name in names), "Evidence already exists")
    evidence = dict(ok=False, fixture=fixture, platform=sys.platform, binary=str(binary), stages=[],
                    inputs=[], events=[], syntheticToastSetup=True, actualCopyNotTested=True)
    native = keyboard = None
    try:
        with tempfile.TemporaryDirectory(prefix="sp-file-toast-ack-") as temporary:
            acknowledgments = Path(temporary)
            checks = private_checks(SPEC, acknowledgments)
            checks["SUPER_PLATINUM_MESSAGE_FILE_TOAST_CHECK"] = "1"
            native = NativeFixture(fixture, captures/f"{label}.log", binary, checks)
            protocol = Protocol(expected_viewport(fixture))
            with audited_fixture(native, evidence) as audit:
                keyboard = KeyboardSession()
                # Bind only after owned pointer normalization, and stop hooks before restoration.
                with keyboard:
                    keyboard.bind(native.process.pid)
                    deadline = time.monotonic()+45
                    while not protocol.done:
                        audit.checkpoint("toast-running")
                        keyboard.check("toast-running")
                        records = read_events(native.log)
                        require(records[:len(evidence["events"])] == evidence["events"], "Log changed")
                        for kind, value in records[len(evidence["events"]):]:
                            protocol.accept(kind, value)
                            evidence["events"].append((kind, value))
                            if kind == "stage":
                                evidence["stages"].append(capture_stage(native, SPEC, label, value,
                                    len(evidence["stages"]), captures, acknowledgments, audit=audit))
                                keyboard.check("toast-captured")
                            elif kind == "input":
                                audit.checkpoint("tab-before")
                                keyboard.check("tab-owned-before")
                                keyboard.send("tab")
                                keyboard.check("tab-owned-after")
                                audit.checkpoint("tab-after")
                                evidence["inputs"].append(value)
                                publish_tab(acknowledgments, value["index"])
                            else:
                                evidence["native"] = value
                        if not protocol.done:
                            require(time.monotonic() < deadline, "Supplement deadline")
                            time.sleep(.025)
                    audit.capture(captures/f"{label}.png", "final")
                    keyboard.check("toast-final")
                evidence["keyboard"] = keyboard.data
                restoration = keyboard.data.get("foregroundRestoration", {})
                require(restoration.get("observerClean") is True and not restoration.get("interruption")
                        and restoration.get("heldModifiers") == [], "Keyboard observer cleanup failed")
            closed = read_events(native.log, closed=True)
            require(closed == evidence["events"], "Closed log changed/duplicated/omitted receipt")
            final = Protocol(expected_viewport(fixture))
            for kind, value in closed:
                final.accept(kind, value)
            require(final.done, "Closed protocol incomplete")
            evidence["closedLogRevalidated"] = True
            for capture in evidence["captures"]:
                png_signature(Path(capture["path"]), evidence.get("expectedPngDimensions"))
            evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        if keyboard is not None:
            evidence["keyboard"] = keyboard.data
        if native is not None and native.log.exists():
            try:
                evidence["closedEvents"] = read_events(native.log, closed=True)
            except Exception as error:
                evidence.update(ok=False, closedLogError=str(error))
        path = captures/f"{label}.json"
        path.write_text(json.dumps(evidence, indent=2)+"\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    require(evidence["ok"], "Final closed-log audit failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, default=ROOT/"tmp/file-toast-native")
    parser.add_argument("--fixture", action="append", choices=("message-file-actions", "message-file-actions-narrow"))
    args = parser.parse_args()
    for fixture in args.fixture or ("message-file-actions", "message-file-actions-narrow"):
        run(fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == "__main__":
    main()
