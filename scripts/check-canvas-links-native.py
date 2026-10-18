#!/usr/bin/env python3
"""Check native Canvas links and unsaved editor protection without Slack access."""
from native_check import CheckSpec, main

SPEC = CheckSpec(
    fixture="canvas-links",
    flag="SUPER_PLATINUM_CANVAS_LINKS_CHECK",
    prefix="canvas-links-check: ",
    required=("channelNavigated", "messageVisible", "replyVisible", "userNavigated",
              "workspaceNavigated", "rejectedLinksRetainedCanvas", "editorDraftPreserved",
              "externalAndMailtoRetained", "noTransport", "blankCredentials"),
    prepared_requests=0,
    timeout=120,
)

if __name__ == "__main__":
    main(SPEC, __doc__)
