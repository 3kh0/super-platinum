#!/usr/bin/env python3
"""Check disconnected native message overflow on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-overflow",
    flag="SUPER_PLATINUM_MESSAGE_OVERFLOW_CHECK",
    prefix="message-overflow-check: ",
    required=(
        "exactOptionPayload", "fullSiblingState", "confirmationCancelNoDispatch",
        "urlAfterConfirmation", "pendingDuplicateRejected", "uncertainNoReplay",
        "navigationRetiresMenuAndConfirmation", "staleConfirmationRetired",
        "keyboardMenu", "noTransport", "bounded", "reachable", "noOverflow",
    ),
    prepared_requests=4,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
