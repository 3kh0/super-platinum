#!/usr/bin/env python3
"""Check disconnected native channel selection on Windows/macOS; never builds."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="message-channels-select",
    flag="SUPER_PLATINUM_MESSAGE_CHANNEL_CHECK",
    prefix="message-channel-check: ",
    required=(
        "compactSingleRowControls", "compactSearch", "initialAndAbsentState",
        "candidatePolicy", "searchCancelNoDispatch", "keyboardResultNavigation",
        "confirmationCancelNoCommit", "pendingDuplicateRejected",
        "nextButtonCarriedFullState", "navigationPickerRetired",
        "navigationCommittedUsersRetained", "staleConfirmationRetired",
        "keyboardFocused", "eachVisible", "noOverflow", "noTransport",
        "publicChannelsOnly", "publicNonmember", "externalPublicIncluded", "noJoinOrDmOpen",
    ),
    prepared_requests=4,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
