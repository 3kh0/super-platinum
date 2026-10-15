#!/usr/bin/env python3
"""Isolated managed Schedule creation/rejection native acceptance.

Preparation under tmp cannot launch; promote into scripts after integration.
Synthetic HTTP/DOM input only. No production Session/Transport or clipboard.
"""
import argparse
import copy
from contextlib import ExitStack
from datetime import datetime
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

# Preparation is isolated under tmp/managed-schedule-native-prep. No active runner edits.
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
FIXTURE = "managed-draft-schedule"
FIXTURES = (FIXTURE, FIXTURE + "-narrow")
SCENARIOS = ("rich-channel", "rich-thread", "retry-channel", "expired-change-thread", "newer-channel", "newer-thread",
             "stale-owner-channel", "stale-visit-thread", "lost-ack-channel", "held-response-thread", "rejected-retry-channel")
RESTART_SCENARIOS = SCENARIOS
FLAG = "SUPER_PLATINUM_MANAGED_DRAFT_SCHEDULE_CHECK"
PREFIX, WAITING, STAGE = "managed-draft-schedule-check: ", "managed-draft-schedule-waiting: ", "managed-draft-schedule-stage: "
UPDATE = runpy.run_path(str(SCRIPTS / "check-draft-update-recovery-native.py"))
require, same, uuid4 = (UPDATE[name] for name in ("require", "same", "uuid4"))
native = UPDATE["native"]
SEND = runpy.run_path(str(SCRIPTS / "check-managed-draft-send-native.py"))

CONTRACT_READY = Path(__file__).resolve().parent == SCRIPTS


def require_contract():
    require(CONTRACT_READY, "Managed Schedule preparation cannot launch before promotion into scripts")


def phases(scenario):
    require(scenario in SCENARIOS, "Unknown managed Schedule scenario")
    return ("exercise", "restart")


def base_stages(scenario, phase):
    require(phase in phases(scenario), "Unknown managed Schedule phase")
    if phase == "restart": return ("restart",)
    if scenario == "rejected-retry-channel":
        return ("initial", "review", "pending", "rejected", "retry-review", "retry-pending", "result")
    if scenario.startswith("stale-"): return ("initial", "review", "stale")
    if scenario.startswith("held-response-"): return ("initial", "review", "pending")
    return ("initial", "review", "pending", "result")


DIALOG_STAGES = {"review", "pending", "retry-review", "retry-pending"}


def stages(scenario, phase):
    return tuple(name for stage in base_stages(scenario, phase)
                 for name in ((stage, stage + "-date") if stage in DIALOG_STAGES else (stage,)))


ACCOUNT = "00000000-0000-4000-8000-000000000121"
THREAD, ACK_TS = "1719800400.000000", "1719800500.000001"
BASE = "Previous acknowledged content"
SUFFIX = " \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301"
NEWER = "Newer local typing" + SUFFIX
SYNC_FIELDS = {"scope", "client_msg_id", "desired", "local_revision", "acknowledged_local_revision", "acknowledged",
               "workflow", "tombstone", "retired_ids", "consumption", "check_deadline", "check_ticket", "observed_events", "blocked_observation"}


def keys(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), "Wrong exact fields: " + label)


def scope(target):
    return dict(account_id=ACCOUNT, key=dict(team="T1", channel="C2", thread=THREAD if target == "thread" else None))


def client(target):
    return "00000000-0000-4000-8000-00000000012" + ("3" if target == "thread" else "2")


def local(text, target, broadcast=None):
    end = len(text.encode("utf-8"))
    return dict(composer=dict(text=text, selection=dict(start=end, end=end),
                             reply_broadcast=target == "thread" if broadcast is None else broadcast), attachments=[])


def snapshot(text, target, broadcast=None):
    destination = dict(channel_id="C2")
    if target == "thread":
        destination["thread_ts"] = THREAD
        if broadcast is None or broadcast:
            destination["broadcast"] = True
    return dict(blocks=[dict(type="rich_text", elements=[dict(type="rich_text_section", elements=[dict(type="text", text=text)])])],
                file_ids=[], destinations=[destination])


