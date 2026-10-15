"""Strict offline evidence for the first protected local-only native cohort."""
import json
import re
import copy
from pathlib import Path
import runpy
import sys

SCRIPTS = next(parent / "scripts" for parent in Path(__file__).resolve().parents
               if (parent / "Cargo.toml").is_file() and (parent / "scripts/native_fixture.py").is_file())
sys.path.insert(0, str(SCRIPTS))
from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import expected_viewport

UTIL = runpy.run_path(str(SCRIPTS / "check-slack-list-board-add-native.py"))
PREF = runpy.run_path(str(SCRIPTS / "check-read-preferences-native.py"))
UPDATE = runpy.run_path(str(SCRIPTS / "check-draft-update-recovery-native.py"))
sha, png, validate_exit, normalize, validate_pointer_cleanup, closed_receipts = (
    UTIL[name] for name in ("binary_sha", "png_signature", "validate_exit", "normalize",
                            "validate_pointer_cleanup", "closed_receipts"))
strict_equal, number, rectangle = (PREF[name] for name in ("strict_equal", "number", "rectangle"))
require, same, native = (UPDATE[name] for name in ("require", "same", "native"))
uuid4 = UPDATE["uuid4"]
SEND = runpy.run_path(str(SCRIPTS / "check-managed-draft-send-native.py"))
SCHEDULE = runpy.run_path(str(SCRIPTS / "managed_draft_schedule_receipts.py"))

FIXTURES = ("protected-drafts", "protected-drafts-narrow")
SCENARIOS = ("consume-channel", "consume-thread", "expired-clear-thread",
             "canceled-clear-channel", "failed-write-return-channel")
FLAG = "SUPER_PLATINUM_PROTECTED_DRAFTS_CHECK"
PREFIX, WAITING, STAGE = ("protected-drafts-" + name + ": " for name in ("check", "waiting", "stage"))
CONTRACT_READY = Path(__file__).resolve().parent == SCRIPTS


def require_contract():
    require(CONTRACT_READY, "Protected preparation cannot launch before promotion into scripts")


def phases(scenario):
    require(scenario in SCENARIOS, "Unknown protected scenario")
    return ("exercise",)


def stages(scenario, phase="exercise"):
    require(phase in phases(scenario), "Unknown protected phase")
    if scenario.startswith("consume-"):
        return ("initial", "input", "admitted", "settled", "newer")
    if scenario == "expired-clear-thread":
        return ("initial", "input", "admitted", "expired", "newer", "settled")
    if scenario == "canceled-clear-channel":
        return ("initial", "input", "admitted", "canceled", "newer", "settled")
    return ("initial", "input", "failed", "away", "returned", "retried", "newer")


