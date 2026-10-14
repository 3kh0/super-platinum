#!/usr/bin/env python3
"""Capture existing checklist toggle/restore and canonical readback offline."""
from canvas_native import CanvasCheck, main

SPEC = CanvasCheck(
    fixture="canvas-checklist",
    flag="SUPER_PLATINUM_CANVAS_CHECKLIST_CHECK",
    required=("duplicateBlocked", "canonicalReadback", "restored", "layout.unsupportedInert"),
    counts={"attempts": 2},
    stages=("saving", "checked", "restored"),
)

if __name__ == "__main__":
    main(SPEC, __doc__)
