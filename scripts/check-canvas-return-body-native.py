"""Returned Canvas Refresh/Retry and original rich reply custody captures."""
import argparse
import os
from pathlib import Path
import canvas_native
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import canvas_return_body as oracle

SPECS = {case: CanvasCheck(
    fixture=oracle.PREFIX, flag='SUPER_PLATINUM_CANVAS_RETURN_BODY_CHECK',
    required=('replacement', 'originalEntryRetained', 'noForeignSend'),
    counts={'requestCount': 6}, stages=oracle.stages(case),
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),
                   ('SUPER_PLATINUM_CANVAS_REPLACEMENT_REPLY_DB', 'replacement-b.sqlite')))
    for case in oracle.CASES}
_base = canvas_native.validate
_closed = canvas_native.validate_closed
_private = canvas_native.private_checks


def case_for(spec):
    cases = [case for case, value in SPECS.items() if spec is value]
    oracle.require(len(cases) == 1, 'Return-body requires exact owned specification')
    return cases[0]


def private_checks(spec, directory):
    case = case_for(spec)
    checks = _private(spec, directory)
    checks[spec.flag] = case
    checks['SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK'] = '1'
    checks['SUPER_PLATINUM_CANVAS_REPLACEMENT_CUSTODY_CHECK'] = '1'
    return checks


def validate(spec, fixture, receipt):
    _base(spec, fixture, receipt)
    oracle.validate(receipt, fixture, case_for(spec))


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require(oracle.same([{k: v for k, v in s.items() if k != 'screenshot'} for s in stages],
                               terminal.get('stages')), 'Captured return-body stage changed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', ROOT / 'tmp/agent-ui')))
    parser.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    parser.add_argument('--case', required=True, choices=oracle.CASES)
    args = parser.parse_args()
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    canvas_native.private_checks = private_checks
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPECS[args.case], fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
