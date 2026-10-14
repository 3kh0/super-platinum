#!/usr/bin/env python3
"""Disconnected attachment editing through real controls and recorded request builders.

Never builds or reads sessions. Ownership rotation is synthetic; no real Slack
request is sent, and in-memory reservations are not restart-durability evidence.
"""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import runpy
import secrets
import sys
import time
from urllib.parse import urlsplit, urlunsplit
from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import CheckSpec, validate_receipt

UTIL = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))
REQUEST_URL = runpy.run_path(str(Path(__file__).with_name("check-draft-remote-review-native.py")))["validate_request_url"]
FLAG = "SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_CHECK"
FIXTURE = "scheduled-attachments"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
PREFIX, WAITING, STAGE = "scheduled-attachments-check: ", "scheduled-attachments-waiting: ", "scheduled-attachments-stage: "
CASES = {
    "ordered-rich": (("initial", "removed", "undo", "selected", "confirmed", "last-file", "empty-confirmed"), 4, 2),
    "empty-body-cancel": (("empty-guard", "empty-restored", "checking", "canceled"), 1, 0),
    "uncertain-owner": (("initial", "selected", "uncertain", "resumed", "reviewed"), 3, 1),
    "known-unsent": (("initial", "selected", "recoverable", "confirmed"), 4, 2),
    "rejected": (("initial", "selected", "recoverable", "confirmed"), 4, 2),
}
FILES = ["F00000001", "F00000002", "F00000003"]
DRAFT, EMPTY = "DrAttachmentFixture", "DrAttachmentEmpty"


def blocks():
    return [{"type": "rich_text", "block_id": "retained-body", "elements": [{"type": "rich_text_section", "elements": [
        {"type": "text", "text": "Attachment review \u65e5\u672c\u8a9e \U0001f680", "style": {"bold": True}},
        {"type": "text", "text": "\nKeep this exact rich body and thread destination."}]}]}]


def destination():
    return [{"channel_id": "C2", "thread_ts": "1719800400.000000", "broadcast": True}]


def ready_tuple(fixture, scenario, nonce, pid):
    if (fixture not in FIXTURES or scenario not in CASES or len(nonce) != 32
            or any(c not in "0123456789abcdef" for c in nonce) or type(pid) is not int or pid <= 0):
        raise RuntimeError("Unknown exact fixture ownership tuple")
    return {"version": 1, "nonce": nonce, "pid": pid, "fixture": fixture, "scenario": scenario}


def validate_raw(raw, ids, empty=False):
    if not isinstance(raw, dict) or raw.get("ok") is not True or set(raw) != {"ok", "draft"}:
        raise RuntimeError("Missing complete positive response envelope")
    draft = raw["draft"]
    expected = {"id": EMPTY if empty else DRAFT,
                "client_msg_id": "00000000-0000-4000-8000-000000000093" if empty else "00000000-0000-4000-8000-000000000092",
                "date_scheduled": 0 if empty else 2100000000,
                "is_deleted": False, "is_sent": False, "file_ids": ids,
                "blocks": [] if empty else blocks(), "destinations": destination()}
    if set(draft) != set(expected) | {"last_updated_ts"} or any(draft.get(k) != v for k, v in expected.items()):
        raise RuntimeError("Raw response changed rich body, identity, schedule, ordered files or destination")
    stamp = draft["last_updated_ts"]
    if not isinstance(stamp, str) or not stamp.endswith(".000001") or not stamp[:-7].isdigit():
        raise RuntimeError("Invalid synthetic full response revision")


