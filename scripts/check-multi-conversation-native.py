#!/usr/bin/env python3
"""Check ordinary and modal multi-conversation selection with retained native stage PNGs; never builds."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

MESSAGE_STAGES = ("initial", "maximum", "selected", "confirmation", "cleared", "uncertain")
MODAL_STAGES = ("maximum", "validation", "selected", "cleared", "uncertain")
MESSAGE_SPEC = CheckSpec(
    fixture="message-multi-conversations-select", flag="SUPER_PLATINUM_MESSAGE_MULTI_CONVERSATION_CHECK",
    prefix="message-multi-conversation-check: ", prepared_requests=4,
    required=("ordinaryFlagUsesInitial", "navigationDoesNotSynthesizeDefault", "compactSingleRowControls", "allConversationKinds", "filterEligibility", "memberIdForwarded", "confirmationHitTested", "initialAndAbsentState", "stagedCancelNoDispatch", "requiredAndMaximum",
              "searchSelectionRetained", "confirmationCancelNoCommit", "optionalClearArray",
              "pendingDuplicateRejected", "nextButtonCarriedFullState", "uncertainNoReplay",
              "noTransport", "noOverflow", "eachVisible", "hitTestedControls", "keyboardFocused"),
)
MODAL_SPEC = CheckSpec(
    fixture="app-modal-multi-conversation", flag="SUPER_PLATINUM_APP_MODAL_MULTI_CONVERSATION_CHECK",
    prefix="app-modal-multi-conversation-check: ", prepared_requests=2,
    required=("viewedOriginOverridesInitial", "navigationRetainsOpeningDefault",
              "initialAndEmptyState", "allConversationKinds", "memberIdForwarded", "requiredAndMaximum",
              "inlineEditsNoDispatch", "searchRetainsSelection", "remoteRetainsDirtyAndRefreshesPristine",
              "staleChoiceRejected", "escapeRetainsModal", "fullSiblingState", "pendingDuplicateRejected",
              "serverFieldErrorRetained", "uncertainNoReplay", "noTransport", "noOverflow",
              "eachVisible", "hitTestedControls", "keyboardFocused", "validationErrorVisible"),
)
FIXTURES = (MESSAGE_SPEC.fixture, MESSAGE_SPEC.fixture+"-narrow", MODAL_SPEC.fixture, MODAL_SPEC.fixture+"-narrow")


def run(fixture, binary, captures):
    spec, stages = (MESSAGE_SPEC, MESSAGE_STAGES) if fixture.startswith("message-") else (MODAL_SPEC, MODAL_STAGES)
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    for stage in stages:
        (captures / f"{fixture}-{stage}.png").unlink(missing_ok=True)
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary, {
            spec.flag: "1", "SUPER_PLATINUM_APP_MODAL_USERS_CAPTURE_DIR": str(captures),
            "SUPER_PLATINUM_MULTI_CONVERSATION_CAPTURE_DIR": str(captures),
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
