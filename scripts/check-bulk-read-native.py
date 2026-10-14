#!/usr/bin/env python3
"""Disconnected Shift+Escape bulk-read UI, raw request evidence and owned captures.

Never builds. Synthetic input/owner changes only; no real session, Transport,
Slack requests, clipboard access or restart durability claim.
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

# Audited helpers only; runpy does not execute either runner's CLI.
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
FIXTURE = "bulk-read"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
FLAG = "SUPER_PLATINUM_BULK_READ_CHECK"
PREFIX, WAITING, STAGE = "bulk-read-check: ", "bulk-read-waiting: ", "bulk-read-capture: "


STAGES = ("initial", "confirmation", "cancelled", "confirmation-checked", "sending", "cleared",
          "undo-sending", "undone", "expiry-ready", "expired", "owner-ready", "owner-changed", "uncertain", "refreshed")
COUNTS = (0, 0, 0, 0, 4, 5, 6, 7, 9, 9, 11, 11, 12, 13)
CHECKS = ("openNoRequest", "cancelNoMutation", "checkboxNoRequest", "independentPreference", "noOptimisticCounts",
          "duplicateClearBlocked", "oneUseUndo", "undoNoNewUndo", "expiredUndoRefused", "ownerUndoRefused",
          "unknownNoReplay", "refreshCannotResolveUnknown", "authoritativeCountsApplied", "unrelatedStatePreserved")
REQUIRED = ("noSession", "noTransport", "syntheticInput", "actualPreparedRequests") + tuple("checks." + name for name in CHECKS)
KINDS = ("preference-read", "preference-write", "preference-read", "clear", "counts", "undo", "counts",
         "clear", "counts", "clear", "counts", "clear", "counts")
PREFERENCE = "confirm_clear_all_unreads"
UNDO_TS = "1719800700.000001"
# poll() retires both the successful notice and its message at expiry;
# ui::view therefore exposes the underlying composer, with no status controls.
COMPOSER_STAGES = ("initial", "cancelled", "expired")
FOCUSED_CONTROLS = {"confirmation": "bulk-read-confirm-submit",
                    "confirmation-checked": "bulk-read-dont-ask",
                    "cancelled": "channel-composer", "expired": "channel-composer"}


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact bulk-read fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=13, required=REQUIRED), STAGES


def ready_tuple(fixture, nonce, pid):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact bulk-read fixture")
    if not isinstance(nonce, str) or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RuntimeError("Invalid readiness nonce")
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid owned PID")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture}


def validate_request(request, sequence, endpoint):
    if (not isinstance(request, dict) or type(request.get("sequence")) is not int
            or request["sequence"] != sequence or request.get("method") != "POST"
            or request.get("path") != f"/api/{endpoint}"):
        raise RuntimeError("Wrong bulk-read request sequence, method or endpoint")
    try:
        url = urlsplit(request.get("url"))
        if (url.scheme != "https" or url.netloc != "fixture.slack.com"
                or url.path != request["path"] or url.fragment
                or url.query != request.get("rawQuery")):
            raise ValueError("Unexpected synthetic request route")
    except (TypeError, ValueError, AttributeError) as error:
        raise RuntimeError("Wrong actual bulk-read URL") from error
    validate_query(request.get("rawQuery"))


def raw_preferences(enabled):
    return {PREFERENCE: enabled, "unknown_pref": {"keep": [1, 2]}}


def raw_counts(channel_ids, cleared):
    return {"ok": True, "channels": [{"id": identity,
        "has_unreads": not cleared and identity == "C2",
        "unread_count": 3 if not cleared and identity == "C2" else 0,
        "unread_count_display": 3 if not cleared and identity == "C2" else 0,
        "mention_count": 1 if not cleared and identity == "C2" else 0,
        "last_read": "1719800600.000000" if cleared else "1719800000.000000"} for identity in channel_ids],
        "ims": [], "groups": [], "mpims": [], "activity_v2": {"at_user": 0 if cleared else 2}}


def snapshot_state(value, channel_ids, cleared):
    if not isinstance(value, dict) or set(value) != {"channels", "activityUnread", "threadCounts", "dockCounts"}:
        raise RuntimeError("Missing full counts snapshot")
    channels = value["channels"]
    if not isinstance(channels, dict) or set(channels) != set(channel_ids):
        raise RuntimeError("Counts membership changed")
    for expected in raw_counts(channel_ids, cleared)["channels"]:
        actual = channels[expected["id"]]
        if not isinstance(actual, dict) or any(not strict_equal(actual.get(k), v) for k, v in expected.items()):
            raise RuntimeError("Snapshot channel cursor/counts disagree with authoritative response")
    if type(value["activityUnread"]) is not int or value["activityUnread"] != (0 if cleared else 2):
        raise RuntimeError("Activity count disagrees with authoritative response")


def validate_wire(receipt):
    initial, final = receipt.get("initialState"), receipt.get("finalState")
    if not isinstance(initial, dict) or not isinstance(initial.get("channels"), dict) or "C2" not in initial["channels"]:
        raise RuntimeError("Missing fixture channel inventory")
    ids = sorted(initial["channels"])
    snapshot_state(initial, ids, False)
    snapshot_state(final, ids, True)
    ledger = receipt.get("ledger")
    if not isinstance(ledger, list) or len(ledger) != len(KINDS):
        raise RuntimeError("Missing ordered actual request ledger or extra replay")
    for index, (entry, kind) in enumerate(zip(ledger, KINDS)):
        endpoint = {"preference-read": "users.prefs.get", "preference-write": "users.prefs.set",
                    "clear": "users.markAllRead", "undo": "users.markAllRead", "counts": "client.counts"}[kind]
        validate_request(entry, index, endpoint)
        if entry.get("kind") != kind or entry.get("retrySafe") is not (kind == "counts"):
            raise RuntimeError("Wrong request kind or builder retry classification")
        if kind == "preference-read":
            fields = [["prefs", PREFERENCE]]
            response = {"ok": True, "prefs": raw_preferences(index == 0)}
        elif kind == "preference-write":
            fields = [["name", PREFERENCE], ["value", "false"], ["_x_reason", "prefs-api/setUserPrefByApi"]]
            response = {"ok": True}
        elif kind == "counts":
            fields = []
            raw = entry.get("response")
            if not isinstance(raw, dict) or not isinstance(raw.get("channels"), list):
                raise RuntimeError("Missing actual counts response")
            observed_ids = [channel.get("id") if isinstance(channel, dict) else None for channel in raw["channels"]]
            if len(observed_ids) != len(ids) or set(observed_ids) != set(ids):
                raise RuntimeError("Counts response has missing/duplicate/invented membership")
            response = raw_counts(observed_ids, index != 6)
        else:
            fields = ([["undo_ts", UNDO_TS]] if kind == "undo" else []) + [["_x_reason",
                "UndoShiftEscapeMarkAllRead" if kind == "undo" else "ShiftEscapeConfirmDialog"]]
            response = None if index == 11 else ({"ok": True} if kind == "undo" else {"ok": True, "undo_ts": UNDO_TS})
            if entry.get("outcome") != ("unknown" if index == 11 else "ack"):
                raise RuntimeError("Wrong bulk acknowledgement or lost-response evidence")
        if not strict_equal(entry.get("fields"), fields) or not strict_equal(entry.get("response"), response):
            raise RuntimeError("Wrong exact actual request form or synthetic response")
    if not strict_equal(receipt.get("finalRaw"), raw_preferences(False)):
        raise RuntimeError("Confirmation preference or unrelated raw key changed")
    reconciliations = receipt.get("reconciliations")
    sequences = (4, 6, 8, 10, 12)
    if not isinstance(reconciliations, list) or len(reconciliations) != len(sequences):
        raise RuntimeError("Missing counts reducer evidence")
    previous = initial
    for item, sequence in zip(reconciliations, sequences):
        if (not isinstance(item, dict) or type(item.get("sequence")) is not int or item["sequence"] != sequence
                or item.get("accepted") is not True or item.get("scope") != "counts-only"
                or not strict_equal(item.get("response"), ledger[sequence]["response"])
                or not strict_equal(item.get("before"), previous)):
            raise RuntimeError("Counts reducer does not follow the actual request/previous state")
        snapshot_state(item.get("after"), ids, sequence != 6)
        previous = item["after"]
    if not strict_equal(previous, final) or not strict_equal(reconciliations[1]["after"], initial):
        raise RuntimeError("Undo/final inventory disagrees with exact reconciled state")
    expiry = receipt.get("expiry", {})
    if (not isinstance(expiry, dict) or not number(expiry.get("durationSeconds")) or expiry["durationSeconds"] != 6
            or not number(expiry.get("elapsedSeconds")) or expiry["elapsedSeconds"] < 6
            or not strict_equal(expiry.get("requestCountBefore"), 9) or not strict_equal(expiry.get("requestCountAfter"), 9)):
        raise RuntimeError("Undo expiry was not elapsed and refused without a request")
    rotation = receipt.get("ownerRotation", {})
    if (not isinstance(rotation, dict) or type(rotation.get("beforeEpoch")) is not int or rotation["beforeEpoch"] < 0
            or type(rotation.get("afterEpoch")) is not int or rotation["afterEpoch"] != rotation["beforeEpoch"] + 1
            or not strict_equal(rotation.get("requestCountBefore"), 11) or not strict_equal(rotation.get("requestCountAfter"), 11)):
        raise RuntimeError("Owner rotation did not invalidate Undo without a request")


def control_names(stage):
    if stage in ("confirmation", "confirmation-checked"):
        return ("bulk-read-close", "bulk-read-dont-ask", "bulk-read-cancel", "bulk-read-confirm-submit")
    if stage in COMPOSER_STAGES:
        return ("channel-composer",)
    extra = ("bulk-read-undo",) if stage in ("cleared", "expiry-ready", "owner-ready") else (
        ("bulk-read-refresh",) if stage in ("uncertain", "refreshed") else ())
    return extra + ("bulk-read-dismiss",)


def validate_geometry(receipt):
    geometry, viewport = receipt.get("geometry"), receipt["viewport"]
    if not isinstance(geometry, list) or len(geometry) != len(STAGES):
        raise RuntimeError("Missing bulk geometry stages")
    initial, final = receipt["initialState"], receipt["finalState"]
    cleared = receipt["reconciliations"][0]["after"]
    for index, (stage, count, entry) in enumerate(zip(STAGES, COUNTS, geometry)):
        def check(ok, field):
            if not ok:
                raise RuntimeError(f"Bulk stage {stage}: invalid {field}")

        confirm = stage in ("confirmation", "confirmation-checked")
        kind = "confirmation" if confirm else "composer" if stage in COMPOSER_STAGES else "status"
        expected_state = initial if index <= 4 or stage == "undone" else cleared
        check(isinstance(entry, dict), "geometry object")
        for field, expected in (("ok", True), ("stage", stage), ("viewport", viewport),
                                ("requestCount", count), ("surfaceKind", kind), ("state", expected_state)):
            check(strict_equal(entry.get(field), expected), field)
        check(number(entry.get("documentWidth")) and 0 < entry["documentWidth"] <= viewport[0] + 1, "documentWidth")
        check(rectangle(entry.get("surface"), viewport), "surface rectangle")
        check(isinstance(entry.get("text"), str), "text")
        controls = entry.get("controls")
        names = control_names(stage)
        if not isinstance(controls, list) or len(controls) != len(names):
            raise RuntimeError(f"Bulk stage {stage}: missing exact controls {names}")
        for control, name in zip(controls, names):
            if (not isinstance(control, dict) or control.get("name") != name or control.get("hit") is not True
                    or control.get("disabled") is not False or not rectangle(control.get("rect"), viewport)):
                raise RuntimeError(f"Bulk stage {stage}: control {name} not visible, enabled and hit tested")
            checked = stage == "confirmation-checked" if name == "bulk-read-dont-ask" else None
            if "checked" not in control or not strict_equal(control["checked"], checked):
                raise RuntimeError(f"Bulk stage {stage}: control {name} has wrong checked state")
        if entry.get("focusedControl") is not None and not isinstance(entry.get("focusedControl"), str):
            raise RuntimeError(f"Bulk stage {stage}: invalid focusedControl type")
        if stage in FOCUSED_CONTROLS:
            check(entry.get("focusedControl") == FOCUSED_CONTROLS[stage],
                  f"focusedControl (expected {FOCUSED_CONTROLS[stage]})")
        if confirm and ("Clear all unreads?" not in entry["text"] or "Cancel" not in entry["text"]):
            raise RuntimeError(f"Bulk stage {stage}: confirmation copy missing")
    if not strict_equal(geometry[-1]["state"], final):
        raise RuntimeError("Last capture differs from final state")


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
    directory = destination / f"bulk-read-{nonce}"
    directory.mkdir()
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session or Transport is installed.",
                                "Behavior checks execute native code; request shapes are independently checked here.",
                                "Counts-only synthetic readback; no full production refresh, trusted input or restart proof."]}
    env = {FLAG: "1", "SUPER_PLATINUM_BULK_READ_NONCE": nonce,
           "SUPER_PLATINUM_BULK_READ_DIR": str(directory)}
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
                    deadline, sampled, stage_count = time.monotonic() + 160, 0., 0
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
                            raise TimeoutError("Native bulk-read assertions timed out")
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-bulk-read")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
