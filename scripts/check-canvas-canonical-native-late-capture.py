"""Actual original native final-capture timeout and explicit destination recovery."""
import argparse
import os
from pathlib import Path
import canvas_native
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import canvas_canonical_native_late_capture as oracle

SPEC = CanvasCheck(
    fixture=oracle.PREFIX, flag='SUPER_PLATINUM_CANVAS_DESTINATION_LATE_CAPTURE_CHECK',
    required=('timedOut', 'competingFinishRefused', 'lateCaptureRecovered',
              'draftPreserved', 'freshTarget', 'nativeClose'),
    counts={'requestCount': 4, 'resolverCount': 3, 'targetReadCount': 1}, stages=oracle.STAGES,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),))
_base = canvas_native.validate
_closed = canvas_native.validate_closed
_private = canvas_native.private_checks


def private_checks(spec, directory):
    oracle.require(spec is SPEC, 'Late capture flags require exact owned specification')
    checks = _private(spec, directory)
    checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'] = '1'
    return checks


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    oracle.require(spec is SPEC, 'Late capture specification changed')
    oracle.validate(receipt, fixture)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require(oracle.same([{k: v for k, v in s.items() if k != 'screenshot'} for s in stages],
                               terminal.get('stages')), 'Captured late stage changed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', ROOT / 'tmp/agent-ui')))
    parser.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    args = parser.parse_args()
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    canvas_native.private_checks = private_checks
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPEC, fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