def validate(fixture, scenario, nonce, pid, receipt):
    if scenario not in CASES or fixture not in FIXTURES:
        raise RuntimeError("Unknown native contract")
    stages, count, writes = CASES[scenario]
    validate_receipt(CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX, prepared_requests=count,
        required=("noSession", "noTransport", "syntheticInput", "noFileDeletion", "noReplay", "actualPreparedRequests")), fixture, receipt)
    for key, expected in {"fixture": fixture, "scenario": scenario, "nonce": nonce, "pid": pid, "updateRequests": writes}.items():
        if receipt.get(key) != expected:
            raise RuntimeError(f"Wrong {key}")
    ledger = receipt.get("attempts", [])
    if len(ledger) != count:
        raise RuntimeError("Actual request ledger incomplete")
    write_index, current_ids = 0, FILES.copy()
    expected_methods = ["drafts.info"] if scenario == "empty-body-cancel" else (
        ["drafts.info", "drafts.update", "drafts.info"] if scenario == "uncertain-owner" else
        ["drafts.info", "drafts.update", "drafts.info", "drafts.update"])
    for index, (request, method) in enumerate(zip(ledger, expected_methods)):
        url = urlsplit(request.get("url", ""))
        if url.path != f"/api/{method}":
            raise RuntimeError("Unexpected exact request endpoint")
        REQUEST_URL(urlunsplit((url.scheme, url.netloc, "/api/drafts.info", url.query, url.fragment)))
        if request.get("method") != "POST" or request.get("retrySafe") is not False:
            raise RuntimeError("Unexpected method, route, or replay policy")
        generation = 2 if ((scenario in ("known-unsent", "rejected") and index >= 2) or (scenario == "uncertain-owner" and index == 2)) else 1
        if request.get("fixtureGeneration") != generation:
            raise RuntimeError("Wrong captured synthetic credential generation")
        form = request.get("form", [])
        names = [pair[0] for pair in form]
        expected_names = ["token", "draft_id"] if method == "drafts.info" else ["token", "destinations", "blocks", "file_ids", "date_scheduled", "is_from_composer", "draft_id", "client_last_updated_ts"]
        if names != expected_names:
            raise RuntimeError("Unexpected ordered request fields")
        fields = dict(form)
        if fields.get("token") != f"fixture-only-token-{generation}" or fields.get("draft_id") != DRAFT:
            raise RuntimeError("Request owner/identity changed")
        headers = request.get("headers")
        if not isinstance(headers, list) or not headers or any(str(pair[0]).lower() in ("cookie", "authorization") for pair in headers):
            raise RuntimeError("Expected synthetic builder headers without session secrets")
        result = request.get("result", {})
        if method == "drafts.info":
            validate_raw(result.get("raw"), current_ids)
        else:
            chosen = [] if scenario == "ordered-rich" and write_index == 1 else FILES[1:]
            if (json.loads(fields["blocks"]) != blocks() or json.loads(fields["destinations"]) != destination()
                    or json.loads(fields["file_ids"]) != chosen or fields["date_scheduled"] != "2100000000"
                    or fields["is_from_composer"] != "true" or not fields["client_last_updated_ts"].isdigit()):
                raise RuntimeError("Frozen update did not preserve exact ordered rich payload")
            if scenario == "known-unsent" and write_index == 0:
                if result != {"error": "known-not-dispatched"}:
                    raise RuntimeError("Known-unsent proof missing")
            elif scenario == "rejected" and write_index == 0:
                if result != {"raw": {"ok": False, "error": "token_expired"}}:
                    raise RuntimeError("Definite rejection envelope missing")
            elif scenario == "uncertain-owner":
                if result != {"error": "synthetic-response-lost", "serverApplied": True}:
                    raise RuntimeError("Uncertain fixture outcome missing")
                current_ids = chosen
            else:
                validate_raw(result.get("raw"), chosen)
                current_ids = chosen
            write_index += 1
    if write_index != writes:
        raise RuntimeError("Update or replay count differs")
    server = receipt.get("server", {})
    if set(server) != {DRAFT, EMPTY}:
        raise RuntimeError("Unexpected simulated remote inventory")
    validate_raw({"ok": True, "draft": server[DRAFT]}, current_ids)
    validate_raw({"ok": True, "draft": server[EMPTY]}, [FILES[0]], empty=True)
    flags = {
        "ordered-rich": ("actualLegacyEditorReserved", "orderedRemoveUndo", "nonemptyBodyLastFile"),
        "empty-body-cancel": ("emptyBodyGuard", "cancelBeforeDispatch", "noMutation"),
        "uncertain-owner": ("uncertainNoReplay", "stableOwnerRetained", "foreignAccountHidden", "competingReservationBlocked", "positiveReadbackOnly", "syntheticCredentialGenerationRotation"),
        "known-unsent": ("explicitRetryOnly", "stagedSelectionRetained", "knownUnsent", "syntheticCredentialGenerationRotation"),
        "rejected": ("explicitRetryOnly", "stagedSelectionRetained", "definiteRawRejection", "syntheticCredentialGenerationRotation"),
    }[scenario]
    if any(receipt.get("facts", {}).get(flag) is not True for flag in flags):
        raise RuntimeError("Missing scenario-specific assertion")
    layouts = receipt.get("layouts", [])
    if [entry.get("stage") for entry in layouts] != list(stages) or any(entry.get("layout", {}).get("noOverflow") is not True for entry in layouts):
        raise RuntimeError("Missing exact geometry stages")


def artifacts(fixture, scenario, nonce, pid, directory, receipt, edges, dimensions):
    validate(fixture, scenario, nonce, pid, receipt)
    stages = CASES[scenario][0]
    if edges != [{"stage": s, "edge": e} for s in stages for e in ("before", "after")] or set(receipt.get("captures", {})) != set(stages):
        raise RuntimeError("Missing or extra ordered capture stage")
    sizes = {}
    for stage in stages:
        path = directory / f"{fixture}-{scenario}-{stage}.png"
        if receipt["captures"][stage] != str(path):
            raise RuntimeError("Capture escaped the exact owned path")
        sizes[stage] = UTIL["png_signature"](path, dimensions)
    return sizes