def validate(fixture, scenario, phase, nonce, pid, storage, receipt):
    keys(receipt, ("ok", "fixture", "scenario", "phase", "nonce", "pid", "isolatedRoot", "target", "viewport",
                   "captures", "snapshots", "details", "finalInventory", "finalProjection", "evidenceScope"), "terminal receipt")
    require(fixture in FIXTURES, "Unknown protected fixture")
    target = "thread" if scenario.endswith("thread") else "channel"
    for name, expected in dict(ok=True, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce, pid=pid,
                               isolatedRoot=str(storage), target=target, viewport=expected_viewport(fixture),
                               evidenceScope="protected local checkpoint/consume/native-clear; fixture construction has no HTTP capability").items():
        same(receipt[name], expected, "Wrong receipt identity: " + name)
    records = receipt["snapshots"]
    require(isinstance(records, list), "Missing stage snapshots")
    same([v.get("stage") for v in records], list(stages(scenario, phase)), "Wrong actual stage order")
    snapshots = {v["stage"]: v for v in records}
    initial = snapshots["initial"]["durable"]
    keys(initial, ("channel", "thread"), "initial scopes")
    failure = scenario == "failed-write-return-channel"
    for name in initial:
        seed_inventory(initial[name], name, failure)
    initial_editors = snapshots["initial"]["projection"]["editors"]
    for key in ("retentionId", "writerId", "enrollment"):
        require(initial_editors["channel"][key] != initial_editors["thread"][key], "Distinct scopes reused editor identity: " + key)
    details = receipt["details"]
    keys(details, ("nativeInitial", "typed", "away", "returned", "retry") if failure
         else ("nativeInitial", "typed", "clicked", "result", "canceledWaiter") if scenario.startswith("canceled-") else ("nativeInitial", "typed", "clicked", "result"), "case details")
    native(details["nativeInitial"], INITIAL, expected_viewport(fixture))
    SCHEDULE["home_anchor"](details["nativeInitial"])
    geometry(details["typed"], fixture, target, REQUESTED)
    same_native(details["nativeInitial"], snapshots["initial"]["geometry"]["native"])
    same_native(details["typed"]["native"], snapshots["input"]["geometry"]["native"])
    typed = changed(initial[target], REQUESTED, target == "thread")
    consumed = None
    if not failure:
        SEND["controls"](details["clicked"], target, expected_viewport(fixture), True)
        consumed = consumed_inventory(typed, snapshots["admitted"]["durable"][target])
    newer_broadcast = target == "thread" and scenario == "expired-clear-thread"
    newer = changed(typed if failure else consumed, NEWER, newer_broadcast)
    for record in records:
        keys(record, ("stage", "geometry", "durable", "projection"), "stage snapshot")
        stage = record["stage"]
        expected_inventory = copy.deepcopy(initial)
        if failure:
            expected_inventory[target] = newer if stage == "newer" else typed if stage == "retried" else initial[target]
        elif stage == "input": expected_inventory[target] = typed
        elif stage in ("newer",) or stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel"): expected_inventory[target] = newer
        elif stage != "initial": expected_inventory[target] = consumed
        same(record["durable"], expected_inventory, "Full durable inventory changed at " + stage)
        text = INITIAL if stage == "initial" else NEWER if stage == "newer" or stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel") else "" if stage == "settled" else REQUESTED
        geometry(record["geometry"], fixture, target, text, locked=stage == "admitted", proof=stage != "admitted")
        validate_projection(record, initial, scenario, target)
    same(receipt["finalInventory"], records[-1]["durable"], "Terminal durable inventory drifted")
    same(receipt["finalProjection"], records[-1]["projection"], "Terminal shell projection drifted")
    validate_native_history(snapshots, details, scenario, target)
    validate_operations(snapshots, scenario, target)
    if failure:
        validate_retention(snapshots, details, target)


def validate_closed_storage(storage, receipt):
    module = runpy.run_path(str(Path(__file__).with_name("protected_drafts_storage.py")))
    return module["validate"](storage, receipt, globals())


def keys(value, fields, label):
    require(isinstance(value, dict) and set(value) == set(fields), "Wrong exact fields: " + label)


def rect_inside(value, parent, viewport):
    require(rectangle(value, viewport) and rectangle(parent, viewport), "Clipped or invalid rectangle")
    require(value["x"] >= parent["x"] - 1 and value["y"] >= parent["y"] - 1
            and value["x"] + value["width"] <= parent["x"] + parent["width"] + 1
            and value["y"] + value["height"] <= parent["y"] + parent["height"] + 1,
            "Child exceeds containing surface")


def hit_grid(value, rect):
    keys(value, ("method", "ok", "points"), "hit grid")
    same(value["method"], "rect-grid", "Unexpected hit sampler")
    same(value["ok"], True, "Obscured surface")
    require(isinstance(value["points"], list) and len(value["points"]) == 9, "Incomplete hit grid")
    for point, (i, j, dx, dy) in zip(value["points"],
            ((i, j, dx, dy) for i, dx in enumerate((.05, .5, .95)) for j, dy in enumerate((.05, .5, .95)))):
        keys(point, ("name", "x", "y", "owned", "element"), "hit point")
        same(point["name"], f"{i}:{j}", "Wrong hit order")
        for axis, fraction, size in (("x", dx, "width"), ("y", dy, "height")):
            require(number(point[axis]) and abs(point[axis] - rect[axis] - fraction * rect[size]) < .000001,
                    "Wrong actual sampled coordinate")
        same(point["owned"], True, "Hit belongs to another element")
        keys(point["element"], ("tag", "id", "classes"), "hit element")
        require(all(isinstance(v, str) for v in point["element"].values()) and point["element"]["tag"],
                "Missing hit diagnostic")


