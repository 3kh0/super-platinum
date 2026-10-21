"""Run closed underlined paragraph/H1-H3 editing fixtures; no build or live Slack."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_underline
SPEC = CanvasCheck(fixture="canvas-underline", flag="SUPER_PLATINUM_CANVAS_UNDERLINE_CHECK",
    required=("cancelNoWrite", "staleDraftPreserved", "duplicateBlocked", "canonicalFormatting", "structurePreserved", "toolbarToggle", "insideOutsideInput", "uncertainNoReplay", "repeatedReadbackLocked", "syntheticInput"),
    counts={"attempts":5,"confirmed":4}, stages=canvas_underline.STAGES)
_base_validate = canvas_native.validate

def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    canvas_underline.validate(receipt)

if __name__ == "__main__":
    canvas_native.validate = validate
    main(SPEC, __doc__)
