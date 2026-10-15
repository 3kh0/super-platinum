#!/usr/bin/env python3
"""Isolated known-ID Schedule recovery native acceptance.

Preparation under tmp cannot launch; promote into scripts after integration.
Synthetic HTTP/DOM input only. No production Session/Transport or clipboard.
"""
import argparse
import copy
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
from urllib.parse import urlsplit

# Preparation is isolated under tmp/managed-schedule-recovery-native-prep. No active runner edits.
SCRIPTS = next((parent / "scripts" for parent in Path(__file__).resolve().parents
                if (parent / "Cargo.toml").is_file() and (parent / "scripts/native_fixture.py").is_file()), None)
if SCRIPTS is None:
    raise RuntimeError("Cannot locate repository-owned native helpers")
sys.path.insert(0, str(SCRIPTS))
from native_fixture import NativeFixture, DEFAULT_BINARY, ROOT
from native_check import expected_viewport

UTIL = runpy.run_path(str(SCRIPTS / "check-slack-list-board-add-native.py"))
PREF = runpy.run_path(str(SCRIPTS / "check-read-preferences-native.py"))
sha = UTIL["binary_sha"]
png = UTIL["png_signature"]
validate_exit = UTIL["validate_exit"]
normalize = UTIL["normalize"]
validate_pointer_cleanup = UTIL["validate_pointer_cleanup"]
closed_receipts = UTIL["closed_receipts"]
strict_equal = PREF["strict_equal"]
number = PREF["number"]
rectangle = PREF["rectangle"]
validate_query = PREF["validate_query"]
FIXTURE = "draft-schedule-recovery"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("positive-channel", "positive-thread", "absent-channel", "changed-payload-channel",
             "cooldown-channel", "typing-read-thread", "typing-commit-thread", "owner-rotation-channel",
             "writer-replacement-channel", "queue-reset-channel", "queue-newer-channel", "storage-retry-channel",
             "pending-check-channel", "unmount-thread", "successor-channel", "reenroll-channel")
RESTART_SCENARIOS = ("cooldown-channel", "pending-check-channel")
RACE_SCENARIOS = ("owner-rotation-channel", "writer-replacement-channel", "queue-reset-channel", "queue-newer-channel",
                  "successor-channel", "reenroll-channel", "unmount-thread")
UNCONFIRMED_SCENARIOS = ("absent-channel", "changed-payload-channel", "cooldown-channel", "pending-check-channel")
FLAG = "SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_CHECK"
PREFIX, WAITING, STAGE = "draft-schedule-recovery-check: ", "draft-schedule-recovery-waiting: ", "draft-schedule-recovery-stage: "
UPDATE = runpy.run_path(str(SCRIPTS / "check-draft-update-recovery-native.py"))
require, same, uuid4 = (UPDATE[name] for name in ("require", "same", "uuid4"))
native = UPDATE["native"]
validate_request_url = UPDATE["validate_request_url"]
ACCOUNT = "00000000-0000-4000-8000-000000000111"
THREAD = "1719800400.000000"
POST_AT = 500
SUFFIX = " \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
REQUESTED, CURRENT, NEWER = "Frozen schedule" + SUFFIX, "Retained local draft" + SUFFIX, "Newer local typing" + SUFFIX
SYNC_FIELDS = {"scope", "client_msg_id", "desired", "local_revision", "acknowledged_local_revision", "acknowledged",
               "workflow", "tombstone", "retired_ids", "consumption", "check_deadline", "check_ticket", "observed_events", "blocked_observation"}


def keys(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), "Wrong exact fields: " + label)


def local_shape(value):
    keys(value, {"composer", "attachments"}, "canonical LocalDraft")
    same(value["attachments"], [], "Unexpected Schedule attachment")
    composer = value["composer"]
    keys(composer, {"text", "selection", "reply_broadcast"}, "canonical composer")
    require(isinstance(composer["text"], str) and type(composer["reply_broadcast"]) is bool, "Invalid canonical composer")
    keys(composer["selection"], {"start", "end"}, "canonical byte selection")
    boundaries, offset = {0}, 0
    for character in composer["text"]:
        offset += len(character.encode("utf-8"))
        boundaries.add(offset)
    for position in composer["selection"].values():
        require(type(position) is int and position in boundaries, "Invalid UTF-8 byte boundary in canonical selection")


def inventory_shape(value, completion=False):
    keys(value, {"scope", "canonical", "stored", "blocked", "alternates"}, "complete inventory")
    local_shape(value["canonical"])
    same(value["blocked"], None, "Unexpected blocked canonical copy")
    same(value["alternates"], [], "Unexpected alternate canonical copies")
    stored = value["stored"]
    keys(stored, {"revision", "state"}, "Stored")
    require(type(stored["revision"]) is int and stored["revision"] > 0, "Invalid durable CAS revision")
    state = stored["state"]
    keys(state, SYNC_FIELDS | ({"local_completion"} if completion else set()), "DraftSync")
    same(state["scope"], value["scope"], "Stored/inventory scope differs")
    require(uuid4(state["client_msg_id"]), "Invalid managed client UUID")
    for field in ("local_revision", "acknowledged_local_revision"):
        require(type(state[field]) is int and state[field] >= 0, "Invalid revision: " + field)
    same(state["tombstone"], False, "Unexpected tombstone")
    same(state["observed_events"], {}, "Unexpected observation authority")
    same(state["blocked_observation"], None, "Unexpected blocked observation")
    return state


