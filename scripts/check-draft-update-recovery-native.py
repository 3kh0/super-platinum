#!/usr/bin/env python3
"""Isolated uncertain Update recovery with one read and exact durable restart checks.

Never builds. Synthetic inputs/credentials/responses; no live Slack or OS IME.
A read is the actual drafts.info POST builder, never a mutating request replay.
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
from uuid import UUID
from urllib.parse import urlsplit

from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import expected_viewport

UTIL = runpy.run_path(str(Path(__file__).with_name("check-slack-list-board-add-native.py")))
REVIEW = runpy.run_path(str(Path(__file__).with_name("check-draft-remote-review-native.py")))
PREF = runpy.run_path(str(Path(__file__).with_name("check-read-preferences-native.py")))
sha = UTIL["binary_sha"]
png = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
strict_equal = PREF["strict_equal"]
number = PREF["number"]
rectangle = PREF["rectangle"]
validate_request_url = REVIEW["validate_request_url"]
FIXTURE = "draft-update-recovery"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("positive-channel", "typing-before-channel", "typing-during-commit-thread",
             "owner-rotation-channel", "cooldown-channel", "pending-check-channel")
FLAG = "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_CHECK"
PREFIX, WAITING, STAGE = "draft-update-recovery-check: ", "draft-update-recovery-waiting: ", "draft-update-recovery-stage: "
ACCOUNT = "00000000-0000-4000-8000-000000000091"
BASE = "Previous acknowledged content"
SUFFIX = " \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
REQUESTED, NEWER = "Requested update" + SUFFIX, "Newer local typing" + SUFFIX


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def same(actual, expected, reason):
    require(strict_equal(actual, expected), reason)


def stages(scenario, phase):
    require(scenario in SCENARIOS and phase in ("exercise", "restart"), "Unknown recovery case")
    if phase == "restart":
        return ("restart",)
    return (("initial", "input", "checking", "result") if scenario == "typing-before-channel"
            else ("initial", "checking", "input", "result") if scenario == "typing-during-commit-thread"
            else ("initial", "checking", "result"))


def scope(target):
    return {"account_id": ACCOUNT, "key": {"team": "T1", "channel": "C2",
            "thread": "1719800400.000000" if target == "thread" else None}}


def snapshot(text, target):
    destination = {"channel_id": "C2"}
    if target == "thread":
        destination.update(thread_ts="1719800400.000000", broadcast=True)
    return {"blocks": [{"type": "rich_text", "elements": [{"type": "rich_text_section",
            "elements": [{"type": "text", "text": text}]}]}], "file_ids": [], "destinations": [destination]}


def remote(target, text, version):
    return {"scope": scope(target), "id": "DrUpdateThread" if target == "thread" else "DrUpdateChannel",
            "client_msg_id": "00000000-0000-4000-8000-000000000093" if target == "thread" else
            "00000000-0000-4000-8000-000000000092", "last_updated_ts": version, "snapshot": snapshot(text, target)}


def uuid4(value):
    try:
        parsed = UUID(value)
        return str(parsed) == value and parsed.version == 4
    except (ValueError, TypeError, AttributeError):
        return False


def operation(value, target):
    require(isinstance(value, dict) and uuid4(value.get("id")), "Missing frozen operation UUID")
    expected = dict(id=value["id"], scope=scope(target), client_msg_id=remote(target, BASE, "10.000001")["client_msg_id"],
                    local_revision=1, kind="Update", snapshot=snapshot(REQUESTED, target),
                    base=remote(target, BASE, "10.000001"), consume_id=None)
    same(value, expected, "Frozen Update identity/content/revision differs")
    return value


def inventory(value, target, text):
    require(isinstance(value, dict), "Missing inventory")
    same(value.get("scope"), scope(target), "Wrong inventory scope")
    same(value.get("blocked"), None, "Unexpected blocked copy")
    same(value.get("alternates"), [], "Unexpected alternate copies")
    local = value.get("canonical", {})
    same(local.get("attachments"), [], "Unexpected attachment")
    same(local.get("composer", {}).get("text"), text, "Wrong durable current text")
    same(local.get("composer", {}).get("reply_broadcast"), target == "thread", "Wrong broadcast choice")
    stored = value.get("stored", {})
    require(type(stored.get("revision")) is int and stored["revision"] > 0, "Missing stored revision")
    state = stored.get("state", {})
    same(state.get("scope"), scope(target), "Wrong stored scope")
    same(state.get("desired"), snapshot(text, target), "Stored desired differs from canonical")
    same(state.get("client_msg_id"), remote(target, BASE, "10.000001")["client_msg_id"], "Wrong stored identity")
    return state


def native(value, text, viewport):
    require(isinstance(value, dict) and value.get("ok") is True, "Missing native projection")
    for name, expected in {"text": text, "display": text, "codePoints": [ord(c) for c in text],
                           "utf16Length": len(text.encode("utf-16-le")) // 2, "viewport": viewport,
                           "composing": False}.items():
        same(value.get(name), expected, "Native Unicode/projection mismatch: " + name)
    require(isinstance(value.get("scope"), str) and value["scope"] and isinstance(value.get("visit"), str)
            and value["visit"], "Missing mounted scope/visit")
    require(type(value.get("start")) is int and type(value.get("end")) is int
            and 0 <= value["start"] <= value["end"] <= value["utf16Length"]
            and value.get("direction") in ("forward", "backward", "none"), "Invalid native selection")
    r = value.get("rect", {})
    require(all(number(r.get(k)) for k in ("left", "top", "right", "bottom", "width", "height")), "Invalid textarea rectangle")
    require(abs(r["right"] - r["left"] - r["width"]) <= 1 and abs(r["bottom"] - r["top"] - r["height"]) <= 1,
            "Inconsistent textarea rectangle")
    require(rectangle(dict(x=r["left"], y=r["top"], width=r["width"], height=r["height"]), viewport), "Textarea clipped")


def check_row(value, op, viewport, disabled=None, contains=None):
    require(isinstance(value, dict) and value.get("ok") is True and value.get("hit") is True, "Unreadable recovery action")
    for field in ("threadPaneAbsent", "rowUnobscured", "buttonUnobscured"):
        require(value.get(field) is True, "Recovery row missing exact visibility proof: " + field)
    same(value.get("viewport"), viewport, "Wrong row viewport")
    same(value.get("kind"), "update", "Wrong recovery kind")
    same(value.get("operation"), op["id"], "Wrong row operation")
    require(type(value.get("disabled")) is bool, "Missing disabled state")
    if disabled is not None:
        same(value["disabled"], disabled, "Wrong Check enabled state")
    require(isinstance(value.get("buttonText"), str) and "Check" in value["buttonText"], "Missing explicit Check action")
    require(isinstance(value.get("text"), str) and (contains is None or contains in value["text"]), "Missing recovery status copy")
    require(number(value.get("documentWidth")) and 0 < value["documentWidth"] <= viewport[0] + 1, "Row document overflows")
    for key in ("row", "button", "panel"):
        require(rectangle(value.get(key), viewport), "Recovery rectangle clipped: " + key)
    panel = value["panel"]
    for key in ("row", "button"):
        bounds = value[key]
        for axis, extent, limit in (("x", "width", viewport[0]), ("y", "height", viewport[1])):
            require(bounds[axis] >= max(0, panel[axis]) - 1
                    and bounds[axis] + bounds[extent] <= min(limit, panel[axis] + panel[extent]) + 1,
                    f"Recovery {key} clipped on {axis} axis against panel/viewport")


def validate(fixture, scenario, phase, nonce, pid, database, receipt):
    names = stages(scenario, phase)
    require(fixture in FIXTURES and isinstance(receipt, dict), "Wrong fixture/receipt")
    target = "thread" if scenario.endswith("thread") else "channel"
    other = "channel" if target == "thread" else "thread"
    count = int(phase == "exercise")
    for key, value in dict(ok=True, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce, pid=pid,
                           target=target, isolatedDatabase=str(database), preparedRequests=count,
                           noSession=True, noTransport=True, noProductionRoute=True, syntheticInput=True,
                           actualPreparedRequests=True, canonicalMessagesUnchanged=True).items():
        same(receipt.get(key), value, "Wrong exact receipt field: " + key)
    before, final, details = receipt.get("initial"), receipt.get("durable"), receipt.get("details")
    require(isinstance(before, dict) and set(before) == {"channel", "thread"}
            and isinstance(final, dict) and set(final) == set(before) and isinstance(details, dict), "Incomplete durable inventories")
    same(before[other], final[other], "Unrelated destination changed")
    inventory(before[other], other, REQUESTED)
    expected = NEWER if scenario.startswith("typing-") else REQUESTED
    state = inventory(final[target], target, expected)
    initial_state = inventory(before[target], target, REQUESTED if phase == "exercise" else expected)
    unresolved = scenario in ("cooldown-channel", "pending-check-channel")
    op = receipt.get("operation")
    if phase == "exercise" or unresolved:
        operation(op, target)
    else:
        same(op, None, "Restart unexpectedly retains active operation")
    if phase == "exercise":
        same(initial_state.get("workflow"), {"Uncertain": op}, "Seed was not frozen uncertain Update")
        same(initial_state.get("check_ticket"), None, "Seed already had a ticket")
        same(initial_state.get("check_deadline"), None, "Seed already had a cooldown")
        same(initial_state.get("acknowledged"), op["base"], "Wrong seeded acknowledgement")
        require(final[target]["stored"]["revision"] > before[target]["stored"]["revision"], "No durable admission commit")
    else:
        same(before, final, "Restart mutated durable inventories")
    same(state.get("workflow"), {"Uncertain": op} if unresolved else "Idle", "Wrong final workflow")
    same(state.get("local_revision"), 2 if expected == NEWER else 1, "Wrong local revision")
    same(state.get("acknowledged_local_revision"), 0 if unresolved else 1, "Wrong acknowledged local revision")
    same(state.get("acknowledged"), remote(target, BASE if unresolved else REQUESTED, "10.000001" if unresolved else "12.000001"), "Wrong positive acknowledgement")
    if scenario != "pending-check-channel":
        same(state.get("check_ticket"), None, "Completed read retained pending ticket")
    else:
        ticket = state.get("check_ticket", {})
        require(isinstance(ticket, dict) and uuid4(ticket.get("id")), "Unresolved read lost durable ticket")
        same(ticket.get("operation"), op, "Pending restart lost exact operation")
    attempts = receipt.get("attempts")
    require(isinstance(attempts, list) and len(attempts) == count, "Missing exact one read / restart zero ledger")
    if phase == "exercise":
        request = attempts[0]
        validate_request_url(request.get("url"))
        for key, value in dict(method="POST", path="/api/drafts.info", rawQuery=urlsplit(request["url"]).query,
            fields=[["draft_id", op["base"]["id"]]], retrySafe=False, operation=op,
            delivered=scenario != "pending-check-channel", outcome="rate-limited" if scenario == "cooldown-channel" else "positive",
            retryAfterSeconds=120 if scenario == "cooldown-channel" else None).items():
            same(request.get(key), value, "Wrong actual read evidence: " + key)
        require(type(request.get("nowMs")) is int and request["nowMs"] > 0, "Missing read admission time")
        raw = {"ok": True, "draft": {"id": op["base"]["id"], "client_msg_id": op["client_msg_id"],
               "last_updated_ts": "12.000001", "team_id": "T1", "user_id": "U00000001", "is_deleted": False,
               "is_sent": False, "date_scheduled": 0, **snapshot(REQUESTED, target)}}
        same(request.get("rawResponse"), None if scenario == "cooldown-channel" else raw, "Wrong exact raw readback")
        held = details.get("held", {})
        require(isinstance(held, dict) and set(held) == set(before), "Missing durable admission inventory")
        same(held[other], before[other], "Admission changed unrelated scope")
        held_state = inventory(held[target], target, NEWER if scenario == "typing-before-channel" else REQUESTED)
        ticket = held_state.get("check_ticket", {})
        require(isinstance(ticket, dict) and uuid4(ticket.get("id")), "Missing durable ticket UUID")
        same(ticket.get("operation"), op, "Read admitted for wrong immutable operation")
        same(held_state.get("workflow"), {"Uncertain": op}, "Admission released operation")
        held_deadline = held_state.get("check_deadline", {})
        same(held_deadline.get("operation"), op, "Admission deadline wrong full operation")
        same(held_deadline.get("operation_id"), op["id"], "Admission deadline wrong id")
        require(type(held_deadline.get("until_ms")) is int
                and request["nowMs"] < held_deadline["until_ms"] <= request["nowMs"] + 30000,
                "Missing admission cooldown (request follows reserve persistence)")
        if scenario == "pending-check-channel":
            same(final, held, "Held response changed durable state")
        else:
            same(details.get("afterReceipt"), final, "Post-receipt inventory differs")
        deadline = state.get("check_deadline", {})
        same(deadline.get("operation"), op, "Deadline not bound to full operation")
        same(deadline.get("operation_id"), op["id"], "Deadline wrong operation id")
        same(deadline.get("until_ms"), request["nowMs"] + 120000 if scenario == "cooldown-channel"
             else held_deadline["until_ms"], "Lost durable cooldown")
    require(type(receipt.get("routedInputs")) is int and receipt["routedInputs"] >= 0, "Invalid routed input count")
    if phase == "exercise" and scenario.startswith("typing-"):
        require(receipt["routedInputs"] >= 1, "Input never reached isolated editor")
    viewport = expected_viewport(fixture)
    same(receipt.get("viewport"), viewport, "Wrong terminal viewport")
    geometry = receipt.get("geometry")
    require(isinstance(geometry, list) and len(geometry) == len(names), "Wrong geometry count")
    for entry, stage in zip(geometry, names):
        same(entry.get("stage"), stage, "Wrong geometry order")
        is_row = stage in ("initial", "checking") or (stage in ("result", "restart") and unresolved)
        if is_row:
            check_row(entry, op, viewport, True if stage == "checking" or (stage == "result" and scenario == "pending-check-channel") else False)
        else:
            native(entry, expected, viewport)
    if phase == "exercise" and scenario.startswith("typing-"):
        native(details.get("input"), NEWER, viewport)
    if not unresolved:
        native(details.get("native"), expected, viewport)
    if phase == "exercise" and scenario == "typing-during-commit-thread":
        for key in ("visit", "scope", "start", "end", "direction"):
            same(details["input"].get(key), details["native"].get(key), "Receipt replaced native visit/selection")
    if phase == "exercise" and scenario == "owner-rotation-channel":
        rotation = details.get("ownerRotation", {})
        require(type(rotation.get("beforeEpoch")) is int, "Missing owner epoch")
        same(rotation.get("afterEpoch"), rotation["beforeEpoch"] + 1, "Missing owner rotation")
        same(rotation.get("managedBefore"), rotation.get("managedAfter"), "Stale receipt projected into new owner")
        require(type(rotation.get("managedBefore")) is int and rotation["managedBefore"] > 0, "Missing managed revision")
    if phase == "restart" and unresolved:
        check_row(details.get("restartRefusal"), op, viewport, False,
                  "unresolved" if scenario == "pending-check-channel" else "cooldown")
    return final


def validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory, receipt, edges, dimensions):
    validate(fixture, scenario, phase, nonce, pid, database, receipt)
    names = stages(scenario, phase)
    same(edges, [{"stage": s, "edge": e} for s in names for e in ("before", "after")], "Missing or extra ordered capture edges")
    require(set(receipt.get("captures", {})) == set(names), "Wrong capture set")
    sizes = {}
    for stage in names:
        path = directory / f"{fixture}-{scenario}-{phase}-{stage}.png"
        same(receipt["captures"][stage], str(path), "Unowned capture path")
        sizes[stage] = png(path, dimensions)
    return sizes


def ready_tuple(nonce, pid, scenario, phase):
    if scenario not in SCENARIOS or phase not in ("exercise", "restart"):
        raise RuntimeError("Unknown update-recovery scenario/phase")
    if (type(pid) is not int or pid <= 0 or not isinstance(nonce, str) or len(nonce) != 32
            or any(c not in "0123456789abcdef" for c in nonce)):
        raise RuntimeError("Invalid readiness owner")
    return {"version": 1, "nonce": nonce, "pid": pid, "scenario": scenario, "phase": phase}


def validate_ready(actual, expected):
    if not strict_equal(actual, expected):
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


def run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest):
    label = f"{fixture}-{scenario}-{phase}"
    env = {FLAG: "1", "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_DB": str(database),
           "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_NONCE": nonce, "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_SCENARIO": scenario,
           "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_PHASE": phase, "SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_CAPTURE_DIR": str(directory)}
    if sha(binary) != digest:
        raise RuntimeError("Binary changed before owned launch")
    evidence = {"ok": False, "phase": phase, "binarySha256Before": digest}
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
                        raise TimeoutError("Native update-recovery receipt timed out")
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
        evidence["binarySha256After"] = sha(binary)
        if evidence["binarySha256After"] != digest:
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
    storage = directory / f"draft-update-recovery-{nonce}"
    storage.mkdir()
    with (storage / "owner.json").open("x", encoding="utf-8") as marker:
        json.dump({"version": 1, "nonce": nonce}, marker)
    database = storage / "draft-update-recovery.sqlite"
    digest = sha(binary)
    evidence = {"ok": False, "fixture": fixture, "scenario": scenario, "nonce": nonce,
                "platform": sys.platform, "binary": str(binary), "binarySha256": digest,
                "isolatedDatabase": str(database), "phases": [],
                "limitations": ["Synthetic DOM input; no trusted keyboard, OS IME, or live Slack.",
                    "Pointer/geometry checks are sampled; actual PNG inspection is still required.",
                    "PNG chunk/CRC/scanline validation does not decode pixel colors.",
                    "Exact frozen Update only; no automatic retry, remote autosave or live synchronization proof.",
                    "Pending response is held through controlled owned process termination; this is not a crash-durability claim."]}
    try:
        for phase in ("exercise", "restart"):
            evidence["phases"].append(run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest))
        first, second = (p["native"]["durable"] for p in evidence["phases"])
        if not strict_equal(first, second):
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-draft-update-recovery")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or SCENARIOS:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
