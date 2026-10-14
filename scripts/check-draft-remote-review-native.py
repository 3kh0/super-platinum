#!/usr/bin/env python3
"""Explicit disconnected draft-version review, native handoff and exact isolated restart.

Never builds. All input/IME is synthetic; no Slack transport/session or clipboard.
Windows pointer and geometry checks are sampled, not an atomic capture guarantee.
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
from urllib.parse import parse_qsl, urlsplit
from uuid import UUID
from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import CheckSpec, validate_receipt

# Reuse the already audited owned-process/PNG/placement primitives without
# altering shared defaults or launching that runner.
UTIL = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))
sha = UTIL["binary_sha"]
png = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
FIXTURE = "draft-remote-review"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("remote-channel", "alternate-thread", "typing-during-read-channel",
             "typing-during-commit-thread", "changed-thread", "deferred-composition-channel")
FLAG = "SUPER_PLATINUM_DRAFT_REVIEW_CHECK"
PREFIX, WAITING, STAGE = "draft-review-check: ", "draft-review-waiting: ", "draft-review-stage: "
SUFFIX = " \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
BEFORE, AFTER, NEWER, ALTERNATE, BLOCKED = (s + SUFFIX for s in
    ("Current local", "Latest Slack", "Newer typing", "Saved alternate", "Preserved local"))
REQUESTS = dict(zip(SCENARIOS, (1, 1, 3, 2, 2, 1)))


def stages(scenario, phase):
    if scenario not in SCENARIOS or phase not in ("exercise", "restart"):
        raise RuntimeError("Unknown review case")
    if phase == "restart":
        return ("restart",)
    middle = ("second-choice",) if scenario in ("typing-during-commit-thread", "changed-thread") else ()
    return tuple(f"review-{i}" for i in range(6)) + ("handoff",) + middle + ("result",)


def ready_tuple(nonce, pid, scenario, phase):
    stages(scenario, phase)
    if (type(pid) is not int or pid <= 0 or len(nonce) != 32
            or any(c not in "0123456789abcdef" for c in nonce)):
        raise RuntimeError("Invalid readiness owner")
    return {"version": 1, "nonce": nonce, "pid": pid, "scenario": scenario, "phase": phase}


def validate_ready(actual, expected):
    if actual != expected or type(actual.get("version")) is not int or type(actual.get("pid")) is not int:
        raise RuntimeError("Wrong exact readiness tuple")


def publish_ready(database, expected):
    path = database.parent / f"ready-{expected['phase']}.json"
    temporary = path.with_suffix(".staged")
    with temporary.open("x", encoding="utf-8") as output:
        json.dump(expected, output)
        output.flush()
        os.fsync(output.fileno())
    try:
        os.link(temporary, path)
    finally:
        temporary.unlink()


def validate_request_url(value):
    # SlackClient::rest_form -> XParams::rest_pairs (default fixture client).
    # Keep the complete recorded URL; only random v4 identifiers vary.
    try:
        url = urlsplit(value)
        pairs = parse_qsl(url.query, keep_blank_values=True, strict_parsing=True)
        names = ("_x_id", "_x_csid", "_x_desktop_ia", "_x_foreground",
                 "_x_frontend_build_type", "_x_gantry", "_x_num_retries",
                 "_x_version_ts", "fp", "slack_route")
        fields = dict(pairs)
        if (url.scheme != "https" or url.netloc != "fixture.slack.com"
                or url.path != "/api/drafts.info" or url.fragment
                or [key for key, _ in pairs] != list(names)):
            raise ValueError("Wrong route/query")
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
        raise RuntimeError("Wrong actual positive-read URL") from error


def validate(fixture, scenario, phase, nonce, pid, database, receipt):
    if fixture not in FIXTURES:
        raise RuntimeError("Unknown exact fixture")
    stages(scenario, phase)
    count = REQUESTS[scenario] if phase == "exercise" else 0
    validate_receipt(CheckSpec(fixture=FIXTURE, flag=FLAG, prefix=PREFIX, prepared_requests=count,
        required=("noSession", "noTransport", "canonicalMessagesUnchanged", "syntheticInput", "syntheticIME", "noProductionRoute")), fixture, receipt)
    target = "thread" if scenario.endswith("thread") else "channel"
    for k, v in {"fixture": fixture, "scenario": scenario, "phase": phase, "nonce": nonce,
                 "pid": pid, "target": target, "isolatedDatabase": str(database)}.items():
        if receipt.get(k) != v:
            raise RuntimeError(f"Wrong exact {k}")
    before, final = receipt.get("initial", {}), receipt.get("durable", {})
    if set(before) != {"channel", "thread"} or set(final) != set(before):
        raise RuntimeError("Incomplete durable inventory")
    expected = (ALTERNATE if scenario == "alternate-thread" else NEWER if scenario in
                ("typing-during-read-channel", "typing-during-commit-thread", "changed-thread") else AFTER)
    other = "channel" if target == "thread" else "thread"
    if before[other] != final[other]:
        raise RuntimeError("Unrelated destination inventory changed")
    row = final[target]
    expected_scope = {"account_id": "00000000-0000-4000-8000-000000000081", "key": {
        "team": "T1", "channel": "C2", "thread": "1719800400.000000" if target == "thread" else None}}
    if row.get("scope") != expected_scope:
        raise RuntimeError("Wrong exact durable account/destination")
    if (row.get("canonical", {}).get("composer", {}).get("text") != expected
            or row["canonical"]["composer"].get("reply_broadcast") is not (target == "thread")
            or row["canonical"].get("attachments") != []
            or row.get("blocked") is not None or row.get("alternates") != []):
        raise RuntimeError("Chosen canonical version or reviewed-inventory retirement mismatch")
    if phase == "restart" and before != final:
        raise RuntimeError("Restart mutated stored inventory")
    attempts = receipt.get("attempts")
    if not isinstance(attempts, list) or len(attempts) != count:
        raise RuntimeError("Actual PreparedRequest ledger missing")
    remote_id = "DrReviewThread" if target == "thread" else "DrReviewChannel"
    for request in attempts:
        validate_request_url(request.get("url"))
        if (request.get("method") != "POST"
                or request.get("retrySafe") is not False
                or request.get("form") != [["token", "fixture-only-token"], ["draft_id", remote_id]]):
            raise RuntimeError("Wrong actual positive-read request")
        raw = request.get("rawResponse", {})
        destination = {"channel_id": "C2"}
        if target == "thread":
            destination.update(thread_ts="1719800400.000000", broadcast=True)
        expected_raw = {"ok": True, "draft": {"id": remote_id,
            "client_msg_id": "00000000-0000-4000-8000-000000000083" if target == "thread" else "00000000-0000-4000-8000-000000000082",
            "last_updated_ts": "12.000001", "team_id": "T1", "user_id": "U00000001",
            "is_deleted": False, "is_sent": False, "date_scheduled": 0,
            "blocks": [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [{"type": "text", "text": AFTER}]}]}],
            "file_ids": [], "destinations": [destination]}}
        if raw != expected_raw:
            raise RuntimeError("Missing exact injected raw response")
        if request.get("inventory", {}).get("scope") != row.get("scope"):
            raise RuntimeError("Request captured another inventory scope")
    result = receipt.get("result", {})
    native = result.get("initialNative" if phase == "restart" else "native", {})
    if (native.get("text") != expected or native.get("display") != expected
            or native.get("codePoints") != [ord(c) for c in expected]
            or native.get("utf16Length") != len(expected.encode("utf-16-le")) // 2):
        raise RuntimeError("Native Unicode projection differs from durable choice")
    if phase == "exercise":
        first = before[target]
        if (first.get("canonical", {}).get("composer", {}).get("text") != BEFORE
                or first.get("blocked", {}).get("local", {}).get("composer", {}).get("text") != BLOCKED
                or len(first.get("alternates", [])) != 2
                or row["stored"]["revision"] <= first["stored"]["revision"]):
            raise RuntimeError("Missing complete initial inventory or durable commit")
        review = result.get("initialReview", {})
        if review.get("selected") != 0 or review.get("applyDisabled") is not True or len(review.get("rows", [])) != 6:
            raise RuntimeError("Explicit full inventory/no default proof missing")
        text = "\n".join(r.get("text", "") for r in review["rows"])
        if not all(value in text for value in (BEFORE, BLOCKED, ALTERNATE, AFTER, "Previously observed Slack version")):
            raise RuntimeError("Incomplete readable version inventory")
        if len(review.get("readableRows", [])) != 6:
            raise RuntimeError("Every inventory version needs its own readable native capture")
        for index, readable in enumerate(review["readableRows"]):
            if readable.get("ok") is not True or readable.get("index") != index:
                raise RuntimeError("Wrong readable inventory row")
        if scenario in ("typing-during-read-channel", "typing-during-commit-thread", "changed-thread"):
            preserved = result.get("preservedBeforeChoice", {}).get(target, {})
            if not any(a.get("local", {}).get("composer", {}).get("text") == NEWER for a in preserved.get("alternates", [])):
                raise RuntimeError("Newer typing not durably preserved before explicit choice")
            if receipt.get("routedInputs", 0) < 1:
                raise RuntimeError("Native input never reached isolated editor")
        if scenario == "deferred-composition-channel":
            finish = result.get("finishAfterRemount", {})
            if (finish.get("noAdditionalRead") is not True
                    or finish.get("before", {}).get("text") != BEFORE
                    or finish.get("after", {}).get("text") != BEFORE
                    or finish["before"].get("visit") == finish["after"].get("visit")):
                raise RuntimeError("Unchanged deferred composer did not finish across actual remount")
            if result.get("preservedBeforeChoice") != final:
                raise RuntimeError("Finish modified the already committed durable choice")
    return final


def validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory, receipt, edges, dimensions):
    validate(fixture, scenario, phase, nonce, pid, database, receipt)
    names = stages(scenario, phase)
    if edges != [{"stage": s, "edge": e} for s in names for e in ("before", "after")]:
        raise RuntimeError("Missing or extra ordered stage evidence")
    if set(receipt.get("captures", {})) != set(names):
        raise RuntimeError("Wrong exact stage capture set")
    sizes = {}
    for stage in names:
        path = directory / f"{fixture}-{scenario}-{phase}-{stage}.png"
        if receipt["captures"][stage] != str(path):
            raise RuntimeError("Unowned capture path")
        sizes[stage] = png(path, dimensions)
    return sizes


def run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest):
    label = f"{fixture}-{scenario}-{phase}"
    env = {FLAG: "1", "SUPER_PLATINUM_DRAFT_REVIEW_DB": str(database),
           "SUPER_PLATINUM_DRAFT_REVIEW_NONCE": nonce, "SUPER_PLATINUM_DRAFT_REVIEW_SCENARIO": scenario,
           "SUPER_PLATINUM_DRAFT_REVIEW_PHASE": phase, "SUPER_PLATINUM_DRAFT_REVIEW_CAPTURE_DIR": str(directory)}
    if sha(binary) != digest:
        raise RuntimeError("Binary changed before owned launch")
    evidence = {"ok": False, "phase": phase}
    native = NativeFixture(fixture, directory / f"{label}.log", binary, env)
    process_api = handle = None
    try:
        with native:
            pid = native.process.pid
            evidence["pid"] = pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(pid)
                evidence["processCreationFiletime"] = process_api.times(handle)["creation_filetime"]
            with ExitStack() as stack:
                def capture_failure():
                    if evidence.get("nativeValidated"):
                        return
                    try:
                        path = directory / f"{label}-failure.png"
                        native.capture(path)
                        evidence["failureScreenshot"] = str(path)
                    except (OSError, RuntimeError, ValueError):
                        pass
                stack.callback(capture_failure)
                expected = ready_tuple(nonce, pid, scenario, phase)
                deadline = time.monotonic() + 25
                while not (waiting := native.receipts(WAITING)):
                    native.require_running()
                    if native.receipts(PREFIX) or time.monotonic() >= deadline:
                        raise RuntimeError("Bounded readiness not reached")
                    time.sleep(.05)
                if len(waiting) != 1:
                    raise RuntimeError("Duplicate readiness")
                validate_ready(waiting[0], expected)
                pointer = normalize(native, evidence, stack, process_api, handle)
                publish_ready(database, expected)
                evidence["readiness"] = expected
                deadline, sampled, edge_count = time.monotonic() + 130, 0., 0
                while True:
                    native.require_running()
                    edges = native.receipts(STAGE)
                    if pointer:
                        for edge in edges[edge_count:]:
                            pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                        if time.monotonic() - sampled >= .2:
                            pointer.check("running", require_clear=True)
                            sampled = time.monotonic()
                    edge_count = len(edges)
                    if receipts := native.receipts(PREFIX):
                        if len(receipts) != 1:
                            raise RuntimeError("Duplicate terminal receipt")
                        evidence["native"] = receipts[0]
                        evidence["pngDimensions"] = validate_artifacts(fixture, scenario, phase, nonce, pid, database,
                            directory, receipts[0], edges, evidence.get("expectedPngDimensions"))
                        evidence["nativeValidated"] = True
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Native review receipt timed out")
                    time.sleep(.05)
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned process did not exit")
        if sys.platform == "win32":
            validate_pointer_cleanup(evidence.get("pointer", {}))
        waiting, terminals = closed_receipts(native.log, WAITING), closed_receipts(native.log, PREFIX)
        if len(waiting) != 1 or terminals != [evidence["native"]]:
            raise RuntimeError("Late extra readiness or terminal receipt")
        validate_ready(waiting[0], expected)
        validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory,
            terminals[0], closed_receipts(native.log, STAGE), evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        if handle:
            evidence["exitIdentity"] = process_api.times(handle)
            validate_exit(evidence["processCreationFiletime"], evidence["exitIdentity"])
        if sha(binary) != digest:
            raise RuntimeError("Binary changed while owned process was running")
        evidence["ok"] = True
    except BaseException as error:
        evidence["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        evidence["processExited"] = native.process is not None and native.process.poll() is not None
        if handle:
            evidence["cleanupProcessIdentity"] = process_api.times(handle)
            process_api.kernel.CloseHandle(handle)
        (directory / f"{label}.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return evidence


def run(fixture, scenario, binary, captures):
    stages(scenario, "exercise")
    directory = captures / f"{fixture}-{scenario}"
    directory.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    storage = directory / f"draft-remote-review-{nonce}"
    storage.mkdir()
    with (storage / "owner.json").open("x", encoding="utf-8") as marker:
        json.dump({"version": 1, "nonce": nonce}, marker)
    database = storage / "draft-review.sqlite"
    digest = sha(binary)
    evidence = {"ok": False, "fixture": fixture, "scenario": scenario, "nonce": nonce,
                "platform": sys.platform, "binary": str(binary), "binarySha256": digest,
                "isolatedDatabase": str(database), "phases": [],
                "limitations": ["Synthetic input/IME; no trusted keyboard or live Slack.",
                    "Pointer/geometry checks are sampled; actual PNG inspection is still required.",
                    "PNG chunk/CRC/scanline validation does not decode pixel colors.",
                    "Reviewed versions are explicitly retired; unrelated destination inventory must remain exact."]}
    try:
        for phase in ("exercise", "restart"):
            evidence["phases"].append(run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest))
        first, second = (p["native"]["durable"] for p in evidence["phases"])
        if first != second:
            raise RuntimeError("Second process changed whole canonical/blocked/alternate dictionaries")
        evidence.update(ok=True, twoProcessRestart=True, exactDurableReadback=True)
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        path = directory / f"{fixture}-{scenario}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/draft-remote-review")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or SCENARIOS:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