def scope(target):
    return {"account_id": ACCOUNT, "key": {"team": "T1", "channel": "C2", "thread": THREAD if target == "thread" else None}}


def client(target):
    return "00000000-0000-4000-8000-000000000113" if target == "thread" else "00000000-0000-4000-8000-000000000112"


def snapshot(text, target, rich=False):
    destination = {"channel_id": "C2"}
    if target == "thread":
        destination.update(thread_ts=THREAD, broadcast=True)
    run = {"type": "text", "text": text}
    if rich:
        run["style"] = {"bold": True}
    elements = [{"type": "rich_text_section", "elements": [run]}]
    if rich:
        elements.append({"type": "rich_text_list", "style": "bullet", "elements": [
            {"type": "rich_text_section", "elements": [{"type": "text", "text": "Frozen checklist item", "style": {"italic": True}}]}]})
    return {"blocks": [{"type": "rich_text", "elements": elements}], "file_ids": [], "destinations": [destination]}


def base(target):
    return dict(scope=scope(target), id="DrScheduleThread" if target == "thread" else "DrScheduleChannel",
                client_msg_id=client(target), last_updated_ts="10.000001", snapshot=snapshot(REQUESTED, target, True))


def operation(value, target):
    require(isinstance(value, dict) and uuid4(value.get("id")) and uuid4(value.get("consume_id")), "Missing frozen Schedule UUIDs")
    expected = dict(id=value["id"], scope=scope(target), client_msg_id=client(target), local_revision=0,
                    kind={"Schedule": {"post_at": POST_AT}}, snapshot=snapshot(REQUESTED, target, True),
                    base=base(target), consume_id=value["consume_id"])
    same(value, expected, "Wrong exact frozen Schedule operation")
    return value


def consumption(op):
    return dict(id=op["consume_id"], **{key: copy.deepcopy(op[key]) for key in
        ("scope", "client_msg_id", "local_revision", "kind", "snapshot", "base")})


def local(text, target):
    end = len(text.encode("utf-8"))
    return dict(composer=dict(text=text, selection=dict(start=end, end=end), reply_broadcast=target == "thread"), attachments=[])


def initial_inventory(target, op):
    operation(op, target)
    state = dict(scope=scope(target), client_msg_id=client(target), desired=snapshot(CURRENT, target),
        local_revision=2, acknowledged_local_revision=0, acknowledged=base(target), workflow={"Uncertain": op},
        tombstone=False, retired_ids=[], consumption=consumption(op), check_deadline=None, check_ticket=None,
        observed_events={}, blocked_observation=None)
    return dict(scope=scope(target), canonical=local(CURRENT, target), stored=dict(revision=1, state=state), blocked=None, alternates=[])


def withheld_inventory(op, index):
    require(isinstance(op, dict) and uuid4(op.get("id")) and uuid4(op.get("consume_id")), "Missing withheld operation identity")
    destination_scope = dict(account_id=ACCOUNT, key=dict(team="T1", channel="C1" if index < 2 else "C2",
        thread=None if index == 0 else THREAD if index == 1 else "1719800300.000000"))
    target = "channel" if index == 0 else "thread"
    frozen = snapshot(("Withheld unknown ID schedule", "Withheld file schedule", "Withheld extra routing schedule")[index], target, True)
    destination = frozen["destinations"][0]
    destination["channel_id"] = destination_scope["key"]["channel"]
    if index:
        destination["thread_ts"] = destination_scope["key"]["thread"]
    if index == 1:
        frozen["file_ids"] = ["FWithheld"]
    elif index == 2:
        destination["unproven_route"] = "retained"
    identity = op.get("client_msg_id") if index == 0 else f"00000000-0000-4000-8000-{114+index:012}"
    require(uuid4(identity), "Invalid withheld client UUID")
    remote = None if index == 0 else dict(scope=destination_scope, id="DrScheduleFile" if index == 1 else "DrScheduleRouting",
        client_msg_id=identity, last_updated_ts="10.000001", snapshot=frozen)
    expected_op = dict(id=op["id"], scope=destination_scope, client_msg_id=identity, local_revision=1 if index == 0 else 0,
        kind={"Schedule": {"post_at": POST_AT}}, snapshot=frozen, base=remote, consume_id=op["consume_id"])
    same(op, expected_op, "Unsupported Schedule payload/routing changed")
    desired = snapshot(CURRENT, target)
    desired["destinations"][0]["channel_id"] = destination_scope["key"]["channel"]
    if index:
        desired["destinations"][0]["thread_ts"] = destination_scope["key"]["thread"]
    state = dict(scope=destination_scope, client_msg_id=identity, desired=desired, local_revision=3 if index == 0 else 2,
        acknowledged_local_revision=0, acknowledged=remote, workflow={"Uncertain": op}, tombstone=False,
        retired_ids=[], consumption=consumption(op), check_deadline=None, check_ticket=None, observed_events={}, blocked_observation=None)
    return dict(scope=destination_scope, canonical=local(CURRENT, target), stored=dict(revision=1,state=state), blocked=None, alternates=[])


