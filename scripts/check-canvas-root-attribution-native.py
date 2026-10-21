"""Inspect original Canvas editor attribution and retained rich drafts offline."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_root_attribution as oracle

SPEC = CanvasCheck(
    fixture='canvas-root-attribution', flag='SUPER_PLATINUM_CANVAS_ROOT_ATTRIBUTION_CHECK',
    required=('readOnly', 'sourceIdentity', 'draftPreserved', 'profileVisible', 'staleRefused'),
    counts={'requestCount': 11}, stages=oracle.STAGES,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),),
    synthetic_context=True,
)
_base = canvas_native.validate
_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    oracle.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    original = [{k: v for k, v in row.items() if k != 'screenshot'} for row in stages]
    if not oracle.same(original, terminal.get('stages')):
        raise RuntimeError('Original attribution capture receipts changed')


if __name__ == '__main__':
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    main(SPEC, __doc__)
