#!/usr/bin/env python3
"""Existing Profile Canvas emoji-title lifecycle; no build or live Slack access."""
import argparse
from pathlib import Path
import profile_emoji_title as source


def flags(extra, case):
    if case not in source.CASES:
        raise RuntimeError('Unknown exact emoji title case')
    extra = dict(extra or {})
    if source.FLAG in extra or source.CASE_FLAG in extra or any(extra.get(k) == '1' for k in (
            'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK', 'SUPER_PLATINUM_PROFILE_LINK_TITLE_CHECK',
            'SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK', 'SUPER_PLATINUM_PROFILE_CHANNEL_COMPLETION_CHECK',
            'SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK')):
        raise RuntimeError('Conflicting or preconfigured emoji title fixture mode')
    return dict(extra, **{source.FLAG: '1', source.CASE_FLAG: case})


def harness(case):
    oracle = source.Oracle(case)
    base = oracle.base['B']
    original = base['NativeFixture']

    class EmojiFixture(original):
        def __init__(self, fixture, log, binary, extra):
            super().__init__(fixture, log, binary, flags(extra, case))

    base.update(validate=oracle.validate, NativeFixture=EmojiFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--captures', type=Path, required=True)
    parser.add_argument('--fixture', action='append', choices=('profile-sections-mentions', 'profile-sections-mentions-narrow'))
    parser.add_argument('--case', choices=source.CASES, required=True)
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