def raw_response(op, scenario):
    if scenario == "cooldown-channel":
        return None
    if scenario == "absent-channel":
        return {"ok": True, "draft": None}
    draft = dict(id=op["base"]["id"], client_msg_id=op["client_msg_id"], last_updated_ts="12.000001",
        team_id="T1", user_id="U00000001", is_deleted=False, is_sent=False, date_created=100,
        date_scheduled=POST_AT, is_from_composer=True, **copy.deepcopy(op["snapshot"]))
    if scenario == "changed-payload-channel":
        draft["blocks"][0]["elements"][0]["elements"][0]["text"] = "Changed remote payload"
    return {"ok": True, "draft": draft}

# Staged code cannot launch; root promotes only after review and compilation.
CONTRACT_READY = Path(__file__).resolve().parent == SCRIPTS


def require_contract():
    require(CONTRACT_READY, "Schedule recovery preparation cannot launch before promotion into scripts")


def phases(scenario):
    require(scenario in SCENARIOS, "Unknown Schedule recovery scenario")
    return ("exercise", "restart") if scenario in RESTART_SCENARIOS else ("exercise",)


def stages(scenario, phase):
    require(phase in phases(scenario), "Unexpected Schedule recovery phase")
    if phase == "restart":
        return ("restart",)
    extra = (("input",) if scenario.startswith("typing-") else ("interleaved",) if scenario in RACE_SCENARIOS
             else ("retry",) if scenario == "storage-retry-channel" else ())
    withheld = ("withheld-0", "withheld-1", "withheld-2") if scenario == "positive-channel" else ()
    return ("initial",) + withheld + ("checking",) + extra + ("result", "scheduled")


def read_attempt(value, op, scenario):
    pending = scenario == "pending-check-channel"
    fields = {"sequence", "method", "url", "fields", "retrySafe", "operation", "admitted", "nowMs", "delivered", "rawResponse", "retryAfterSeconds"}
    keys(value, fields | (set() if pending else {"result"}), "actual Schedule read ledger")
    validate_request_url(value["url"])
    for key, expected in dict(sequence=1, method="POST", fields=[["draft_id", op["base"]["id"]]], retrySafe=False,
            operation=op, delivered=not pending, retryAfterSeconds=120 if scenario == "cooldown-channel" else None).items():
        same(value[key], expected, "Wrong Schedule prepared request: " + key)
    require(type(value["nowMs"]) is int and value["nowMs"] > 0, "Missing actual read start time")
    raw = None if pending else raw_response(op, scenario)
    same(value["rawResponse"], raw, "Wrong delivered Schedule response")
    if not pending:
        result = value["result"]
        keys(result, {"requested", "confirmed", "cooldownUntilMs", "issue", "hasMore", "raw"}, "ScheduleRead result")
        for key, expected in dict(requested=True, confirmed=scenario not in UNCONFIRMED_SCENARIOS,
                issue="RateLimited" if scenario == "cooldown-channel" else "UnsupportedResponse" if scenario in UNCONFIRMED_SCENARIOS else None,
                hasMore=False, raw=raw).items():
            same(result[key], expected, "Wrong actual ScheduleRead decode: " + key)
        if scenario == "cooldown-channel":
            require(type(result["cooldownUntilMs"]) is int and result["cooldownUntilMs"] >= value["nowMs"] + 120000,
                    "Lost received-time Retry-After floor")
        else:
            same(result["cooldownUntilMs"], None, "Invented server cooldown")
    return value


def held_inventory(initial, actual, op, now_ms):
    state = inventory_shape(actual)
    ticket, deadline = state["check_ticket"], state["check_deadline"]
    keys(ticket, {"id", "operation"}, "durable ticket")
    require(uuid4(ticket["id"]), "Missing durable reservation UUID")
    same(ticket["operation"], op, "Ticket lost frozen Schedule payload")
    keys(deadline, {"operation_id", "until_ms", "operation"}, "durable admission deadline")
    same(deadline["operation_id"], op["id"], "Deadline belongs to wrong operation")
    same(deadline["operation"], op, "Deadline lost complete frozen Schedule")
    require(type(deadline["until_ms"]) is int and now_ms < deadline["until_ms"] <= now_ms + 30000,
            "Wrong durable 30-second admission floor")
    expected = copy.deepcopy(initial)
    expected["stored"]["revision"] += 1
    expected["stored"]["state"].update(check_ticket=ticket, check_deadline=deadline)
    same(actual, expected, "Admission changed more than exact ticket/deadline/CAS")
    return expected


def completed_inventory(held, actual, op, scenario):
    confirmed = scenario not in UNCONFIRMED_SCENARIOS
    inventory_shape(actual, completion=confirmed)
    expected = copy.deepcopy(held)
    expected["stored"]["revision"] += 1
    state = expected["stored"]["state"]
    state["check_ticket"] = None
    if confirmed:
        successor = actual["stored"]["state"]["client_msg_id"]
        require(uuid4(successor) and successor != op["client_msg_id"], "Confirmed Schedule needs fresh managed successor UUID")
        state.update(client_msg_id=successor, acknowledged=None, acknowledged_local_revision=1,
            workflow="Idle", retired_ids=[op["base"]["id"]], consumption=None,
            local_completion=dict(intent=consumption(op), successor_client_msg_id=successor))
    return expected


def bounded(value, names, viewport):
    same(value.get("ok"), True, "Failed DOM evidence")
    same(value.get("viewport"), viewport, "Wrong measured viewport")
    require(number(value.get("documentWidth")) and 0 < value["documentWidth"] <= viewport[0] + 1, "Document overflows")
    for name in names:
        require(rectangle(value.get(name), viewport), "Clipped Schedule rectangle: " + name)


