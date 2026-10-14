#!/usr/bin/env python3
"""Check disconnected native user selection on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-users-select",
    flag="SUPER_PLATINUM_MESSAGE_USER_CHECK",
    prefix="message-user-check: ",
    required=(
        "compactSingleRowControls", "compactSearch", "initialAndAbsentState",
        "candidatePolicy", "searchCancelNoDispatch", "keyboardResultNavigation",
        "confirmationCancelNoCommit", "pendingDuplicateRejected",
        "nextButtonCarriedFullState", "navigationPickerRetired",
        "navigationCommittedUsersRetained", "staleConfirmationRetired",
        "keyboardFocused", "eachVisible", "noOverflow", "noTransport",
    ),
    prepared_requests=4,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
