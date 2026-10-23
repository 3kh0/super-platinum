"""Capture mounted Profile time using the existing owned staged native harness."""
import argparse
import os
from pathlib import Path
import canvas_native
import canvas_evidence
from canvas_native import CanvasCheck
from native_fixture import DEFAULT_BINARY, ROOT
import profile_local_time as oracle

SPEC = CanvasCheck(fixture=oracle.PREFIX, flag='SUPER_PLATINUM_PROFILE_TIME_CHECK',
                   required=(), counts={}, stages=oracle.STAGES)
_base = canvas_native.validate
_closed = canvas_native.validate_closed
_normalize = canvas_evidence.normalize


def validate(spec, fixture, receipt):
    oracle.require(spec is SPEC and fixture in oracle.FIXTURES, 'Unknown Profile time fixture')
    _base(spec, fixture, receipt)
    oracle.validate(receipt, fixture)


def validate_closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require([{k:v for k,v in s.items() if k != 'screenshot'} for s in stages]
                   == terminal.get('stages'), 'Original Profile time stage changed after capture')


def normalize(native, evidence, stack, process_api, handle):
    fixture = evidence.get('fixture')
    oracle.require(fixture in oracle.FIXTURES and native.fixture == fixture
                   and native.checks.get(SPEC.flag) == '1', 'Unknown Profile time normalization fixture')
    expected = 'U1' if fixture in ('profile-hover-card', 'profile-hover-card-narrow') else None
    return _normalize(native, evidence, stack, process_api, handle, expected_profile_hover=expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path,
                        default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', ROOT/'tmp/agent-ui')))
    parser.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    args = parser.parse_args()
    canvas_evidence.normalize = normalize
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPEC, fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