def contained(value, child, parent):
    a, b = value[child], value[parent]
    for axis, extent in (("x", "width"), ("y", "height")):
        require(a[axis] >= b[axis] - 1 and a[axis] + a[extent] <= b[axis] + b[extent] + 1,
                f"Schedule {child} clipped by {parent} on {axis}")


def row(value, op, viewport, disabled=False, clicked=False, needle=None):
    require(isinstance(value, dict), "Missing Schedule recovery row")
    bounded(value, ("row", "button", "panel", "header"), viewport)
    for name, expected in dict(kind="schedule", operation=op["id"], disabled=disabled, clickRequested=clicked,
            clicked=clicked, hit=True, threadPaneAbsent=True, rowUnobscured=True, buttonUnobscured=True, headerUnobscured=True).items():
        same(value.get(name), expected, "Wrong recovery row evidence: " + name)
    same(value.get("buttonText"), "Checking\u2026" if disabled else "Check schedule", "Wrong explicit Schedule action")
    require(isinstance(value.get("text"), str) and (needle is None or needle in value["text"]), "Missing recovery row copy")
    contained(value, "row", "panel")
    contained(value, "button", "panel")


def composer(value, target, viewport):
    require(isinstance(value, dict), "Missing Schedule composer measurement")
    bounded(value, ("field", "composer", "header"), viewport)
    for name, expected in dict(kind="composer", fieldUnobscured=True, composerUnobscured=True, headerUnobscured=True,
            fieldFocused=True, activeId=target+"-composer").items():
        same(value.get(name), expected, "Wrong preserved composer geometry: " + name)
    contained(value, "field", "composer")
    require(value["composer"]["y"] >= value["header"]["y"] + value["header"]["height"] - 1, "Composer covers header")
    for name in ("scope", "visit"):
        require(isinstance(value.get(name), str) and value[name], "Missing native composer ownership: " + name)


def queue_row(op, newer=False):
    raw = raw_response(op, "positive-channel")["draft"]
    if newer:
        raw["last_updated_ts"] = "13.000001"
        raw["blocks"][0]["elements"][0]["elements"][0]["text"] = "Newer queued schedule"
    return {key: raw[key] for key in ("id", "date_scheduled", "date_created", "client_msg_id", "blocks", "file_ids",
                                     "destinations", "is_deleted", "is_sent", "last_updated_ts")}


def view(inventory):
    return dict(local=inventory["canonical"], stored=inventory["stored"], blocked=None, uncommitted=None)


def projection_shape(value):
    keys(value, {"account", "epoch", "team", "viewId", "queue", "managed", "editors", "composer", "threadComposer", "toast"}, "shell projection")
    same(value["account"], ACCOUNT, "Wrong projection account")
    same(value["team"], "T1", "Wrong projection team")
    require(type(value["epoch"]) is int and value["epoch"] >= 0 and type(value["viewId"]) is int and value["viewId"] > 0, "Invalid shell ownership")
    q = value["queue"]
    keys(q, {"generation", "matchesCapturedScope", "filter", "items", "loading", "loaded", "hasMore", "limit", "error", "busy"}, "full queue")
    for field in ("generation", "limit"):
        require(type(q[field]) is int and q[field] >= 0, "Invalid queue counter")
    for field in ("matchesCapturedScope", "loading", "loaded", "hasMore"):
        require(type(q[field]) is bool, "Invalid queue flag")
    require(q["filter"] in ("Drafts", "Scheduled") and isinstance(q["items"], dict), "Invalid queue inventory")
    same(q["busy"], [], "Unexpected queued mutation")
    same(q["error"], None, "Unexpected queue error")
    for field in ("composer", "threadComposer"):
        local_shape(dict(composer=value[field], attachments=[]))
    for field in ("managed", "editors"):
        keys(value[field], {"channel", "thread"}, "full " + field)
    for target in ("channel", "thread"):
        managed = value["managed"][target]
        keys(managed, {"local", "stored", "blocked", "uncommitted"}, "managed view")
        local_shape(managed["local"])
        same(managed["blocked"], None, "Unexpected blocked managed view")
        same(managed["uncommitted"], None, "Unexpected uncommitted managed view")
        editor = value["editors"][target]
        if editor is not None:
            keys(editor, {"sameCaptured", "serial", "view", "pending", "running", "unsaved"}, "Editor snapshot")
            require(type(editor["sameCaptured"]) is bool and type(editor["serial"]) is int and editor["serial"] >= 0, "Invalid Editor ownership/serial")
            for field, expected in (("pending", None), ("running", False), ("unsaved", False)):
                same(editor[field], expected, "Unsettled Editor: " + field)
            keys(editor["view"], {"local", "stored", "blocked", "uncommitted"}, "Editor view")
    require(value["toast"] is None or isinstance(value["toast"], str), "Invalid projection toast")
    return q


