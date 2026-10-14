#!/usr/bin/env python3
"""Disconnected reading preferences: actual request ledger and owned native captures.

Never builds or loads a session. Input and owner changes are synthetic; no
Transport or real Slack request is used. This is not restart-durability evidence.
"""
import argparse
from contextlib import ExitStack
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import sys
import time
from urllib.parse import parse_qsl, urlsplit
from uuid import UUID

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

# Shared audited process/PNG/placement helpers; loading does not launch an app.
UTIL = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))
binary_sha = UTIL["binary_sha"]
png_signature = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
publish_ready = UTIL["publish_ready"]
validate_ready = UTIL["validate_ready"]
FIXTURE = "read-preferences"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
FLAG = "SUPER_PLATINUM_READ_PREFERENCES_CHECK"
PREFIX, WAITING, STAGE = "read-preferences-check: ", "read-preferences-waiting: ", "read-preferences-capture: "
STAGES = ("initial", "saving", "newest-read", "uncertain", "mismatch", "confirmed", "cooldown", "rotation", "unsupported", "restored")
CHECKS = ("openNoWrite", "duplicateBlocked", "unknownReadOnlyCheck", "mismatchRetained",
          "exactReadbackConfirmed", "cooldownNoReplay", "lateOwnerRefused", "malformedRevoked",
          "unknownPreserved", "unrelatedPreferencesPreserved")
REQUIRED = ("noTransport", "noSession", "syntheticInput", "actualPreparedRequests") + tuple("checks." + key for key in CHECKS)
PREFS = ("mark_msgs_read_immediately", "start_scroll_at_oldest")
PATCHES = ((True, False), (False, False), (True, True), (True, True))
READ_SEQUENCES = (0, 1, 3, 4, 6, 7, 8, 10, 11, 13, 14, 15)
WRITE_SEQUENCES = (2, 5, 9, 12)
READ_VALUES = ((True, True), (True, True), (True, False), (True, False),
               (True, False), (False, False), (False, False), (False, False),
               (False, False), (True, True), ("unsupported", True), (True, True))
CONTROL_NAMES = ("reading-preference-left-off", "reading-preference-newest-read",
                 "reading-preference-newest-unseen", "close", "reading-preferences-reload")


def strict_equal(left, right):
    """JSON equality must not collapse raw boolean values into numbers."""
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(strict_equal(left[k], right[k]) for k in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(strict_equal(a, b) for a, b in zip(left, right))
    return left == right


def strict_json(value):
    def pairs(items):
        result = {}
        for key, item in items:
            if key in result:
                raise ValueError("Duplicate JSON preference key")
            result[key] = item
        return result
    def constant(_):
        raise ValueError("Nonfinite JSON")
    try:
        if not isinstance(value, str):
            raise ValueError("Preference form field is not a string")
        return json.loads(value, object_pairs_hook=pairs, parse_constant=constant)
    except (TypeError, ValueError) as error:
        raise RuntimeError("Malformed or duplicate preference JSON") from error


def validate_query(query):
    try:
        if not isinstance(query, str):
            raise ValueError("Raw query missing")
        pairs = parse_qsl(query, keep_blank_values=True, strict_parsing=True)
        names = ("_x_id", "_x_csid", "_x_desktop_ia", "_x_foreground",
                 "_x_frontend_build_type", "_x_gantry", "_x_num_retries",
                 "_x_version_ts", "fp", "slack_route")
        if [key for key, _ in pairs] != list(names):
            raise ValueError("Wrong exact default query fields")
        fields = dict(pairs)
        for key in ("_x_id", "_x_csid", "fp"):
            parsed = UUID(fields[key])
            if parsed.hex != fields[key] or parsed.version != 4:
                raise ValueError("Wrong random request identifier")
        expected = {"_x_desktop_ia": "true", "_x_foreground": "true",
                    "_x_frontend_build_type": "current", "_x_gantry": "true",
                    "_x_num_retries": "0", "_x_version_ts": "capture-required",
                    "slack_route": "default"}
        if any(fields[key] != value for key, value in expected.items()):
            raise ValueError("Wrong default request parameters")
    except (TypeError, ValueError, AttributeError, KeyError) as error:
        raise RuntimeError("Wrong actual preference request query") from error


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def rectangle(value, viewport):
    return (isinstance(value, dict) and set(value) == {"x", "y", "width", "height"}
            and all(number(v) for v in value.values())
            and value["x"] >= -1 and value["y"] >= -1
            and value["width"] > 0 and value["height"] > 0
            and value["x"] + value["width"] <= viewport[0] + 1
            and value["y"] + value["height"] <= viewport[1] + 1)


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact read-preference fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=16, required=REQUIRED), STAGES


