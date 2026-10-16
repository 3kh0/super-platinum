#!/usr/bin/env python3
"""Disconnected actual message attachment actions; never builds or sends."""
from canvas_native import CanvasCheck, main

SPEC = CanvasCheck(
    fixture="message-file-actions",
    flag="SUPER_PLATINUM_MESSAGE_FILE_ACTIONS_CHECK",
    required=("noRequests", "permissions", "exactCopyStub", "cancelPreserved",
              "offlinePreserved", "saveOfflinePreserved", "saveJournalEmpty", "accountReset", "threadIdentity", "deletedRefused"),
    private_files=(("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB", "saved-file-recovery.sqlite"),),
    counts={"stages": 7, "commands": 3},
    stages=("initial", "menu", "confirmation", "offline", "account-reset", "thread-menu", "deleted"),
)

if __name__ == "__main__":
    main(SPEC, __doc__)