def rich(target, rejected=False, changed=False):
    result = snapshot("", target)
    if rejected:
        result["blocks"] = [dict(type="rich_text", block_id="frozen-rich", elements=[
            dict(type="rich_text_section", elements=[dict(type="text", text="Changed rich schedule " if changed else "Frozen rich schedule ", style=dict(bold=True)),
                dict(type="user", user_id="U00000001"), dict(type="text", text=" \u65e5\u672c\u8a9e \U0001f680 e\u0301")]),
            dict(type="rich_text_quote", elements=[dict(type="text", text="Preserved quote", style=dict(italic=True))]),
            dict(type="rich_text_list", style="ordered", offset=2, indent=0, elements=[dict(type="rich_text_section", elements=[dict(type="text", text="List item")])]),
            dict(type="future_rich_node", opaque_payload=dict(keep=[1, "opaque", True]))])]
    else:
        result["blocks"] = [dict(type="rich_text", elements=[
            dict(type="rich_text_section", elements=[dict(type="text", text="Frozen schedule" + SUFFIX, style=dict(bold=True))]),
            dict(type="rich_text_quote", elements=[dict(type="text", text="Quoted detail", style=dict(italic=True))]),
            dict(type="rich_text_list", style="ordered", offset=2, elements=[dict(type="rich_text_section", elements=[dict(type="text", text="List item")])])])]
    return result


def base(target):
    return dict(scope=scope(target), id="DrSchedule" + target.title(), client_msg_id=client(target),
                last_updated_ts="10.000001", snapshot=snapshot(BASE, target))


def consumption(op):
    return dict(id=op["consume_id"], **{field: copy.deepcopy(op[field]) for field in
        ("scope", "client_msg_id", "local_revision", "kind", "snapshot", "base")})


def operation(op, target, payload, post_at):
    require(isinstance(op, dict) and uuid4(op.get("id")) and uuid4(op.get("consume_id")), "Missing actual Schedule operation/intent UUID")
    require(type(post_at) is int and post_at > 0 and post_at % 60 == 0, "Schedule time is not a positive minute boundary")
    same(op, dict(id=op["id"], scope=scope(target), client_msg_id=client(target), local_revision=0,
        kind={"Schedule": {"post_at": post_at}}, snapshot=payload, base=base(target), consume_id=op["consume_id"]), "Wrong immutable Schedule operation")


def seed(target, rejected_op=None):
    text = NEWER if rejected_op else BASE
    state = dict(scope=scope(target), client_msg_id=client(target), desired=snapshot(text, target),
        local_revision=2 if rejected_op else 0, acknowledged_local_revision=0, acknowledged=base(target),
        workflow={"Rejected": rejected_op} if rejected_op else "Idle", tombstone=False, retired_ids=[],
        consumption=consumption(rejected_op) if rejected_op else None, check_deadline=None, check_ticket=None,
        observed_events={}, blocked_observation=None)
    return dict(scope=scope(target), canonical=local(text, target), stored=dict(revision=1, state=state), blocked=None, alternates=[])


def inventory_shape(pair):
    keys(pair, {"channel", "thread"}, "paired inventories")
    for target, item in pair.items():
        keys(item, {"scope", "canonical", "stored", "blocked", "alternates"}, "inventory")
        same(item["scope"], scope(target), "Wrong inventory scope")
        same(item["blocked"], None, "Unexpected blocked copy")
        same(item["alternates"], [], "Unexpected alternate copy")
        keys(item["canonical"], {"composer", "attachments"}, "canonical")
        composer = item["canonical"]["composer"]
        keys(composer, {"text", "selection", "reply_broadcast"}, "composer")
        require(isinstance(composer["text"], str) and type(composer["reply_broadcast"]) is bool, "Invalid composer value")
        same(item["canonical"], local(composer["text"], target, composer["reply_broadcast"]), "Wrong canonical bytes/attachments")
        keys(item["stored"], {"revision", "state"}, "Stored")
        require(type(item["stored"]["revision"]) is int and item["stored"]["revision"] > 0, "Wrong stored CAS revision")
        state = item["stored"]["state"]
        keys(state, SYNC_FIELDS | ({"local_completion"} if "local_completion" in state else set()), "DraftSync")


def pending(initial, target, op, rejected=False):
    value = copy.deepcopy(initial)
    item = value[target]
    item["stored"]["revision"] = 3
    item["stored"]["state"].update(workflow={"InFlight": op}, consumption=consumption(op))
    if not rejected:
        item["canonical"] = local("", target, False)
        item["stored"]["state"].update(desired=None, local_revision=1)
    return value


def typed(value, target):
    result = copy.deepcopy(value)
    item = result[target]
    item["canonical"] = local(NEWER, target, False)
    item["stored"]["revision"] += 1
    item["stored"]["state"]["local_revision"] += 1
    item["stored"]["state"]["desired"] = snapshot(NEWER, target, False)
    return result