def ready_tuple(fixture, nonce, pid):
    contract(fixture)
    if not isinstance(nonce, str) or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RuntimeError("Invalid readiness nonce")
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid owned PID")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture}


def raw_preferences(values):
    return {**dict(zip(PREFS, values)), "unknown_pref": {"keep": [1, 2]}}


def validate_request(request, sequence, endpoint):
    if (not isinstance(request, dict) or type(request.get("sequence")) is not int
            or request["sequence"] != sequence or request.get("method") != "POST"
            or request.get("retrySafe") is not False or request.get("path") != f"/api/{endpoint}"):
        raise RuntimeError("Wrong actual preference request sequence, method, endpoint, or replay policy")
    try:
        url = urlsplit(request.get("url"))
        if (url.scheme != "https" or url.netloc != "fixture.slack.com"
                or url.path != request["path"] or url.fragment
                or url.query != request.get("rawQuery")):
            raise ValueError("Unexpected synthetic route")
    except (TypeError, ValueError, AttributeError) as error:
        raise RuntimeError("Wrong actual preference request URL") from error
    validate_query(request.get("rawQuery"))


def validate_wire(receipt):
    attempts, reads = receipt.get("attempts"), receipt.get("readRequests")
    if not isinstance(attempts, list) or len(attempts) != 4 or not isinstance(reads, list) or len(reads) != 12:
        raise RuntimeError("Actual read/write ledger is missing, incomplete, or replayed")
    for request, sequence, values in zip(reads, READ_SEQUENCES, READ_VALUES):
        validate_request(request, sequence, "users.prefs.get")
        if request.get("fields") != [["prefs", ",".join(PREFS)]]:
            raise RuntimeError("Wrong exact ordered preference read form")
        if not strict_equal(request.get("response"), {"ok": True, "prefs": raw_preferences(values)}):
            raise RuntimeError("Raw baseline/readback differs, including missing or nonboolean values")
    outcomes = ({"kind": "ack", "response": {"ok": True}, "applied": True},
                {"kind": "unknown", "applied": False},
                {"kind": "rate-limited", "status": 429, "retryAfterSeconds": 2, "applied": False},
                {"kind": "ack", "response": {"ok": True}, "applied": True})
    for request, sequence, values, outcome in zip(attempts, WRITE_SEQUENCES, PATCHES, outcomes):
        validate_request(request, sequence, "users.prefs.set")
        fields = request.get("fields")
        if (not isinstance(fields, list) or len(fields) != 2
                or any(not isinstance(pair, list) or len(pair) != 2 for pair in fields)
                or fields[0][0] != "prefs"
                or fields[1] != ["_x_reason", "prefs-api/setMultipleUserPrefsByApi"]):
            raise RuntimeError("Wrong exact ordered preference write form")
        if not strict_equal(strict_json(fields[0][1]), dict(zip(PREFS, values))):
            raise RuntimeError("Write must contain exactly both raw JSON boolean preferences")
        if not strict_equal(request.get("outcome"), outcome):
            raise RuntimeError("Missing exact acknowledgement, uncertain, or cooldown outcome")
    if not strict_equal(receipt.get("finalRaw"), raw_preferences((True, True))):
        raise RuntimeError("Final raw preference inventory differs or unrelated key was lost")


