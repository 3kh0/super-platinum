#!/usr/bin/env python3
"""Named disconnected shared rich-editor link controls; never builds or sends.

Lifecycle is the existing canvas_native.run contract, with strict actual prepared
forms and per-stage geometry validated before ACK and again on the closed log.
"""
import argparse
import copy
import json
import math
import os
from pathlib import Path
import sys
import tempfile
import time

from canvas_native import CanvasCheck, capture_stage, private_checks, validate, validate_closed
from canvas_evidence import audited_fixture, closed_receipts, png_signature
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

SPEC = CanvasCheck(
    fixture="forward-links", flag="SUPER_PLATINUM_RICH_LINKS_CHECK",
    required=("noRequests", "create", "edit", "remove", "undoRedo", "cancel",
              "selectionRestored", "invalidRefused", "canvasParagraph", "inlineCode", "inlineCodeRemove", "inlineCodeUndoRedo"),
    counts={"stages": 9, "preparedRequests": 5},
    stages=("initial", "create-form", "created", "edit-form", "edited", "removed", "cancelled", "coded", "uncoded"),
)


def expected_blocks(url=None, code=False):
    leaf = {"type": "text", "text": "Worth a look \U0001f680", "style": {"bold": True}}
    if code:
        leaf["style"]["code"] = True
    if url is not None:
        leaf.update(type="link", url=url)
    return [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [
        leaf, {"type": "text", "text": "\nThe original conversation stays attached."}]}]}]


def validate_geometry(fixture, layout, stage):
    if layout.get("ok") is not True or layout.get("viewport") != expected_viewport(fixture):
        raise RuntimeError("Link stage viewport/layout differs")
    opened = stage in ("create-form", "edit-form")
    if layout.get("panelOpen") is not opened:
        raise RuntimeError("Link form visibility differs")
    expected = (["rich-link-url", "rich-link-apply", "rich-link-cancel"] +
                (["rich-link-remove"] if stage == "edit-form" else [])) if opened else ["composer-icon-btn rich-link-open", "composer-icon-btn rich-code-toggle"]
    controls = layout.get("controls", [])
    if [item.get("class") for item in controls] != expected:
        raise RuntimeError("Link controls differ")
    if opened and layout.get("focus") != "rich-link-url":
        raise RuntimeError("Link URL does not own focus")
    width, height = expected_viewport(fixture)
    dialog = layout.get("dialog", {})
    if set(dialog) != {"left", "right", "top", "bottom", "width", "height"} or any(type(v) not in (int, float) or not math.isfinite(v) for v in dialog.values()):
        raise RuntimeError("Missing dialog bounds")
    body = layout.get("body", {})
    if set(body) != {"left", "right", "top", "bottom", "width", "height"} or any(type(v) not in (int, float) or not math.isfinite(v) for v in body.values()):
        raise RuntimeError("Missing editor scrollport bounds")
    for item in controls:
        r = item.get("rect", {})
        if (item.get("hit") is not True or set(r) != {"left", "right", "top", "bottom", "width", "height"}
                or any(type(v) not in (int, float) or not math.isfinite(v) for v in r.values())
                or not (0 <= r["left"] < r["right"] <= width and 0 <= r["top"] < r["bottom"] <= height)
                or not (dialog["left"] <= r["left"] and r["right"] <= dialog["right"] and dialog["top"] <= r["top"] and r["bottom"] <= dialog["bottom"])
                or not (body["left"] <= r["left"] and r["right"] <= body["right"] and body["top"] <= r["top"] and r["bottom"] <= body["bottom"])
                or r["width"] <= 0 or r["height"] <= 0):
            raise RuntimeError("Link control is clipped, hidden or obstructed")


