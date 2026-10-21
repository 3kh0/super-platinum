#!/usr/bin/env python3
"""Fresh-process deletion recovery via the original closed DB-only runner."""
import argparse
from pathlib import Path
import runpy

R=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-own-edit-restart-native.py')))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--build-result',type=Path)
    parser.add_argument('--captures',required=True,type=Path)
    parser.add_argument('--fixture',action='append',choices=R['C']['FIXTURES'])
    parser.add_argument('--mode',required=True,choices=('pending','unknown'))
    args=parser.parse_args()
    for fixture in args.fixture or R['C']['FIXTURES']:
        R['run_pair'](args.mode,fixture,args.binary,args.captures.resolve()/fixture,
                      build_result=args.build_result,operation='delete')


if __name__=='__main__':main()
