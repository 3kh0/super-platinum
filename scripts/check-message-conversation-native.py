#!/usr/bin/env python3
"""Check disconnected native conversation selection on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-conversations-select",
    flag="SUPER_PLATINUM_MESSAGE_CONVERSATION_CHECK",
    prefix="message-conversation-check: ",
    required=(
        "compactSingleRowControls", "compactSearch", "initialAndAbsentState",
        "candidatePolicy", "searchCancelNoDispatch", "keyboardResultNavigation",
        "confirmationCancelNoCommit", "pendingDuplicateRejected",
        "nextButtonCarriedFullState", "navigationPickerRetired",
        "navigationCommittedUsersRetained", "staleConfirmationRetired",
        "keyboardFocused", "eachVisible", "noOverflow", "noTransport",
        "memberIdForwarded", "publicNonmember", "externalFilter", "noDmOpen",
    ),
    prepared_requests=4,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
