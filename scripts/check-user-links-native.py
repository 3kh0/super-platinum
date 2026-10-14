#!/usr/bin/env python3
"""Disconnected human-user links: actual request ledger and owned native captures.

Never builds. Input/navigation and credential rotation are synthetic. Uses fake
session metadata with no Transport; no real Slack, clipboard or persistence proof.
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
FIXTURE = "user-links"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
FLAG = "SUPER_PLATINUM_USER_LINKS_CHECK"
PREFIX, WAITING, STAGE = "user-links-check: ", "user-links-waiting: ", "user-links-capture: "
STAGES = ("initial", "cached", "created", "pending", "stale", "error", "rotation", "restored")
CHECKS = ("cachedNoRequest", "duplicateNoRequest", "staleNoInsert", "wrongRecipientRefused",
          "errorStayedPut", "rotationNoInsert", "retryOpened", "outgoingDraftPreserved",
          "unsupportedNoRequest", "messageClosesProfile")
REQUIRED = ("noTransport", "syntheticSession") + tuple("checks." + name for name in CHECKS)
USERS = ("UCREATED", "UDELAY", "UWRONG", "UERROR", "UROTATE", "UROTATE")


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact user-link fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=6, required=REQUIRED), STAGES


def ready_tuple(fixture, nonce, pid):
    contract(fixture)
    if not isinstance(nonce, str) or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RuntimeError("Invalid readiness nonce")
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid owned PID")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture}


def validate_request_url(value):
    # Exact default SlackClient rest_form query, with random v4 UUID values.
    try:
        url = urlsplit(value)
        pairs = parse_qsl(url.query, keep_blank_values=True, strict_parsing=True)
        names = ("_x_id", "_x_csid", "_x_desktop_ia", "_x_foreground",
                 "_x_frontend_build_type", "_x_gantry", "_x_num_retries",
                 "_x_version_ts", "fp", "slack_route")
        if (url.scheme != "https" or url.netloc != "user-links.slack.com"
                or url.path != "/api/conversations.open" or url.fragment
                or [key for key, _ in pairs] != list(names)):
            raise ValueError("Wrong route/query")
        fields = dict(pairs)
        for key in ("_x_id", "_x_csid", "fp"):
            identifier = fields[key]
            parsed = UUID(identifier)
            if parsed.hex != identifier or parsed.version != 4:
                raise ValueError("Invalid request identifier")
        expected = {"_x_desktop_ia": "true", "_x_foreground": "true",
                    "_x_frontend_build_type": "current", "_x_gantry": "true",
                    "_x_num_retries": "0", "_x_version_ts": "capture-required",
                    "slack_route": "default"}
        if any(fields[key] != item for key, item in expected.items()):
            raise ValueError("Wrong default fixture xparams")
    except (TypeError, ValueError, AttributeError, KeyError) as error:
        raise RuntimeError("Wrong actual conversations.open URL") from error


def validate_wire(receipt):
    attempts = receipt.get("attempts")
    if not isinstance(attempts, list) or len(attempts) != len(USERS):
        raise RuntimeError("Missing actual request ledger or unexpected replay")
    for index, (attempt, user) in enumerate(zip(attempts, USERS)):
        if not isinstance(attempt, dict) or attempt.get("method") != "POST":
            raise RuntimeError("Wrong actual request method")
        validate_request_url(attempt.get("url"))
        token = "fake-user-links-rotated" if index == 5 else "fake-user-links"
        if attempt.get("fields") != [["token", token], ["users", user], ["return_im", "true"]]:
            raise RuntimeError("Wrong actual recipient/credential/form shape or order")


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def rectangle(value):
    return (isinstance(value, dict) and set(value) == {"x", "y", "width", "height"}
            and all(number(v) for v in value.values()) and value["width"] > 0 and value["height"] > 0)


def validate_geometry(receipt):
    geometry = receipt.get("geometry")
    if not isinstance(geometry, list) or len(geometry) != len(STAGES):
        raise RuntimeError("Missing stage geometry")
    for stage, item in zip(STAGES, geometry):
        profile = stage in ("cached", "created")
        if (not isinstance(item, dict) or item.get("stage") != stage or item.get("ok") is not True
                or item.get("viewport") != receipt["viewport"]
                or item.get("profileVisible") is not profile
                or not number(item.get("documentWidth"))
                or not 0 < item["documentWidth"] <= receipt["viewport"][0] + 1
                or not rectangle(item.get("composer"))):
            raise RuntimeError("Invalid user-link stage geometry")
        if profile:
            if not rectangle(item.get("profile")) or item.get("closeProfileHit") is not True:
                raise RuntimeError("Profile/Close control was not measured and hit tested")
        elif "profile" not in item or item["profile"] is not None or item.get("closeProfileHit", False) is not None:
            raise RuntimeError("Unexpected profile or Close hit in profile-free stage")


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
    if receipts[0] != admitted:
        raise RuntimeError("Closed terminal receipt changed after admission")
    return validate_native(expected["fixture"], expected["nonce"], expected["pid"], directory,
                           receipts[0], closed_receipts(path, STAGE), expected_dimensions=expected_dimensions)


def run(fixture, binary, captures):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"user-links-{nonce}"
    directory.mkdir()
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; only synthetic session metadata is installed.",
                                "Behavior checks execute native code; request shapes are independently checked here.",
                                "Composer bounds are measured, not proof it remains visible beside the narrow profile."]}
    env = {FLAG: "1", "SUPER_PLATINUM_USER_LINKS_NONCE": nonce,
           "SUPER_PLATINUM_USER_LINKS_DIR": str(directory)}
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
                            raise TimeoutError("Native user-link assertions timed out")
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-user-links")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
