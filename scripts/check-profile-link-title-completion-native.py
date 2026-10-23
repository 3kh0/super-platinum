#!/usr/bin/env python3
"""Closed Profile link-title completion/Use/Save/reopen; no builds or live Slack."""
import argparse
from pathlib import Path
import runpy
import profile_link_completion_source as source
import profile_link_title_completion as oracle


def flags(checks):
    checks=dict(checks or {})
    if source.FLAG in checks or any(checks.get(k)=='1' for k in (
        'SUPER_PLATINUM_PROFILE_FILE_TITLES_CHECK','SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK',
        'SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CHECK','SUPER_PLATINUM_PROFILE_FILE_COMPLETION_OFF')):
        raise RuntimeError('Conflicting/preconfigured Profile link completion mode')
    return dict(checks,**{source.FLAG:'1'})


def harness():
    h=runpy.run_path(str(Path(__file__).with_name('check-profile-file-completion-native.py')))
    base=h['B'];original=base['NativeFixture']
    class LinkCompletionFixture(original):
        def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
            super().__init__(fixture,log,binary,flags(checks),**kwargs)
    def validate(fixture,receipt,captures,**options):
        return oracle.validate(fixture,receipt,captures,h,**options)
    base.update(STAGES=source.COMPLETION_STAGES,validate=validate,NativeFixture=LinkCompletionFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=('profile-sections-mentions','profile-sections-mentions-narrow'))
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or base['FIXTURES']:
        source.require(not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve())


if __name__=='__main__':main()