def validate_geometry(receipt):
    geometry, viewport = receipt.get("geometry"), receipt["viewport"]
    if not isinstance(geometry, list) or len(geometry) != len(STAGES):
        raise RuntimeError("Missing exact reading-preference geometry stages")
    for stage, entry in zip(STAGES, geometry):
        if (not isinstance(entry, dict) or entry.get("stage") != stage or entry.get("ok") is not True
                or not strict_equal(entry.get("viewport"), viewport)
                or not number(entry.get("documentWidth"))
                or not 0 < entry["documentWidth"] <= viewport[0] + 1
                or not rectangle(entry.get("dialog"), viewport)):
            raise RuntimeError("Unbounded or missing Reading dialog geometry")
        controls = entry.get("controls")
        if not isinstance(controls, list) or len(controls) != len(CONTROL_NAMES):
            raise RuntimeError("Missing Reading radio/close/reload controls")
        for index, (control, name) in enumerate(zip(controls, CONTROL_NAMES)):
            disabled = stage not in ("initial", "newest-read", "confirmed", "restored") if index < 3 else (
                stage in ("saving", "cooldown") if index == 4 else False)
            if (not isinstance(control, dict) or control.get("name") != name or control.get("hit") is not True
                    or control.get("disabled") is not disabled or not rectangle(control.get("rect"), viewport)
                    or (type(control.get("checked")) is not bool if index < 3 else control.get("checked", False) is not None)):
                raise RuntimeError("Reading control missing, disabled incorrectly, occluded, or outside viewport")
        # Pending browser activation must not replace canonical readback state.
        selected = {"initial": 0, "saving": 0, "newest-read": 1, "uncertain": 1,
                    "mismatch": 1, "confirmed": 2, "restored": 0}.get(stage)
        if [c["checked"] for c in controls[:3]] != [i == selected for i in range(3)]:
            raise RuntimeError("Canonical Reading mode selection differs")
        panel, ring, focused = entry.get("panel"), entry.get("focusRing"), entry.get("focusedControl")
        expected_focus = {"newest-read": "reading-preference-newest-read", **dict.fromkeys(
            ("mismatch", "confirmed", "unsupported", "restored"), "reading-preferences-reload")}.get(stage)
        if entry.get("focusRingContained") is not True or not rectangle(panel, viewport):
            raise RuntimeError("Missing bounded Reading focus panel")
        if expected_focus is not None and (focused != expected_focus or ring is None):
            raise RuntimeError("Reading control focus was not preserved")
        if ring is None:
            if focused is not None:
                raise RuntimeError("Focused Reading control lacks outline geometry")
        elif (focused not in CONTROL_NAMES or not rectangle(ring, viewport)
              or ring["x"] < panel["x"] - .5 or ring["y"] < panel["y"] - .5
              or ring["x"] + ring["width"] > panel["x"] + panel["width"] + .5
              or ring["y"] + ring["height"] > panel["y"] + panel["height"] + .5):
            raise RuntimeError("Reading focus ring is clipped")


def validate_native(fixture, nonce, pid, directory, receipt, stages_seen,
                    platform=sys.platform, expected_dimensions=None):
    spec, stages = contract(fixture)
    if not isinstance(receipt, dict):
        raise RuntimeError("Missing native receipt")
    validate_receipt(spec, fixture, receipt, platform)
    if type(receipt.get("preparedRequests")) is not int:
        raise RuntimeError("Prepared request count must be an integer")
    viewport = receipt.get("viewport")
    if not isinstance(viewport, list) or len(viewport) != 2 or any(type(v) is not int for v in viewport):
        raise RuntimeError("Native viewport must contain exactly two integer dimensions")
    for key, value in (("fixture", fixture), ("nonce", nonce), ("pid", pid)):
        if receipt.get(key) != value or (key == "pid" and type(receipt[key]) is not int):
            raise RuntimeError(f"Wrong terminal receipt {key}")
    validate_wire(receipt)
    validate_geometry(receipt)
    captures = receipt.get("captures")
    if not isinstance(captures, dict) or set(captures) != set(stages):
        raise RuntimeError("Missing or extra capture stage")
    expected_edges = [{"stage": stage, "edge": edge} for stage in stages for edge in ("before", "after")]
    if stages_seen != expected_edges:
        raise RuntimeError("Capture checkpoints are missing, extra, or reordered")
    dimensions = {}
    for stage in stages:
        path = directory / f"{stage}.png"
        if captures[stage] != str(path):
            raise RuntimeError(f"Wrong exact owned capture path: {stage}")
        dimensions[stage] = png_signature(path, expected_dimensions)
    return dimensions


def validate_closed_log(path, expected, directory, admitted, expected_dimensions=None):
    waiting, receipts = closed_receipts(path, WAITING), closed_receipts(path, PREFIX)
    if len(waiting) != 1 or len(receipts) != 1:
        raise RuntimeError("Closed log has missing or duplicate ownership/terminal receipts")
    validate_ready(waiting[0], expected)
    if not strict_equal(receipts[0], admitted):
        raise RuntimeError("Closed terminal receipt changed after admission")
    return validate_native(expected["fixture"], expected["nonce"], expected["pid"], directory,
                           receipts[0], closed_receipts(path, STAGE), expected_dimensions=expected_dimensions)


