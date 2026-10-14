#!/usr/bin/env python3
"""Trusted Windows editing in disconnected composers; no clipboard or send keys."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from windows_keyboard import KeyboardSession, TEXT

FIXTURES = ("threads", "threads-keyboard-narrow")


def plan():
    full = len(TEXT.encode("utf-16-le")) // 2
    short = full - 1
    result = []
    for field in ("channel-composer", "thread-composer"):
        for action, before, after, before_selection, selection in (
            ("type", "", TEXT, [0, 0], [full, full]),
            ("backspace", TEXT, TEXT[:-1], [full, full], [short, short]),
            ("select-left", TEXT[:-1], TEXT[:-1], [short, short], [short - 1, short]),
            ("replace", TEXT[:-1], TEXT[:-2] + "Z", [short - 1, short], [short, short]),
        ):
            result.append(dict(ordinal=len(result), field=field, action=action, before=before,
                               after=after, before_selection=before_selection, selection=selection))
    return result


def validate(receipt, stage, fixture, ready=False):
    if (receipt.get("ok") is not True or type(receipt.get("ordinal")) is not int
            or any(receipt.get(key) != stage[key] for key in ("ordinal", "field", "action"))
            or any(receipt.get(key) is not True for key in ("noSession", "noTransport", "noSend"))):
        raise RuntimeError("Invalid trusted keyboard stage identity or isolation")
    dom = receipt.get("dom", {})
    if (dom.get("focused") is not True or dom.get("visible") is not True
            or dom.get("viewport") != expected_viewport(fixture)
            or dom.get("text") != stage["before" if ready else "after"]
            or dom.get("selection") != stage["before_selection" if ready else "selection"]):
        raise RuntimeError("Keyboard DOM text, selection, visibility, or viewport differs")
    if ready:
        return
    if receipt.get("exactRustState") is not True:
        raise RuntimeError("Native edit did not reach the exact Rust composer state")
    events = receipt.get("events")
    if not isinstance(events, list) or not 0 < len(events) < 256:
        raise RuntimeError("Missing bounded trusted event evidence")
    for event in events:
        if (event.get("trusted") is not True or event.get("field") != stage["field"]
                or event.get("composing") is not False or event.get("key") == "Enter"
                or event.get("type") not in ("keydown", "keyup", "beforeinput", "input")):
            raise RuntimeError("Untrusted, forbidden or mistargeted input event")
    for kind in ("beforeinput", "input"):
        changes = [event for event in events if event["type"] == kind]
        if stage["action"] == "select-left":
            if changes:
                raise RuntimeError("Selection changed composer text")
        else:
            expected = "deleteContentBackward" if stage["action"] == "backspace" else "insertText"
            if not changes or any(event.get("inputType") != expected for event in changes):
                raise RuntimeError("Missing trusted beforeinput/input edit evidence")
    if stage["action"] == "select-left" and not any(event["type"] == "keydown" and event.get("key") == "ArrowLeft" for event in events):
        raise RuntimeError("Missing trusted selection key")


def acknowledge(directory, ordinal, kind):
    pending = directory / f"{ordinal}.{kind}.pending"
    with pending.open("x", encoding="utf-8", newline="\n") as output:
        output.write("complete\n")
    os.link(pending, directory / f"{ordinal}.{kind}.ack")
    pending.unlink()


def protocol_receipts(log, expected_count):
    prefixes = ("trusted-keyboard-ready: ", "trusted-keyboard-stage: ", "trusted-keyboard-check: ")
    records = []
    for line in log.splitlines(keepends=True):
        if not line.endswith("\n"):
            continue
        for prefix in prefixes:
            if prefix in line:
                receipt = json.loads(line.split(prefix, 1)[1])
                if not isinstance(receipt, dict) or receipt.get("ok") is not True:
                    detail = receipt.get("error", "invalid receipt") if isinstance(receipt, dict) else "invalid receipt"
                    raise RuntimeError(f"Native keyboard assertion failed: {detail}")
                offset = len(records)
                expected = prefixes[offset % 2] if offset < 16 else prefixes[2]
                if (offset >= 17 or prefix != expected
                        or (offset < 16 and (type(receipt.get("ordinal")) is not int or receipt["ordinal"] != offset // 2))):
                    raise RuntimeError("Unsolicited, duplicate or out-of-order keyboard receipt")
                records.append(receipt)
    if len(records) > expected_count:
        raise RuntimeError("Keyboard advanced beyond the acknowledged stage")
    return records


def wait_receipt(native, keyboard, expected_count):
    deadline = time.monotonic() + 40
    while True:
        native.require_running()
        keyboard.check("waiting", modifiers=True)
        receipts = protocol_receipts(native.log.read_text(encoding="utf-8", errors="replace"), expected_count)
        if len(receipts) == expected_count:
            return receipts[-1]
        if time.monotonic() >= deadline:
            raise TimeoutError(f"Missing keyboard receipt; see {native.log}")
        time.sleep(.03)


def run(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    label = f"trusted-keyboard-{fixture}"
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary), "ok": False,
                "scope": "SendInput native editing; no OS IME, clipboard, Enter, or send", "stages": []}
    native = keyboard = None
    try:
        with binary.open("rb") as source:
            evidence["binarySha256"] = hashlib.file_digest(source, "sha256").hexdigest()
        with tempfile.TemporaryDirectory(prefix="sp-keyboard-ack-") as folder:
            acknowledgments = Path(folder)
            keyboard = KeyboardSession()
            with keyboard:
                native = NativeFixture(fixture, captures / f"{label}.log", binary, {
                    "SUPER_PLATINUM_TRUSTED_KEYBOARD_CHECK": "1",
                    "SUPER_PLATINUM_TRUSTED_KEYBOARD_ACK_DIR": folder,
                })
                with native:
                    evidence["pid"] = native.process.pid
                    try:
                        keyboard.bind(native.process.pid)
                        for stage in plan():
                            ordinal = stage["ordinal"]
                            ready = wait_receipt(native, keyboard, 2 * ordinal + 1)
                            validate(ready, stage, fixture, ready=True)
                            keyboard.check(f"stage-{ordinal}-admitted")
                            keyboard.send(stage["action"])
                            acknowledge(acknowledgments, ordinal, "input")
                            result = wait_receipt(native, keyboard, 2 * ordinal + 2)
                            validate(result, stage, fixture)
                            screenshot = captures / f"{label}-{ordinal}-{stage['action']}.png"
                            keyboard.check(f"stage-{ordinal}-before-capture")
                            native.capture(screenshot)
                            keyboard.check(f"stage-{ordinal}-after-capture")
                            evidence["stages"].append({"ready": ready, "result": result, "screenshot": str(screenshot)})
                            acknowledge(acknowledgments, ordinal, "captured")
                        final = wait_receipt(native, keyboard, 17)
                        if (final.get("stages") != 8 or final.get("channelText") != TEXT[:-2] + "Z"
                                or final.get("threadText") != TEXT[:-2] + "Z"
                                or any(final.get(key) is not True for key in ("ok", "noSession", "noTransport", "noSend", "clipboardUntouched"))):
                            raise RuntimeError("Incomplete final keyboard receipt")
                        evidence["native"] = final
                        evidence["keyboardVerified"] = True
                    except BaseException:
                        try:
                            screenshot = captures / f"{label}-failure.png"
                            native.capture(screenshot)
                            evidence["failureScreenshot"] = str(screenshot)
                        except (OSError, RuntimeError, ValueError):
                            pass
                        raise
                    finally:
                        # Restore before terminating the owned window. Never
                        # steal focus back after the user selected another app.
                        keyboard.restore()
                    if (keyboard.interruption or not keyboard.data["foregroundRestoration"]["observerClean"]):
                        raise RuntimeError(keyboard.interruption or "Input observer cleanup failed")
            evidence["processExited"] = native.process.poll() is not None
            if not evidence["processExited"]:
                raise RuntimeError("Owned keyboard fixture did not exit")
            evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        if keyboard is not None:
            evidence["input"] = keyboard.data
        if native is not None and native.process is not None:
            evidence["pid"] = native.process.pid
            evidence["processExited"] = native.process.poll() is not None
            evidence["exitCode"] = native.process.poll()
        path = captures / f"{label}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"], "error": evidence.get("error"),
                          "foregroundRestoration": keyboard.data.get("foregroundRestoration") if keyboard else None}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-keyboard")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("This trusted input runner requires Windows")
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
