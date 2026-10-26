#!/usr/bin/env python3
"""Observed deleted Canvas references; reuses the owned Profile launcher."""
import argparse
from pathlib import Path
import runpy

FLAG = 'SUPER_PLATINUM_PROFILE_CANVAS_DELETED_CHECK'
CASE_FLAG = 'SUPER_PLATINUM_PROFILE_CANVAS_DELETED_CASE'
CASES = ('recovery', 'partial', 'failure', 'cancel', 'retirement')
CONFLICTS = ('SUPER_PLATINUM_PROFILE_CANVAS_UNAVAILABLE_CHECK', 'SUPER_PLATINUM_PROFILE_MISSING_GROUP_TITLE_CHECK', 'SUPER_PLATINUM_PROFILE_GROUP_TITLE_CHECK', 'SUPER_PLATINUM_PROFILE_CHANNEL_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_CHECK',
             'SUPER_PLATINUM_PROFILE_PRIVATE_CHANNEL_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_NONMEMBER_CHANNEL_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_MISSING_CHANNEL_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_LINK_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK',
             'SUPER_PLATINUM_PROFILE_CHANNEL_COMPLETION_CHECK',
             'SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK')


def flags(extra, case):
    if case not in CASES:
        raise RuntimeError('Unknown exact deleted Canvas case')
    extra = dict(extra or {})
    if FLAG in extra or CASE_FLAG in extra or any(extra.get(k) == '1' for k in CONFLICTS):
        raise RuntimeError('Conflicting or preconfigured deleted Canvas fixture mode')
    return dict(extra, **{FLAG: '1', CASE_FLAG: case})


def harness(case):
    flags({}, case)
    import profile_canvas_deleted as oracle
    file = runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
    file['MODE'].update(list=False, blocks=True)
    base = file['B']
    original = base['NativeFixture']

    class UnavailableCanvasFixture(original):
        def __init__(self, fixture, log, binary, extra):
            super().__init__(fixture, log, binary, flags(extra, case))

    def validate(fixture, receipt, captures, **options):
        return oracle.validate(fixture, receipt, captures, base, case=case, **options)

    base.update(PREFIX='profile-canvas-deleted-check: ', stages=lambda: oracle.STAGES[case],
                validate=validate, kind=lambda: 'canvas', NativeFixture=UnavailableCanvasFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, required=True)
    parser.add_argument('--fixture', action='append', choices=('profile-sections-mentions', 'profile-sections-mentions-narrow'))
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args()
    fixtures = args.fixture or ['profile-sections-mentions', 'profile-sections-mentions-narrow']
    if len(fixtures) != len(set(fixtures)):
        parser.error('Duplicate fixture')
    base = harness(args.case)
    for fixture in fixtures:
        base['require'](not (args.captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        base['run_fixture'](fixture, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
