#!/usr/bin/env python3
"""Disconnected Escape reading shortcuts, native captures and actual mark builders.

Never builds. Synthetic keyboard events and acknowledgements; no real session,
Transport, Slack, clipboard, trusted OS input or restart durability claim.
"""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import runpy
import secrets
import sys
import time
from urllib.parse import urlsplit

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

UTIL = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))
PREF = runpy.run_path(str(Path(__file__).with_name("check-read-preferences-native.py")))
binary_sha = UTIL["binary_sha"]
png_signature = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
publish_ready = UTIL["publish_ready"]
validate_ready = UTIL["validate_ready"]
strict_equal = PREF["strict_equal"]
validate_query = PREF["validate_query"]
rectangle = PREF["rectangle"]
number = PREF["number"]
FIXTURE = "reading-shortcuts"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
FLAG = "SUPER_PLATINUM_READ_SHORTCUTS_CHECK"
PREFIX, WAITING, STAGE = "reading-shortcuts-check: ", "reading-shortcuts-waiting: ", "reading-shortcuts-capture: "
STAGES = ("initial", "metadata-read", "ghost-cleared", "composer-empty", "composer-nonempty",
          "thread-read", "thread-closed", "focused-primary", "blocked-keys", "stale-events")
REQUEST_STAGES = ("metadata-read", "composer-empty", "composer-nonempty", "thread-read", "focused-primary")


CHECKS = ("metadataOnly", "ghostNoRequest", "emptyComposer", "nonemptyComposerPreserved",
          "threadFirstRead", "threadSecondCloses", "otherPaneTargetsPrimary", "blockedKeys",
          "staleEvents", "draftsPreserved")
REQUIRED = ("noSession", "noTransport", "syntheticAcknowledgements", "syntheticKeyboard") + tuple("checks." + name for name in CHECKS)
COUNTS = (0, 1, 1, 2, 3, 4, 4, 5, 5, 5)
DRAFT = "Keep this channel draft.\nSecond line \u65e5\u672c."
REPLY = "Keep this thread reply.\nSecond line."
THREAD_ROOT = "1719800400.000000"


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact Escape shortcut fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=5, required=REQUIRED), STAGES


def ready_tuple(fixture, nonce, pid):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact Escape shortcut fixture")
    if not isinstance(nonce, str) or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RuntimeError("Invalid readiness nonce")
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid owned PID")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture}


def validate_request_url(value, endpoint):
    try:
        url = urlsplit(value)
        if (url.scheme != "https" or url.netloc != "reading-shortcuts.slack.com"
                or url.path != f"/api/{endpoint}" or url.fragment):
            raise ValueError("Unexpected synthetic request route")
    except (TypeError, ValueError, AttributeError) as error:
        raise RuntimeError("Wrong actual mark request URL") from error
    validate_query(url.query)


def request_fields(stage):
    fields = [["token", "fake-reading-shortcuts"], ["channel", "C2"]]
    if stage == "thread-read":
        fields += [["thread_ts", THREAD_ROOT], ["ts", "1719800404.000000"], ["read", "1"]]
    else:
        fields += [["ts", "1750000099.000001" if stage == "metadata-read" else "1750000048.000001"]]
    return fields


def validate_wire(receipt):
    attempts = receipt.get("attempts")
    if not isinstance(attempts, list) or len(attempts) != 5:
        raise RuntimeError("Missing exact five actual mark requests")
    for request, stage in zip(attempts, REQUEST_STAGES):
        thread = stage == "thread-read"
        endpoint = "subscriptions.thread.mark" if thread else "conversations.mark"
        if not isinstance(request, dict) or request.get("stage") != stage or request.get("method") != "POST":
            raise RuntimeError("Wrong actual mark method/stage order")
        validate_request_url(request.get("url"), endpoint)
        fields = request_fields(stage)
        target = {"kind": "thread" if thread else "conversation", "team": "T1", "channel": "C2"}
        if thread:
            target["rootTs"] = THREAD_ROOT
        if (not strict_equal(request.get("fields"), fields) or not strict_equal(request.get("target"), target)
                or request.get("ts") != dict(fields)["ts"]):
            raise RuntimeError("Wrong actual mark form, target or timestamp")
    if not strict_equal(receipt.get("selection"), {"ok": True, "start": 3, "end": 8, "direction": "backward"}):
        raise RuntimeError("Native selection changed across nonempty-composer Escape")
    guards = receipt.get("guardEvents")
    kinds = ("control", "meta", "alt", "repeat", "composing", "key229", "completion", "menu")
    if not isinstance(guards, list) or len(guards) != len(kinds):
        raise RuntimeError("Missing guarded keyboard evidence")
    for event, kind in zip(guards, kinds):
        if (not isinstance(event, dict) or event.get("kind") != kind
                or type(event.get("requestCount")) is not int or event["requestCount"] != 5
                or (kind != "menu" and type(event.get("prevented")) is not bool)
                or (kind == "completion" and event["prevented"] is not True)):
            raise RuntimeError("Invalid keyboard guard/no-dispatch receipt")
        if kind in kinds[:6]:
            expected_facts = dict(ctrlKey=False, metaKey=False, altKey=False, repeat=False, isComposing=False, keyCode=0)
            key = {"control":"ctrlKey", "meta":"metaKey", "alt":"altKey", "repeat":"repeat", "composing":"isComposing", "key229":"keyCode"}[kind]
            expected_facts[key] = 229 if kind == "key229" else True
            if not strict_equal(event.get("facts"), expected_facts):
                raise RuntimeError("Synthetic key did not carry exact blocked event facts")
    expected = [{"kind": kind, "requestCount": 5} for kind in ("scope", "account", "focus-return")]
    if not strict_equal(receipt.get("staleEvents"), expected):
        raise RuntimeError("Missing stale scope/account/focus rejection")