def validate_links(fixture, receipt):
    validate(SPEC, fixture, receipt)
    validate_geometry(fixture, receipt["layout"], "uncoded")
    prepared = receipt.get("prepared")
    if not isinstance(prepared, list) or len(prepared) != 5:
        raise RuntimeError("Five prepared notes required")
    ids = set()
    source = None
    for item, (url, code) in zip(prepared, (("https://example.com/guide?q=1#note", False), ("https://example.com/updated", False), (None, False), (None, True), (None, False))):
        blocks = expected_blocks(url, code)
        if (any(item.get(k) is not True for k in ("ok", "sourceKept", "canvasParagraph"))
                or item.get("blocks") != blocks or type(item.get("preparedFields")) is not int
                or item["preparedFields"] != 7):
            raise RuntimeError("Typed note, source or Canvas paragraph differs")
        form = item.get("prepared")
        if (not isinstance(form, list) or len(form) != 7
                or any(not isinstance(pair, list) or len(pair) != 2 or any(not isinstance(s, str) for s in pair) for pair in form)
                or [pair[0] for pair in form] != ["token", "channel", "timestamp", "share_channel", "client_msg_id", "text", "blocks"]):
            raise RuntimeError("Actual prepared form differs")
        values = dict(form)
        if (values["token"] != "" or values["text"] != "" or values["share_channel"] != "C_LIST_NOTE_FIXTURE"
                or not values["channel"] or not values["timestamp"] or not values["client_msg_id"]
                or json.loads(values["blocks"]) != blocks):
            raise RuntimeError("Actual prepared payload differs")
        current = (values["channel"], values["timestamp"])
        if source is not None and source != current:
            raise RuntimeError("Forward source changed")
        source = current
        if values["client_msg_id"] in ids:
            raise RuntimeError("Prepared note identity reused")
        ids.add(values["client_msg_id"])

def run(spec, fixture, binary, captures):
    captures.mkdir(parents=True, exist_ok=True)
    # Preserve older evidence; a new run needs new names/directory.
    names = [f"{fixture}.log", f"{fixture}.json", f"{fixture}.png",
             f"{fixture}-failure.png", *[f"{fixture}-{s}.png" for s in spec.stages]]
    if any((captures / name).exists() for name in names):
        raise RuntimeError("Refusing to overwrite existing Canvas evidence")
    evidence = {"fixture": fixture, "platform": sys.platform, "binary": str(binary),
                "ok": False, "stages": []}
    native = None
    try:
        with tempfile.TemporaryDirectory(prefix="sp-canvas-ack-") as directory:
            acknowledgments = Path(directory)
            checks = private_checks(spec, acknowledgments)
            native = NativeFixture(fixture, captures / f"{fixture}.log", binary, checks)
            with audited_fixture(native, evidence) as audit:
                deadline = time.monotonic() + 180
                seen = 0
                sampled = 0.
                while True:
                    native.require_running()
                    if time.monotonic() - sampled >= .2:
                        audit.checkpoint("running")
                        sampled = time.monotonic()
                    stages = native.receipts(f"{spec.fixture}-stage: ")
                    for stage in stages[seen:]:
                        validate_geometry(fixture, stage["layout"], stage["stage"])
                        evidence["stages"].append(capture_stage(
                            native, spec, fixture, stage, seen, captures, acknowledgments, audit=audit))
                        seen += 1
                    receipts = native.receipts(f"{spec.fixture}-check: ")
                    if receipts:
                        evidence["terminalReceipts"] = receipts
                        if len(receipts) != 1:
                            raise RuntimeError("Expected exactly one final canvas receipt")
                        evidence["native"] = receipts[0]
                        validate_links(fixture, receipts[0])
                        if seen != len(spec.stages):
                            raise RuntimeError("Canvas check ended before all intermediate captures")
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(f"{fixture}: no final canvas receipt; see {native.log}")
                    time.sleep(.05)
                screenshot = captures / f"{fixture}.png"
                audit.capture(screenshot, "final")
                evidence["screenshot"] = str(screenshot)
            validate_closed(spec, fixture, native.log, evidence["stages"], evidence["native"])
            validate_links(fixture, evidence["native"])
            for stage in evidence["stages"]:
                validate_geometry(fixture, stage["layout"], stage["stage"])
            evidence["closedLogRevalidated"] = True
            for capture in evidence["captures"]:
                png_signature(Path(capture["path"]), evidence.get("expectedPngDimensions"))
            evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        # Retain the actual failed terminal/error and late records on failure too.
        if native is not None and native.log.exists():
            try:
                evidence["closedStageReceipts"] = closed_receipts(native.log, spec.fixture + "-stage: ")
                evidence["closedTerminalReceipts"] = closed_receipts(native.log, spec.fixture + "-check: ")
            except Exception as error:
                evidence.update(ok=False, closedLogError=f"{type(error).__name__}: {error}")
        path = captures / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    if not evidence["ok"]:
        raise RuntimeError("Final Canvas closed-log audit failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/forward-links")
    parser.add_argument("--fixture", action="append", choices=(SPEC.fixture, SPEC.fixture + "-narrow"))
    args = parser.parse_args()
    for fixture in args.fixture or (SPEC.fixture, SPEC.fixture + "-narrow"):
        run(SPEC, fixture, args.binary.resolve(), args.captures.resolve())

if __name__ == "__main__":
    main()