def _validate(fixture, scenario, phase, nonce, pid, database, receipt):
    names = stages(scenario, phase)
    require(fixture in FIXTURES and isinstance(receipt, dict), "Unknown fixture/receipt")
    target = "thread" if scenario.endswith("thread") else "channel"
    other = "channel" if target == "thread" else "thread"
    count = int(phase == "exercise")
    for field, expected in dict(ok=True, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce, pid=pid,
            target=target, isolatedDatabase=str(database), noSession=True, noTransport=True, noCache=True,
            noProductionRoute=True, syntheticInput=True, actualPreparedRequests=True, canonicalMessagesUnchanged=True,
            preparedRequests=count, refusedInputs=0, viewport=expected_viewport(fixture)).items():
        same(receipt.get(field), expected, "Wrong terminal Schedule field: " + field)
    require(type(receipt.get("routedInputs")) is int and receipt["routedInputs"] >= int(phase == "exercise" and scenario.startswith("typing-")), "Missing routed input proof")
    initial, final, details = receipt.get("initial"), receipt.get("durable"), receipt.get("details")
    for value in (initial, final):
        keys(value, {"channel", "thread", "withheld"}, "all durable destinations")
    require(isinstance(details, dict), "Missing detailed transition evidence")
    op = operation(receipt.get("operation"), target)
    withheld = receipt.get("withheld")
    require(isinstance(withheld, list) and len(withheld) == (3 if scenario == "positive-channel" else 0), "Missing unsupported Schedule fixtures")
    same(initial["withheld"], [withheld_inventory(item, i) for i, item in enumerate(withheld)], "Wrong withheld durable state")
    same(final["withheld"], initial["withheld"], "Read changed unsupported schedules")
    sibling_op = initial[other]["stored"]["state"]["workflow"].get("Uncertain")
    same(initial[other], initial_inventory(other, sibling_op), "Wrong untouched sibling seed")
    same(final[other], initial[other], "Read changed sibling destination")
    seed = initial_inventory(target, op)
    attempts = receipt.get("attempts")
    require(isinstance(attempts, list) and len(attempts) == count, "Not exactly one read / zero restart requests")
    if phase == "restart":
        state = inventory_shape(initial[target])
        expected = copy.deepcopy(seed)
        expected["stored"]["revision"] = 2 if scenario == "pending-check-channel" else 3
        ticket, deadline = state["check_ticket"], state["check_deadline"]
        keys(deadline, {"operation_id", "operation", "until_ms"}, "retained restart deadline")
        same(deadline["operation_id"], op["id"], "Restart deadline identity changed")
        same(deadline["operation"], op, "Restart deadline payload changed")
        require(type(deadline["until_ms"]) is int and deadline["until_ms"] > 0, "Missing restart deadline")
        if scenario == "pending-check-channel":
            keys(ticket, {"id", "operation"}, "retained unresolved ticket")
            require(uuid4(ticket["id"]), "Missing retained ticket UUID")
            same(ticket["operation"], op, "Restart ticket payload changed")
        else:
            same(ticket, None, "Cooldown restart invented ticket")
        expected["stored"]["state"].update(check_ticket=ticket, check_deadline=deadline)
        same(initial[target], expected, "Restart changed retained Schedule state")
        same(final, initial, "Explicit restart refusal changed durable inventory")
    else:
        same(initial[target], seed, "Wrong target Schedule seed")
        request = read_attempt(attempts[0], op, scenario)
        admitted = request["admitted"]
        keys(admitted, set(initial), "request-time admission inventory")
        admitted_target = held_inventory(seed, admitted[target], op, request["nowMs"])
        expected_admitted = copy.deepcopy(initial); expected_admitted[target] = admitted_target
        same(admitted, expected_admitted, "Request admission changed another destination")
        lineage = scenario in ("successor-channel", "reenroll-channel")
        if lineage:
            held_target = completed_inventory(admitted_target, details["held"][target], op, scenario)
        else:
            held_target = admitted_target
        expected_held = copy.deepcopy(initial); expected_held[target] = held_target
        same(details.get("held"), expected_held, "Wrong actual gate inventory")
        before_completion = copy.deepcopy(admitted_target)
        if scenario.startswith("typing-"):
            before_completion["canonical"] = local(NEWER, target)
            before_completion["stored"]["revision"] += 1
            before_completion["stored"]["state"].update(desired=snapshot(NEWER, target), local_revision=3)
        if lineage:
            expected_target = lineage_inventory(details, initial, held_target, target, scenario)
        elif scenario == "pending-check-channel":
            expected_target = before_completion
        else:
            expected_target = completed_inventory(before_completion, final[target], op, scenario)
            if scenario == "cooldown-channel":
                expected_target["stored"]["state"]["check_deadline"]["until_ms"] = request["result"]["cooldownUntilMs"]
        expected_final = copy.deepcopy(initial); expected_final[target] = expected_target
        same(final, expected_final, "Final whole durable state differs from exact transition")
        same(details.get("afterReceipt"), final, "Post-receipt and final durable dictionaries differ")
        if scenario == "storage-retry-channel":
            same(details.get("failedStorage"), admitted, "Failed storage lost exact ticket or changed content")
            same(details["failedStorageProjection"]["queue"]["items"], {}, "Failed storage admitted queued Schedule")
    validate_projection(receipt, target, op)
    validate_dom(receipt, target, op, names)
    return final


