#!/usr/bin/env python3
"""Closed List transcript empty Canvas titles; never builds or contacts Slack."""
import argparse
from pathlib import Path
import runpy

FLAG = 'SUPER_PLATINUM_LIST_EMPTY_CANVAS_TRANSCRIPT_CHECK'
CASE_FLAG = 'SUPER_PLATINUM_LIST_EMPTY_CANVAS_TRANSCRIPT_CASE'
CASES = {'plain-empty': 'cold-files', 'blocks-empty': 'cold-files-recovery'}


def harness(case):
    if case not in CASES:
        raise RuntimeError('Unknown List empty Canvas transcript case')
    loaded = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    base = loaded['run_fixture'].__globals__
    original = base['NativeFixture']; prior = base['case_contract']; actual = CASES[case]

    class EmptyTranscriptFixture(original):
        def __init__(self, fixture, log, binary=None, checks=None, **kwargs):
            checks = dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') != actual or FLAG in checks or CASE_FLAG in checks:
                raise RuntimeError('Unexpected empty Canvas transcript fixture mode')
            checks.update({FLAG: '1', CASE_FLAG: case})
            super().__init__(fixture, log, binary, checks, **kwargs)

    def contract(requested):
        if requested != actual:
            return prior(requested)
        import slack_list_empty_canvas_transcript as oracle
        return oracle.STAGES[case], oracle.validator(case)

    base.update(NativeFixture=EmptyTranscriptFixture, case_contract=contract)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, required=True)
    parser.add_argument('--fixture', action='append', choices=('slack-list', 'slack-list-narrow'))
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args(); base = harness(args.case)
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        base['run_fixture'](fixture, args.binary.resolve(), args.captures.resolve(), CASES[args.case])


if __name__ == '__main__':
    main()
