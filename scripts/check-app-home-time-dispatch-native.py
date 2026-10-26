#!/usr/bin/env python3
"""Home Time uses the accepted Date passive lifecycle with an independent oracle.

No guard loop is copied or altered. Time typed buffers, selected canonical values,
and committed parent wire are checked separately; synthetic events are not OS input.
"""
from pathlib import Path
import runpy
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tmp/app-home-time-dispatch-20261025/scripts'))
import time_oracle as oracle

_engine = runpy.run_path(str(Path(__file__).with_name('check-app-home-date-dispatch-native.py')))
_scope = _engine['run_fixture'].__globals__
_native = _scope['NativeFixture']


def _time_native(fixture, log, binary, env):
    # The unchanged engine provides all owned readiness, PID/caption, capture,
    # passive pointer, terminal, closed-log and cleanup checks.
    return _native(fixture, log, binary, dict(env, SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CHECK='0', SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CASE=''))


_scope.update(
    oracle=oracle,
    FLAG='SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CHECK',
    CASE_FLAG='SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CASE',
    PREFIX='app-home-time-dispatch-check: ',
    FIXTURES=('app-home-time-dispatch', 'app-home-time-dispatch-narrow'),
    STAGES=oracle.STAGES,
    capture_keys=oracle.capture_keys,
    NativeFixture=_time_native,
)
H, BASE = _scope['H'], _scope['BASE']
PREFIX, EDGE, WAITING = (_scope[k] for k in ('PREFIX', 'EDGE', 'WAITING'))
STAGES, capture_keys = oracle.STAGES, oracle.capture_keys
validate_log, run_fixture, main = (_scope[k] for k in ('validate_log', 'run_fixture', 'main'))

if __name__ == '__main__':
    main()
