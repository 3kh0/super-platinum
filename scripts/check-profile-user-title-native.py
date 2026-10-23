#!/usr/bin/env python3
"""Closed Profile Canvas user-title checks; no build or live Slack access."""
import argparse
from pathlib import Path
import runpy

FLAG='SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK'
CASE_FLAG='SUPER_PLATINUM_PROFILE_USER_TITLE_CASE'
CASES=('known','recovery','failure','cancel','hidden')


def harness(case):
    if case not in CASES:
        raise RuntimeError('Unknown Profile user-title case')
    # Isolated globals preserve the accepted file/mentions runner and its
    # original owner, capture edges, closed-log, binary and cleanup checks.
    file=runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
    file['MODE'].update(list=False,blocks=True)
    base=file['B'];native=base['NativeFixture']
    import profile_user_title as oracle

    class UserTitleFixture(native):
        def __init__(self,fixture,log,binary,extra):
            if FLAG in extra or CASE_FLAG in extra:
                raise RuntimeError('Unexpected preconfigured user-title mode')
            super().__init__(fixture,log,binary,dict(extra,**{FLAG:'1',CASE_FLAG:case}))

    def validate(fixture,receipt,captures,**options):
        return oracle.validate(fixture,receipt,captures,base,case=case,**options)

    base.update(PREFIX='profile-user-title-check: ',stages=lambda:oracle.STAGES[case],
                validate=validate,kind=lambda:'canvas',NativeFixture=UserTitleFixture)
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
