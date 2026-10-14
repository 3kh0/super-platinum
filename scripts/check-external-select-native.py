#!/usr/bin/env python3
"""Check ordinary and modal single external option selection with retained native stage PNGs; never builds."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

MESSAGE_STAGES = ("initial", "short-query", "loading", "groups", "confirmation", "selected", "error", "uncertain")
MODAL_STAGES = ("initial", "short-query", "loading", "groups", "selected", "cleared", "validation", "error", "uncertain")
COMMON = ("exactRawWire", "heldLoading", "minimumAndDebounce", "groupOrder", "initialOutsideResults",
          "selectionSurvivedEmptyResults", "uncertainNoReplay", "cooldownExpiryWithoutReplay", "cooldownAcrossVisits",
          "fullSiblingState", "noTransport", "noOverflow", "eachVisible", "hitTestedControls", "compactSearch")
MESSAGE_SPEC = CheckSpec(fixture="message-external-select", flag="SUPER_PLATINUM_EXTERNAL_SELECT_CHECK",
    prefix="external-select-check: ", prepared_requests=2,
    required=COMMON + ("confirmationCancelNoCommit",))
MODAL_SPEC = CheckSpec(fixture="app-modal-external-select", flag="SUPER_PLATINUM_EXTERNAL_SELECT_CHECK",
    prefix="external-select-check: ", prepared_requests=2,
    required=COMMON + ("inlineNoDispatch", "optionalClear", "closeBesideSearch", "focusRestoredAfterDoneAndEscape"))
FIXTURES = (MESSAGE_SPEC.fixture, MESSAGE_SPEC.fixture+"-narrow", MODAL_SPEC.fixture, MODAL_SPEC.fixture+"-narrow")


def validate_attempts(fixture, receipt):
    modal = fixture.startswith("app-modal-")
    attempts = receipt.get("attempts")
    if not isinstance(attempts, list) or len(attempts) != 2:
        raise RuntimeError("Missing actual single-external request ledger")
    for index, attempt in enumerate(attempts):
        try:
            state = attempt["state"]
            encoded = json.loads(attempt["fields"]["state"])
            if encoded != (state if modal else {"values": state}):
                raise ValueError("Encoded state differs from retained state")
            values = state["values"] if modal else state
            selected = values["choice"]["pick"]["selected_option"]
            expected = None if modal and index == 0 else {
                "text": {"type": "plain_text", "text": "Release Beta" if modal else "Release Alpha"},
                "value": "beta" if modal else "alpha,desktop",
                "description": {"type": "plain_text", "text": "App-provided release option"},
            }
            if selected != expected:
                raise ValueError("Raw selected option changed")
            if modal:
                if values["note"]["value"]["value"] != "Keep this sibling":
                    raise ValueError("Modal sibling changed")
            else:
                if values["review"]["environment"]["selected_option"]["value"] != "prod":
                    raise ValueError("Message sibling changed")
                action = attempt["action"]
                if json.loads(attempt["fields"]["actions"]) != [action]:
                    raise ValueError("Encoded action differs from retained action")
                if action["type"] != ("external_select" if index == 0 else "button"):
                    raise ValueError("Wrong action order")
                if index == 0 and action["selected_option"] != expected:
                    raise ValueError("Action selected option differs from state")
        except (KeyError, TypeError, ValueError) as error:
            raise RuntimeError(f"Invalid single-external request {index}: {error}") from error


def run(fixture, binary, captures):
    spec, stages = (MESSAGE_SPEC, MESSAGE_STAGES) if fixture.startswith("message-") else (MODAL_SPEC, MODAL_STAGES)
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    for stage in stages:
        (captures / f"{fixture}-{stage}.png").unlink(missing_ok=True)
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary, {
            spec.flag: "1", "SUPER_PLATINUM_EXTERNAL_CAPTURE_DIR": str(captures),
        }) as native:
            evidence["pid"] = native.process.pid
            try:
                deadline, last_focus = time.monotonic() + 120, 0.0
                while True:
                    native.require_running()
                    receipts = native.receipts(spec.prefix)
                    if receipts:
                        for receipt in receipts:
                            evidence["native"] = receipt
                            validate_receipt(spec, fixture, receipt)
                            validate_attempts(fixture, receipt)
                            for stage in stages:
                                path = captures / f"{fixture}-{stage}.png"
                                if receipt.get("captures", {}).get(stage) != str(path) or not path.is_file():
                                    raise RuntimeError(f"{fixture}: missing {stage} capture")
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
                    path = captures / f"{fixture}-failure.png"
                    native.capture(path)
                    evidence["failure_screenshot"] = str(path)
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_UI_CAPTURE_DIR", ROOT / "tmp/agent-ui")))
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary, args.captures.resolve())
