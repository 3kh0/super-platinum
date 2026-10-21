"""Closed pending Canvas/List destination retirement and fresh return acceptance."""
import argparse
import os
from pathlib import Path
import canvas_native
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import canvas_canonical_native_retirement as oracle

SPEC = CanvasCheck(
    fixture=oracle.PREFIX,
    flag='SUPER_PLATINUM_CANVAS_DESTINATION_RETIREMENT_CHECK',
    required=('draftPreserved', 'freshTarget', 'lateRefused', 'nativeClose'),
    counts={'requestCount': 6, 'resolverCount': 2, 'targetReadCount': 2},
    stages=oracle.STAGES,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),))
_base = canvas_native.validate
_closed = canvas_native.validate_closed
_private = canvas_native.private_checks


def private_checks(spec, directory):
    oracle.require(spec is SPEC, 'Retirement extra flag requires exact owned specification')
    checks = _private(spec, directory)
    checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'] = '1'
    return checks


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    expected = oracle.D.CANVAS if '-canvas' in fixture else oracle.D.LIST
    oracle.require(spec is SPEC and fixture in oracle.FIXTURES
                   and receipt.get('destinationId') == expected,
                   'Named retirement destination differs')
    oracle.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require(oracle.same([{k: v for k, v in s.items() if k != 'screenshot'}
                               for s in stages], terminal.get('stages')),
                   'Captured retirement stage changed')


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