def completed(value, target, op, successor=None, rejected=False, uncertain=False):
    result = copy.deepcopy(value)
    item = result[target]
    item["stored"]["revision"] += 1
    state = item["stored"]["state"]
    if rejected or uncertain:
        state["workflow"] = {"Rejected" if rejected else "Uncertain": op}
    else:
        require(uuid4(successor) and successor != client(target), "Schedule completion did not create a fresh successor UUID")
        state.update(client_msg_id=successor, acknowledged=None, acknowledged_local_revision=1,
            workflow="Idle", retired_ids=[op["base"]["id"]], consumption=None,
            local_completion=dict(intent=consumption(op), successor_client_msg_id=successor))
    return result


def wire(value, op, index, outcome):
    keys(value, {"sequence", "method", "url", "form", "retrySafe", "operation", "consume", "rawResponse", "error", "outcome", "delivered"}, "actual prepared request")
    url = urlsplit(value["url"])
    require(url.scheme == "https" and url.netloc == "fixture.slack.com" and url.path == "/api/drafts.update" and not url.fragment,
            "Wrong actual Schedule route")
    validate_query(url.query)
    for name, expected in dict(sequence=index, method="POST", retrySafe=False, operation=op,
        consume=consumption(op), outcome=outcome, delivered=outcome != "pending", error="transport" if outcome == "uncertain" else None).items():
        same(value[name], expected, "Wrong request evidence: " + name)
    fields = value["form"]
    require(isinstance(fields, list) and len(fields) == 7, "Unexpected Schedule form fields")
    timestamp = fields[-1][1]
    require(isinstance(timestamp, str) and re.fullmatch(r"[1-9][0-9]*", timestamp), "Invalid actual mutation time")
    require(op["kind"]["Schedule"]["post_at"] * 1000 > int(timestamp), "Dispatched Schedule time was already expired")
    encode = lambda v: json.dumps(v, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    same(fields, [["blocks", encode(op["snapshot"]["blocks"])], ["destinations", encode(op["snapshot"]["destinations"])],
        ["file_ids", "[]"], ["is_from_composer", "true"], ["date_scheduled", str(op["kind"]["Schedule"]["post_at"])],
        ["draft_id", op["base"]["id"]], ["client_last_updated_ts", timestamp]], "Wrong exact ordered Schedule form")
    raw = None
    if outcome == "rejected": raw = dict(ok=False, error="invalid_blocks")
    if outcome == "scheduled":
        raw = dict(ok=True, draft=dict(id=op["base"]["id"], client_msg_id=client("thread" if op["scope"]["key"]["thread"] else "channel"),
            last_updated_ts=ACK_TS, date_created=10, date_scheduled=op["kind"]["Schedule"]["post_at"],
            is_sent=False, is_deleted=False, is_from_composer=True, **copy.deepcopy(op["snapshot"])))
    same(value["rawResponse"], raw, "Wrong actual decoder input")
    return raw


def proof(value, viewport, required=True):
    keys(value, {"rect", "visible", "points", "unobscured", "disabled", "text", "focused"}, "DOM control proof")
    bounds = value["rect"]
    keys(bounds, {"x", "y", "width", "height"}, "measured control rectangle")
    require(all(number(v) for v in bounds.values()) and bounds["width"] > 0 and bounds["height"] > 0, "Invalid control rectangle")
    same(value["visible"], rectangle(bounds, viewport), "Untruthful viewport visibility measurement")
    require(type(value["unobscured"]) is bool, "Missing actual ownership aggregate")
    if required:
        same(value["visible"], True, "Clipped Schedule control")
        same(value["unobscured"], True, "Obscured Schedule control")
    require(type(value["disabled"]) is bool and type(value["focused"]) is bool and isinstance(value["text"], str), "Invalid control state")
    points, bounds = value["points"], value["rect"]
    require(isinstance(points, list) and len(points) == 9, "Missing nine actual hit samples")
    for point, (x, y) in zip(points, [(x, y) for x in (.15, .5, .85) for y in (.15, .5, .85)]):
        keys(point, {"x", "y", "owned", "hit"}, "hit point")
        require(type(point["owned"]) is bool, "Missing actual hit ownership")
        if required: same(point["owned"], True, "Schedule control hit belongs to another element")
        require(number(point["x"]) and number(point["y"]) and abs(point["x"]-bounds["x"]-bounds["width"]*x) < 1e-6
                and abs(point["y"]-bounds["y"]-bounds["height"]*y) < 1e-6, "Wrong ordered hit sample coordinates")
        if point["hit"] is None:
            same(point["owned"], False, "Missing element cannot own hit point")
        else:
            keys(point["hit"], {"tag", "id", "classes"}, "actual hit element")
            require(all(isinstance(v, str) for v in point["hit"].values()) and point["hit"]["tag"], "Missing hit element diagnostics")
    same(value["unobscured"], all(point["owned"] for point in points), "Ownership aggregate differs from actual hit points")


def viewport_proof(value, viewport):
    same(value["ok"], True, "DOM action failed")
    same(value["viewport"], viewport, "Wrong DOM viewport")
    require(number(value["documentWidth"]) and 0 < value["documentWidth"] <= viewport[0]+1, "Document overflows")


def action(value, viewport, disabled=False, text=None):
    keys(value, {"ok", "clicked", "control", "viewport", "documentWidth"}, "DOM action")
    viewport_proof(value, viewport)
    proof(value["control"], viewport)
    same(value["clicked"], not disabled, "Wrong actual click admission")
    same(value["control"]["disabled"], disabled, "Wrong native disabled state")
    if text is not None: same(value["control"]["text"].strip(), text, "Wrong actual control copy")


def review(value, payload=None, time=None, has_ok=True):
    keys(value, {"blocks", "date", "postAt", "generation", "composing"} | ({"ok"} if has_ok else set()), "rich dialog read")
    if has_ok: same(value["ok"], True, "Rich editor read failed")
    require(isinstance(value["date"], str) and re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}", value["date"]), "Malformed native local date")
    require(type(value["postAt"]) is int and value["postAt"] > 0 and value["postAt"] % 60 == 0, "Invalid native schedule time")
    same(value["date"], datetime.fromtimestamp(value["postAt"]).strftime("%Y-%m-%dT%H:%M"), "Local date differs from dispatched time")
    require(isinstance(value["generation"], str) and value["generation"], "Missing rich editor generation")
    same(value["composing"], "false", "Composition was not settled")
    if payload is not None: same(value["blocks"], payload["blocks"], "Native rich structure changed")
    if time is not None: same(value["postAt"], time, "Native schedule time changed")


SCHEDULED_NOTICE = "Message scheduled."
UNCERTAIN_NOTICE = "Schedule is unconfirmed. Check its status in Drafts before trying again."
REJECTED_NOTICE = "Message was not scheduled. Review it in Drafts to retry or change the schedule."


def dismissed_notice(value, viewport, text):
    keys(value, {"ok", "before", "clicked", "afterAbsent"}, "actual notice dismissal")
    for field in ("ok", "clicked", "afterAbsent"):
        same(value[field], True, "Notice dismissal failed: " + field)
    before = value["before"]
    keys(before, {"text", "role", "toast", "dismiss", "viewport", "documentWidth"}, "notice before dismissal")
    same(before["text"], text, "Wrong completion notice copy")
    same(before["role"], "status", "Wrong actual notice role")
    same(before["viewport"], viewport, "Wrong notice viewport")
    require(number(before["documentWidth"]) and 0 < before["documentWidth"] <= viewport[0]+1, "Notice document overflows")
    for field in ("toast", "dismiss"):
        proof(before[field], viewport)
        same(before[field]["disabled"], False, "Notice dismissal is disabled")
    same(before["toast"]["text"], text, "Measured notice copy differs")
    child, parent = before["dismiss"]["rect"], before["toast"]["rect"]
    require(child["x"] >= parent["x"]-1 and child["y"] >= parent["y"]-1
            and child["x"]+child["width"] <= parent["x"]+parent["width"]+1
            and child["y"]+child["height"] <= parent["y"]+parent["height"]+1,
            "Dismiss control is outside its notice")


def geometry(receipt, scenario, phase, viewport):
    names = stages(scenario, phase)
    rows = receipt["geometry"]
    require(isinstance(rows, list) and len(rows) == len(names), "Missing geometry stages")
    rejected = scenario in ("retry-channel", "expired-change-thread")
    previous = None
    for row, name in zip(rows, names):
        stage = name.removesuffix("-date")
        keys(row, {"stage", "ok", "surface", "controls", "viewport", "documentWidth", "current", "active", "text", "busy", "threadPaneAbsent", "bodyView", "bodyScroll"}, "stage geometry")
        same(row["stage"], name, "Wrong geometry order")
        viewport_proof(row, viewport)
        surface = "composer" if stage in ("restart", "stale") or stage == "initial" and not rejected else "panel" if stage in ("initial", "result", "rejected") else "dialog"
        same(row["surface"], surface, "Wrong stage surface: " + name)
        same(row["threadPaneAbsent"], True if surface == "panel" else None, "Missing panel thread-overlay absence proof")
        chrome = {"dialog", "header", "title", "destination", "footer", "primary", "close", "body"}
        editor_controls, date_controls = {"editor", "editorLabel", "toolbar"}, {"date", "dateLabel", "hint"}
        expected = chrome | editor_controls | date_controls if surface == "dialog" else {"panel", "header", "row"} if surface == "panel" else {"field", "footer", "schedule"}
        keys(row["controls"], expected, "stage controls: " + name)
        selected = date_controls if name.endswith("-date") else editor_controls
        required = chrome | selected if surface == "dialog" else expected
        for control_name, control in row["controls"].items(): proof(control, viewport, control_name in required)
        parent = "dialog" if surface == "dialog" else "panel" if surface == "panel" else "footer"
        bounds = row["controls"][parent]["rect"]
        for control_name in required:
            control = row["controls"][control_name]
            rect = control["rect"]
            for axis, extent in (("x", "width"), ("y", "height")):
                require(rect[axis] >= bounds[axis]-1 and rect[axis]+rect[extent] <= bounds[axis]+bounds[extent]+1,
                        "Stage control clipped by parent: " + name + "/" + axis)
        keys(row["active"], {"id", "tag", "text"}, "active element")
        require(all(isinstance(v, str) for v in row["active"].values()) and isinstance(row["text"], str), "Missing actual DOM text")
        if surface == "dialog":
            frozen = receipt["frozen"]
            review(row["current"], dict(blocks=frozen["blocks"]), frozen["postAt"], False)
            expected_generation = receipt["details"]["retryReview"]["generation"] if stage.startswith("retry-") else frozen["generation"]
            same(row["current"]["generation"], expected_generation, "Dialog remounted during reviewed operation: " + name)
            busy = stage in ("pending", "retry-pending")
            same(row["busy"], "true" if busy else "false", "Wrong busy state: " + name)
            same(row["controls"]["primary"]["disabled"], busy or stage == "stale", "Wrong submit admission: " + name)
            same(row["controls"]["close"]["disabled"], busy, "Wrong close admission: " + name)
            controls = row["controls"]
            review_mode = rejected or stage in ("retry-review", "retry-pending")
            same(controls["title"]["text"], "Review schedule" if review_mode else "Schedule message", "Wrong dialog title")
            same(controls["destination"]["text"], "To #ship" + (" \u00b7 Thread reply" if scenario.endswith("thread") else ""), "Wrong visible destination")
            button_text = "Saving\u2026" if busy else "Save new schedule" if scenario == "expired-change-thread" else "Retry same schedule" if review_mode else "Schedule message"
            same(controls["primary"]["text"], button_text, "Wrong visible submit action")
            same(controls["editorLabel"]["text"], "Message", "Wrong rich editor label")
            same(controls["dateLabel"]["text"], "Date and time", "Wrong schedule date label")
            same(controls["hint"]["text"], "Uses your local time zone. " + ("Your separate composer draft will be kept." if review_mode else "Newer typing will be kept."), "Wrong preservation hint")
            same(controls["date"]["disabled"], busy or (review_mode and scenario != "expired-change-thread"), "Wrong actual date editing admission")
            for child, parent in [("title", "header"), ("close", "header"), ("primary", "footer")] + [(c, "body") for c in selected]:
                child_rect, parent_rect = controls[child]["rect"], controls[parent]["rect"]
                for axis, extent in (("x", "width"), ("y", "height")):
                    require(child_rect[axis] >= parent_rect[axis]-1 and child_rect[axis]+child_rect[extent] <= parent_rect[axis]+parent_rect[extent]+1,
                            f"Dialog {child} clipped by {parent} on {axis}")
            for before, after in (("header", "destination"), ("destination", "body"), ("body", "footer")):
                require(controls[before]["rect"]["y"] + controls[before]["rect"]["height"] <= controls[after]["rect"]["y"] + 1,
                        f"Dialog {before} overlaps {after}")
            body_scroll(row, "date" if name.endswith("-date") else "editor", viewport)
            if name.endswith("-date"):
                require(previous is not None and previous["stage"] == stage, "Date capture lost preceding editor capture")
                for field in ("current", "viewport", "documentWidth", "active", "text", "busy", "threadPaneAbsent"):
                    same(row[field], previous[field], "Body scrolling changed dialog identity/content/focus: " + field)
                for control_name in chrome:
                    for field in ("rect", "visible", "unobscured", "disabled", "text", "focused"):
                        same(row["controls"][control_name][field], previous["controls"][control_name][field], "Body scrolling changed stable chrome: " + control_name + "/" + field)
                same(row["bodyScroll"]["before"], previous["bodyScroll"]["after"], "Unrecorded body scroll between paired captures")
        else:
            same(row["current"], None, "Unexpected live dialog read")
            same(row["busy"], None, "Unexpected non-dialog busy state")
            same(row["bodyView"], None, "Nondialog capture claims body selection")
            same(row["bodyScroll"], None, "Nondialog capture claims body scrolling")
        previous = row


def body_scroll(row, requested, viewport):
    same(row["bodyView"], requested, "Wrong selected body surface")
    value = row["bodyScroll"]
    keys(value, {"requested", "before", "after"}, "actual body scroll")
    same(value["requested"], requested, "Wrong body scroll request")
    for when in ("before", "after"):
        sample = value[when]
        keys(sample, {"generation", "scrollTop", "scrollHeight", "clientHeight", "rect"}, "body scroll sample")
        same(sample["generation"], row["current"]["generation"], "Body scroll changed rich editor generation")
        require(rectangle(sample["rect"], viewport), "Body viewport clipped")
        same(sample["rect"], row["controls"]["body"]["rect"], "Body rectangle changed during scrolling")
        require(all(number(sample[k]) for k in ("scrollTop", "scrollHeight", "clientHeight")), "Nonfinite body scroll measurement")
        require(sample["clientHeight"] > 0 and sample["scrollHeight"] >= sample["clientHeight"]
                and -1 <= sample["scrollTop"] <= sample["scrollHeight"] - sample["clientHeight"] + 1, "Invalid body scroll extent")
        require(sample["clientHeight"] <= sample["rect"]["height"] + 1, "Body client height exceeds measured box")
    for field in ("generation", "scrollHeight", "clientHeight", "rect"):
        same(value["before"][field], value["after"][field], "Scroll changed body layout/identity: " + field)
    destination = 0 if requested == "editor" else value["after"]["scrollHeight"] - value["after"]["clientHeight"]
    require(abs(value["after"]["scrollTop"] - destination) <= 1, "Body did not reach requested top/bottom")


def validate_restart_pair(scenario, first, second):
    expected = copy.deepcopy(first["durable"])
    if scenario == "held-response-thread":
        item = expected["thread"]["stored"]
        item["revision"] += 1
        item["state"]["workflow"] = {"Uncertain": first["operation"]}
    same(second["initial"], expected, "Restart did not preserve exact paired durable dictionaries")
    same(second["durable"], expected, "Restart performed an unexpected write")
    same(second["attempts"], [], "Restart replayed Schedule")



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

def _validate(fixture, scenario, phase, nonce, pid, database, receipt):
    require(fixture in FIXTURES and scenario in SCENARIOS and phase in phases(scenario), "Unknown Schedule run")
    target = "thread" if scenario.endswith("thread") else "channel"
    viewport = expected_viewport(fixture)
    for name, expected in dict(ok=True, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce, pid=pid,
        isolatedDatabase=str(database), target=target, viewport=viewport, noSession=True, noTransport=True, noCache=True,
        noProductionRoute=True, syntheticInput=True, actualDialogControl=True, actualPreparedRequests=True, refusedInputs=0).items():
        same(receipt[name], expected, "Wrong terminal field: " + name)
    stale = scenario.startswith("stale-")
    held, lost = scenario.startswith("held-"), scenario.startswith("lost-")
    rejected = scenario in ("retry-channel", "expired-change-thread")
    actual_retry = scenario == "rejected-retry-channel"
    newer = scenario.startswith("newer-") or held or lost
    count = 0 if phase == "restart" or stale else 2 if actual_retry else 1
    same(receipt["preparedRequests"], count, "Wrong actual prepared count")
    require(isinstance(receipt["attempts"], list) and len(receipt["attempts"]) == count, "Request ledger missing/replayed")
    require(type(receipt["routedInputs"]) is int and receipt["routedInputs"] >= (1 if phase == "exercise" and (newer or actual_retry) else 0), "Actual newer input not routed")
    if phase == "restart" or stale: same(receipt["routedInputs"], 0, "No-op phase routed a local write")
    initial, durable, details = receipt["initial"], receipt["durable"], receipt["details"]
    notices = {}
    if phase == "exercise" and not stale and not held:
        notices["completionToast"] = UNCERTAIN_NOTICE if lost else SCHEDULED_NOTICE
        if actual_retry: notices["rejectionToast"] = REJECTED_NOTICE
    for field in ("completionToast", "rejectionToast"):
        if field in notices: dismissed_notice(details[field], viewport, notices[field])
        else: require(field not in details, "Unexpected notice dismissal: " + field)
    inventory_shape(initial)
    inventory_shape(durable)
    sibling = "channel" if target == "thread" else "thread"
    same(initial[sibling], seed(sibling), "Untouched sibling was not exact initial state")
    same(durable[sibling], initial[sibling], "Schedule changed unrelated draft")
    # This disconnected path has neither realtime nor chat projection. Exact bags
    # are intentional here, not an oracle for concurrent live Slack messages.
    message_shape = SEND["message_inventory"]
    message_shape(receipt["messagesInitial"])
    message_shape(receipt["messagesFinal"])
    same(receipt["messagesFinal"], receipt["messagesInitial"], "Schedule changed chat/pending delivery")
    same(receipt["queueInitial"], {}, "Unexpected initial remote inventory fetch")
    native(details["nativeInitial"], initial[target]["canonical"]["composer"]["text"], viewport)
    home_anchor(details["nativeInitial"])
    op, intent = receipt["operation"], receipt["consumption"]
    if phase == "restart":
        same(receipt["frozen"], None, "Restart fabricated current dialog")
        same(durable, initial, "Restart wrote or replayed")
        same(receipt["queueFinal"], {}, "Restart fabricated scheduled inventory")
        if stale:
            same(initial[target], seed(target), "Stale review persisted anything")
            same(op, None, "Stale restart retained operation")
            same(intent, None, "Stale restart retained consumption")
        else:
            state = initial[target]["stored"]["state"]
            if held or lost:
                operation(op, target, rich(target), op["kind"]["Schedule"]["post_at"])
                same(intent, consumption(op), "Restart lost frozen intent")
                action(details["blockedReplay"], viewport, True)
            else:
                same(op, None, "Confirmed restart fabricated in-flight operation")
                same(intent, None, "Confirmed restart fabricated intent")
                retained = state["local_completion"]["intent"]
                op = dict(id="01234567-89ab-4def-8123-456789abcdef", consume_id=retained["id"],
                    **{k: copy.deepcopy(v) for k, v in retained.items() if k != "id"})
                operation(op, target, rich(target, rejected, scenario == "expired-change-thread"), op["kind"]["Schedule"]["post_at"])
            expected = pending({which: seed(which) for which in ("channel", "thread")}, target, op)
            if rejected:
                expected[target]["canonical"] = local(NEWER, target)
                expected[target]["stored"]["state"].update(desired=snapshot(NEWER, target), local_revision=2)
            elif newer or actual_retry:
                expected = typed(expected, target)
            if actual_retry: expected[target]["stored"]["revision"] = 7
            if held or lost:
                expected = completed(expected, target, op, uncertain=True)
            else:
                expected = completed(expected, target, op, successor=state["client_msg_id"])
            same(initial, expected, "Restart full Schedule state differs")
        geometry(receipt, scenario, phase, viewport)
        return
    payload = rich(target, rejected, scenario == "expired-change-thread")
    frozen = receipt["frozen"]
    review(frozen, payload)
    post_at = frozen["postAt"]
    review(details["reviewInitial"], rich(target, True) if rejected else snapshot(BASE, target))
    action(details["open"], viewport, text="Review schedule" if rejected else None)
    initial_op = initial[target]["stored"]["state"]["workflow"].get("Rejected") if rejected else None
    if rejected:
        original_time = initial_op["kind"]["Schedule"]["post_at"]
        operation(initial_op, target, rich(target, True), original_time)
        same(initial[target], seed(target, initial_op), "Seeded rejection changed exact frozen state")
        review(details["reviewInitial"], rich(target, True), original_time)
        if scenario == "retry-channel": same(post_at, original_time, "Retry changed schedule time")
        else:
            require(post_at > original_time, "Change did not replace expired schedule time")
            action(details["expiredRetry"], viewport, True, "Retry same schedule")
            action(details["change"], viewport, False, "Change schedule")
    else:
        same(initial[target], seed(target), "Ordinary initial acknowledged seed changed")
    if stale:
        same(op, None, "Stale review dispatched an operation")
        same(intent, None, "Stale review consumed local draft")
        same(durable, initial, "Stale review persisted a write")
        same(receipt["queueFinal"], {}, "Stale review projected scheduled item")
        require("staleClick" not in details, "Retired dialog cannot claim a disabled submit click")
        retained, retired = details["staleRetained"], details["staleRetired"]
        keys(retained, {"ok", "generation", "dialogConnected", "submitConnected", "primary", "viewport", "documentWidth"}, "retained old dialog")
        viewport_proof(retained, viewport)
        for field in ("dialogConnected", "submitConnected"):
            same(retained[field], True, "Old dialog was not connected before retirement")
        same(retained["generation"], frozen["generation"], "Retained wrong reviewed dialog")
        proof(retained["primary"], viewport)
        same(retained["primary"]["disabled"], False, "Old reviewed submit was already disabled")
        same(retained["primary"]["text"], "Schedule message", "Wrong retained submit")
        keys(retired, {"ok", "generation", "dialogDisconnected", "submitDisconnected", "currentDialogAbsent"}, "retired old dialog")
        for field in ("ok", "dialogDisconnected", "submitDisconnected", "currentDialogAbsent"):
            same(retired[field], True, "Old dialog did not retire: " + field)
        same(retired["generation"], retained["generation"], "Retired wrong dialog generation")
        before, after = details["staleNativeBefore"], details["staleNativeAfter"]
        native(before, BASE, viewport)
        native(after, BASE, viewport)
        for field in ("scope", "visit"):
            same(before[field], details["nativeInitial"][field], "Pre-retirement composer unexpectedly remounted")
            require(after[field] != before[field], "Retired composer identity was reused: " + field)
        context = details["contextBefore"]
        keys(context, {"account", "epoch", "viewId", "team"}, "stale ownership context")
        same(context["account"], ACCOUNT, "Wrong stale account")
        same(context["team"], "T1", "Wrong stale team")
        require(all(type(context[k]) is int and context[k] >= 0 for k in ("epoch", "viewId")), "Wrong owner/visit counters")
        after = dict(context)
        after["epoch" if scenario == "stale-owner-channel" else "viewId"] += 1
        same(details["contextAfter"], after, "Stale case did not change intended owner/visit")
        geometry(receipt, scenario, phase, viewport)
        return
    operation(op, target, payload, post_at)
    same(intent, consumption(op), "Wrong actual frozen consumption")
    primary = "Save new schedule" if scenario == "expired-change-thread" else "Retry same schedule" if rejected else "Schedule message"
    action(details["submit"], viewport, text=primary)
    if rejected:
        require(op["id"] != initial_op["id"], "Explicit rejected retry reused operation ID")
        if scenario == "retry-channel": same(intent, consumption(initial_op), "Retry changed immutable intent")
        else: require(intent["id"] != initial_op["consume_id"], "Change did not replace immutable intent")
    first_op = details["firstOperation"] if actual_retry else op
    operation(first_op, target, payload, post_at)
    expected = pending(initial, target, first_op, rejected)
    same(details["pending"], expected, "Wrong exact consumed/prepared state")
    if actual_retry:
        require(first_op["id"] != op["id"], "Explicit Retry reused operation UUID")
        same(consumption(first_op), intent, "Explicit Retry changed frozen payload/time/base/client/intent")
        wire(receipt["attempts"][0], first_op, 1, "rejected")
        expected = completed(expected, target, first_op, rejected=True)
        same(details["afterRejection"], expected, "Raw denial did not persist exact Rejected state")
        native(details["nativeAfterRejection"], "", viewport)
        expected = typed(expected, target)
        same(details["afterRejectionTyping"], expected, "Typing after rejection lost separate local content")
        native(details["newerNative"], NEWER, viewport)
        action(details["retryOpen"], viewport, text="Review schedule")
        review(details["retryReview"], payload, post_at)
        action(details["retrySubmit"], viewport, text="Retry same schedule")
        expected[target]["stored"]["revision"] += 2
        expected[target]["stored"]["state"]["workflow"] = {"InFlight": op}
        same(details["retryPending"], expected, "Explicit Retry consumed local content or changed authority")
    elif newer:
        expected = typed(expected, target)
        same(details["afterNewer"], expected, "Held newer input did not persist exact state")
        native(details["newerNative"], NEWER, viewport)
    raw = wire(receipt["attempts"][-1], op, count, "pending" if held else "uncertain" if lost else "scheduled")
    if not held:
        expected = completed(expected, target, op, successor=durable[target]["stored"]["state"]["client_msg_id"], uncertain=lost)
    same(durable, expected, "Wrong complete final Schedule dictionary")
    if not held and not rejected:
        native(details["nativeAfter"], NEWER if newer or actual_retry else "", viewport)
        if actual_retry:
            native(details["retryHomeMount"], NEWER, viewport)
            home_anchor(details["retryHomeMount"])
            for field in ("text", "display", "codePoints", "utf16Length", "scope", "visit", "start", "end", "direction"):
                same(details["retryHomeMount"][field], details["nativeAfter"][field], "Retry remount changed before final read: " + field)
        if newer:
            for field in ("text", "display", "codePoints", "utf16Length", "scope", "visit", "start", "end", "direction"):
                same(details["nativeAfter"][field], details["newerNative"][field], "Completion rewrote newer native state: " + field)
    if lost: action(details["blockedReplay"], viewport, True)
    expected_queue = {} if held or lost else {op["base"]["id"]: {k: v for k, v in raw["draft"].items() if k != "is_from_composer"}}
    same(receipt["queueFinal"], expected_queue, "Schedule queue does not match actual acknowledgement")
    geometry(receipt, scenario, phase, viewport)


def validate(*args):
    try:
        return _validate(*args)
    except (KeyError, TypeError, ValueError, AttributeError, IndexError, OverflowError, OSError) as error:
        raise RuntimeError("Malformed managed Schedule receipt: " + str(error)) from error
