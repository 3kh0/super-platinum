#!/usr/bin/env python3
"""Disconnected normal/narrow paragraph structure workflow."""
from canvas_native import CanvasCheck, main
SPEC = CanvasCheck(
    fixture="canvas-structure", flag="SUPER_PLATINUM_CANVAS_STRUCTURE_CHECK",
    required=("inserted", "deleteConfirmed", "cancelSafe", "duplicateSafe", "originalRestored", "stalePreserved", "uncertainRetained", "reopenBlocked"),
    counts={"attempts": 3},
    stages=("initial", "insert-draft", "insert-saving", "inserted", "delete-confirmation", "delete-saving", "restored", "stale", "uncertain", "reopened"),
)
if __name__ == "__main__":
    main(SPEC, __doc__)
