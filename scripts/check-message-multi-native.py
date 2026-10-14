#!/usr/bin/env python3
"""Check disconnected native checkbox/multi-select on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-multi-controls",
    flag="SUPER_PLATINUM_MESSAGE_MULTI_CHECK",
    prefix="message-multi-check: ",
    required=(
        "largePickerScrollAndFocus", "absentAndEmptyDefaults", "untouchedAbsentApply",
        "checkboxConfirmation", "checkboxRetainedRaw", "emptyRemount", "multiCancel",
        "maxSelected", "integratedConfirmation", "pendingDuplicateRejected",
        "rawSiblingState", "initialDisplayNotWire", "navigationDraftRetired",
        "stalePickerRetired", "accountPickerRetired", "retainedSelectionRemoval",
        "noTransport", "layout.ok",
    ),
    prepared_requests=10,
    viewport_path="layout.viewport",
)

if __name__ == "__main__":
    main(SPEC, __doc__)
