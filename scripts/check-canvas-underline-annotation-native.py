"""Closed underline annotation selection, journal, canonical readback and no replay."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_underline_annotation as oracle
SPEC=CanvasCheck(fixture="canvas-comment-underline",flag="SUPER_PLATINUM_CANVAS_ANNOTATION_CHECK",
    required=("existingReadOnly","staleNoWrite","createdOnce","canonicalReadback","uncertainNoReplay","syntheticInput"),
    counts={"writes":3},stages=oracle.STAGES,
    private_files=(("SUPER_PLATINUM_CANVAS_ANNOTATION_DB","annotations.sqlite"),))
_base=canvas_native.validate
def validate(spec,fixture,receipt):
    _base(spec,fixture,receipt)
    oracle.validate(receipt)
if __name__=="__main__":
    canvas_native.validate=validate
    main(SPEC,__doc__)