def lineage_inventory(details, initial, completed, target, scenario):
    mutation = details.get("mutation")
    keys(mutation, {"scenario", "before", "after", "durableBefore", "durableAfter"}, "lineage mutation")
    same(mutation["scenario"], scenario, "Wrong lineage mutation")
    expected_before = copy.deepcopy(initial); expected_before[target] = completed
    same(mutation["durableBefore"], expected_before, "Lineage mutation preceded positive durable completion")
    actual = mutation["durableAfter"][target]
    inventory_shape(actual)
    expected = copy.deepcopy(completed)
    state = expected["stored"]["state"]
    del state["local_completion"]
    if scenario == "successor-channel":
        intent = actual["stored"]["state"]["consumption"]
        require(isinstance(intent, dict) and uuid4(intent.get("id")), "Missing later intent UUID")
        same(intent, dict(id=intent["id"], scope=scope(target), client_msg_id=state["client_msg_id"], local_revision=2,
            kind={"Schedule": {"post_at": 501}}, snapshot=snapshot(CURRENT, target), base=None), "Wrong exact later Schedule intent")
        require(intent["id"] != details["held"][target]["stored"]["state"]["local_completion"]["intent"]["id"], "Later intent reused old ID")
        expected["stored"]["revision"] = 5
        state.update(local_revision=4, consumption=intent)
    else:
        identity = actual["stored"]["state"]["client_msg_id"]
        require(uuid4(identity) and identity != state["client_msg_id"], "Reenrollment did not create new lineage")
        expected["stored"]["revision"] = 1
        state.update(client_msg_id=identity, local_revision=1, acknowledged_local_revision=0,
                     retired_ids=[], check_deadline=None, check_ticket=None)
    expected_after = copy.deepcopy(initial); expected_after[target] = expected
    same(mutation["durableAfter"], expected_after, "Later lineage durable dictionary differs")
    return expected


def validate_projection(receipt, target, op):
    scenario, phase, details = receipt["scenario"], receipt["phase"], receipt["details"]
    initial, final = receipt["initialProjection"], receipt["finalProjection"]
    projection_shape(initial); projection_shape(final)
    for field in ("messagesInitial", "messagesFinal"):
        messages = receipt.get(field)
        keys(messages, {"channel", "thread", "pending", "pending_delivery"}, "chat message inventory")
        keys(messages["pending"], {"channel", "thread"}, "pending message bags")
        for name in ("channel", "thread"):
            require(messages[name] is None or isinstance(messages[name], list), "Invalid message bag")
            require(isinstance(messages["pending"][name], list) and all(isinstance(ts, str) for ts in messages["pending"][name]), "Invalid pending list")
        require(isinstance(messages["pending_delivery"], dict), "Missing pending delivery map")
    same(receipt["messagesFinal"], receipt["messagesInitial"], "Schedule read mutated chat messages/pending delivery")
    expected_queue = dict(generation=1, matchesCapturedScope=True, filter="Drafts", items={}, loading=False,
        loaded=False, hasMore=False, limit=100, error=None, busy=[])
    same(initial["queue"], expected_queue, "Wrong disconnected initial queue")
    for which in ("channel", "thread"):
        same(initial["managed"][which], view(receipt["initial"][which]), "Initial managed view differs from durable state")
        editor = initial["editors"][which]
        if which == target and scenario in ("successor-channel", "reenroll-channel"):
            same(editor, None, "Lineage case accidentally retained an Editor")
        else:
            require(isinstance(editor, dict), "Missing captured Editor")
            same(editor["sameCaptured"], True, "Initial Editor replaced")
            same(editor["view"], view(receipt["initial"][which]), "Initial Editor differs from durable state")
    if phase == "restart":
        same(final["queue"], initial["queue"], "Refused restart check changed queue")
        same(final["managed"], initial["managed"], "Refused restart changed managed view")
        same(final["editors"], initial["editors"], "Refused restart changed Editor")
        return
    for name in ("heldProjection", "resultProjection", "beforeScheduledClick"):
        projection_shape(details[name])
        same(details[name]["queue"]["filter"], "Drafts", "Recovery automatically switched filter")
    held, result = details["heldProjection"], details["resultProjection"]
    same(held["queue"], initial["queue"], "Read gate changed queue before projection")
    same(held["managed"], initial["managed"], "Read gate prematurely projected managed receipt")
    reference = held
    suppressed = scenario in RACE_SCENARIOS and scenario != "unmount-thread"
    if scenario in RACE_SCENARIOS:
        mutation = details["mutation"]
        keys(mutation, {"scenario", "before", "after", "durableBefore", "durableAfter"}, "actual race mutation")
        same(mutation["scenario"], scenario, "Wrong race mutation")
        for side in ("before", "after"):
            projection_shape(mutation[side])
            same(mutation[side], details["beforeInterleave" if side == "before" else "afterInterleave"], "Race projection snapshot mismatch")
        before, reference = mutation["before"], mutation["after"]
        if scenario not in ("successor-channel", "reenroll-channel"):
            same(mutation["durableBefore"], details["held"], "Race did not begin with exact held durable state")
            same(mutation["durableAfter"], mutation["durableBefore"], "Nonstorage interleave changed durable state")
        expected_mutation = copy.deepcopy(before)
        if scenario == "owner-rotation-channel":
            expected_mutation["epoch"] += 1
        elif scenario == "writer-replacement-channel":
            replacement = reference["editors"][target]
            require(isinstance(replacement, dict), "Missing replacement Editor")
            same(replacement["sameCaptured"], False, "Race did not replace Editor")
            same(replacement["view"], view(details["held"][target]), "Replacement Editor has wrong durable receipt")
            expected_mutation["editors"][target] = replacement
            expected_mutation["managed"][target] = view(details["held"][target])
        elif scenario == "queue-reset-channel":
            expected_mutation["queue"]["generation"] += 2
        elif scenario == "queue-newer-channel":
            expected_mutation["queue"]["generation"] += 1
            expected_mutation["queue"]["items"] = {op["base"]["id"]: queue_row(op, True)}
        elif scenario in ("successor-channel", "reenroll-channel"):
            expected_mutation["managed"][target] = view(mutation["durableAfter"][target])
        same(reference, expected_mutation, "Race changed unexpected projection fields")
    elif scenario.startswith("typing-"):
        reference = details["inputProjection"]
        projection_shape(reference)
    expected_queue = copy.deepcopy(reference["queue"])
    if scenario not in UNCONFIRMED_SCENARIOS and not suppressed:
        expected_queue["items"] = {op["base"]["id"]: queue_row(op)}
        expected_queue["generation"] += 1
    same(result["queue"], expected_queue, "Late Schedule replaced/reset/duplicated queue content")
    if scenario in ("owner-rotation-channel", "writer-replacement-channel", "successor-channel", "reenroll-channel"):
        same(result, reference, "Stale completion changed full current shell projection")
    else:
        expected = copy.deepcopy(reference)
        expected["queue"] = expected_queue
        if scenario != "pending-check-channel":
            expected["managed"][target] = view(receipt["durable"][target])
            expected["editors"][target]["view"] = view(receipt["durable"][target])
        if scenario not in UNCONFIRMED_SCENARIOS:
            expected["toast"] = "Schedule confirmed. Your current text was kept."
        same(result, expected, "Recovery altered current text/selection/writer or projected incorrect receipt")
    before_click = details["beforeScheduledClick"]
    same(before_click["queue"], result["queue"], "Queue changed before explicit tab click")
    expected_queue["filter"] = "Scheduled"
    same(final["queue"], expected_queue, "Final queue changed beyond explicit filter selection")
    same(final["managed"], result["managed"], "Tab switch changed managed durable projection")
    same(final["editors"], result["editors"], "Tab switch changed Editor projection")


