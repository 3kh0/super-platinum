#!/usr/bin/env python3
"""Datetime-specific semantic oracle; accepted Date passive lifecycle unchanged."""
from pathlib import Path
import runpy
import sys
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tmp/app-home-datetime-dispatch-20261025/scripts'))
import dt_oracle as oracle
_engine=runpy.run_path(str(Path(__file__).with_name('check-app-home-date-dispatch-native.py')))
_scope=_engine['run_fixture'].__globals__
_native=_scope['NativeFixture']
def _datetime_native(fixture,log,binary,env):
 return _native(fixture,log,binary,dict(env,SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CHECK='0',SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CASE='',SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CHECK='0',SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CASE=''))
_scope.update(oracle=oracle,FLAG='SUPER_PLATINUM_APP_HOME_DATETIME_DISPATCH_CHECK',CASE_FLAG='SUPER_PLATINUM_APP_HOME_DATETIME_DISPATCH_CASE',PREFIX='app-home-datetime-dispatch-check: ',FIXTURES=oracle.FIXTURES,STAGES=oracle.STAGES,capture_keys=oracle.capture_keys,NativeFixture=_datetime_native)
H,BASE=_scope['H'],_scope['BASE']
PREFIX,EDGE,WAITING=(_scope[k]for k in('PREFIX','EDGE','WAITING'))
STAGES,capture_keys=oracle.STAGES,oracle.capture_keys
validate_log,run_fixture,main=(_scope[k]for k in('validate_log','run_fixture','main'))
if __name__=='__main__':main()
