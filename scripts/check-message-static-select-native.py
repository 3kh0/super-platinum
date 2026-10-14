#!/usr/bin/env python3
"""Check disconnected native static selection on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-static-select",
    flag="SUPER_PLATINUM_MESSAGE_SELECT_CHECK",
    prefix="message-select-check: ",
    required=(
        "cancelPreservedSelectionAndNode", "pendingDuplicateRejected",
        "nextButtonCarriedFullState", "navigationRemountRetainedSelections",
        "optionsEditCancelledConfirmation", "eachVisible", "noOverflow", "noTransport",
    ),
    prepared_requests=3,
    timeout=60,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
