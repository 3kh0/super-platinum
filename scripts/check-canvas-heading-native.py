"""Run the closed native H1–H3 editing fixture (no build or live Slack)."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_heading

SPEC = CanvasCheck(
    fixture="canvas-heading", flag="SUPER_PLATINUM_CANVAS_HEADING_CHECK",
    required=("cancelNoWrite", "staleDraftPreserved", "duplicateBlocked", "canonicalFormatting",
              "structurePreserved", "duplicateIdentity", "uncertainNoReplay", "repeatedReadbackLocked", "syntheticInput"),
    counts={"attempts": 4, "confirmed": 3}, stages=canvas_heading.STAGES,
)
_base_validate = canvas_native.validate


def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    canvas_heading.validate(receipt)


if __name__ == "__main__":
    canvas_native.validate = validate
    main(SPEC, __doc__)
