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

STAGES = (
    "rich", "references", "hierarchy", "editor-rich", "editor-select",
    "saved-view-columns", "saved-view-groups", "saved-date-today", "saved-date-week",
    "board-multi", "board-paging", "board-completion", "detail-table", "detail-board",
    "detail-unavailable", "date-editor", "date-hidden-editor", "link-editor", "user-editor",
    "channel-editor", "list-created", "list-create-uncertain",
    "list-duplicated", "list-duplicate-uncertain",
)


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
    if receipt.get("recordDetails", {}).get("refreshPreservesPanelPageFocus") is not True:
        raise RuntimeError(f"{fixture}: record detail refresh stability was not verified")
    for kind in ("table", "board", "unavailable"):
        path = captures / f"{fixture}-detail-{kind}.png"
        if receipt.get("recordDetails", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
            raise RuntimeError(f"{fixture}: missing record detail {kind} capture")
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
                # This suite includes 24 bounded native stage captures. Windows
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
