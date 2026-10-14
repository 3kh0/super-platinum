#!/usr/bin/env python3
"""Check bounded inline modal user selection and capture its native stages; never builds."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from native_check import CheckSpec, validate_receipt
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

STAGES = ("maximum", "validation", "selected", "uncertain")
SPEC = CheckSpec(
    fixture="app-modal-users", flag="SUPER_PLATINUM_APP_MODAL_USERS_CHECK",
    prefix="app-modal-users-check: ", prepared_requests=2,
    required=("initialAndEmptyState", "selfAndEligibility", "requiredAndMaximum",
              "inlineEditsNoDispatch", "searchRetainsSelection",
              "remoteRetainsDirtyAndRefreshesPristine", "staleChoiceRejected",
              "escapeRetainsModal", "fullSiblingState", "pendingDuplicateRejected",
              "serverFieldErrorRetained", "uncertainNoReplay", "noTransport",
              "noOverflow", "eachVisible", "hitTestedControls", "keyboardFocused",
              "validationErrorVisible"),
)
SINGLE_STAGES = ("selected", "cleared", "uncertain")
SINGLE_SPEC = CheckSpec(
    fixture="app-modal-user", flag="SUPER_PLATINUM_APP_MODAL_USER_CHECK",
    prefix="app-modal-user-check: ", prepared_requests=1,
    required=("initialAndEmptyState", "selfAndEligibility", "requiredCannotClear",
              "optionalClearNull", "singleChoiceCollapsesAndFocuses",
              "inlineEditsNoDispatch", "searchRetainsSelection",
              "remoteRetainsDirtyAndRefreshesPristine", "staleChoiceRejected",
              "escapeAndDoneRetainModal", "fullSiblingState", "pendingDuplicateRejected",
              "uncertainNoReplay", "noTransport", "noOverflow", "eachVisible",
              "hitTestedControls", "keyboardFocused"),
)

CONVERSATION_STAGES = ("initial", "selected", "cleared", "uncertain")
CONVERSATION_SPEC = CheckSpec(
    fixture="app-modal-conversation", flag="SUPER_PLATINUM_APP_MODAL_CONVERSATION_CHECK",
    prefix="app-modal-conversation-check: ", prepared_requests=1,
    required=("originDefaultAndSavedNull", "responseFlagPreserved", "allConversationKinds", "filterEligibility",
              "requiredCannotClear", "optionalClearNull", "singleChoiceCollapsesAndFocuses",
              "inlineEditsNoDispatch", "searchRetainsSelection", "remoteRetainsDirtyAndRefreshesPristine",
              "staleChoiceRejected", "escapeAndDoneRetainModal", "fullSiblingState", "pendingDuplicateRejected",
              "uncertainNoReplay", "noTransport", "noOverflow", "eachVisible", "hitTestedControls", "keyboardFocused"),
)

CHANNEL_SPEC = CheckSpec(
    fixture="app-modal-channel", flag="SUPER_PLATINUM_APP_MODAL_CHANNEL_CHECK",
    prefix="app-modal-channel-check: ", prepared_requests=1,
    required=("initialChannelWithoutOriginFallback", "responseFlagPreserved", "publicChannelsOnly",
              "excludesPrivateDmMpimArchived", "requiredCannotClear", "optionalClearNull",
              "singleChoiceCollapsesAndFocuses", "inlineEditsNoDispatch", "searchRetainsSelection",
              "remoteRetainsDirtyAndRefreshesPristine", "staleChoiceRejected", "escapeAndDoneRetainModal",
              "fullSiblingState", "pendingDuplicateRejected", "uncertainNoReplay", "noTransport",
              "noOverflow", "eachVisible", "hitTestedControls", "keyboardFocused"),
)


def run(fixture, binary, captures):
    single = fixture in ("app-modal-user", "app-modal-user-narrow")
    spec, stages = (SINGLE_SPEC, SINGLE_STAGES) if single else (SPEC, STAGES)
    if fixture in ("app-modal-conversation", "app-modal-conversation-narrow"):
        spec, stages = CONVERSATION_SPEC, CONVERSATION_STAGES
    if fixture in ("app-modal-channel", "app-modal-channel-narrow"):
        spec, stages = CHANNEL_SPEC, CONVERSATION_STAGES
    captures.mkdir(parents=True, exist_ok=True)
    evidence = {"fixture": fixture, "platform": sys.platform,
                "binary": str(binary.resolve()), "ok": False}
    for stage in stages:
        (captures / f"{fixture}-{stage}.png").unlink(missing_ok=True)
    try:
        with NativeFixture(fixture, captures / f"{fixture}.log", binary, {
            spec.flag: "1", "SUPER_PLATINUM_APP_MODAL_USERS_CAPTURE_DIR": str(captures),
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
    parser.add_argument("--fixture", action="append", choices=("app-modal-users", "app-modal-users-narrow", "app-modal-user", "app-modal-user-narrow", "app-modal-conversation", "app-modal-conversation-narrow", "app-modal-channel", "app-modal-channel-narrow"))
    args = parser.parse_args()
    for fixture in args.fixture or ("app-modal-users", "app-modal-users-narrow"):
        run(fixture, args.binary, args.captures.resolve())
