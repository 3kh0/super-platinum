#!/usr/bin/env python3
"""Disconnected Custom new shared view: owned normal/narrow processes and fresh stage evidence.

Never builds. Windows placement and pointer checks are sampled observations, not
continuous input monitoring or atomic proof of every captured pixel.
"""
import argparse
from canvas_evidence import CaptureAudit, pointer_cleanup
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import secrets
import struct
import sys
import time
import zlib

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

FLAG = "SUPER_PLATINUM_LIST_VIEW_CREATE_CHECK"
PREFIX = "slack-list-view-create-check: "
WAITING = "slack-list-view-create-waiting: "
STAGE = "slack-list-view-create-stage: "
FIXTURE = "slack-list-view-create"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
STAGES = ("initial", "refused", "editing", "saving", "uncertain", "available", "final")
NARROW_STAGES = ("initial", "refused", "editing", "saving", "uncertain", "checking", "stale", "available", "final")
REQUIRED = ("noTransport", "noSession", "syntheticInput", "actualBuilderDecoderReducers",
            "explicitConfirmation", "writeReplyNotProjected", "explicitOpen", "exactReadback",
            "partialPopulationPreserved", "noMutationReplay", "selectedViewPreservedUntilOpen", "groupingCancelNoRequest", "groupingSchemaRefused")


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact List new shared view fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=11 if fixture.endswith("-narrow") else 10, required=REQUIRED), NARROW_STAGES if fixture.endswith("-narrow") else STAGES


def ready_tuple(fixture, nonce, pid):
    if fixture not in FIXTURES or not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise RuntimeError("Invalid readiness fixture or nonce")
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid owned PID")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture}


def validate_ready(actual, expected):
    if (not isinstance(actual, dict) or actual != expected
            or type(actual.get("version")) is not int or type(actual.get("pid")) is not int):
        raise RuntimeError("Wrong exact readiness owner")


def publish_ready(directory, expected):
    path, staged = directory / "ready.json", directory / "ready.staged"
    with staged.open("x", encoding="utf-8") as output:
        output.write(json.dumps(expected))
        output.flush()
        os.fsync(output.fileno())
    try:
        os.link(staged, path)  # Atomic create-new publication; never replaces.
    finally:
        staged.unlink()


def binary_sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def validate_exit(creation, current):
    if (type(creation) is not int or creation <= 0
            or current.get("creation_filetime") != creation
            or type(current.get("exit_filetime")) is not int
            or current["exit_filetime"] < creation):
        raise RuntimeError("Retained process creation/exit identity does not match")


def png_signature(path, expected_dimensions=None):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("Native stage PNG is missing or linked")
    if path.stat().st_size > 64 * 1024 * 1024:
        raise RuntimeError("Oversized native stage PNG")
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise RuntimeError("Invalid PNG signature")
    offset, header, compressed, saw_idat, ended_idat, ended = 8, None, bytearray(), False, False, False
    while offset < len(data):
        if len(data) - offset < 12:
            raise RuntimeError("Truncated PNG chunk")
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        end = offset + 12 + size
        if end > len(data):
            raise RuntimeError("Truncated PNG chunk payload")
        payload = data[offset + 8:end - 4]
        if zlib.crc32(kind + payload) & 0xffffffff != struct.unpack_from(">I", data, end - 4)[0]:
            raise RuntimeError("PNG chunk CRC mismatch")
        if header is None and kind != b"IHDR":
            raise RuntimeError("PNG IHDR must be first")
        if kind == b"IHDR":
            if header is not None or size != 13:
                raise RuntimeError("Invalid or repeated IHDR")
            header = struct.unpack(">IIBBBBB", payload)
        elif kind == b"IDAT":
            if ended_idat:
                raise RuntimeError("Noncontiguous PNG IDAT")
            saw_idat = True
            compressed.extend(payload)
        elif kind == b"IEND":
            if size or not saw_idat or end != len(data):
                raise RuntimeError("Invalid IEND or trailing bytes")
            ended = True
            break
        elif saw_idat:
            ended_idat = True
        offset = end
    if not ended or header is None:
        raise RuntimeError("Incomplete PNG")
    width, height, depth, color, compression, filtering, interlace = header
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color)
    allowed = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
    if (not channels or depth not in allowed[color] or compression or filtering or interlace
            or not 0 < width <= 16384 or not 0 < height <= 16384 or width * height > 40_000_000):
        raise RuntimeError("Unsupported or invalid native PNG encoding/dimensions")
    if expected_dimensions is not None and [width, height] != expected_dimensions:
        raise RuntimeError("PNG dimensions differ from the owned pre-ready window")
    stride = (width * channels * depth + 7) // 8 + 1
    expected_size = height * stride
    if expected_size > 256 * 1024 * 1024:
        raise RuntimeError("PNG scanlines exceed bounded validation budget")
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(compressed, expected_size + 1)
    except zlib.error as error:
        raise RuntimeError("Invalid PNG compressed payload") from error
    if (len(raw) != expected_size or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail
            or any(raw[start] > 4 for start in range(0, expected_size, stride))):
        raise RuntimeError("PNG scanline payload is incomplete or invalid")
    return [width, height]


