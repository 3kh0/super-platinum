"""Inspect explicit Canvas editor recovery with a retained rich reply offline."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_editor_recovery as oracle

SPEC = CanvasCheck(
    fixture='canvas-editor-recovery', flag='SUPER_PLATINUM_CANVAS_EDITOR_RECOVERY_CHECK',
    required=('readOnly', 'draftPreserved', 'staleRefused', 'explicitRecovery'),
    counts={'requestCount': 4, 'memberRequestCount': 5}, stages=oracle.STAGES,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),),
    synthetic_context=True,
)
_base, _closed = canvas_native.validate, canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    oracle.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    originals = [{k: v for k, v in row.items() if k != 'screenshot'} for row in stages]
    oracle.require(oracle.same(originals, terminal.get('stages')), 'Original recovery capture receipts changed')


if __name__ == '__main__':
    canvas_native.validate, canvas_native.validate_closed = validate, validate_closed
    main(SPEC, __doc__)
