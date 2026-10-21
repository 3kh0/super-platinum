"""Read hidden Canvas roots through native controls with disconnected original responses."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_special_roots

SPEC = CanvasCheck(
    fixture='canvas-special-roots', flag='SUPER_PLATINUM_CANVAS_SPECIAL_ROOTS_CHECK',
    required=('readOnly', 'hiddenSuppressed', 'noHiddenMedia', 'originalIdentity',
              'explicitRetry', 'returnedToCanvas', 'syntheticInput'),
    counts={'requestCount': 8}, stages=canvas_special_roots.STAGES,
)
_base_validate = canvas_native.validate
_base_validate_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    canvas_special_roots.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _base_validate_closed(spec, fixture, log, stages, terminal)
    originals = [{k: v for k, v in stage.items() if k != 'screenshot'} for stage in stages]
    if not canvas_special_roots.same(originals, terminal.get('stages')):
        raise RuntimeError('Captured special-root body/source differs from terminal proof')


if __name__ == '__main__':
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    main(SPEC, __doc__)