# Source fragment inserted into the staged standalone runner, not imported by it.
def initial_population():
    return {"ok": True, "list_id": "F_LIST_FIXTURE", "records": [
        {"id": "R_HYDRATE_PARENT", "list_id": "F_LIST_FIXTURE", "position": "0000000001",
         "date_created": 1, "created_by": "U1", "fields": [
             {"key": "NAME", "text": "Project parent"}, {"key": "done", "checkbox": False},
             {"key": "notes", "text": "Known parent; children are not loaded yet."}, {"key":"status","select":["todo"]}]},
        {"id": "R_HYDRATE_OTHER", "list_id": "F_LIST_FIXTURE", "position": "9999999999",
         "date_created": 2, "created_by": "U1", "fields": [
             {"key": "NAME", "text": "Unrelated retained item"}, {"key": "done", "checkbox": True}, {"key":"status","select":["done"]}]}],
        "response_metadata": {"next_cursor": "fixture-opaque-unloaded+/=", "keep": "exact"}}


def document():
    return {"id": "F_LIST_FIXTURE", "mode": "list", "title": "Incomplete project checklist",
            "access": "write", "is_locked": False, "list_limits": {"over_row_maximum": False},
            "list_metadata": {"schema": [
                {"id": "C_NAME", "key": "NAME", "name": "Item", "type": "text", "is_primary_column": True},
                {"id": "C_CHECK", "key": "done", "name": "Done", "type": "checkbox"},
                {"id": "C_RICH", "key": "notes", "name": "Notes", "type": "rich_text"}], "views": []}}


