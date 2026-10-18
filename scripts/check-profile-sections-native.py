#!/usr/bin/env python3
"""Exercise offline section Save/Check/Keep at normal and narrow native sizes."""
from native_check import CheckSpec, main


SPEC = CheckSpec(
    fixture="profile-sections-save",
    flag="SUPER_PLATINUM_PROFILE_SECTIONS_CHECK",
    prefix="profile-sections-check: ",
    required=(
        "duplicateGuard", "confirmedSave", "rejectionRetainsDraft",
        "mismatchRetainsDraft", "repeatedCheckNoReplay", "explicitKeep",
        "canonicalReopen", "noSession", "noTransport", "stageLayouts",
        "layout.ok", "layout.managedFieldPreserved",
    ),
    prepared_requests=3,
    timeout=120,
)


if __name__ == "__main__":
    main(SPEC, __doc__)
