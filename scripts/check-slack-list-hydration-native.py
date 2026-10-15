#!/usr/bin/env python3
"""Disconnected known-parent hydration: owned normal/narrow processes and fresh stage evidence.

Never builds. Windows placement and pointer checks are sampled observations, not
continuous input monitoring or atomic proof of every captured pixel.
"""
import argparse
from contextlib import ExitStack
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

FLAG = "SUPER_PLATINUM_LIST_HYDRATION_CHECK"
PREFIX = "slack-list-hydration-check: "
WAITING = "slack-list-hydration-waiting: "
STAGE = "slack-list-hydration-stage: "
FIXTURE = "slack-list-hydration"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
STAGES = ("initial", "pending", "confirmation", "detail", "children", "stale", "final")
REQUIRED = ("noTransport", "noSession", "syntheticInput", "mountedDetail", "actualBuilderDecoderMerge",
            "backRevealsChildren", "partialPopulationPreserved", "deleteConfirmationRequired", "duplicateBlocked",
            "lateCloseReopenRejected", "heldCurrentMerged", "absentChildRetained", "noMutationReplay")


def contract(fixture):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact List hydration fixture")
    return CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX,
                     prepared_requests=3, required=REQUIRED), STAGES


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
             {"key": "notes", "text": "Known parent; children are not loaded yet."}]},
        {"id": "R_HYDRATE_OTHER", "list_id": "F_LIST_FIXTURE", "position": "9999999999",
         "date_created": 2, "created_by": "U1", "fields": [
             {"key": "NAME", "text": "Unrelated retained item"}, {"key": "done", "checkbox": False}]}],
        "response_metadata": {"next_cursor": "fixture-opaque-unloaded+/=", "keep": "exact"}}


def document():
    return {"id": "F_LIST_FIXTURE", "mode": "list", "title": "Incomplete project checklist",
            "access": "write", "is_locked": False, "list_limits": {"over_row_maximum": False},
            "list_metadata": {"schema": [
                {"id": "C_NAME", "key": "NAME", "name": "Item", "type": "text", "is_primary_column": True},
                {"id": "C_CHECK", "key": "done", "name": "Done", "type": "checkbox"},
                {"id": "C_RICH", "key": "notes", "name": "Notes", "type": "rich_text"}], "views": []}}


def response(index):
    parent = initial_population()["records"][0]
    parent["fields"][0]["text"] = ("Hydrated project parent", "STALE response must not appear",
                                     "Hydrated project parent again")[index]
    child = {"id": ("R_HYDRATE_CHILD", "R_HYDRATE_STALE", "R_HYDRATE_SECOND")[index],
             "list_id": "F_LIST_FIXTURE", "parent_record_id": "R_HYDRATE_PARENT",
             "position": "0000000002" if index == 0 else "0000000003",
             "view_positions": {"V_FUTURE": "preserved"}, "opaque": {"keep": True},
             "date_created": 99 + index, "created_by": "U1", "fields": [
                 {"key": "NAME", "text": ("Hydrated first subtask", "STALE child", "Hydrated second subtask")[index]},
                 {"key": "done", "checkbox": False}]}
    return {"ok": True, "list": document(), "record": parent, "subtasks": [child]}


def populations():
    first = initial_population()
    first["records"][0] = response(0)["record"]
    first["records"].append(response(0)["subtasks"][0])
    final = initial_population()
    final["records"][0] = response(2)["record"]
    final["records"].extend([response(0)["subtasks"][0], response(2)["subtasks"][0]])
    return first, final


def validate_wire(receipt):
    if receipt.get("applied") != [True, False, True] or receipt.get("mutationRequests") != 0:
        raise RuntimeError("Wrong actual merge outcomes or mutation count")
    attempts = receipt.get("attempts")
    if not isinstance(attempts, list) or len(attempts) != 3:
        raise RuntimeError("Missing exact hydration request ledger")
    expected_fields = [["token", "fixture-not-a-token"], ["list_id", "F_LIST_FIXTURE"],
                       ["id", "R_HYDRATE_PARENT"], ["include_is_subscribed", "true"],
                       ["_x_reason", "get-record-subtasks"]]
    opens, serials = set(), set()
    for item in attempts:
        if (item.get("method") != "POST"
                or item.get("url", "").split("?")[0] != "https://example.test/api/lists.records.info"
                or item.get("fields") != expected_fields):
            raise RuntimeError("Wrong actual hydration method/owner/fields")
        if any(type(item.get(key)) is not int or item[key] <= 0 for key in ("open", "detail", "revision", "serial")):
            raise RuntimeError("Missing captured visit/revision identities")
        opens.add(item["open"])
        serials.add(item["serial"])
    if (len(opens) != 3 or len(serials) != 3
            or receipt.get("openGenerations") != [item["open"] for item in attempts]
            or receipt.get("recordOpenRequests") != 3):
        raise RuntimeError("Detail reopening did not retire/recreate actual mount ownership")
    first, final = populations()
    if (receipt.get("initialPopulation") != initial_population()
            or receipt.get("responses") != [response(i) for i in range(3)]
            or receipt.get("afterFirst") != first or receipt.get("beforeLate") != first
            or receipt.get("finalPopulation") != final):
        raise RuntimeError("Exact population/raw response oracle failed")


def validate_native(fixture, nonce, pid, directory, receipt, stages_seen, platform=sys.platform, expected_dimensions=None):
    spec, stages = contract(fixture)
    validate_receipt(spec, fixture, receipt, platform)
    if type(receipt.get("preparedRequests")) is not int:
        raise RuntimeError("Prepared request count must be an integer")
    validate_wire(receipt)
    geometry = receipt.get("geometry")
    if (not isinstance(geometry, list) or [g.get("stage") for g in geometry] != list(STAGES)
            or any(any(g.get(flag) is not True for flag in ("ok", "bounded", "partial", "duplicateBlocked"))
                   or g.get("viewport") != receipt.get("viewport") or g.get("hitPointsPerControl") != 9
                   or not isinstance(g.get("controls"), list) or len(g["controls"]) < 2
                   or any(c.get("hit") is not True for c in g["controls"]) for g in geometry)):
        raise RuntimeError("Incomplete hydration control geometry")
    for item in geometry:
        stage = item["stage"]
        expected = (["#slack-list-delete-cancel", "#slack-list-delete-confirm-button"] if stage == "confirmation" else
                    ["#slack-list-record-back", "#slack-list-record-delete", "#slack-list-record-duplicate"]
                    if stage in ("pending", "detail", "stale") else
                    ["#slack-list-close", 'tr[data-row-id="R_HYDRATE_PARENT"] .slack-list-record-open'])
        if [control.get("selector") for control in item["controls"]] != expected:
            raise RuntimeError("Wrong controls were measured")
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


def run(fixture, binary, captures):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"slack-list-hydration-{nonce}"
    directory.mkdir()
    with (directory / "owner.json").open("x", encoding="utf-8") as owner:
        json.dump({"version": 1, "nonce": nonce, "fixture": fixture}, owner)
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session/storage is read."]}
    env = {FLAG: "1", "SUPER_PLATINUM_LIST_HYDRATION_NONCE": nonce,
           "SUPER_PLATINUM_LIST_HYDRATION_READY_DIR": str(directory),
           "SUPER_PLATINUM_LIST_HYDRATION_CAPTURE_DIR": str(directory)}
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
                            raise TimeoutError("Native List hydration assertions timed out")
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-slack-list-hydration")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