def surface(value, mode, viewport, scheduled_count=None):
    require(isinstance(value, dict), "Missing Schedule surface")
    bounded(value, ("row", "panel", "header", "tab"), viewport)
    for field, expected in dict(kind=mode, rowUnobscured=True, headerUnobscured=True, threadPaneAbsent=True,
            clickRequested=mode == "tab", clicked=mode == "tab", checkCount=0,
            selectedBefore="true" if mode == "scheduled" else "false").items():
        same(value.get(field), expected, "Wrong Schedule surface field: " + field)
    require(isinstance(value.get("text"), str) and isinstance(value.get("actions"), list)
            and all(isinstance(a, str) for a in value["actions"]), "Missing visible surface text/actions")
    require(isinstance(value.get("buttonText"), str) and value["buttonText"].strip().startswith("Scheduled"), "Missing real Scheduled tab")
    require(type(value.get("rowCount")) is int and value["rowCount"] >= 0, "Missing actual row count")
    if mode != "tab":
        contained(value, "row", "panel")
    if mode == "local":
        require(CURRENT in value["text"] and "Continue editing locally" in value["actions"], "Lost retained local draft/action")
    if mode == "scheduled":
        same(value["rowCount"], scheduled_count, "Wrong actual Scheduled row count")
    content = "tab" if mode == "tab" else "empty-body" if mode == "scheduled" and scheduled_count == 0 else "row"
    same(value.get("contentKind"), content, "Wrong actual Schedule content surface")
    if content == "empty-body":
        same(value["row"], value["panel"], "Empty-body evidence must measure entire actual body")
        same(value["text"].strip(), "", "Empty-body evidence invented visible content")
        same(value["actions"], [], "Empty body unexpectedly exposed actions")



def home_anchor(value):
    """Validate actual mount/Latest navigation observations; never scroll here."""
    anchor = value.get("homeAnchor")
    keys(anchor, {"landed", "before", "after"}, "home timeline landing")
    same(anchor["landed"], True, "Shared Latest navigation did not land")
    fields = {"expected", "connected", "scope", "windowEnd", "latestMounted", "latestHeight",
              "inputGeneration", "scrollTop", "scrollHeight", "clientHeight", "bottomGap", "ready", "atBottom"}
    for label in ("before", "after"):
        sample = anchor[label]
        keys(sample, fields, "home timeline " + label)
        expected = sample["expected"]
        keys(expected, {"scope", "latestId", "windowEnd"}, "expected timeline identity")
        require(all(isinstance(expected[k], str) and expected[k] for k in ("scope", "latestId")),
                "Missing expected timeline owner/latest row")
        require(type(expected["windowEnd"]) is int and expected["windowEnd"] > 0, "Invalid expected timeline end")
        same(sample["scope"], expected["scope"], "Timeline scope changed")
        same(sample["windowEnd"], expected["windowEnd"], "Timeline virtual window not committed")
        for field in ("connected", "latestMounted", "ready"):
            same(sample[field], True, "Home timeline is not ready: " + field)
        require(type(sample["inputGeneration"]) is int and sample["inputGeneration"] >= 0,
                "Invalid timeline input generation")
        for field in ("latestHeight", "scrollTop", "scrollHeight", "clientHeight", "bottomGap"):
            require(number(sample[field]), "Nonfinite timeline measurement: " + field)
        require(sample["latestHeight"] > 0 and sample["clientHeight"] > 0 and sample["scrollHeight"] > 0,
                "Timeline/latest row has no rendered height")
        require(abs(sample["bottomGap"] - (sample["scrollHeight"] - sample["scrollTop"] - sample["clientHeight"])) <= 1e-6,
                "Inconsistent timeline bottom gap")
        same(sample["atBottom"], -1 <= sample["bottomGap"] <= 2, "False timeline bottom assertion")
    same(anchor["after"]["atBottom"], True, "Home timeline did not reach latest row")
    same(anchor["after"]["expected"], anchor["before"]["expected"], "Timeline identity changed during navigation")
    same(anchor["after"]["inputGeneration"], anchor["before"]["inputGeneration"], "User input interrupted Latest navigation")

