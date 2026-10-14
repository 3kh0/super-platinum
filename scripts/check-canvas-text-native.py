#!/usr/bin/env python3
"""Capture bounded paragraph editing, dirty drafts and uncertain saves offline."""
from canvas_native import CanvasCheck, main

SPEC = CanvasCheck(
    fixture="canvas-text",
    flag="SUPER_PLATINUM_CANVAS_TEXT_CHECK",
    required=("initialLoadRecovery", "latestPairWins", "cancelNoWrite",
              "unsupportedPreserved", "staleDraftPreserved", "duplicateBlocked",
              "canonicalFormatting", "uncertainNoReplay", "disconnected"),
    counts={"attempts": 2, "boundedPairedReads": 3},
    stages=("unsupported-draft", "stale-draft", "saving", "confirmed", "uncertain"),
)

if __name__ == "__main__":
    main(SPEC, __doc__)
