#!/usr/bin/env python3
"""Check disconnected native multi-user selection on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-multi-users-select",
    flag="SUPER_PLATINUM_MESSAGE_MULTI_USER_CHECK",
    prefix="message-multi-user-check: ",
    required=(
        "initialAndAbsentState", "stagedCancelNoDispatch", "requiredAndMaximum",
        "searchSelectionRetained", "pendingDuplicateRejected", "nextButtonCarriedFullState",
        "stalePickerRetired", "noTransport", "eachVisible", "noOverflow", "keyboardFocused",
        "hitTestedControls", "compactSingleRowControls",
    ),
    prepared_requests=3,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
