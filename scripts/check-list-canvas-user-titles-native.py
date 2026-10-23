#!/usr/bin/env python3
"""Closed List Canvas user-title custody; reuses the original owned harness."""
import argparse
from pathlib import Path
import runpy

FLAG='SUPER_PLATINUM_LIST_CANVAS_USER_TITLE_CHECK'
CASES=tuple('user-title-'+name for name in ('known','hidden','recovery','draft-recovery','retirement'))


def harness():
    base=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
    # runpy's returned mapping is not the functions' execution globals.
    base=base['run_fixture'].__globals__
    original=base['NativeFixture'];old_contract=base['case_contract']

    class UserTitleFixture(original):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            checks=dict(checks or {})
            if checks.get('SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE') not in CASES or FLAG in checks:
                raise RuntimeError('Unexpected List Canvas user-title fixture mode')
            checks[FLAG]='1'
            super().__init__(fixture,log,binary,checks,**kwargs)

    def contract(case):
        if case not in CASES:return old_contract(case)
        import slack_list_canvas_user_titles as oracle
        return oracle.STAGES[case],oracle.validator(case)

    base.update(case_contract=contract,NativeFixture=UserTitleFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('slack-list','slack-list-narrow'))
    parser.add_argument('--case',required=True,choices=CASES)
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or base['FIXTURES']:
        base['require'](not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve(),args.case)


if __name__=='__main__':main()
