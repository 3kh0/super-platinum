#!/usr/bin/env python3
"""Original owned List harness, exact ordinary-link Canvas title lifecycle only."""
import argparse
from pathlib import Path
import runpy

FLAG = 'SUPER_PLATINUM_LIST_LINK_TITLE_CHECK'
CASE = 'link-title-lifecycle'


def harness():
    base = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    base = base['run_fixture'].__globals__
    original, old_contract = base['NativeFixture'], base['case_contract']

    class LinkTitleFixture(original):
        def __init__(self, fixture, log, binary=None, checks=None, **kwargs):
            checks = dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') != CASE or FLAG in checks:
                raise RuntimeError('Unexpected ordinary-link List fixture mode')
            checks[FLAG] = '1'
            super().__init__(fixture, log, binary, checks, **kwargs)

    def contract(case):
        if case != CASE:
            return old_contract(case)
        import slack_list_link_title as oracle
        return oracle.STAGES[case], oracle.validator(case)

    base.update(case_contract=contract, NativeFixture=LinkTitleFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, required=True)
    parser.add_argument('--fixture', action='append', choices=('slack-list', 'slack-list-narrow'))
    parser.add_argument('--case', choices=(CASE,), default=CASE)
    args = parser.parse_args()
    base = harness()
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        base['run_fixture'](fixture, args.binary.resolve(), args.captures.resolve(), args.case)


if __name__ == '__main__':
    main()