def create_document(fixture=FIXTURE):
    raw = document()
    raw["title"] = "Add shared view review"
    raw["list_metadata"]["schema"].append({"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"todo","label":"To do"},{"value":"done","label":"Done"}]},"opaque":{"keep":3}})
    raw["list_limits"]["over_view_maximum"] = False
    raw["created"], raw["user"] = 1700000000, "U1"
    raw["list_metadata"]["schema"][0].update(options={"format": "text"}, opaque={"keep": 1})
    raw["list_metadata"]["opaque"] = {"keep": 2}
    raw["list_metadata"]["default_view"] = "V_CUSTOM"
    raw["list_metadata"]["views"] = [
        {"id": "V_CUSTOM", "name": "Project delivery", "type": "board" if fixture.endswith("-narrow") else "table", "position": "001",
         "columns": [{"key": "NAME", "id": "C_NAME", "visible": True}, {"key": "done", "id": "C_CHECK", "visible": True}, {"key": "notes", "id": "C_RICH", "visible": True}],
         "grouping": {"group_by": "done", "group_by_column_id": "C_CHECK"}, "filters": [], "sorts": [], "opaque": {"preserve": [1, {"future": True}]}},
        {"id": "V_OTHER", "name": "Other shared view", "type": "table", "position": "002", "columns": [{"key": "NAME", "id": "C_NAME", "visible": True}],
         "grouping": {}, "filters": [], "sorts": [], "opaque": "sibling"}]
    return raw


def current_document(fixture=FIXTURE):
    raw = create_document(fixture)
    raw["list_metadata"]["views"].insert(1, {
        "id":"V_SERVER_NEW", "name":"Delivery overview", "type":"board" if fixture.endswith("-narrow") else "table", "position":"0015",
        "columns":[{"key":"NAME","id":"C_NAME","visible":True,"width":240},{"key":"done","id":"C_CHECK","visible":True},{"key":"notes","id":"C_RICH","visible":True}],
        "grouping":{"group_by":grouping_key(fixture)}, "filters":[], "sorts":[], "show_completed_items":True, "opaque":{"server_defaults":[1,{"future":True}]}})
    return raw


def grouping_key(fixture):
    return "status" if fixture.endswith("-narrow") else "done"


def refused_document(fixture):
    raw = create_document(fixture)
    next(c for c in raw["list_metadata"]["schema"] if c["key"] == grouping_key(fixture))["type"] = "text"
    return raw


def expected_groups(fixture):
    return ([{"key":"todo","label":"To do","count":1},{"key":"done","label":"Done","count":1}]
            if fixture.endswith("-narrow") else [{"key":"true","label":"Checked","count":1},{"key":"","label":"Ungrouped","count":1}])


def expected_wire(fixture=FIXTURE):
    token = [["token", "fixture-not-a-token"]]
    def req(path, fields): return {"method": "POST", "path": "/api/" + path, "fields": token + fields}
    reads = [req("files.info", [["file", "F_LIST_FIXTURE"]]),
             req("team.prefs.get", [["target_team", "T1"], ["_x_reason", "native-list-permission"]]),
             req("users.prefs.get", [["prefs", "task_lists,assigned_to_list"], ["_x_reason", "native-list-completion-permission"]])]
    write = req("lists.views.create", [["list_id", "F_LIST_FIXTURE"], ["name", "Delivery overview"],
                ["type", "board" if fixture.endswith("-narrow") else "table"], ["grouping", json.dumps({"group_by":grouping_key(fixture)},separators=(",",":"))], ["_x_reason", "create-view"]])
    return reads + reads + [write] + ([reads[0]] if fixture.endswith("-narrow") else []) + reads



def validate_wire(receipt):
    fixture = receipt.get("fixture")
    if fixture not in FIXTURES: raise RuntimeError("Unknown exact fixture")
    narrow = fixture.endswith("-narrow")
    before, after = create_document(fixture), current_document(fixture)
    permission = {"ok": True, "prefs": {"allow_lists": "full"}}
    personal = {"ok": True, "prefs": {"task_lists": "[]", "assigned_to_list": ""}}
    response = {"syntheticFailure":"response_lost"} if narrow else {"ok":True,"list":{"id":after["id"],"list_metadata":after["list_metadata"]},"opaque_reply":{"retain":True}}
    responses = [{"ok":True,"file":refused_document(fixture)},permission,personal] + [{"ok":True,"file":before},permission,personal,response] + ([{"ok":True,"file":after}] if narrow else []) + [{"ok":True,"file":after},permission,personal]

    ledger = receipt.get("ledger", {})
    if ledger.get("requests") != expected_wire(fixture) or ledger.get("responses") != responses:
        raise RuntimeError("Exact request/response oracle failed")
    if ledger.get("server") != after or ledger.get("writePending") is not False or ledger.get("readPending") is not False:
        raise RuntimeError("Unsettled or incorrect synthetic source")
    finished = ledger.get("finished")
    if (not isinstance(finished, list) or len(finished) != (3 if narrow else 2)
            or [(e.get("kind"), e.get("applied")) for e in finished] != ([("write",False)]+([("check",False)] if narrow else [])+[("check",True)])
            or any(type(e.get("serial")) is not int or e["serial"] <= 0 for e in finished)
            or len({e["serial"] for e in finished}) != 1):
        raise RuntimeError("Actual completion ledger differs")
    if (receipt.get("before") != before or receipt.get("after") != after
            or receipt.get("initialPopulation") != initial_population() or receipt.get("finalPopulation") != initial_population()
            or receipt.get("layoutKinds") != [before["list_metadata"]["views"][0]["type"]]*2
            or receipt.get("selectedView") != "V_SERVER_NEW" or receipt.get("selectedViewPreservedUntilOpen") is not True
            or receipt.get("positiveReplyRetained") is not (not narrow) or receipt.get("lateReadRejected") is not narrow
            or receipt.get("retainedMutationReply") != (None if narrow else response)
            or receipt.get("writeRequests") != 1 or receipt.get("readRequests") != (10 if narrow else 9)):
        raise RuntimeError("Exact schema/population or request count differs")
    if (receipt.get("groupingKey") != grouping_key(fixture)
            or receipt.get("groupingCancelNoRequest") is not True or receipt.get("groupingSchemaRefused") is not True
            or receipt.get("groupProjection") != {"ok":True,"groups":expected_groups(fixture)}):
        raise RuntimeError("Grouping selection, refusal, or mounted projection differs")
    visits = receipt.get("visitGenerations")
    if (not isinstance(visits, list) or (narrow and (len(visits)!=2 or any(type(v) is not int or v<=0 for v in visits) or visits[0]==visits[1])) or (not narrow and visits != [])):
        raise RuntimeError("Old and new mounted visits are not distinct")


def validate_picker_stage(g, stage):
    expected_label = "Delivery overview" if stage == "final" else "Project delivery"
    if (g.get("pickerExact") is not True or g.get("pickerID") != ("V_SERVER_NEW" if stage=="final" else "V_CUSTOM")
            or g.get("pickerLabel") != expected_label):
        raise RuntimeError("Picker identity or visible label differs from retained saved view")


def validate_native(fixture, nonce, pid, directory, receipt, stages_seen, platform=sys.platform, expected_dimensions=None):
    spec, stages = contract(fixture)
    validate_receipt(spec, fixture, receipt, platform)
    if type(receipt.get("preparedRequests")) is not int:
        raise RuntimeError("Prepared request count must be an integer")
    validate_wire(receipt)
    geometry = receipt.get("geometry")
    phase_map={"initial":None,"refused":"NotRequested","editing":"Editing","saving":"Saving","uncertain":"Uncertain","checking":"Checking","stale":"Uncertain","available":"Available","final":None}
    phase_controls={None:["slack-list-new-view","slack-list-close"],"Editing":["slack-list-new-view-layout","slack-list-new-view-grouping","slack-list-rename-name","slack-list-rename-leave","slack-list-rename-close","slack-list-rename-confirm"],"NotRequested":["slack-list-rename-leave","slack-list-rename-close"],"Saving":["slack-list-rename-leave"],"Checking":["slack-list-rename-leave"],"Uncertain":["slack-list-rename-leave","slack-list-rename-check"],"Available":["slack-list-rename-leave","slack-list-rename-close","slack-list-rename-check","slack-list-new-view-open"]}
    if not isinstance(geometry, list) or len(geometry) != len(stages):
        raise RuntimeError("Missing exact geometry stages")
    for g, stage in zip(geometry, stages):
        phase=phase_map[stage]; ids=phase_controls[phase]
        validate_picker_stage(g, stage)
        if (g.get("actualLayout") != ("board" if fixture.endswith("-narrow") else "table")
                or g.get("stage") != stage or g.get("phase") != phase
                or any(g.get(k) is not True for k in ("ok", "bounded", "partial", "focusedInside"))
                or g.get("viewport") != receipt.get("viewport") or g.get("hitPointsPerControl") != 9
                or [c.get("id") for c in g.get("controls", [])] != ids
                or any(c.get("hit") is not True for c in g.get("controls", []))
                or (phase and (g.get("input") != "Delivery overview" or g.get("grouping") != grouping_key(fixture) or g.get("creationLayout") != ("board" if fixture.endswith("-narrow") else "table")))):
            raise RuntimeError("Rename control geometry or mounted state differs")
    for key, value in (("fixture", fixture), ("nonce", nonce), ("pid", pid)):
        if receipt.get(key) != value or (key == "pid" and type(receipt[key]) is not int):
            raise RuntimeError(f"Wrong terminal receipt {key}")
    captures = receipt.get("captures")
    if not isinstance(captures, dict) or set(captures) != set(stages):
        raise RuntimeError("Missing, duplicate, or extra capture stage")
    expected_edges = [{"stage": stage, "edge": edge} for stage in stages for edge in ("before", "after")]
    if stages_seen != expected_edges:
        raise RuntimeError("Capture checkpoints are missing, extra, or reordered")
    dimensions = {}
    for stage in stages:
        path = directory / f"{fixture}-{stage}.png"
        if captures[stage] != str(path):
            raise RuntimeError(f"Wrong exact owned capture path: {stage}")
        dimensions[stage] = png_signature(path, expected_dimensions)
    return dimensions


def validate_pointer_cleanup(pointer):
    if (pointer.get("movement_observed") is not False or pointer.get("restore_observed") is not True
            or pointer.get("restore_attempted") is not True or pointer.get("restore_error")
            or pointer.get("restore_skipped")):
        raise RuntimeError("Pointer moved or restoration was not verified after cleanup")


def closed_receipts(path, prefix):
    results = []
    for line in path.read_text(encoding="utf-8", errors="strict").splitlines(keepends=True):
        if prefix in line:
            if not line.endswith("\n"):
                raise RuntimeError("Incomplete final log receipt")
            results.append(json.loads(line.split(prefix, 1)[1]))
    return results


def validate_closed_log(path, expected, directory, admitted, expected_dimensions=None):
    waiting, receipts = closed_receipts(path, WAITING), closed_receipts(path, PREFIX)
    if len(waiting) != 1 or len(receipts) != 1:
        raise RuntimeError("Closed log contains missing or duplicate ownership/terminal receipts")
    validate_ready(waiting[0], expected)
    if receipts[0] != admitted:
        raise RuntimeError("Closed terminal receipt changed after admission")
    return validate_native(expected["fixture"], expected["nonce"], expected["pid"], directory,
                           receipts[0], closed_receipts(path, STAGE), expected_dimensions=expected_dimensions)


def normalize(native, evidence, stack, process_api, handle):
    if sys.platform != "win32":
        evidence["normalization"] = {"enabled": False, "reason": "Windows helpers do not apply"}
        return None
    from visual_placement import fit_work_area
    from visual_pointer import FixturePointer
    visual = runpy.run_path(str(Path(__file__).with_name("check-visual-native.py")))
    creation = evidence["processCreationFiletime"]

    def verify():
        native.require_running()
        current = process_api.times(handle)
        if current["creation_filetime"] != creation or current["exit_filetime"] != 0:
            raise RuntimeError("Retained owned process identity changed")

    observe = lambda: visual["window_observation"](native.process.pid)
    evidence["placement"] = {}
    fit_work_area(observe, verify, evidence["placement"], position_policy="top-left")
    pointer = FixturePointer(observe, verify)
    stack.callback(pointer.close)  # Runs while the owned process/window still exists.
    evidence["pointer"] = pointer.data
    pointer.park()
    pointer.settle(native, visual["fingerprint"])
    pointer.check("before_ready", require_clear=True)
    evidence["expectedPngDimensions"] = evidence["placement"]["after"]["window_rect_physical"][2:]
    return pointer


def diagnostic_capture(audit, native, evidence):
    # Only while the pointer lease and owned process are still held. Never
    # replaces the primary error; not an admitted stage or acceptance image.
    if audit is None:
        evidence["failureCaptureSkipped"] = "Owned pointer normalization did not complete"
        return
    path = native.log.with_name(native.log.stem + "-failure.png")
    evidence["failureCaptureScope"] = "Bounded diagnostic with sampled process/pointer checks before restoration"
    try:
        audit.capture(path, "failure")
        evidence["failureScreenshot"] = str(path)
    except BaseException as error:
        evidence["failureCaptureError"] = f"{type(error).__name__}: {error}"


def final_audit(native, evidence, api, handle, expected, directory):
    """Collect every independent final audit, even after workflow/cleanup failure."""
    failures = []

    def collect(stage, action):
        try:
            action()
        except BaseException as error:
            failures.append({"stage": stage, "type": type(error).__name__, "message": str(error)})

    def process():
        code = None if native.process is None else native.process.poll()
        evidence.update(processExited=code is not None, exitCode=code)
        if code is None:
            raise RuntimeError("Owned process exit not observed")
    collect("process", process)
    evidence["cleanupErrors"] = list(native.cleanup_errors)
    if evidence["cleanupErrors"]:
        failures.append({"stage": "cleanup", "type": "RuntimeError", "message": "Native fixture cleanup reported errors"})
    if evidence.get("pointerCleanupError"):
        failures.append({"stage": "pointer-cleanup", "type": "RuntimeError", "message": evidence["pointerCleanupError"]})
    if handle is not None:
        def identity():
            value = api.times(handle)
            evidence.update(cleanupProcessIdentity=value, exitIdentity=value)
            validate_exit(evidence["processCreationFiletime"], value)
        collect("identity", identity)
        def close_handle():
            if not api.kernel.CloseHandle(handle):
                raise OSError("Could not close retained rename process handle")
        collect("handle", close_handle)
    if sys.platform == "win32":
        collect("pointer", lambda: validate_pointer_cleanup(evidence.get("pointer", {})))

    evidence["closedLog"] = {}
    for label, prefix in (("waiting", WAITING), ("terminal", PREFIX), ("stages", STAGE)):
        def read(label=label, prefix=prefix):
            evidence["closedLog"][label] = closed_receipts(native.log, prefix)
        collect("closed-log-" + label, read)
    def validate_log():
        if expected is None or "native" not in evidence:
            raise RuntimeError("Workflow did not admit an owned terminal receipt")
        evidence["pngDimensions"] = validate_closed_log(native.log, expected, directory, evidence["native"],
                                                         evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        evidence["pngValidation"] = "Complete chunk/CRC/IEND and bounded noninterlaced IDAT scanline validation; no pixel-color decoding"
    collect("closed-log-validation", validate_log)
    def digest():
        evidence["binarySha256After"] = binary_sha(native.binary)
        if evidence["binarySha256After"] != evidence["binarySha256"]:
            raise RuntimeError("Binary changed during native run")
    collect("binary", digest)
    evidence["auditErrors"] = failures
    return failures


def run(fixture, binary, captures):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"slack-list-view-create-{nonce}"
    directory.mkdir()
    with (directory / "owner.json").open("x", encoding="utf-8") as owner:
        json.dump({"version": 1, "nonce": nonce, "fixture": fixture}, owner)
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session/storage is read."]}
    env = {FLAG: "1", "SUPER_PLATINUM_LIST_VIEW_CREATE_NONCE": nonce,
           "SUPER_PLATINUM_LIST_VIEW_CREATE_READY_DIR": str(directory),
           "SUPER_PLATINUM_LIST_VIEW_CREATE_CAPTURE_DIR": str(directory)}
    native = NativeFixture(fixture, destination / f"{fixture}.log", binary, env)
    process_api = handle = expected = primary = None
    try:
        with native:
            evidence["pid"] = native.process.pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence["processCreationFiletime"] = process_api.times(handle)["creation_filetime"]
            with pointer_cleanup(evidence) as stack:
                audit = None
                try:
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
                    audit = CaptureAudit(native, evidence, process_api, handle, pointer)
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
                            raise TimeoutError("Native List new shared view assertions timed out")
                        time.sleep(.05)
                except BaseException:
                    diagnostic_capture(audit, native, evidence)
                    raise
    except BaseException as error:
        primary = error
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        # All these observations happen after pointer restoration and native
        # cleanup, on success AND failure. Keep the original exception primary.
        failures = final_audit(native, evidence, process_api, handle, expected, directory)
        evidence["ok"] = primary is None and not failures
        if failures:
            detail = "Rename final evidence audit failed: " + repr(failures)
            if primary is not None and hasattr(primary, "add_note"):
                primary.add_note(detail)
            elif primary is None:
                evidence["error"] = detail
        path = destination / f"{fixture}.json"
        try:
            path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
            print(json.dumps({"receipt": str(path), **evidence}), flush=True)
        except BaseException as error:
            if primary is None:
                raise
            if hasattr(primary, "add_note"):
                primary.add_note("Could not publish rename audit receipt: " + repr(error))
        if primary is None and failures:
            raise RuntimeError(evidence["error"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-slack-list-view-create")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