def geometry(value, fixture, target, text, *, locked=False, focused=True, proof=False, expected_composing=False):
    viewport = expected_viewport(fixture)
    keys(value, ("ok", "target", "targetMatches", "field", "composer", "pane", "header", "hitSamples",
                 "viewport", "documentWidth", "native", "proof", "readOnly", "disabled", "admission",
                 "fieldFocused", "activeId", "scroll"), "composer geometry")
    for name, expected in (("ok", True), ("target", target), ("targetMatches", True), ("viewport", viewport),
                           ("readOnly", locked), ("disabled", False), ("fieldFocused", focused)):
        same(value[name], expected, "Wrong composer geometry: " + name)
    require(number(value["documentWidth"]) and 0 < value["documentWidth"] <= viewport[0] + 1,
            "Document overflows viewport")
    if locked:
        require(isinstance(value["admission"], str) and value["admission"].isdigit()
                and int(value["admission"]) > 0, "Missing actual admission token")
    else:
        same(value["admission"], None, "Admission was not released")
    if focused:
        same(value["activeId"], target + "-composer", "Focus moved from actual field")
    rect_inside(value["composer"], value["pane"], viewport)
    rect_inside(value["field"], value["composer"], viewport)
    rect_inside(value["header"], value["pane"], viewport)
    require(value["composer"]["y"] >= value["header"]["y"] + value["header"]["height"] - 1,
            "Composer overlaps conversation header")
    keys(value["hitSamples"], ("field", "composer", "header"), "sample surfaces")
    for label in value["hitSamples"]:
        hit_grid(value["hitSamples"][label], value[label])
    native(value["native"], text, viewport, expected_composing=expected_composing)
    r = value["native"]["rect"]
    same(value["field"], dict(x=r["left"], y=r["top"], width=r["width"], height=r["height"]),
         "Native field and geometry disagree")
    keys(value["scroll"], ("document", "pane", "composer"), "scroll observation")
    require(all(number(v) and v >= 0 for v in value["scroll"].values()), "Invalid scroll observation")
    same(value["scroll"]["document"], 0, "Ancestor document was scrolled")
    if proof:
        projection(value["proof"], value["native"], target)
    else:
        same(value["proof"], None, "Unexpected projection ticket capture")


def projection(value, dom, target):
    keys(value, ("status", "token", "current"), "actual shared projection proof")
    same(value["status"], "captured", "Projection was not captured")
    require(isinstance(value["token"], str) and value["token"].isdigit() and int(value["token"]) > 0,
            "Missing actual projection token")
    s = value["current"]
    keys(s, ("id", "scope", "visit", "input", "composition", "composing", "wire", "start", "end",
             "direction", "display_start", "display_end", "display_len"), "shared native snapshot")
    for label in ("input", "composition"):
        require(type(s[label]) is int and 0 <= s[label] <= 9007199254740991, "Invalid native generation")
    for label, expected in (("id", target + "-composer"), ("scope", dom["scope"]), ("visit", dom["visit"]),
                            ("composing", False), ("wire", dom["text"]), ("start", dom["start"]),
                            ("end", dom["end"]), ("direction", dom["direction"]),
                            ("display_start", dom["start"]), ("display_end", dom["end"]),
                            ("display_len", dom["utf16Length"])):
        same(s[label], expected, "Projection and actual DOM differ: " + label)


def same_native(before, after):
    for key in ("text", "display", "codePoints", "utf16Length", "scope", "visit", "start", "end", "direction"):
        same(after[key], before[key], "Native ownership/text/selection changed: " + key)


ACCOUNT = "00000000-0000-4000-8000-000000000191"
OTHER_ACCOUNT = "00000000-0000-4000-8000-000000000192"
THREAD = "1719800400.000000"
INITIAL = "Protected local draft"
REQUESTED = "Frozen local intent \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301\nsecond line"
NEWER = "Newer local typing \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"


