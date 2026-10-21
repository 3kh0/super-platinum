"""Closed H1-H3 native annotation, original styles/control identity and stale refusal."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_heading_comments as oracle
SPEC=CanvasCheck(fixture="canvas-comment-headings",flag="SUPER_PLATINUM_CANVAS_ANNOTATION_CHECK",
    required=("existingReadOnly","staleNoWrite","createdOnce","canonicalReadback","syntheticInput"),
    counts={"writes":6},stages=oracle.STAGES,
    private_files=(("SUPER_PLATINUM_CANVAS_ANNOTATION_DB","annotations.sqlite"),))
_base=canvas_native.validate
def validate(spec,fixture,receipt):
    _base(spec,fixture,receipt)
    oracle.validate(receipt)
if __name__=="__main__":
    canvas_native.validate=validate
    main(SPEC,__doc__)
