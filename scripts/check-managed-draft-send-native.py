#!/usr/bin/env python3
"""Isolated managed acknowledged Send native acceptance.

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

# Preparation is isolated under tmp/ack-send-native-prep. No active runner edits.
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
FIXTURE = "managed-draft-send"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("ack-newer-channel", "ack-newer-thread", "lost-ack-channel", "lost-ack-thread",
             "held-response-channel", "held-response-thread")
FLAG = "SUPER_PLATINUM_MANAGED_DRAFT_SEND_CHECK"
PREFIX, WAITING, STAGE = "managed-draft-send-check: ", "managed-draft-send-waiting: ", "managed-draft-send-stage: "
UPDATE = runpy.run_path(str(SCRIPTS / "check-draft-update-recovery-native.py"))
require, same, uuid4 = (UPDATE[name] for name in ("require", "same", "uuid4"))
native = UPDATE["native"]
ACCOUNT = "00000000-0000-4000-8000-000000000101"
BASE = "Previous acknowledged content"
REQUESTED = "Frozen send \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301\nsecond line"
NEWER = "Newer local typing \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
THREAD = "1719800400.000000"
ACK_TS = "1719800500.000001"
BLOCKED_TEXT = "A saved message needs review in Drafts before sending another from this composer."
SYNC_FIELDS = {"scope", "client_msg_id", "desired", "local_revision", "acknowledged_local_revision", "acknowledged",
               "workflow", "tombstone", "retired_ids", "consumption", "check_deadline", "check_ticket", "observed_events", "blocked_observation"}
# Kept disabled while this preparation directory is unintegrated. Root owns
# promotion/launches; pure validators below are independently usable.
CONTRACT_READY = Path(__file__).resolve().parent == SCRIPTS


def require_contract():
    if not CONTRACT_READY:
        raise RuntimeError("Managed Send preparation cannot launch before promotion into scripts")


def stages(scenario, phase):
    require(scenario in SCENARIOS and phase in ("exercise", "restart"), "Unknown Send scenario/phase")
    return ("restart",) if phase == "restart" else ("initial", "input", "pending", "newer", "result")


def scope(target):
    return {"account_id": ACCOUNT, "key": {"team": "T1", "channel": "C2", "thread": THREAD if target == "thread" else None}}


def client(target):
    return "00000000-0000-4000-8000-000000000103" if target == "thread" else "00000000-0000-4000-8000-000000000102"


def snapshot(text, target, broadcast):
    destination = {"channel_id": "C2"}
    if target == "thread":
        destination["thread_ts"] = THREAD
        if broadcast:
            destination["broadcast"] = True
    return {"blocks": [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [{"type": "text", "text": text}]}]}],
            "file_ids": [], "destinations": [destination]}


def base(target):
    return {"scope": scope(target), "id": "DrSendThread" if target == "thread" else "DrSendChannel",
            "client_msg_id": client(target), "last_updated_ts": "10.000001", "snapshot": snapshot(BASE, target, target == "thread")}


def identities(op, consume, target):
    require(isinstance(op, dict) and uuid4(op.get("id")), "Missing actual Send operation UUID")
    require(isinstance(consume, dict) and uuid4(consume.get("id")), "Missing actual consume UUID")
    frozen = dict(scope=scope(target), client_msg_id=client(target), local_revision=1, kind="Send",
                  snapshot=snapshot(REQUESTED, target, target == "thread"), base=base(target))
    same(consume, dict(frozen, id=consume["id"]), "Wrong exact immutable consume intent")
    same(op, dict(frozen, id=op["id"], consume_id=consume["id"]), "Wrong exact prepared Send operation")


def inventory(value, target, text, broadcast, stored=True):
    require(isinstance(value, dict) and set(value) == {"scope", "canonical", "stored", "blocked", "alternates"}, "Incomplete Send inventory")
    same(value["scope"], scope(target), "Wrong inventory owner/destination")
    same(value["blocked"], None, "Unexpected blocked copy")
    same(value["alternates"], [], "Unexpected alternate copies")
    local = value["canonical"]
    require(isinstance(local, dict) and set(local) == {"composer", "attachments"}, "Incomplete canonical draft")
    composer = local["composer"]
    require(isinstance(composer, dict) and set(composer) == {"text", "selection", "reply_broadcast"}, "Incomplete composer dictionary")
    # Actual field.rs oninput writes byte-end selection and checkpoints once;
    # synthetic DOM selection/focus does not dispatch another input or save.
    byte_end = len(text.encode("utf-8"))
    same(composer["selection"], {"start": byte_end, "end": byte_end}, "Wrong canonical byte selection")
    same(local.get("attachments"), [], "Unexpected canonical attachment")
    same(local.get("composer", {}).get("text"), text, "Wrong canonical text")
    same(local.get("composer", {}).get("reply_broadcast"), broadcast, "Wrong current broadcast choice")
    if not stored:
        same(value["stored"], None, "Acknowledged Send did not release managed row")
        return None
    row = value["stored"]
    require(isinstance(row, dict) and set(row) == {"revision", "state"}
            and type(row.get("revision")) is int and row["revision"] > 0, "Missing stored CAS revision/dictionary")
    state = row.get("state")
    require(isinstance(state, dict) and set(state) == SYNC_FIELDS, "Missing complete exact sync state dictionary")
    same(state.get("scope"), scope(target), "Wrong sync scope")
    same(state.get("client_msg_id"), client(target), "Frozen client identity changed")
    same(state.get("desired"), snapshot(text, target, broadcast) if text else None, "Canonical and desired differ")
    same(state.get("acknowledged"), base(target), "Acknowledged base changed before completion")
    for field, expected in (("acknowledged_local_revision", 0), ("tombstone", False), ("retired_ids", []),
                            ("check_deadline", None), ("check_ticket", None), ("observed_events", {}), ("blocked_observation", None)):
        same(state.get(field), expected, "Unexpected sync state field: " + field)
    require("local_completion" not in state, "Unexpected local completion authority")
    return state


def inventories(value, target, text, broadcast, stored=True):
    require(isinstance(value, dict) and set(value) == {"channel", "thread"}, "Missing paired inventories")
    other = "channel" if target == "thread" else "thread"
    sibling = inventory(value[other], other, BASE, other == "thread")
    for field, expected in (("workflow", "Idle"), ("local_revision", 0), ("consumption", None)):
        same(sibling[field], expected, "Unexpected untouched sibling state: " + field)
    same(value[other]["stored"]["revision"], 1, "Untouched sibling CAS revision changed")
    return inventory(value[target], target, text, broadcast, stored)


def validate_hit_samples(value):
    samples = value.get("hitSamples")
    require(isinstance(samples, dict) and set(samples) == {"field", "composer", "button"}, "Missing complete hit-sample diagnostics")
    for surface in ("field", "composer", "button"):
        proof, bounds = samples[surface], value[surface]
        rounded = surface == "button"
        keys = {"ok", "method", "points"} | ({"computedRadii", "inset", "radius"} if rounded else set())
        require(isinstance(proof, dict) and set(proof) == keys, "Wrong hit proof fields: " + surface)
        same(proof["ok"], True, "Failed hit proof: " + surface)
        same(proof["method"], "rounded-arc" if rounded else "rect-grid", "Wrong hit method: " + surface)
        width, height = bounds["width"], bounds["height"]
        if rounded:
            same(proof["inset"], 1, "Rounded Send inset must be one CSS pixel")
            require(number(proof["radius"]) and 0 <= proof["radius"] <= min(width, height) / 2
                    and min(width, height) > 2, "Unsupported rounded Send radius/dimensions")
            computed = proof["computedRadii"]
            require(isinstance(computed, list) and len(computed) == 4, "Missing four computed corner radii")
            for css in computed:
                require(isinstance(css, str), "Computed radius must be CSS text")
                parts = css.strip().split()
                require(len(parts) in (1, 2) and all(re.fullmatch(r"[0-9]+(?:\.[0-9]+)?px", part) for part in parts),
                        "Unsupported non-pixel corner radius")
                require(all(number(float(part[:-2])) and float(part[:-2]) == proof["radius"] for part in parts),
                        "Nonuniform/noncircular computed Send radius")
            radius = proof["radius"]
            distance = radius - (radius - 1) / math.sqrt(2) if radius > 1 else 1
            expected = [("center", width / 2, height / 2), ("top", width / 2, 1),
                ("right", width - 1, height / 2), ("bottom", width / 2, height - 1), ("left", 1, height / 2),
                ("top-left", distance, distance), ("top-right", width - distance, distance),
                ("bottom-right", width - distance, height - distance), ("bottom-left", distance, height - distance)]
        else:
            expected = [(f"{i}:{j}", width * x, height * y)
                        for i, x in enumerate((.05, .5, .95)) for j, y in enumerate((.05, .5, .95))]
        points = proof["points"]
        require(isinstance(points, list) and len(points) == 9, "Expected exactly nine hit points: " + surface)
        for point, (name, dx, dy) in zip(points, expected):
            require(isinstance(point, dict) and set(point) == {"name", "x", "y", "owned", "element"}, "Wrong hit point fields: " + surface)
            same(point["name"], name, "Wrong ordered hit point name: " + surface)
            same(point["owned"], True, f"Unowned {surface} hit point: {name}")
            require(number(point["x"]) and number(point["y"]), "Invalid hit coordinates: " + surface)
            require(bounds["x"] < point["x"] < bounds["x"] + width
                    and bounds["y"] < point["y"] < bounds["y"] + height, "Hit point outside surface: " + surface)
            require(abs(point["x"] - bounds["x"] - dx) <= 1e-6
                    and abs(point["y"] - bounds["y"] - dy) <= 1e-6,
                    f"Wrong {surface} hit coordinates: {name}")
            element = point["element"]
            require(isinstance(element, dict) and set(element) == {"tag", "id", "classes"}
                    and all(isinstance(part, str) for part in element.values()) and element["tag"],
                    "Missing actual hit element diagnostics: " + surface)


def controls(value, target, viewport, clicked=False):
    require(isinstance(value, dict), "Missing Send control measurement")
    for field, expected in dict(ok=True, kind="composer", target=target, viewport=viewport,
        clickRequested=clicked, clicked=clicked, disabled=False, fieldUnobscured=True,
        composerUnobscured=True, buttonUnobscured=True, targetMatches=True).items():
        same(value.get(field), expected, "Wrong Send geometry/control field: " + field)
    require(number(value.get("documentWidth")) and 0 < value["documentWidth"] <= viewport[0] + 1, "Composer document overflows")
    for name in ("field", "composer", "button", "pane", "header"):
        require(rectangle(value.get(name), viewport), "Send rectangle clipped: " + name)
    validate_hit_samples(value)
    for name, parent in (("field", "composer"), ("button", "composer"), ("composer", "pane"), ("header", "pane")):
        bounds, panel = value[name], value[parent]
        for axis, extent in (("x", "width"), ("y", "height")):
            require(bounds[axis] >= panel[axis] - 1 and bounds[axis] + bounds[extent] <= panel[axis] + panel[extent] + 1,
                    f"Send {name} clipped on {axis} by {parent}")
    require(value["composer"]["y"] >= value["header"]["y"] + value["header"]["height"] - 1, "Composer covers conversation header")
    for name in ("scope", "visit"):
        require(isinstance(value.get(name), str) and value[name], "Missing native owner/visit")
    require(type(value.get("fieldFocused")) is bool and type(value.get("buttonFocused")) is bool
            and not (value["fieldFocused"] and value["buttonFocused"]), "Missing/inconsistent actual focus evidence")
    if clicked:
        same(value["fieldFocused"], False, "Clicked Send unexpectedly retained textarea focus")
        same(value["buttonFocused"], True, "Actual Send click did not focus its button")


def focus(value, target, field):
    same(value, dict(ok=True, fieldFocused=field, buttonFocused=not field,
                     activeId=target + "-composer" if field else ""), "Wrong explicit Send focus evidence")


def same_native(before, after):
    for field in ("text", "display", "codePoints", "utf16Length", "scope", "visit", "start", "end", "direction"):
        same(before.get(field), after.get(field), "Native text/selection/visit changed: " + field)


def validate_wire(attempt, op, consume, target, scenario):
    require(isinstance(attempt, dict), "Missing actual request")
    try:
        url = urlsplit(attempt.get("url"))
        require(url.scheme == "https" and url.netloc == "fixture.slack.com" and url.path == "/api/chat.postMessage" and not url.fragment,
                "Wrong actual Send route")
        validate_query(url.query)
    except (ValueError, TypeError, AttributeError) as error:
        raise RuntimeError("Malformed actual Send URL") from error
    for field, expected in dict(sequence=1, method="POST", retrySafe=False, operation=op, consume=consume).items():
        same(attempt.get(field), expected, "Wrong actual request evidence: " + field)
    fields = attempt.get("form")
    require(isinstance(fields, list) and fields and isinstance(fields[0], list) and len(fields[0]) == 2 and fields[0][0] == "blocks",
            "Actual form missing ordered frozen blocks")
    # serde_json::Value objects serialize in canonical key order; validate both
    # the exact bytes and decoded content, without accepting duplicate keys.
    blocks = json.dumps(snapshot(REQUESTED, target, target == "thread")["blocks"], ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    expected = [["blocks", blocks], ["channel", "C2"], ["client_msg_id", client(target)], ["draft_id", base(target)["id"]]]
    if target == "thread":
        expected += [["thread_ts", THREAD], ["reply_broadcast", "true"]]
    same(fields, expected, "Wrong exact frozen chat.postMessage form")
    positive, held = scenario.startswith("ack-newer-"), scenario.startswith("held-response-")
    message = dict(client_msg_id=client(target), user="U00000001", channel="C2", ts=ACK_TS, text=REQUESTED,
                   blocks=snapshot(REQUESTED, target, target == "thread")["blocks"])
    if target == "thread":
        message["thread_ts"] = THREAD
    raw = dict(ok=True, channel="C2", ts=ACK_TS, draft_id=base(target)["id"], message=message)
    for field, expected in dict(rawResponse=raw if positive else None, error="transport" if not positive and not held else None,
        outcome="sent" if positive else "pending" if held else "uncertain", delivered=not held).items():
        same(attempt.get(field), expected, "Wrong acknowledgement evidence: " + field)


def message_inventory(value):
    require(isinstance(value, dict) and set(value) == {"channel", "thread", "pending", "pending_delivery"},
            "Incomplete exact message/pending inventory")
    for which in ("channel", "thread"):
        require(value[which] is None or (isinstance(value[which], list)
                and all(isinstance(m, dict) for m in value[which])), "Invalid message bag")
    pending = value["pending"]
    require(isinstance(pending, dict) and set(pending) == {"channel", "thread"}, "Incomplete pending timestamp sets")
    for entries in pending.values():
        require(isinstance(entries, list) and all(isinstance(ts, str) and ts for ts in entries)
                and len(entries) == len(set(entries)), "Invalid/duplicate pending timestamps")
    delivery = value["pending_delivery"]
    require(isinstance(delivery, dict) and all(isinstance(ts, str) and ts and status in ("Unconfirmed", "Failed")
            for ts, status in delivery.items()), "Invalid pending delivery map")


def frozen_message(target, timestamp):
    # Exact serde Message defaults from core models/core.rs plus the fields
    # produced by shared send_projection::message. No extra delivery authority.
    value = dict.fromkeys(("user", "bot_id", "username", "bot_profile", "icons", "type", "subtype", "ts", "client_msg_id",
        "text", "team", "channel", "thread_ts", "parent_user_id", "reply_count", "reply_users_count", "latest_reply", "edited", "message"))
    value.update(reply_users=[], reactions=[], blocks=snapshot(REQUESTED, target, target == "thread")["blocks"], files=[], attachments=[])
    value.update(user="U00000001", type="message", subtype="thread_broadcast" if target == "thread" else None, ts=timestamp,
        client_msg_id=client(target), text=REQUESTED, channel="C2", thread_ts=THREAD if target == "thread" else None)
    return value


def validate_messages(receipt, target, positive):
    details = receipt["details"]
    initial, held, final = receipt["messagesInitial"], details.get("messagesHeld"), details.get("messagesFinal")
    for content in (initial, held, final):
        message_inventory(content)
    def matches(content, which):
        return [m for m in content[which] or [] if m.get("client_msg_id") == client(target)]
    for which in ("channel", "thread"):
        require(not matches(initial, which), "Send identity already present before action")
    pending = matches(held, target)
    require(len(pending) == 1, "Missing or duplicate optimistic Send projection")
    timestamp = pending[0].get("ts")
    require(isinstance(timestamp, str) and re.fullmatch(r"[1-9][0-9]*\.[0-9]{6}", timestamp)
            and timestamp != ACK_TS, "Invalid optimistic timestamp or fabricated acknowledgement")
    same(details.get("optimisticTs"), timestamp, "Recorded optimistic identity differs from actual pending message")
    require(timestamp not in initial["pending_delivery"] and all(timestamp not in items for items in initial["pending"].values()),
            "Optimistic timestamp reused existing pending identity")
    for content, stage in ((held, "held"), (final, "final")):
        acknowledged = stage == "final" and positive
        expected_pending = copy.deepcopy(initial["pending"])
        if not acknowledged:
            expected_pending[target].append(timestamp)
        same(content["pending"], expected_pending, stage + " pending timestamps differ from actual Send lifecycle")
        expected_delivery = copy.deepcopy(initial["pending_delivery"])
        if stage == "final" and receipt["scenario"].startswith("lost-ack-"):
            expected_delivery[timestamp] = "Unconfirmed"
        same(content["pending_delivery"], expected_delivery, stage + " fabricated/lost pending delivery state")
        for which in ("channel", "thread"):
            expected_copies = 1 if which == target or (acknowledged and target == "thread" and which == "channel") else 0
            copies = matches(content, which)
            require(len(copies) == expected_copies, f"{stage} wrong Send copy count in {which}")
            if expected_copies:
                same(copies[0], frozen_message(target, ACK_TS if acknowledged else timestamp),
                     f"{stage} wrong exact frozen Message in {which}")
            # This equality is specific to the disconnected fixture and current
            # send_projection, which neither ingests realtime nor updates parent
            # counters. It is not an invariant for a live Slack message bag.
            unrelated = [m for m in content[which] or [] if m.get("client_msg_id") != client(target)]
            same(unrelated, initial[which] or [], f"{stage} changed unrelated messages in {which}")
            if expected_copies == 0:
                same(content[which], initial[which], f"{stage} changed empty/missing message bag in {which}")
            if acknowledged:
                require(all(m.get("ts") != timestamp for m in content[which] or []), "Acknowledgement retained optimistic copy")


def validate(fixture, scenario, phase, nonce, pid, database, receipt):
    names = stages(scenario, phase)
    require(fixture in FIXTURES and isinstance(receipt, dict), "Unknown managed Send fixture")
    target = "thread" if scenario.endswith("thread") else "channel"
    other = "channel" if target == "thread" else "thread"
    positive, held = scenario.startswith("ack-newer-"), scenario.startswith("held-response-")
    viewport, count = expected_viewport(fixture), int(phase == "exercise")
    for field, expected in dict(ok=True, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce, pid=pid,
        target=target, isolatedDatabase=str(database), viewport=viewport, noSession=True, noTransport=True, noCache=True,
        noProductionRoute=True, syntheticInput=True, actualSendControl=True, actualPreparedRequests=True,
        preparedRequests=count, refusedInputs=0).items():
        same(receipt.get(field), expected, "Wrong exact Send receipt: " + field)
    message_inventory(receipt.get("messagesInitial"))
    initial, final, details = (receipt.get(k) for k in ("initial", "durable", "details"))
    require(isinstance(details, dict), "Missing Send stage details")
    final_state = inventories(final, target, NEWER, False, not positive)
    initial_state = inventories(initial, target, BASE if phase == "exercise" else NEWER,
        target == "thread" if phase == "exercise" else False, phase == "exercise" or not positive)
    same(initial[other], final[other], "Unrelated destination changed")
    op, consume = receipt.get("operation"), receipt.get("consumption")
    if phase == "exercise" or not positive:
        identities(op, consume, target)
    else:
        same(op, None, "Positive restart retained operation")
        same(consume, None, "Positive restart retained consumption")
    if not positive:
        same(final_state.get("workflow"), {"InFlight" if held and phase == "exercise" else "Uncertain": op}, "Wrong unresolved Send workflow")
        same(final_state.get("consumption"), consume, "Unresolved consumption changed")
        same(final_state.get("local_revision"), 3, "Newer input local revision lost")
    attempts = receipt.get("attempts")
    require(isinstance(attempts, list) and len(attempts) == count, "Send dispatched missing/extra/restart request")
    require(type(receipt.get("routedInputs")) is int and receipt["routedInputs"] >= (2 if phase == "exercise" else 0), "Actual input not routed")
    native(details.get("nativeInitial"), BASE if phase == "exercise" else NEWER, viewport)
    geometry = receipt.get("geometry")
    require(isinstance(geometry, list) and len(geometry) == len(names), "Missing complete composer geometry")
    texts = (NEWER,) if phase == "restart" else (BASE, REQUESTED, "", NEWER, NEWER)
    for stage, text, entry in zip(names, texts, geometry):
        controls(entry, target, viewport)
        same(entry.get("stage"), stage, "Wrong Send stage order")
        if phase == "exercise" and stage in ("input", "newer", "result"):
            same(entry["fieldFocused"], True, "Typing/response stage lost field focus")
            same(entry["buttonFocused"], False, "Typing/response stage focused Send")
        native(entry.get("native"), text, viewport)
        for field in ("scope", "visit"):
            same(entry[field], entry["native"][field], "Measured another native field")
        r = entry["native"]["rect"]
        for outer, inner in (("x", "left"), ("y", "top"), ("width", "width"), ("height", "height")):
            require(abs(entry["field"][outer] - r[inner]) <= 1, "Native textarea differs from measured field")
    if phase == "restart":
        same(initial, final, "Restart Send attempt changed durable state")
        if not positive:
            same(details.get("beforeBlocked"), initial, "Missing complete before-blocked inventory")
            same(details.get("afterBlocked"), initial, "Blocked Send mutated inventory")
            message_inventory(details.get("messagesAfterBlocked"))
            same(details["messagesAfterBlocked"], receipt["messagesInitial"], "Blocked Send projected a message or delivery state")
            controls(details.get("blockedClick"), target, viewport, True)
            focus(details.get("blockedFocus"), target, False)
            feedback = details.get("blockedFeedback")
            require(isinstance(feedback, dict) and set(feedback) == {"ok", "modelText", "renderedText", "rendered", "rect", "viewport"},
                    "Missing actual shared admission feedback")
            for field, expected in dict(ok=True, modelText=BLOCKED_TEXT, renderedText=BLOCKED_TEXT, rendered=True, viewport=viewport).items():
                same(feedback[field], expected, "Wrong actual blocked Send feedback: " + field)
            require(rectangle(feedback["rect"], viewport), "Blocked Send feedback clipped")
            for field in ("nativeBeforeBlocked", "nativeAfterBlocked"):
                native(details.get(field), NEWER, viewport)
            same_native(details["nativeBeforeBlocked"], details["nativeAfterBlocked"])
        return final
    validate_wire(attempts[0], op, consume, target, scenario)
    same(initial_state.get("workflow"), "Idle", "Seed is not acknowledged Idle")
    same(initial_state.get("consumption"), None, "Seed already consumed")
    same(initial_state.get("local_revision"), 0, "Wrong seed local revision")
    before_send, after_consume, after_typing = (details.get(k) for k in ("beforeSend", "heldAfterConsume", "heldAfterTyping"))
    for value, text, broadcast, revision, workflow, intent in (
        (before_send, REQUESTED, target == "thread", 1, "Idle", None),
        (after_consume, "", False, 2, {"InFlight": op}, consume),
        (after_typing, NEWER, False, 3, {"InFlight": op}, consume)):
        state = inventories(value, target, text, broadcast)
        same(value[other], initial[other], "Send stage changed unrelated destination")
        same(state.get("local_revision"), revision, "Wrong durable content revision")
        same(state.get("workflow"), workflow, "Wrong admitted Send stage workflow")
        same(state.get("consumption"), intent, "Wrong admitted stage consumption")
    revisions = [v[target]["stored"]["revision"] for v in (initial, before_send, after_consume, after_typing)]
    same(revisions, [1, 2, 4, 5], "Wrong persisted seed/edit/consume/prepare/newer CAS revisions")
    if held:
        same(final, after_typing, "Held response mutated durable state")
    elif not positive:
        expected = copy.deepcopy(after_typing)
        expected[target]["stored"]["state"]["workflow"] = {"Uncertain": op}
        expected[target]["stored"]["revision"] += 1
        same(final, expected, "Lost acknowledgement changed frozen content or unrelated state")
    else:
        same(final[target]["canonical"], after_typing[target]["canonical"], "Release changed newer canonical draft")
    for field, text in (("nativeInput", REQUESTED), ("nativeConsumed", ""), ("nativeNewer", NEWER),
                        ("nativeBeforeRelease", NEWER), ("nativeFinal", NEWER)):
        native(details.get(field), text, viewport)
    same_native(details["nativeNewer"], details["nativeBeforeRelease"])
    same_native(details["nativeNewer"], details["nativeFinal"])
    for field in ("beforeReleaseFocus", "finalFocus"):
        focus(details.get(field), target, True)
    for field in ("actualClick", "duplicateClick"):
        controls(details.get(field), target, viewport, True)
    validate_messages(receipt, target, positive)
    return final


def validate_restart_pair(scenario, exercise, restart):
    expected = copy.deepcopy(exercise)
    if scenario.startswith("held-response-"):
        target = "thread" if scenario.endswith("thread") else "channel"
        stored = expected[target]["stored"]
        workflow = stored["state"]["workflow"]
        require(isinstance(workflow, dict) and set(workflow) == {"InFlight"}, "Held process lacks InFlight operation")
        stored["state"]["workflow"] = {"Uncertain": workflow["InFlight"]}
        stored["revision"] += 1
    same(restart, expected, "Restart changed more than exact held InFlight normalization")


def validate_artifacts(fixture, scenario, phase, nonce, pid, database, directory, receipt, edges, dimensions):
    validate(fixture, scenario, phase, nonce, pid, database, receipt)
    names = stages(scenario, phase)
    same(edges, [{"stage": s, "edge": e} for s in names for e in ("before", "after")], "Missing/extra ordered Send capture edges")
    require(isinstance(receipt.get("captures"), dict) and set(receipt["captures"]) == set(names), "Wrong Send capture set")
    sizes = {}
    for stage in names:
        path = directory / f"{fixture}-{scenario}-{phase}-{stage}.png"
        same(receipt["captures"][stage], str(path), "Unowned Send capture path")
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
    require_contract()
    label = f"{fixture}-{scenario}-{phase}"
    env = {FLAG: "1", "SUPER_PLATINUM_MANAGED_DRAFT_SEND_DB": str(database),
           "SUPER_PLATINUM_MANAGED_DRAFT_SEND_NONCE": nonce, "SUPER_PLATINUM_MANAGED_DRAFT_SEND_SCENARIO": scenario,
           "SUPER_PLATINUM_MANAGED_DRAFT_SEND_PHASE": phase, "SUPER_PLATINUM_MANAGED_DRAFT_SEND_CAPTURE_DIR": str(directory)}
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
                        raise TimeoutError("Native managed-send receipt timed out")
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
    require_contract()
    stages(scenario, "exercise")
    directory = captures / f"{fixture}-{scenario}"
    directory.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    storage = directory / f"managed-draft-send-{nonce}"
    storage.mkdir()
    with (storage / "owner.json").open("x", encoding="utf-8") as marker:
        json.dump({"version": 1, "nonce": nonce}, marker)
    database = storage / "managed-draft-send.sqlite"
    digest = sha(binary)
    evidence = {"ok": False, "fixture": fixture, "scenario": scenario, "nonce": nonce,
                "platform": sys.platform, "binary": str(binary), "binarySha256": digest,
                "isolatedDatabase": str(database), "phases": [],
                "limitations": ["Synthetic DOM input; no trusted keyboard, OS IME, or live Slack.",
                    "Pointer/geometry checks are sampled; actual PNG inspection is still required.",
                    "PNG chunk/CRC/scanline validation does not decode pixel colors.",
                    "Exact frozen Send only; no automatic retry, live delivery, or synchronization proof.",
                    "Pending response is held through controlled owned process termination; this is not a crash-durability claim."]}
    try:
        for phase in ("exercise", "restart"):
            evidence["phases"].append(run_phase(fixture, scenario, phase, nonce, database, binary, directory, digest))
        first, second = (p["native"]["durable"] for p in evidence["phases"])
        validate_restart_pair(scenario, first, second)
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
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-managed-draft-send")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or SCENARIOS:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