def scope(target):
    return dict(account_id=ACCOUNT, key=dict(team="T1", channel="C2", thread=THREAD if target == "thread" else None))


def local(text, broadcast=False):
    end = len(text.encode("utf-8"))
    return dict(composer=dict(text=text, selection=dict(start=end, end=end), reply_broadcast=broadcast), attachments=[])


def encoded(text, target, broadcast):
    destination = {"channel_id": "C2"}
    if target == "thread": destination["thread_ts"] = THREAD
    if broadcast: destination["broadcast"] = True
    return dict(blocks=[dict(type="rich_text", elements=[dict(type="rich_text_section", elements=[dict(type="text", text=text)])])],
                file_ids=[], destinations=[destination])


def seed_inventory(value, target, failure):
    row = value.get("row", {})
    state, lineage = row.get("state", {}), row.get("lineage", {})
    require(uuid4(state.get("client_msg_id")) and uuid4(lineage.get("enrollment")), "Missing minted opaque identities")
    expected_state = dict(scope=scope(target), client_msg_id=state["client_msg_id"], desired=encoded(INITIAL, target, target == "thread"),
        local_revision=1, acknowledged_local_revision=0, acknowledged=None, workflow="Idle", tombstone=False,
        retired_ids=[], consumption=None, check_deadline=None, check_ticket=None, observed_events={}, blocked_observation=None)
    expected_row = dict(scope=scope(target), revision=1 if failure else 2, state=expected_state,
                        lineage=dict(enrollment=lineage["enrollment"], verifiedEpoch=0 if failure else 2, basis="Unverified" if failure else "Local"))
    same(value, dict(scope=scope(target), clock=5, row=expected_row, local=local(INITIAL, target == "thread"),
                     blocked=None, alternates=[], origins=[], uncommitted=None), "Wrong complete isolated seed inventory")


def changed(before, text, broadcast):
    after = copy.deepcopy(before)
    target = "thread" if before["scope"]["key"]["thread"] else "channel"
    after["clock"] += 2
    after["local"] = local(text, broadcast)
    after["row"]["revision"] += 1
    after["row"]["state"]["local_revision"] += 1
    after["row"]["state"]["desired"] = encoded(text, target, broadcast)
    after["row"]["state"]["tombstone"] = False
    return after


def consumed_inventory(before, actual):
    intent = actual.get("row", {}).get("state", {}).get("consumption", {})
    require(isinstance(intent, dict) and uuid4(intent.get("id")), "Missing actual consumed intent")
    state = before["row"]["state"]
    expected_intent = dict(id=intent["id"], scope=before["scope"], client_msg_id=state["client_msg_id"],
        local_revision=state["local_revision"], kind="Send", snapshot=state["desired"], base=None)
    after = copy.deepcopy(before)
    after["clock"] += 2
    after["local"] = local("")
    after["row"]["revision"] += 1
    after["row"]["state"].update(local_revision=state["local_revision"] + 1, desired=None, consumption=expected_intent)
    same(actual, after, "Typed consume changed frozen content, workflow, or unrelated inventory")
    return after


def review(inventory):
    return {key: inventory[key] for key in ("row", "local", "blocked", "alternates", "origins", "uncommitted")}


def retention_id(value):
    # Account-owned editor slots use the process-local checked AtomicU64
    # barrier counter; durable LocalWriter IDs use a separate UUID constructor.
    return (isinstance(value, str) and re.fullmatch(r"protected-[1-9][0-9]{0,19}", value) is not None
            and int(value[len("protected-"):]) < (1 << 64) - 1)