def validate_dom(receipt, target, op, names):
    scenario, phase, details, viewport = receipt["scenario"], receipt["phase"], receipt["details"], receipt["viewport"]
    native(details.get("nativeInitial"), CURRENT, viewport)
    home_anchor(details["nativeInitial"])
    geometry = receipt.get("geometry")
    require(isinstance(geometry, list) and len(geometry) == len(names), "Wrong ordered geometry count")
    for value, stage in zip(geometry, names):
        same(value.get("stage"), stage, "Wrong capture stage order")
        if stage in ("initial", "checking", "retry", "restart") or (stage == "result" and scenario in UNCONFIRMED_SCENARIOS):
            needle = ("could not be saved" if stage == "retry" else "unresolved" if stage == "restart" and scenario == "pending-check-channel"
                else "cooldown" if stage in ("result", "restart") and scenario == "cooldown-channel"
                else "still unconfirmed" if stage == "result" and scenario != "pending-check-channel" else None)
            row(value, op, viewport, disabled=stage == "checking" or (stage == "result" and scenario == "pending-check-channel"), needle=needle)
        elif stage in ("input", "interleaved") or (stage == "result" and (scenario.startswith("typing-") or scenario in RACE_SCENARIOS)):
            composer(value, target, viewport)
        else:
            mode = "withheld" if stage.startswith("withheld-") else "local" if stage == "result" else "scheduled"
            count = len(receipt["finalProjection"]["queue"]["items"])
            surface(value, mode, viewport, count)
            if mode == "withheld":
                index = int(stage[-1]); required = ("Withheld unknown ID schedule", "Withheld file schedule", "Withheld extra routing schedule")[index]
                require(required in value["text"] and not any("Check" in action for action in value["actions"]), "Unsupported schedule gained a Check action or lost content")
            if mode == "scheduled" and count:
                require(("Newer queued schedule" if scenario == "queue-newer-channel" else REQUESTED) in value["text"]
                        and "Frozen checklist item" in value["text"], "Queue lost actual frozen rich content")
    if phase == "restart":
        row(details.get("restartClick"), op, viewport, clicked=True)
        row(details.get("restartRefusal"), op, viewport, needle="cooldown" if scenario == "cooldown-channel" else "unresolved")
        return
    row(details.get("actualClick"), op, viewport, clicked=True)
    surface(details.get("scheduledClick"), "tab", viewport)
    if scenario == "storage-retry-channel":
        row(details.get("retryClick"), op, viewport, clicked=True, needle="could not be saved")
    if scenario == "cooldown-channel":
        row(details.get("cooldownClick"), op, viewport, clicked=True)
    if scenario in UNCONFIRMED_SCENARIOS and scenario != "pending-check-channel":
        row(details.get("feedback"), op, viewport, needle="cooldown" if scenario == "cooldown-channel" else "still unconfirmed")
    if scenario.startswith("typing-") or scenario in RACE_SCENARIOS:
        text = NEWER if scenario.startswith("typing-") else CURRENT
        before, after = details.get("nativeBeforeRelease"), details.get("nativeFinal")
        for value in (before, after):
            native(value, text, viewport)
            same([value["start"], value["end"], value["direction"]],
                [len(text[:text.index("\U0001f680")].encode("utf-16-le"))//2, len(text.encode("utf-16-le"))//2, "backward"], "Backward Unicode selection was not preserved")
        for field in ("text", "display", "codePoints", "utf16Length", "scope", "visit", "start", "end", "direction"):
            same(after[field], before[field], "Late completion changed native field: " + field)
        if scenario.startswith("typing-"):
            native(details.get("input"), NEWER, viewport)
            same(details["input"], before, "Typed native input changed before release")
    if scenario in ("positive-channel", "positive-thread"):
        for field in ("nativeByScopeInitial", "nativeByScopeFinal"):
            keys(details.get(field), {"channel", "thread"}, "both native composers")
            for value in details[field].values():
                native(value, CURRENT, viewport)
                home_anchor(value)


def validate(*args):
    try:
        return _validate(*args)
    except (KeyError, TypeError, AttributeError, IndexError) as error:
        raise RuntimeError(f"Malformed Schedule receipt: {type(error).__name__}: {error}") from error


def validate_restart_pair(scenario, first, second):
    require(scenario in RESTART_SCENARIOS, "Unexpected two-process scenario")
    same(first, second, "Second process changed whole canonical/blocked/alternate dictionaries")
