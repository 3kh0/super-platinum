#!/usr/bin/env python3
"""Closed Home static-select remount through the existing owned native fixture guards.

No build or live Slack; native PNGs still require individual visual review.
"""
import argparse
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import re
import runpy
import secrets
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

sys.path.insert(0, str(ROOT / 'tmp/app-home-static-remount-20261025/scripts'))
import static_remount_oracle as oracle

# Reuse the accepted placement, pointer, 250ms response, ready publication,
# strict closed-log and Windows process identity implementation unchanged.
BASE = runpy.run_path(str(Path(__file__).with_name('check-profile-mentions-native.py')))
H = BASE['H']
FLAG = 'SUPER_PLATINUM_APP_HOME_STATIC_REMOUNT_CHECK'
PREFIX = 'app-home-static-remount-check: '
EDGE = 'app-home-external-capture: '
WAITING = 'app-home-external-waiting: '
FIXTURES = ('app-home-static-remount', 'app-home-static-remount-narrow')
STAGES = oracle.STAGES
capture_keys = oracle.capture_keys
require = BASE['require']


def validate_log(native, fixture, captures, case, *, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate Home terminal receipt')
    receipt = receipts[0]
    oracle.validate(fixture, case, receipt, captures, BASE, png=png)
    require(native.receipts(WAITING) == [receipt['readiness']], 'Terminal readiness differs from original owned handshake')
    expected = [dict(stage=s, edge=e) for s in oracle.capture_keys(receipt) for e in ('before', 'after')]
    require(native.receipts(EDGE) == expected, 'Wrong exact Home capture edges')
    return receipt


def run_fixture(fixture, case, binary, captures):
    require(fixture in FIXTURES and case in oracle.STAGES, 'Unknown Home fixture/case')
    require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'app-home-external-{nonce}'
    directory.mkdir(exist_ok=False)
    digest = H['binary_sha'](binary)
    evidence = dict(fixture=fixture, case=case, nonce=nonce, binary=str(binary), binarySha256=digest,
                    ok=False, readyDirectory=str(directory), limitations=[
                        'Closed synthetic endpoint and Home source; no live Slack network or server validation.',
                        'Placement/pointer checks are sampled observations, not atomic pixel proof.',
                        'Cached Home return is an explicit offline adapter after real tab navigation; synthetic DOM events are not trusted OS input.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary, {
        FLAG:'1', 'SUPER_PLATINUM_APP_HOME_STATIC_DISPATCH_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_STATIC_DISPATCH_CASE':'', 'SUPER_PLATINUM_APP_HOME_TEXT_BUTTON_SYNC_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_TEXT_BUTTON_SYNC_CASE':'', 'SUPER_PLATINUM_APP_HOME_TEXT_FOCUS_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_TEXT_FOCUS_CASE':'', 'SUPER_PLATINUM_APP_HOME_TEXT_DISPATCH_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_TEXT_DISPATCH_CASE':'', 'SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_SELECT_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_SELECT_CASE':'', 'SUPER_PLATINUM_APP_HOME_CONVERSATION_SELECT_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_CHANNEL_SELECT_CHECK':'0',
        'SUPER_PLATINUM_APP_HOME_MULTI_CHANNEL_SELECT_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_USER_SELECT_CHECK':'0',
        'SUPER_PLATINUM_APP_HOME_USER_SELECT_CASE':'', 'SUPER_PLATINUM_APP_HOME_MULTI_USER_SELECT_CHECK':'0',
        'SUPER_PLATINUM_APP_HOME_MULTI_USER_SELECT_CASE':'', 'SUPER_PLATINUM_UI_CAPTURE_DIR':str(captures),
        'SUPER_PLATINUM_APP_HOME_MULTI_EXTERNAL_SELECT_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_MULTI_EXTERNAL_SELECT_CASE':'',
        'SUPER_PLATINUM_APP_HOME_EXTERNAL_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_EXTERNAL_CASE':'',
        'SUPER_PLATINUM_APP_HOME_UNIFIED_CONTEXT_CHECK':'0', 'SUPER_PLATINUM_APP_HOME_UNIFIED_CONTEXT_CASE':'',
        'SUPER_PLATINUM_APP_HOME_EXTERNAL_NONCE':nonce,
        'SUPER_PLATINUM_APP_HOME_EXTERNAL_READY_DIR':str(directory)})
    process_api = handle = None
    try:
        with native:
            evidence['pid'] = native.process.pid
            if sys.platform == 'win32':
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence['processCreationFiletime'] = process_api.times(handle)['creation_filetime']
            try:
                with ExitStack() as stack:
                    expected = dict(version=1, fixture=fixture, nonce=nonce, pid=native.process.pid)
                    require(re.fullmatch('[0-9a-f]{32}', nonce) and native.process.pid > 0, 'Invalid owned readiness')
                    deadline = time.monotonic() + 25
                    while not (waiting := native.receipts(WAITING)):
                        native.require_running()
                        require(not native.receipts(PREFIX) and not native.receipts(EDGE)
                                and time.monotonic() < deadline, 'Readiness failed or timed out')
                        time.sleep(.05)
                    require(len(waiting) == 1 and not native.receipts(EDGE) and not native.receipts(PREFIX),
                            'Duplicate readiness or interaction before placement')
                    H['validate_ready'](waiting[0], expected)
                    pointer = H['normalize'](native, evidence, stack, process_api, handle, pointer_mode='passive')
                    H['publish_ready'](directory, expected)
                    evidence['readiness'] = expected
                    # The source's combined recovery/retirement lane has a 240s
                    # terminal deadline. Per-response 250ms and all edge guards stay unchanged.
                    deadline, sampled, count = time.monotonic() + 270, 0., 0
                    while True:
                        native.require_running()
                        edges = native.receipts(EDGE)
                        if pointer:
                            for edge in edges[count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check('running', require_clear=True)
                                sampled = time.monotonic()
                        count = len(edges)
                        if native.receipts(PREFIX):
                            evidence['native'] = validate_log(native, fixture, captures, case)
                            BASE['validate_owned'](evidence['native'], expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'Home external native receipt timed out')
                        time.sleep(.05)
            except BaseException:
                try:
                    path = captures / f'{fixture}-native-error.png'
                    require(not path.exists(), 'Diagnostic capture must be fresh')
                    native.capture(path)
                    evidence['failureScreenshot'] = str(path)
                except (OSError, RuntimeError, ValueError) as error:
                    evidence['failureCaptureError'] = str(error)
                raise
        require(native.process.poll() is not None and not native.cleanup_errors, 'Process or cleanup incomplete')
        if sys.platform == 'win32':
            H['validate_pointer_cleanup'](evidence.get('pointer', {}))
        require(H['closed_receipts'](native.log, WAITING) == [expected], 'Closed readiness changed')
        for prefix in (PREFIX, EDGE):
            require(H['closed_receipts'](native.log, prefix) == native.receipts(prefix), 'Closed log parse changed')
        require(validate_log(native, fixture, captures, case) == evidence['native'], 'Closed receipt changed')
        BASE['validate_owned'](evidence['native'], expected, evidence.get('expectedPngDimensions'))
        if handle:
            evidence['exitIdentity'] = process_api.times(handle)
            H['validate_exit'](evidence['processCreationFiletime'], evidence['exitIdentity'])
        require(H['binary_sha'](binary) == digest, 'Binary changed during native run')
        evidence.update(ok=True, closedLogRevalidated=True)
    except BaseException as error:
        evidence['error'] = str(error)
        raise
    finally:
        evidence['processExited'] = native.process is not None and native.process.poll() is not None
        evidence['applicationExitCode'] = native.process.poll() if native.process is not None else None
        evidence['cleanupErrors'] = native.cleanup_errors
        if handle:
            try:
                evidence['cleanupProcessIdentity'] = process_api.times(handle)
            except OSError as error:
                evidence.update(ok=False, cleanupIdentityError=str(error))
            process_api.kernel.CloseHandle(handle)
        path = captures / f'{fixture}.json'
        path.write_text(json.dumps(evidence, indent=2)+'\n', encoding='utf-8')
        print(json.dumps(dict(receipt=str(path), **evidence)), flush=True)
    require(evidence['ok'], 'Final process identity audit failed')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    p.add_argument('--captures', type=Path, required=True)
    p.add_argument('--fixture', action='append', choices=FIXTURES)
    p.add_argument('--case', choices=tuple(oracle.STAGES), required=True)
    args = p.parse_args()
    fixtures = args.fixture or list(FIXTURES)
    require(len(fixtures) == len(set(fixtures)), 'Duplicate fixture execution')
    for fixture in fixtures:
        run_fixture(fixture, args.case, args.binary.resolve(), args.captures.resolve())


if __name__ == '__main__':
    main()