def validate_geometry(receipt):
    geometry, viewport = receipt.get("geometry"), receipt["viewport"]
    if not isinstance(geometry, list) or len(geometry) != len(STAGES):
        raise RuntimeError("Missing Escape stage geometry")
    for entry, stage, count in zip(geometry, STAGES, COUNTS):
        thread = stage in ("thread-read", "focused-primary")
        if (not isinstance(entry, dict) or entry.get("ok") is not True or entry.get("stage") != stage
                or not strict_equal(entry.get("viewport"), viewport)
                or type(entry.get("requestCount")) is not int or entry["requestCount"] != count
                or not number(entry.get("documentWidth")) or not 0 < entry["documentWidth"] <= viewport[0] + 1
                or not rectangle(entry.get("timeline"), viewport)
                or not rectangle(entry.get("composer"), viewport)
                or type(entry.get("activeId")) is not str
                or entry.get("threadVisible") is not thread
                or entry.get("channelText") != ("" if stage == "composer-empty" else DRAFT)):
            raise RuntimeError("Invalid Escape geometry, request count or preserved draft")
        if thread:
            if (not rectangle(entry.get("thread"), viewport) or not rectangle(entry.get("threadComposer"), viewport)
                    or entry.get("threadText") != REPLY):
                raise RuntimeError("Missing open-thread geometry or preserved reply")
        elif any(key not in entry or entry[key] is not None for key in ("thread", "threadComposer", "threadText")):
            raise RuntimeError("Closed thread still has captured geometry/text")


def validate_native(fixture, nonce, pid, directory, receipt, stages_seen,
                    platform=sys.platform, expected_dimensions=None):
    spec, stages = contract(fixture)
    if not isinstance(receipt, dict):
        raise RuntimeError("Missing native receipt")
    validate_receipt(spec, fixture, receipt, platform)
    if type(receipt.get("preparedRequests")) is not int:
        raise RuntimeError("Prepared request count must be an integer")
    for key, value in (("fixture", fixture), ("nonce", nonce), ("pid", pid)):
        if receipt.get(key) != value or (key == "pid" and type(receipt[key]) is not int):
            raise RuntimeError(f"Wrong terminal receipt {key}")
    validate_wire(receipt)
    validate_geometry(receipt)
    captures = receipt.get("captures")
    if not isinstance(captures, dict) or set(captures) != set(stages):
        raise RuntimeError("Missing or extra capture stage")
    if stages_seen != [{"stage": stage, "edge": edge} for stage in stages for edge in ("before", "after")]:
        raise RuntimeError("Capture checkpoints missing, extra or reordered")
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
    directory = destination / f"reading-shortcuts-{nonce}"
    directory.mkdir()
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session or Transport is installed.",
                                "Behavior checks execute native code; request shapes are independently checked here.",
                                "Synthetic key events and acknowledgements do not establish trusted keyboard or HTTP behavior."]}
    env = {FLAG: "1", "SUPER_PLATINUM_READ_SHORTCUTS_NONCE": nonce,
           "SUPER_PLATINUM_READ_SHORTCUTS_DIR": str(directory)}
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
            try:
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
                            break
                        if time.monotonic() >= deadline:
                            raise TimeoutError("Native Escape assertions timed out")
                        time.sleep(.05)
            except BaseException:
                try:
                    path = destination / f"{fixture}-failure.png"
                    native.capture(path)
                    evidence["failureScreenshot"] = str(path)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
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
            except OSError as error:
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-reading-shortcuts")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