def validate_projection(record, initial, scenario, target):
    p, stage = record["projection"], record["stage"]
    keys(p, ("account", "epoch", "team", "channel", "thread", "viewId", "sessionPresent", "transportPresent", "cachePresent",
             "isolated", "channelGeneration", "threadGeneration", "fieldScopes", "channelComposer", "threadComposer", "book", "editors", "routed", "refused", "inflight", "fixtureActions", "nativeAdmission", "result", "operations"), "shell projection")
    failure = scenario == "failed-write-return-channel"
    for name, expected in dict(account=OTHER_ACCOUNT if stage == "away" else ACCOUNT, team="T1", channel="C2",
        thread=THREAD if target == "thread" else None, sessionPresent=False, transportPresent=False, cachePresent=False,
        isolated=True, refused=0).items():
        same(p[name], expected, "Wrong isolated shell state: " + name)
    for name in ("epoch", "viewId", "channelGeneration", "threadGeneration"):
        require(type(p[name]) is int and p[name] >= 0, "Invalid shell ownership counter")
    keys(p["fieldScopes"], ("channel", "thread"), "Rust field scopes")
    for name in ("channel", "thread"):
        thread = 'Some("' + THREAD + '")' if name == "thread" else "None"
        destination = "None" if name == "thread" and p["thread"] is None else f'Some(DraftKey {{ team: "T1", channel: "C2", thread: {thread} }})'
        expected_scope = f'{p["viewId"]}:{p["epoch"]}:Some("{p["account"]}"):{destination}:{p[name + "Generation"]}'
        same(p["fieldScopes"][name], expected_scope, "Wrong Rust owner/destination scope")
    same(record["geometry"]["native"]["scope"], p["fieldScopes"][target], "Native field belongs to different Rust owner")
    routed = 0 if stage == "initial" else 2 if stage == "newer" or stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel") else 1
    same(p["routed"], routed, "Input routing count changed")
    held = stage == "admitted" or scenario in ("expired-clear-thread", "canceled-clear-channel") and stage in ("expired", "canceled", "newer")
    same(p["inflight"], held, "Wrong action waiter lifetime")
    keys(p["book"], ("channel", "thread"), "composer book")
    keys(p["editors"], ("channel", "thread"), "protected editors")
    for name in ("channel", "thread"):
        shell_local = initial[name]["local"]
        if name == target and stage != "initial":
            text = record["geometry"]["native"]["text"]
            broadcast = name == "thread" and (scenario == "expired-clear-thread" or stage not in ("settled", "newer"))
            shell_local = local(text, broadcast)
        same(p[name + "Composer"], local("")["composer"] if name == "thread" and p["thread"] is None else shell_local["composer"], "Shell composer changed")
        same(p["book"][name], shell_local if shell_local["composer"]["text"] else None, "Book projection differs")
        editor = p["editors"][name]
        if stage == "away":
            same(editor, None, "Registry retained editor while away")
            continue
        keys(editor, ("retentionId", "writerId", "enrollment", "serial", "review", "uncommitted", "pending", "running", "parked", "error", "failedInputs"), "Editor snapshot")
        require(retention_id(editor["retentionId"]) and uuid4(editor["writerId"]), "Missing retained writer identities")
        same(editor["enrollment"], initial[name]["row"]["lineage"]["enrollment"], "Writer enrollment changed")
        serial = routed if name == target else 0
        same(editor["serial"], serial, "Writer serial changed")
        failed = failure and name == target and stage in ("input", "failed", "returned")
        same(editor["review"], review(record["durable"][name]), "Editor view and actual durable inventory diverge")
        for key, expected in dict(uncommitted=local(REQUESTED) if failed else None, pending=local(REQUESTED) if failed else None,
            running=False, parked=False, error="Storage" if failed else None, failedInputs=[local(REQUESTED)] if failed else []).items():
            same(editor[key], expected, "Wrong retained Editor state: " + key)


