"""Closed native paragraph edit with a formatted static canonical title."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_formatted_title as title

SPEC = CanvasCheck(
    fixture="canvas-text", flag="SUPER_PLATINUM_CANVAS_FORMATTED_TITLE_EDIT_CHECK",
    required=("cancelNoWrite", "duplicateBlocked", "independentReadback",
              "titleRefusalPreserved", "syntheticInput"),
    counts={"attempts": 1}, stages=title.STAGES,
)
_base_validate = canvas_native.validate
_base_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    title.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _base_closed(spec, fixture, log, stages, terminal)
    expected = [{k: v for k, v in stage.items() if k != "screenshot"} for stage in stages]
    title.require(terminal.get("stages") == expected, "Terminal proofs differ from original captured stages")


if __name__ == "__main__":
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    main(SPEC, __doc__)
