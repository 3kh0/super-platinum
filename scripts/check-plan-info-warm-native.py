#!/usr/bin/env python3
"""Closed private-cache PlanInfo display fixture; no build or live Slack access."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys
import plan_info_warm as oracle
from native_fixture import DEFAULT_BINARY


def harness():
    path=Path(__file__).with_name('check-workflow-activity-native.py')
    spec=importlib.util.spec_from_file_location('_plan_info_warm_runner',path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    original_fixture=module.NativeFixture
    def owned_fixture(fixture,log,binary=None,checks=None,**kwargs):
        oracle.require(fixture in oracle.FIXTURES and isinstance(checks,dict)
            and checks.get(module.FLAG)=='1'
            and checks.get('SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE')==oracle.CASE
            and oracle.FLAG not in checks and oracle.DB_FLAG not in checks,
            'PlanInfo warm requires exact original Activity fixture checks')
        directory=Path(checks['SUPER_PLATINUM_UI_CAPTURE_DIR']).resolve()
        oracle.require(Path(log).resolve().parent==directory,'Foreign PlanInfo log directory')
        database=directory/oracle.DB_NAME
        oracle.require(not any(Path(str(database)+suffix).exists() for suffix in ('','-wal','-shm','-journal')),
            'PlanInfo warm requires a fresh isolated database')
        return original_fixture(fixture,log,binary,{**checks,oracle.FLAG:'1',oracle.DB_FLAG:str(database)},**kwargs)
    def validate(fixture,receipt,captures,scenario,platform=sys.platform,png=True):
        oracle.require(scenario==oracle.CASE,'Foreign PlanInfo Activity case')
        return oracle.validate(fixture,receipt,captures,module.__dict__,platform=platform,png=png)
    closed=module.validate_closed
    def validate_closed(native,expected,captures,admitted,scenario,dimensions=None,**kwargs):
        result=closed(native,expected,captures,admitted,scenario,dimensions,**kwargs)
        oracle.validate_database(Path(captures)/oracle.DB_NAME,result)
        return result
    module.NativeFixture=owned_fixture;module.validate=validate
    module.validate_closed=validate_closed;module.STAGES={oracle.CASE:oracle.STAGES}
    return module


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path(os.environ.get('SUPER_PLATINUM_BINARY',DEFAULT_BINARY)))
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',choices=oracle.FIXTURES,required=True)
    args=parser.parse_args();captures=args.captures.resolve()
    oracle.require(not (captures/(args.fixture+'.log')).exists(),'Use a fresh PlanInfo capture directory')
    harness().run_fixture(args.fixture,args.binary.resolve(),captures,oracle.CASE)


if __name__=='__main__':main()