def validate_native_history(snapshots, details, scenario, target):
    initial, typed = snapshots["initial"], snapshots["input"]
    failure = scenario == "failed-write-return-channel"
    for snapshot in snapshots.values():
        stage, dom = snapshot["stage"], snapshot["geometry"]["native"]
        if not failure:
            for key in ("epoch", "viewId", "channelGeneration", "threadGeneration"):
                same(snapshot["projection"][key], initial["projection"][key], "Owner/generation changed during local action")
            for name in ("channel", "thread"):
                for key in ("retentionId", "writerId", "enrollment"):
                    same(snapshot["projection"]["editors"][name][key], initial["projection"]["editors"][name][key],
                         "Local action replaced retained writer")
        if stage not in ("away", "returned", "retried") and (not failure or stage != "newer"):
            same(dom["scope"], initial["geometry"]["native"]["scope"], "Native destination scope changed")
            same(dom["visit"], initial["geometry"]["native"]["visit"], "Native field was replaced")
        if stage in ("input", "admitted", "expired", "canceled", "failed", "newer") or stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel"):
            text = dom["text"]
            same(dom["start"], len(text[:text.index("\U0001f680")].encode("utf-16-le")) // 2, "Native selection start changed")
            same(dom["end"], len(text.encode("utf-16-le")) // 2, "Native selection end changed")
            same(dom["direction"], "backward", "Native selection direction changed")
        if stage != "admitted":
            expected_input = (0 if stage in ("initial", "away", "returned", "retried") else 1 if failure or stage != "newer" and not (stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel")) else 2)
            same(snapshot["geometry"]["proof"]["current"]["input"], expected_input, "Actual native input generation changed")
            same(snapshot["geometry"]["proof"]["current"]["composition"], 0, "Unexpected composition generation")
    captured = typed["geometry"]["proof"]["current"]
    if not failure:
        held = snapshots["admitted"]["projection"]["nativeAdmission"]
        require(isinstance(held, list) and len(held) == 2, "Admission did not run capture then lock")
        same(held[0], dict(kind="capture", evidence=dict(before=captured)), "Actual admission captured different predecessor")
        keys(held[1], ("kind", "evidence"), "actual lock event")
        token = held[1]["evidence"].get("token")
        require(isinstance(token, str) and token.isdigit() and int(token) > 0, "Missing actual lock token")
        same(held[1], dict(kind="lock", evidence=dict(token=token, before=captured)), "Actual lock differed from captured field")
        same(snapshots["admitted"]["geometry"]["admission"], token, "Held DOM admission token differs")
        final_history = copy.deepcopy(held)
        if scenario.startswith("consume-"):
            after = copy.deepcopy(captured)
            after.update(wire="", start=0, end=0, direction="forward", display_start=0, display_end=0, display_len=0)
            final_history.append(dict(kind="clear-ack", evidence=dict(token=token, before=captured, after=after)))
            same(snapshots["settled"]["geometry"]["proof"]["current"], after, "Cleared native field differs from actual ACK")
        final_history.append(dict(kind="release-requested", evidence=dict(token=token)))
        for stage, snapshot in snapshots.items():
            history = [] if stage in ("initial", "input") else final_history if stage == "settled" or stage == "newer" and scenario.startswith("consume-") else held
            same(snapshot["projection"]["nativeAdmission"], history, "Unexpected actual admission event order at " + stage)
        operation_id = snapshots["admitted"]["projection"]["operations"]["Ok"][0]["id"]
        same(details["result"], dict(kind="completed", operationId=operation_id, operationStatus="ConsumedAwaitingDelivery",
             nativeClear="Cleared" if scenario.startswith("consume-") else "Deferred", before=review(typed["durable"][target]),
             after=review(snapshots["admitted"]["durable"][target]), serial=1), "Wrong actual action result")
        if scenario.startswith("canceled-"):
            same(details["canceledWaiter"], {"kind":"canceled"}, "Missing actual waiter cancellation")

    for stage, snapshot in snapshots.items():
        p = snapshot["projection"]
        if failure:
            same(p["nativeAdmission"], [], "Failed write entered action admission")
            same(p["result"], None, "Failed write fabricated consume result")
        else:
            expected_result = None if stage in ("initial", "input", "admitted", "expired") or stage == "newer" and scenario == "expired-clear-thread" else {"kind":"canceled"} if scenario.startswith("canceled-") and stage in ("canceled", "newer") else details["result"]
            same(p["result"], expected_result, "Action result published at wrong stage")
        expected_actions = []
        if stage != "initial":
            expected_actions.append(dict(kind="input", key=scope(target)["key"], serial=1, local=local(REQUESTED, target == "thread")))
        if not failure and stage not in ("initial", "input"):
            expected_actions.append(dict(kind="consumed", before=review(typed["durable"][target]), after=review(snapshots["admitted"]["durable"][target]), serial=1))
        if failure:
            for edge in ("away", "returned", "retry"):
                threshold = {"away": 3, "returned": 4, "retry": 5}[edge]
                if list(snapshots).index(stage) >= threshold: expected_actions.append(details[edge])
        elif scenario.startswith("canceled-") and stage in ("canceled", "newer", "settled"):
            expected_actions.append(dict(kind="waiter-canceled"))
        if stage == "newer" or stage == "settled" and scenario in ("expired-clear-thread", "canceled-clear-channel"):
            expected_actions.append(dict(kind="input", key=scope(target)["key"], serial=2,
                                         local=local(NEWER, target == "thread" and scenario == "expired-clear-thread")))
        same(p["fixtureActions"], expected_actions, "Fixture action ledger changed at " + stage)


def validate_operations(snapshots, scenario, target):
    failure = scenario.startswith("failed-")
    operation_id = None
    for stage, snapshot in snapshots.items():
        value = snapshot["projection"]["operations"]
        keys(value, ("Ok",), "retained operation inventory")
        if failure or stage in ("initial", "input"):
            same(value["Ok"], [], "Operation appeared before action admission")
            continue
        require(isinstance(value["Ok"], list) and len(value["Ok"]) == 1, "Actual whole operation missing")
        operation = value["Ok"][0]
        current_id = operation.get("id")
        require(type(current_id) is int and 0 < current_id <= 2**64-1, "Invalid whole-operation identity")
        if operation_id is None: operation_id = current_id
        same(current_id, operation_id, "Whole operation replaced across waiter cancellation")
        complete = stage == "settled" or stage == "newer" and scenario.startswith("consume-")
        expected = dict(id=operation_id, scope=scope(target), epoch=snapshots["initial"]["projection"]["epoch"], serial=1,
            request="Send", local=local(REQUESTED, target == "thread"), book=local(REQUESTED, target == "thread"),
            phase="NativeClear", status="ConsumedAwaitingDelivery" if complete else "Running",
            native=snapshots["input"]["geometry"]["proof"]["current"],
            review=review(snapshots["input"]["durable"][target]),
            consumed=dict(before=review(snapshots["input"]["durable"][target]), after=review(snapshots["admitted"]["durable"][target]),
                serial=1, nativeClear=("Cleared" if scenario.startswith("consume-") else "Deferred") if complete else "NotAttempted"),
            preservation=None, httpPresent=False, receiptPresent=False, live=True)
        same(operation, expected, "Wrong exact whole-operation evidence at " + stage)


def validate_retention(snapshots, details, target):
    initial = snapshots["initial"]["projection"]
    identity = initial["editors"][target]["retentionId"]
    epoch = initial["epoch"]
    same(details["away"], dict(kind="away", account=OTHER_ACCOUNT, epoch=epoch + 1, retentionId=identity,
        strongCount=2, failedInputs=[local(REQUESTED)], pending=local(REQUESTED)), "Detached failed writer ownership was not proved")
    same(details["returned"], dict(kind="returned", account=ACCOUNT, epoch=epoch + 2, retentionId=identity), "Return replaced retained Editor")
    same(details["retry"], dict(kind="input-retried", retentionId=identity, serialBefore=1, serialAfter=1,
        review=review(snapshots["retried"]["durable"][target]), pending=None, failedInputs=[]), "Retry changed immutable command/serial")
    for stage, snapshot in snapshots.items():
        p = snapshot["projection"]
        same(p["epoch"], epoch + (1 if stage == "away" else 2 if stage in ("returned", "retried", "newer") else 0), "Account epoch transition changed")
        same(p["viewId"], initial["viewId"], "Window owner changed")
        if stage != "away":
            for name in ("channel", "thread"):
                for key in ("retentionId", "writerId", "enrollment"):
                    same(p["editors"][name][key], initial["editors"][name][key], "Registry return created replacement writer")
    for first, second in (("failed", "away"), ("away", "returned")):
        before, after = (snapshots[key]["geometry"]["native"] for key in (first, second))
        require(before["scope"] != after["scope"] and before["visit"] != after["visit"], "Account change did not retire actual field mount")
