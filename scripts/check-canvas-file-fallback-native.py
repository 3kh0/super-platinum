"""Read file-bearing Canvas fallback roots through native controls with disconnected original responses."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_file_fallback

SPEC = CanvasCheck(
    fixture='canvas-file-fallback', flag='SUPER_PLATINUM_CANVAS_FILE_FALLBACK_CHECK',
    required=('readOnly', 'fallbackOnly', 'noDecoyMedia', 'originalIdentity',
              'explicitRetry', 'returnedToCanvas', 'syntheticInput'),
    counts={'requestCount': 8}, stages=canvas_file_fallback.STAGES,
)
_base_validate = canvas_native.validate
_base_validate_closed = canvas_native.validate_closed


def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    canvas_file_fallback.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _base_validate_closed(spec, fixture, log, stages, terminal)
    originals = [{k: v for k, v in stage.items() if k != 'screenshot'} for stage in stages]
    if not canvas_file_fallback.same(originals, terminal.get('stages')):
        raise RuntimeError('Captured file-fallback body/source differs from terminal proof')


if __name__ == '__main__':
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    main(SPEC, __doc__)