def run(fixture, binary, captures):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"read-preferences-{nonce}"
    directory.mkdir()
    with (directory / "owner.json").open("x", encoding="utf-8") as owner:
        json.dump({"version": 1, "nonce": nonce, "fixture": fixture}, owner)
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session is installed or read.",
                                "Behavior checks execute native code; request shapes are independently checked here.",
                                "Native flow checks preflight/readback snapshots; baseline-drift refusal is a separate Rust unit test.",
                                "Credential fields are omitted from the recorded form; this runner validates the remaining exact fields.",
                                "In-memory settings retention is not process-restart persistence evidence."]}
    env = {FLAG: "1", "SUPER_PLATINUM_READ_PREFERENCES_NONCE": nonce,
           "SUPER_PLATINUM_READ_PREFERENCES_DIR": str(directory)}
    native = NativeFixture(fixture, destination / f"{fixture}.log", binary, env)
    process_api = handle = None
    try:
        with native:
            evidence["pid"] = native.process.pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence["processCreationFiletime"] = process_api.times(handle)["creation_filetime"]
            with ExitStack() as stack:
                expected = ready_tuple(fixture, nonce, native.process.pid)
                deadline = time.monotonic() + 25
                while not (waiting := native.receipts(WAITING)):
                    native.require_running()
                    if native.receipts(PREFIX) or time.monotonic() >= deadline:
                        raise RuntimeError("Fixture did not reach bounded readiness")
                    time.sleep(.05)
                if len(waiting) != 1:
                    raise RuntimeError("Duplicate readiness request")
                validate_ready(waiting[0], expected)
                pointer = normalize(native, evidence, stack, process_api, handle)
                def failure_capture():
                    if evidence.get("nativeValidated"):
                        return
                    try:
                        if pointer:
                            pointer.check("failure", require_clear=True)
                        path = destination / f"{fixture}-failure.png"
                        native.capture(path)
                        evidence["failureScreenshot"] = str(path)
                    except (OSError, RuntimeError, ValueError):
                        pass
                # LIFO cleanup keeps the pointer parked and owned window
                # alive for any failure image, then restores it before exit.
                stack.callback(failure_capture)
                publish_ready(directory, expected)
                evidence["readiness"] = expected
                deadline, sampled, stage_count = time.monotonic() + 120, 0., 0
                while True:
                    native.require_running()
                    edges = native.receipts(STAGE)
                    if pointer:
                        for edge in edges[stage_count:]:
                            pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                        if time.monotonic() - sampled >= .2:
                            pointer.check("running", require_clear=True)
                            sampled = time.monotonic()
                    stage_count = len(edges)
                    receipts = native.receipts(PREFIX)
                    if receipts:
                        if len(receipts) != 1:
                            raise RuntimeError("Duplicate terminal receipt")
                        evidence["native"] = receipts[0]
                        evidence["pngDimensions"] = validate_native(fixture, nonce, native.process.pid, directory, receipts[0], edges,
                                                                     expected_dimensions=evidence.get("expectedPngDimensions"))
                        if pointer:
                            pointer.check("terminal", require_clear=True)
                        evidence["nativeValidated"] = True
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Native read-preference assertions timed out")
                    time.sleep(.05)
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned process did not exit")
        if sys.platform == "win32":
            validate_pointer_cleanup(evidence.get("pointer", {}))
        evidence["pngDimensions"] = validate_closed_log(native.log, expected, directory, evidence["native"],
                                                        evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        evidence["pngValidation"] = "Complete chunk/CRC/IEND and bounded noninterlaced IDAT scanline validation; no pixel-color decoding"
        if handle:
            exit_identity = process_api.times(handle)
            evidence["exitIdentity"] = exit_identity
            validate_exit(evidence["processCreationFiletime"], exit_identity)
        evidence["binarySha256After"] = binary_sha(binary)
        if evidence["binarySha256After"] != digest:
            raise RuntimeError("Binary changed during native run")
        evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        # This executes after pointer restoration and NativeFixture cleanup, so
        # failure receipts also retain their actual cleanup outcomes.
        evidence["processExited"] = native.process is not None and native.process.poll() is not None
        if handle:
            try:
                evidence["cleanupProcessIdentity"] = process_api.times(handle)
                validate_exit(evidence["processCreationFiletime"], evidence["cleanupProcessIdentity"])
            except (OSError, RuntimeError) as error:
                evidence.update(ok=False, cleanupIdentityError=str(error))
            process_api.kernel.CloseHandle(handle)
        path = destination / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)
    if not evidence["ok"]:
        raise RuntimeError("Final retained process audit failed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-read-preferences")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
