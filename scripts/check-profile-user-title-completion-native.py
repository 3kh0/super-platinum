#!/usr/bin/env python3
"""Closed query-owned Profile Canvas user-title completion; never builds."""
import argparse
from pathlib import Path
import runpy

FLAG='SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CHECK'
CASE_FLAG='SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CASE'
CASES=('known','hidden','recovery','retirement')


def harness(case):
    if case not in CASES:
        raise RuntimeError('Unknown Profile user-title completion case')
    loaded=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
    base=loaded['B'];original=base['NativeFixture']
    import profile_user_title_completion as oracle

    class UserTitleCompletionFixture(original):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if FLAG in checks or CASE_FLAG in checks:
                raise RuntimeError('Unexpected preconfigured user-title completion mode')
            checks.update({FLAG:'1',CASE_FLAG:case})
            super().__init__(fixture,log,binary,checks,**kwargs)

    def validate(fixture,receipt,captures,**options):
        return oracle.validate(fixture,receipt,captures,loaded,case=case,**options)

    base.update(PREFIX='profile-user-title-completion-check: ',SCOPE='profile-user-title-completion',
                STAGES=oracle.STAGES[case],validate=validate,NativeFixture=UserTitleCompletionFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('profile-sections-mentions','profile-sections-mentions-narrow'))
    parser.add_argument('--case',choices=CASES,required=True)
    args=parser.parse_args();base=harness(args.case)
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve())


if __name__=='__main__':main()
