#!/usr/bin/env python3
"""Closed Profile English empty-Canvas-title checks; never builds or uses Slack."""
import argparse
from pathlib import Path
import runpy

FLAG = 'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK'
CASE_FLAG = 'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CASE'
CASES = ('existing', 'completion')


def harness(case):
    if case not in CASES:
        raise RuntimeError('Unknown empty Canvas title case')
    filename = 'check-profile-file-titles-native.py' if case == 'existing' else 'check-profile-file-completion-native.py'
    loaded = runpy.run_path(str(Path(__file__).with_name(filename)))
    base = loaded['B']
    original = base['NativeFixture']
    import profile_empty_canvas_title as oracle

    def flags(extra):
        extra = dict(extra or {})
        if FLAG in extra or CASE_FLAG in extra:
            raise RuntimeError('Unexpected preconfigured empty Canvas title mode')
        return dict(extra, **{FLAG: '1', CASE_FLAG: case})

    if case == 'existing':
        loaded['MODE'].update(list=False, blocks=True)

        class EmptyTitleFixture(original):
            def __init__(self, fixture, log, binary, extra):
                super().__init__(fixture, log, binary, flags(extra))
    else:
        class EmptyTitleFixture(original):
            def __init__(self, fixture, log, binary=None, checks=None, **kwargs):
                super().__init__(fixture, log, binary, flags(checks), **kwargs)
        base['STAGES'] = oracle.COMPLETION_STAGES

    def validate(fixture, receipt, captures, **options):
        return oracle.validate(fixture, receipt, captures, loaded, case=case, **options)

    base.update(validate=validate, NativeFixture=EmptyTitleFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, required=True)
    parser.add_argument('--fixture', action='append', choices=('profile-sections-mentions', 'profile-sections-mentions-narrow'))
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args()
    base = harness(args.case)
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        base['run_fixture'](fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
