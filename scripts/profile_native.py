"""Portable isolated runners for the existing own-profile/status native hooks.

Profile text/title hooks enforce two prepared writes and two readbacks internally,
but publish neither those counts nor isolation booleans. Keep that source contract
distinct from the status hook's explicit runtime fields; never synthesize them.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import warnings

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


SPECS = {
    "profile-edit": {
        "flag": "SUPER_PLATINUM_PROFILE_EDIT_CHECK", "prefix": "profile-edit-check: ",
        "required": ("cancelNoWrite", "requiredValidation", "normalizedChangedFields",
                     "duplicateGuard", "canonicalReadback", "statusAndUnknownPreserved",
                     "uncertainReopenNoReplay", "permissionReason", "caretPreserved"),
        "prepared": 2, "readbacks": 2, "source": "profile_edit", "nested_layout": True,
    },
    "profile-title": {
        "flag": "SUPER_PLATINUM_PROFILE_TITLE_CHECK", "prefix": "profile-title-check: ",
        "required": ("cancelNoWrite", "lengthValidation", "normalizedTitle", "duplicateGuard",
                     "canonicalReadback", "siblingsPreserved", "clearAndUncertainNoReplay",
                     "permissionReason", "caretPreserved"),
        "prepared": 2, "readbacks": 2, "source": "profile_title", "nested_layout": True,
    },
    "self-status-editor": {
        "flag": "SUPER_PLATINUM_SELF_STATUS_CHECK", "prefix": "self-status-check: ",
        "required": ("cancelAndEscapeDiscardedDraft", "typingAndEmojiRetained", "pastDateRejected",
                     "duplicatePendingSaveRejected", "serverErrorRetainedDraft",
                     "uncertainCheckedWithoutResend", "noTransport"),
        "prepared": 3, "readbacks": 1, "source": "self_status", "nested_layout": False,
    },
}


def terminal_receipts(text, prefix, final=False):
    receipts = []
    for line in text.splitlines(keepends=True):
        if prefix not in line:
            continue
        if not line.endswith("\n"):
            if final:
                raise RuntimeError("Incomplete terminal native receipt at process exit")
            continue
        receipt = json.loads(line.split(prefix, 1)[1])
        if not isinstance(receipt, dict):
            raise RuntimeError("Native receipt must be an object")
        receipts.append(receipt)
    if len(receipts) > 1 or (final and len(receipts) != 1):
        raise RuntimeError("Expected exactly one complete terminal native receipt")
    return receipts


def validate_receipt(kind, fixture, receipt, platform=sys.platform):
    spec = SPECS[kind]
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertions failed: {receipt}")
    missing = [flag for flag in spec["required"] if receipt.get(flag) is not True]
    if missing:
        raise RuntimeError(f"{fixture}: missing true assertions: {missing}")
    layout = receipt.get("layout") if spec["nested_layout"] else receipt
    if not isinstance(layout, dict) or layout.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native layout assertion failed")
    dimensions = [layout.get("width"), layout.get("height")]
    if (any(type(value) is not int for value in dimensions)
            or dimensions != expected_viewport(fixture, platform)):
        raise RuntimeError(f"{fixture}: incorrect native viewport {dimensions}")
    if kind == "profile-edit" and (type(layout.get("fields")) is not int or layout["fields"] != 3):
        raise RuntimeError(f"{fixture}: expected three profile input fields")
    if kind in ("profile-edit", "self-status-editor"):
        height = layout.get("dialogHeight")
        if (type(height) not in (int, float) or not math.isfinite(height)
                or not 0 < height <= dimensions[1] + 1):
            raise RuntimeError(f"{fixture}: invalid dialog height")
    if kind == "self-status-editor":
        for key, expected in (("preparedRequests", spec["prepared"]), ("readbacks", spec["readbacks"])):
            if type(receipt.get(key)) is not int or receipt[key] != expected:
                raise RuntimeError(f"{fixture}: unexpected {key}")
        for key in ("bodyOverflow", "footerBottom"):
            value = layout.get(key)
            if type(value) not in (int, float) or not math.isfinite(value):
                raise RuntimeError(f"{fixture}: invalid {key}")
        if not 0 <= layout["bodyOverflow"] <= 1 or not 0 < layout["footerBottom"] <= dimensions[1] + 1:
            raise RuntimeError(f"{fixture}: status body/footer exceeds viewport")


def source_contract(kind):
    spec = SPECS[kind]
    return {"fixture_source": f"src/desktop/{spec['source']}/fixture.rs",
            "check_source": f"src/desktop/{spec['source']}/check.rs",
            "enabled_gate": "fixture installed, exact check flag, core session and transport absent",
            "prepared_writes_asserted_by_native_hook": spec["prepared"],
            "readbacks_asserted_by_native_hook": spec["readbacks"],
            "counts_published_in_native_receipt": kind == "self-status-editor",
            "isolation_published_in_native_receipt": kind == "self-status-editor",
            "status_noTransport_semantics": "snapshot combines absent session and transport"
                if kind == "self-status-editor" else None}


def png_dimensions(path, response):
    with path.open("rb") as stream:
        header = stream.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise RuntimeError("Native screenshot is missing its PNG header")
    dimensions = list(struct.unpack(">II", header[16:24]))
    data = response.get("data", response)
    if min(dimensions) <= 0 or dimensions != [data.get("width"), data.get("height")]:
        raise RuntimeError("Native screenshot dimensions differ from capture response")
    return dimensions


def run(kind, fixture, binary, captures):
    spec = SPECS[kind]
    captures.mkdir(parents=True, exist_ok=True)
    log = captures / f"{fixture}.log"
    if any((captures / f"{fixture}{suffix}").exists()
           for suffix in (".log", ".json", ".png", "-state.json", "-failure.png")):
        raise RuntimeError(f"{fixture}: use a fresh capture destination; existing evidence is preserved")
    native = NativeFixture(fixture, log, binary, {spec["flag"]: "1"})
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary.resolve()),
                "ok": False, "log": str(log), "source_contract": source_contract(kind),
                "input": "synthetic DOM fixture events; no OS input", "inspected": False,
                "isolation": "NativeFixture private endpoint/TMP; inherited SUPER_PLATINUM flags stripped"}
    scratch = None
    started = time.monotonic()
    try:
        with binary.open("rb") as stream:
            evidence["binary_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
        with warnings.catch_warnings(record=True) as cleanup_warnings:
            try:
                with native:
                    evidence["pid"] = native.process.pid
                    scratch = Path(native._temporary.name)
                    try:
                        deadline = time.monotonic() + 60
                        while True:
                            native.require_running()
                            receipts = terminal_receipts(log.read_text(encoding="utf-8", errors="replace"), spec["prefix"])
                            if receipts:
                                evidence["native"] = receipts[0]
                                validate_receipt(kind, fixture, receipts[0])
                                break
                            if time.monotonic() >= deadline:
                                raise TimeoutError(f"{fixture}: native assertions timed out")
                            time.sleep(.1)
                        snapshot = native.control("state", timeout=5)
                        state_path = captures / f"{fixture}-state.json"
                        state_path.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
                        evidence["state"] = str(state_path)
                        screenshot = captures / f"{fixture}.png"
                        capture = native.capture(screenshot)
                        evidence.update(screenshot=str(screenshot), capture=capture,
                                        png_dimensions=png_dimensions(screenshot, capture))
                    except BaseException:
                        try:
                            screenshot = captures / f"{fixture}-failure.png"
                            native.capture(screenshot)
                            evidence["failure_screenshot"] = str(screenshot)
                        except (OSError, RuntimeError, ValueError) as error:
                            evidence["failure_capture_error"] = str(error)
                        raise
            finally:
                evidence["cleanup_warnings"] = [str(item.message) for item in cleanup_warnings]
        # Reject late duplicates/failures and incomplete terminal lines after exit.
        receipts = terminal_receipts(log.read_text(encoding="utf-8", errors="replace"), spec["prefix"], final=True)
        validate_receipt(kind, fixture, receipts[0])
        if receipts[0] != evidence["native"]:
            raise RuntimeError("Terminal native receipt changed after capture")
        evidence["terminal_receipt_count"] = len(receipts)
        with binary.open("rb") as stream:
            evidence["binary_sha256_after"] = hashlib.file_digest(stream, "sha256").hexdigest()
        if evidence["binary_sha256_after"] != evidence["binary_sha256"]:
            raise RuntimeError("Native executable changed during fixture run")
        if native.process.poll() is None or scratch.exists() or evidence["cleanup_warnings"]:
            raise RuntimeError("Owned process or private scratch cleanup is incomplete")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        evidence.update(owned_process_exited=native.process is not None and native.process.poll() is not None,
                        exit_code=native.process.returncode if native.process else None,
                        private_temp_removed=not scratch.exists() if scratch else None,
                        elapsed_seconds=time.monotonic() - started)
        receipt_path = captures / f"{fixture}.json"
        receipt_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"fixture": fixture, "ok": evidence["ok"], "receipt": str(receipt_path)}), flush=True)


def main(kind):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    fixtures = (kind, f"{kind}-narrow")
    parser.add_argument("--fixture", action="append", choices=fixtures,
                        help="Repeat to select; default runs normal and narrow")
    args = parser.parse_args()
    for fixture in dict.fromkeys(args.fixture or fixtures):
        run(kind, fixture, args.binary.resolve(), args.captures.resolve())
