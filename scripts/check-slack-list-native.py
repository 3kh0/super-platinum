#!/usr/bin/env python3
"""Check isolated native Lists interactions/captures on Windows/macOS; never builds."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from slack_list_board_geometry import validate_geometry, require, flags, paint, inside, number, overlap, rectangle

STAGES = (
    "rich", "references", "hierarchy", "editor-rich", "editor-select",
    "saved-view-columns", "saved-view-groups", "saved-date-today", "saved-date-week",
    "board-multi", "board-paging", "board-completion", "board-header", "board-add-group",
    "detail-table", "detail-board",
    "detail-unavailable", "date-editor", "date-hidden-editor", "link-editor", "user-editor",
    "channel-editor", "list-created", "list-create-uncertain",
    "list-duplicated", "list-duplicate-uncertain",
    "guidance-locked", "guidance-stale", "guidance-restored",
)


def validate_header_captures(receipt, fixture, captures):
    for kind, stage in (("header", "board-header"), ("addGroup", "board-add-group")):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("boardViews", {}).get("headerCaptures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing focused board {kind} capture")
    proofs = receipt.get("boardViews", {}).get("headerCaptureGeometry", {})
    width, height = expected_viewport(fixture)
    viewport = dict(left=0, top=0, right=width, bottom=height)
    owner = None
    for kind in ("header", "addGroup"):
        proof = proofs.get(kind, {})
        flags(proof, "ok")
        require(proof.get("target") == kind and proof.get("viewport") == [width, height], "wrong focused capture target/viewport")
        current = proof.get("owner", {})
        require(current.get("list") == "F_LIST_FIXTURE" and current.get("view") == "V_BOARD_MULTI", "foreign focused capture owner")
        require(all(isinstance(current.get(k), str) and current[k].isdigit() for k in ("generation", "layout")), "missing focused capture generations")
        require(owner is None or current == owner, "focused capture owner changed")
        owner = current
        bounds = paint(proof, viewport)
        for key in ("board", "layout", "dialog"):
            offset = proof.get("scroll", {}).get(key)
            require(isinstance(offset, list) and len(offset) == 2 and all(number(v) and v >= 0 for v in offset), "invalid focused capture scroll")
        if kind == "addGroup":
            require(proof.get("text") == "Add group" and proof.get("title") is None and proof.get("actions") == [], "wrong focused Add group")
            require(inside(paint(proof.get("icon"), viewport, hit=False), bounds), "focused icon outside Add group")
        else:
            require(proof.get("icon") is None, "wrong focused header icon")
            title = proof.get("title", {})
            require(title.get("text") == "Engineering" and inside(paint(title, viewport), bounds), "wrong focused heading")
            actions = proof.get("actions", [])
            original = receipt.get("boardViews", {}).get("headerGeometry", {}).get("lanes", [{}])[0].get("actions", [])
            require(len(actions) == len(original) == 6, "missing focused actions")
            rects = []
            for actual, expected in zip(actions, original):
                require(all(actual.get(k) == expected.get(k) for k in ("text", "disabled", "choice", "delete", "move", "add", "laneKey")), "focused action identity changed")
                require(type(actual.get("disabled")) is bool, "invalid focused disabled state")
                r = paint(actual, viewport, hit=not actual["disabled"])
                require(inside(r, bounds) and not any(overlap(r, prior) for prior in rects), "focused actions outside header or overlapping")
                rects.append(r)



def validate_record_captures(receipt, fixture):
    """Check original record operands; a field inside an offscreen scroller fails."""
    width, height = expected_viewport(fixture)
    viewport = dict(left=0, top=0, right=width, bottom=height)

    def box(value):
        require(isinstance(value, dict) and all(number(value.get(k)) for k in
                ("left", "top", "right", "bottom")), "missing record client bounds")
        require(value["right"] > value["left"] and value["bottom"] > value["top"],
                "empty record client bounds")
        return value

    def contained(inner, outer):
        return (inner["left"] >= outer["left"] and inner["right"] <= outer["right"]
                and inner["top"] >= outer["top"] and inner["bottom"] <= outer["bottom"])

    proofs = receipt.get("recordDetails", {}).get("captureGeometry", {})
    for kind, selected in (("table", "Engineering"), ("board", "Design")):
        proof = proofs.get(kind, {})
        measured = proof.get("before")
        require(isinstance(measured, dict) and measured == proof.get("after"),
                "record capture changed across screenshot")
        flags(measured, "ok", "current", "originalBounds", "fullField", "fieldHit")
        require(measured.get("viewport") == [width, height], "wrong record viewport")
        identity = measured.get("identity", {})
        require(identity.get("record") == "R_LIST_002" and identity.get("column") == "C_MULTI"
                and all(isinstance(identity.get(k), str) and identity[k].isdigit()
                        for k in ("open", "action")), "wrong original record field")
        field = rectangle(measured.get("field"))
        scroll = measured.get("scroll", {})
        bounds, client = rectangle(scroll.get("rect")), box(scroll.get("client"))
        require(contained(field, client) and contained(field, viewport),
                "record field clipped by client or viewport")
        require(field["top"] >= bounds["top"] and field["bottom"] <= bounds["bottom"]
                and bounds["left"] >= 0 and bounds["right"] <= width,
                "original record bounds predicate failed")
        for axis, start, end in (("Width", "left", "right"), ("Height", "top", "bottom")):
            size = scroll.get("client" + axis)
            extent = scroll.get("scroll" + axis)
            require(number(size) and size > 0 and abs(size - client[end] + client[start]) < .01
                    and number(extent) and extent >= size, "invalid record scroll dimensions")
        require(all(number(scroll.get(k)) and scroll[k] >= 0 for k in ("scrollLeft", "scrollTop")),
                "invalid record scroll offsets")
        for key, actual in (("top", field["top"] - client["top"]),
                            ("bottom", client["bottom"] - field["bottom"]),
                            ("left", field["left"] - client["left"]),
                            ("right", client["right"] - field["right"])):
            value = measured.get("deltas", {}).get(key)
            require(number(value) and value >= 0 and abs(value - actual) < .000001,
                    "forged record field clearance")
        for key, text in (("label", "Teams"), ("value", selected)):
            value = measured.get(key, {})
            require(value.get("text") == text and contained(rectangle(value.get("rect")), field),
                    "record label or selected value lost")
        edit = measured.get("edit", {})
        require(edit.get("disabled") is False and contained(rectangle(edit.get("rect")), field),
                "record original edit unavailable or outside field")
        header = proof.get("header", {})
        flags(header, "restored")
        for key, text, aria in (("refresh", "Refresh", None), ("close", "", "Close list")):
            control = header.get(key, {})
            flags(control, "ok", "bounded", "contained", "hit", "separate", "focused")
            require(control.get("id") == "slack-list-" + key and control.get("text") == text
                    and control.get("ariaLabel") == aria and control.get("disabled") is False,
                    "wrong original record header control")
            r, other = rectangle(control.get("rect")), rectangle(control.get("otherRect"))
            rectangle(control.get("header"))
            clip = box(control.get("headerClient"))
            require(contained(r, viewport) and contained(r, clip),
                    "record header control clipped")
            require(r["right"] <= other["left"] or other["right"] <= r["left"]
                    or r["bottom"] <= other["top"] or other["bottom"] <= r["top"],
                    "record header controls overlap")
            offsets = control.get("scroll")
            require(isinstance(offsets, list) and len(offsets) == 2
                    and all(number(v) and v >= 0 for v in offsets), "invalid revealed header scroll")


def validate_editor_hover(receipt, fixture):
    """Verify original editor custody around real background-hover attempts."""
    width, height = expected_viewport(fixture)
    viewport = dict(left=0, top=0, right=width, bottom=height)

    def contained(inner, outer):
        return (inner["left"] >= outer["left"] and inner["right"] <= outer["right"]
                and inner["top"] >= outer["top"] and inner["bottom"] <= outer["bottom"])

    def snapshot(value, query, selected):
        require(isinstance(value, dict), "missing original editor snapshot")
        flags(value, "sameRoot", "sameInput")
        require(all(isinstance(value.get(k), str) and value[k].isdigit()
                    for k in ("generation", "dialogGeneration")), "missing original editor generation")
        require(value.get("viewport") == dict(width=width, height=height),
                "wrong editor hover viewport")
        require(value.get("query") == query and value.get("selected") == selected,
                "editor query or selected identity changed")
        start, end = value.get("selectionStart"), value.get("selectionEnd")
        require(type(start) is int and type(end) is int and 0 <= start <= end <= len(query),
                "invalid original editor selection")
        root = rectangle(value.get("root"))
        require(contained(root, viewport), "editor root clipped by viewport")
        controls = value.get("controls", {})
        bounds = []
        for key, label in (("cancel", "Cancel"), ("save", "Save")):
            control = controls.get(key, {})
            require(control.get("id") == "slack-list-editor-" + key
                    and control.get("text") == label and control.get("disabled") is False,
                    "original editor control changed or disabled")
            flags(control, "centerHit")
            rect = rectangle(control.get("rect"))
            require(contained(rect, root) and contained(rect, viewport),
                    "editor control clipped by root or viewport")
            bounds.append(rect)
        a, b = bounds
        require(a["right"] <= b["left"] or b["right"] <= a["left"]
                or a["bottom"] <= b["top"] or b["bottom"] <= a["top"],
                "editor Cancel and Save overlap")

    for domain, user, label, query, selected, initial in (
        ("user", "U00000001", "Alice Example", "Whitney",
         ["U00000001", "W00000003"], ["U00000001"]),
        ("channel", "U1", "Maya Chen", "gamma",
         ["C00000001", "C00000003"], ["C00000001"]),
    ):
        proof = receipt.get(domain + "Edits", {}).get("hover", {})
        for key in ("preHover", "afterClose"):
            hover = proof.get(key, {})
            require(hover.get("user") == user and hover.get("label") == label
                    and hover.get("visible") is True, "missing original hover or re-enable")
        blocked = proof.get("blockedEditor", {})
        flags(blocked, "hoverAbsent", "sourceConnected")
        require(type(blocked.get("observedMs")) is int and blocked["observedMs"] >= 500,
                "missing bounded hover observation")
        for key, expected_query, expected_selected in (
            ("blockedEditor", "", initial), ("capture", query, selected),
        ):
            pair = proof.get(key, {})
            before = pair.get("before")
            require(isinstance(before, dict) and before == pair.get("after"),
                    "original editor changed during hover or capture")
            snapshot(before, expected_query, expected_selected)
        require(proof["blockedEditor"]["before"]["dialogGeneration"]
                == proof["capture"]["before"]["dialogGeneration"],
                "editor capture moved to a successor List dialog")


def validate_receipt(receipt, fixture, captures):
    if receipt.get("ok") is not True:
        raise RuntimeError(f"{fixture}: native assertion failed: {receipt}")
    for stage, proof in (("table", receipt.get("horizontal", {})),
                         ("reordered", receipt.get("savedViews", {}).get("reorderedHorizontal", {}))):
        for flag in ("compactColumns", "primaryPinned", "opaqueFocusState", "syntheticNavigationHitTest", "homeRestoresLeft"):
            if proof.get(flag) is not True:
                raise RuntimeError(f"{fixture}: missing {stage} horizontal proof: {flag}")
    if receipt.get("savedViews", {}).get("hiddenPrimaryUnpinned") is not True:
        raise RuntimeError(f"{fixture}: hidden primary kept stale scroll clearance")
    for surface, proof in (("table", receipt.get("keyboardHelp", {})),
                           ("board", receipt.get("boardViews", {}).get("keyboardHelp", {})),
                           ("record", receipt.get("recordDetails", {}).get("keyboardHelp", {}))):
        for flag in ("collapsedAccessible", "descriptionPreserved", "nativeDisclosure", "expandedBounded", "closeRestoresContent", "focusRestored"):
            if proof.get(flag) is not True:
                raise RuntimeError(f"{fixture}: missing {surface} keyboard help proof: {flag}")
    for stage in ("rich", "references", "hierarchy"):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("captures", {}).get(stage) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing {stage} capture")
    for phase in ("locked", "stale", "restored"):
        path = captures / f"{fixture}-guidance-{phase}.png"
        if receipt.get("guidanceCaptures", {}).get(phase) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing {phase} guidance capture")
    for kind in ("rich", "select"):
        path = captures / f"{fixture}-editor-{kind}.png"
        if receipt.get("cellEditors", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing {kind} editor capture")
    for kind, stage in (("columns", "saved-view-columns"), ("groups", "saved-view-groups")):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("savedViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing saved view {kind} capture")
    for kind, stage in (("today", "saved-date-today"), ("week", "saved-date-week")):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("dateViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing date view {kind} capture")
    for flag in ("timestampRelativeLabel", "timestampTooltip", "timestampMinuteReprojection", "timestampReadOnly", "noWrites"):
        if receipt.get("dateViews", {}).get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing timestamp display proof: {flag}")
    for kind, stage in (("multi", "board-multi"), ("paging", "board-paging"), ("completion", "board-completion")):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("boardViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing board view {kind} capture")
    validate_header_captures(receipt, fixture, captures)
    if receipt.get("recordDetails", {}).get("refreshPreservesPanelPageFocus") is not True:
        raise RuntimeError(f"{fixture}: record detail refresh stability was not verified")
    for kind in ("table", "board", "unavailable"):
        path = captures / f"{fixture}-detail-{kind}.png"
        if receipt.get("recordDetails", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing record detail {kind} capture")
    validate_record_captures(receipt, fixture)
    for kind, stage in (("editor", "date-editor"), ("hidden", "date-hidden-editor")):
        date_editor = captures / f"{fixture}-{stage}.png"
        if receipt.get("dateEdits", {}).get("captures", {}).get(kind) != str(date_editor) or not date_editor.is_file():
            raise RuntimeError(f"{fixture}: missing date {kind} capture")
    for flag in ("cancelNoWrite", "cancelPendingNormalization", "uncontrolledDrafts", "normalizedSave", "rename", "displayModes", "removeLink", "unsafeStorageInert", "duplicateLocked", "keyboardFocus", "boundedLayout", "cleanup"):
        if receipt.get("linkEdits", {}).get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing Link editor proof: {flag}")
    link_editor = captures / f"{fixture}-link-editor.png"
    if receipt.get("linkEdits", {}).get("captures", {}).get("editor") != str(link_editor) or not link_editor.is_file():
        raise RuntimeError(f"{fixture}: missing Link editor capture")
    user_editor = captures / f"{fixture}-user-editor.png"
    if receipt.get("userEdits", {}).get("captures", {}).get("editor") != str(user_editor) or not user_editor.is_file():
        raise RuntimeError(f"{fixture}: missing people editor capture")
    for flag in ("candidateFilters", "guestAndExternal", "typingAndCancelNoWrite", "inputSurvivesResultRender", "singleReplace", "clear", "uncachedRemoval", "multiSelection", "taskAssigneeReadOnly", "keyboardEntryAndFocus", "duplicateLocked", "canonicalWrites", "cleanup"):
        if receipt.get("userEdits", {}).get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing people editor verification {flag}")
    channel_editor = captures / f"{fixture}-channel-editor.png"
    if receipt.get("channelEdits", {}).get("captures", {}).get("editor") != str(channel_editor) or not channel_editor.is_file():
        raise RuntimeError(f"{fixture}: missing channel editor capture")
    for flag in ("candidateFilters", "privateAndExternal", "typingAndCancelNoWrite", "inputSurvivesResultRender", "singleReplace", "clear", "uncachedRemoval", "multiSelection", "keyboardEntryAndFocus", "duplicateLocked", "canonicalWrites", "cleanup"):
        if receipt.get("channelEdits", {}).get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing channel editor verification {flag}")
    validate_editor_hover(receipt, fixture)
    for kind, stage in (("created", "list-created"), ("uncertain", "list-create-uncertain")):
        path = captures / f"{fixture}-{stage}.png"
        if receipt.get("creation", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing creation {kind} capture")
    duplicate = receipt.get("duplication", {})
    for flag in ("canonicalAdjacentPosition", "serverOwnedFields", "newServerIdentity",
                 "singleAttempt", "pendingMutationLocks", "canonicalReadback",
                 "uncertainNoReplay", "explicitRetirement", "noTransport", "cleanup"):
        if duplicate.get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing Duplicate item proof: {flag}")
    if duplicate.get("preparedRequests") != 2:
        raise RuntimeError(f"{fixture}: incorrect Duplicate item request count")
    for flag in ("hitTestedControls", "bodyReachable"):
        if duplicate.get("geometry", {}).get(flag) is not True:
            raise RuntimeError(f"{fixture}: missing Duplicate item geometry: {flag}")
    for kind, stage in (("duplicated", "list-duplicated"), ("uncertain", "list-duplicate-uncertain")):
        path = captures / f"{fixture}-{stage}.png"
        if duplicate.get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing Duplicate item {kind} capture")
    viewport = receipt.get("layout", {}).get("viewport")
    if viewport != expected_viewport(fixture):
        raise RuntimeError(f"{fixture}: incorrect native viewport: {viewport}")
    validate_geometry(receipt, expected_viewport(fixture))


def run_fixture(fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    # These exact output files belong to this check. An old stage PNG must not
    # satisfy this process's capture contract if the native capture stops running.
    for stage in STAGES:
        (captures / f"{fixture}-{stage}.png").unlink(missing_ok=True)
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary, {
            "SUPER_PLATINUM_SLACK_LIST_CHECK": "1",
            "SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR": str(captures),
        }) as native:
            evidence["pid"] = native.process.pid
            try:
                # This suite includes 29 bounded native stage captures. Windows
                # starts PowerShell for each capture; retain a finite allowance
                # for those real child processes as well as the interactions.
                deadline = time.monotonic() + 240
                last_focus = 0.0
                while True:
                    native.require_running()
                    receipts = native.receipts("slack-list-check: ")
                    if receipts:
                        for receipt in receipts:
                            evidence["native"] = receipt
                            validate_receipt(receipt, fixture, captures)
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{fixture}: native assertions timed out")
                    if time.monotonic() - last_focus >= 1:
                        native.focus()
                        last_focus = time.monotonic()
                    time.sleep(.1)
                screenshot = captures / f"{fixture}.png"
                native.capture(screenshot)
                evidence.update(ok=True, screenshot=str(screenshot))
            except BaseException:
                try:
                    screenshot = captures / f"{fixture}-failure.png"
                    native.capture(screenshot)
                    evidence["failure_screenshot"] = str(screenshot)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
    except BaseException as error:
        evidence.update(ok=False, error=str(error))
        raise
    finally:
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), **evidence}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path,
                        default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append", choices=("slack-list", "slack-list-narrow"),
                        help="run only this fixture (repeatable); default runs both")
    args = parser.parse_args()
    for fixture in args.fixture or ("slack-list", "slack-list-narrow"):
        run_fixture(fixture, args.binary, args.captures.resolve())


if __name__ == "__main__":
    main()
