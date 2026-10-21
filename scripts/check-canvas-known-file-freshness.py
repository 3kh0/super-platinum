"""Closed event-owned Canvas file refresh with original rich reply custody."""
import argparse
import os
from pathlib import Path

import canvas_native
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import canvas_known_file_freshness as oracle

SPECS = {case: CanvasCheck(
    fixture=oracle.PREFIX, flag='SUPER_PLATINUM_CANVAS_KNOWN_FILE_CHECK',
    required=('draftPreserved', 'originalReadRetained', 'noNavigation', 'noAutomaticLoop'),
    counts={'requestCount': 2, 'acquisitionCount': oracle.COUNTS[case][-1], 'resolverCount': 0},
    stages=stages, private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),))
    for case, stages in oracle.STAGES.items()}
_base = canvas_native.validate
_closed = canvas_native.validate_closed
_private = canvas_native.private_checks


def case_for(spec):
    cases = [case for case, original in SPECS.items() if original is spec]
    oracle.require(len(cases) == 1, 'Known freshness requires exact owned specification')
    return cases[0]


def private_checks(spec, directory):
    case = case_for(spec)
    checks = _private(spec, directory)
    checks.update(SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK='1',
                  SUPER_PLATINUM_CANVAS_KNOWN_FILE_CASE=case,
                  SUPER_PLATINUM_CANVAS_COLD_FILE_CHECK='0')
    return checks


def validate(spec, fixture, receipt):
    case = case_for(spec)
    _base(spec, fixture, receipt)
    oracle.require(fixture in oracle.FIXTURES and receipt.get('case') == case,
                   'Wrong known freshness fixture/case')
    oracle.validate(receipt)


def validate_closed(spec, fixture, log, stages, terminal):
    case_for(spec)
    _closed(spec, fixture, log, stages, terminal)
    oracle.require(oracle.same([{k: v for k, v in s.items() if k != 'screenshot'}
                               for s in stages], terminal.get('stages')),
                   'Original known freshness stage evidence changed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', ROOT / 'tmp/agent-ui')))
    parser.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    parser.add_argument('--case', choices=tuple(SPECS), required=True)
    args = parser.parse_args()
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    canvas_native.private_checks = private_checks
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPECS[args.case], fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
