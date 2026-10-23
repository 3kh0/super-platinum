#!/usr/bin/env python3
"""Closed existing-Canvas link-title lifecycle; no build or live Slack access."""
import argparse
from pathlib import Path
import runpy
import profile_link_title as oracle


def flags(extra):
    extra=dict(extra or {})
    if oracle.FLAG in extra or any(extra.get(k)=='1' for k in (
        'SUPER_PLATINUM_PROFILE_EMPTY_CANVAS_TITLE_CHECK','SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK',
        'SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK')):
        raise RuntimeError('Conflicting or preconfigured link-title fixture mode')
    return dict(extra,**{oracle.FLAG:'1'})


def harness():
    loaded=runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
    loaded['MODE'].update(list=False,blocks=True)
    base=loaded['B'];original=base['NativeFixture']
    class LinkFixture(original):
        def __init__(self,fixture,log,binary,extra):
            super().__init__(fixture,log,binary,flags(extra))
    base.update(validate=oracle.validate,NativeFixture=LinkFixture)
    return base


def main():
    from native_fixture import DEFAULT_BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=oracle.FIXTURES)
    args=parser.parse_args();base=harness()
    for fixture in args.fixture or oracle.FIXTURES:
        oracle.require(not (args.captures/f'{fixture}.log').exists(),'Use a fresh capture directory')
        base['run_fixture'](fixture,args.binary.resolve(),args.captures.resolve())


if __name__=='__main__':main()
