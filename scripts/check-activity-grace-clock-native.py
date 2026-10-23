#!/usr/bin/env python3
"""Closed original Activity clock/zone fixture; does not build or use live Slack."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys
import activity_grace_clock as oracle
from native_fixture import DEFAULT_BINARY, ROOT


def harness():
    """Bind one exact opt-in to the established Activity custody pipeline."""
    path=Path(__file__).with_name('check-workflow-activity-native.py')
    spec=importlib.util.spec_from_file_location('_activity_grace_clock_runner',path)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    original_fixture=module.NativeFixture
    def owned_fixture(fixture,log,binary=None,checks=None,**kwargs):
        oracle.require(fixture in oracle.FIXTURES and isinstance(checks,dict)
                       and checks.get(module.FLAG)=='1'
                       and checks.get('SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE')==oracle.CASE
                       and oracle.FLAG not in checks,
                       'Grace clock requires exact original Activity fixture checks')
        return original_fixture(fixture,log,binary,{**checks,oracle.FLAG:'1'},**kwargs)
    def validate(fixture,receipt,captures,scenario,platform=sys.platform,png=True):
        oracle.require(scenario==oracle.CASE and fixture in oracle.FIXTURES,
                       'Foreign Activity clock fixture/case')
        return oracle.validate(fixture,receipt,captures,module.__dict__,platform=platform,png=png)
    module.NativeFixture=owned_fixture
    module.validate=validate
    module.STAGES={oracle.CASE:oracle.STAGES}
    return module


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path(os.environ.get('SUPER_PLATINUM_BINARY',DEFAULT_BINARY)))
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',choices=oracle.FIXTURES,required=True)
    args=parser.parse_args();captures=args.captures.resolve()
    oracle.require(not (captures/(args.fixture+'.log')).exists(),'Use a fresh Activity clock capture directory')
    harness().run_fixture(args.fixture,args.binary.resolve(),captures,oracle.CASE)


if __name__=='__main__':main()
