#!/usr/bin/env python3
"""Disconnected inserted paragraph followed by two mounted text edits and restoration."""
from canvas_native import CanvasCheck, main
SPEC = CanvasCheck(
    fixture="canvas-structure-text", flag="SUPER_PLATINUM_CANVAS_STRUCTURE_TEXT_CHECK",
    required=("inserted", "firstEdit", "secondEdit", "sealedReadbacks", "baseZeroPreserved", "originalRestored", "duplicateSafe", "controlsHit"),
    counts={"structureAttempts":2,"textAttempts":2},
    stages=("initial","insert-draft","insert-saving","inserted","first-draft","first-saving","first-accepted","second-draft","second-saving","second-accepted","delete-confirmation","delete-saving","restored"),
)
if __name__ == "__main__": main(SPEC,__doc__)