def run(fixture, scenario, binary, captures):
    destination = captures / f"{fixture}-{scenario}"
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"scheduled-attachments-{nonce}"
    directory.mkdir()
    with (directory / "owner.json").open("x", encoding="utf-8") as marker:
        json.dump({"version": 1, "nonce": nonce, "fixture": fixture, "scenario": scenario}, marker)
    digest = UTIL["binary_sha"](binary)
    evidence = {"ok": False, "fixture": fixture, "scenario": scenario, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected responses and owner/credential-generation changes, not actual Slack.",
                    "Legacy editor mounting/reservation only; no legacy sends or updates executed.",
                    "Sampled geometry and pointer checks; actual image inspection remains required.",
                    "In-memory operation retention, no process-restart persistence claim."]}
    env = {FLAG: "1", "SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_NONCE": nonce,
           "SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_SCENARIO": scenario,
           "SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_READY_DIR": str(directory),
           "SUPER_PLATINUM_SCHEDULED_ATTACHMENTS_CAPTURE_DIR": str(directory)}
    native = NativeFixture(fixture, destination / f"{fixture}-{scenario}.log", binary, env)
    api = handle = None
    try:
        with native:
            pid = native.process.pid
            evidence["pid"] = pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                api = WindowsAPI(); handle = api.open(pid)
                evidence["processCreationFiletime"] = api.times(handle)["creation_filetime"]
            with ExitStack() as stack:
                def failure_capture():
                    if evidence.get("nativeValidated"):
                        return
                    try:
                        path = destination / f"{fixture}-{scenario}-failure.png"
                        native.capture(path); evidence["failureScreenshot"] = str(path)
                    except (OSError, RuntimeError, ValueError):
                        pass
                stack.callback(failure_capture)
                expected = ready_tuple(fixture, scenario, nonce, pid)
                deadline = time.monotonic() + 25
                while not (waiting := native.receipts(WAITING)):
                    native.require_running()
                    if native.receipts(PREFIX) or time.monotonic() >= deadline:
                        raise RuntimeError("Fixture readiness missing")
                    time.sleep(.05)
                if waiting != [expected]:
                    raise RuntimeError("Wrong or repeated readiness tuple")
                pointer = UTIL["normalize"](native, evidence, stack, api, handle)
                UTIL["publish_ready"](directory, expected)
                evidence["readiness"] = expected
                deadline, last_sample, edge_count = time.monotonic() + 110, 0., 0
                while True:
                    native.require_running()
                    edges = native.receipts(STAGE)
                    if pointer:
                        for edge in edges[edge_count:]:
                            pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                        if time.monotonic() - last_sample > .2:
                            pointer.check("running", require_clear=True); last_sample = time.monotonic()
                    edge_count = len(edges)
                    if receipts := native.receipts(PREFIX):
                        if len(receipts) != 1:
                            raise RuntimeError("Repeated terminal receipt")
                        evidence["native"] = receipts[0]
                        evidence["pngDimensions"] = artifacts(fixture, scenario, nonce, pid, directory, receipts[0], edges, evidence.get("expectedPngDimensions"))
                        evidence["nativeValidated"] = True
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Attachment check timed out")
                    time.sleep(.05)
        if native.process.poll() is None:
            raise RuntimeError("Owned process did not exit")
        if sys.platform == "win32":
            UTIL["validate_pointer_cleanup"](evidence.get("pointer", {}))
        if UTIL["closed_receipts"](native.log, WAITING) != [expected] or UTIL["closed_receipts"](native.log, PREFIX) != [evidence["native"]]:
            raise RuntimeError("Late extra readiness or terminal receipt")
        artifacts(fixture, scenario, nonce, pid, directory, evidence["native"], UTIL["closed_receipts"](native.log, STAGE), evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        if handle:
            evidence["exitIdentity"] = api.times(handle)
            UTIL["validate_exit"](evidence["processCreationFiletime"], evidence["exitIdentity"])
        if UTIL["binary_sha"](binary) != digest:
            raise RuntimeError("Binary changed during native run")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        evidence["processExited"] = native.process is not None and native.process.poll() is not None
        if handle:
            evidence["cleanupProcessIdentity"] = api.times(handle); api.kernel.CloseHandle(handle)
        path = destination / f"{fixture}-{scenario}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/scheduled-attachments")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--scenario", action="append", choices=CASES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or CASES:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
