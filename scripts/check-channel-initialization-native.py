#!/usr/bin/env python3
"""Offline shared channel initialization acceptance, normal and narrow.

Never builds or loads a real Slack session. Read captured PNGs before claiming
visual acceptance; measured geometry is a bounded observation, not atomic pixels.
"""
import argparse
from contextlib import ExitStack
import hashlib
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import struct
import sys
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from native_check import expected_viewport

PREFIX = 'channel-initialization-check: '
EDGE = 'channel-initialization-capture: '
WAITING = 'channel-initialization-waiting: '
FLAG = 'SUPER_PLATINUM_CHANNEL_INITIALIZATION_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = ('warm', 'init', 'channels', 'rejected', 'socket', 'replacement')
FIXTURES = ('channel-initialization', 'channel-initialization-narrow')


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def ready_tuple(fixture, nonce, pid):
    require(fixture in FIXTURES and isinstance(nonce, str) and re.fullmatch('[0-9a-f]{32}', nonce),
            'Invalid readiness fixture or nonce')
    require(type(pid) is int and pid > 0, 'Invalid owned PID')
    return dict(version=1, fixture=fixture, nonce=nonce, pid=pid)


def validate_ready(actual, expected):
    H['validate_ready'](actual, expected)


def validate_owned(receipt, expected, dimensions=None):
    validate_ready(receipt.get('readiness'), expected)
    if dimensions is not None:
        require(all(row['geometry']['pngDimensions'] == dimensions for row in receipt['stages']),
                'Native capture dimensions changed after placement')


def validate_closed(native, expected, captures, admitted, dimensions=None, *, platform=sys.platform, png=True):
    waiting = H['closed_receipts'](native.log, WAITING)
    require(len(waiting) == 1, 'Missing or duplicate closed readiness request')
    validate_ready(waiting[0], expected)
    # Strict newline parsing also rejects partial trailing terminal/capture records.
    for prefix in (PREFIX, EDGE):
        require(H['closed_receipts'](native.log, prefix) == native.receipts(prefix), 'Closed log parse changed')
    receipt = validate_log(native, expected['fixture'], captures, platform, png)
    require(receipt == admitted, 'Closed log changed after terminal receipt')
    validate_owned(receipt, expected, dimensions)
    return receipt


def rectangle(value, viewport):
    require(isinstance(value, dict), 'Missing measured rectangle')
    require(all(type(value.get(k)) in (int, float) and math.isfinite(value[k]) for k in
                ('left', 'top', 'right', 'bottom', 'width', 'height')), 'Nonfinite rectangle')
    require(value['width'] > 0 and value['height'] > 0
            and abs(value['right'] - value['left'] - value['width']) <= 1
            and abs(value['bottom'] - value['top'] - value['height']) <= 1, 'Invalid rectangle arithmetic')
    require(value['left'] >= 0 and value['top'] >= 0 and value['right'] <= viewport[0] + 1
            and value['bottom'] <= viewport[1] + 1, 'Control or feedback clipped outside viewport')


def validate_png(path, dimensions):
    with path.open('rb') as source:
        header = source.read(24)
    require(len(header) == 24 and header[:8] == b'\x89PNG\r\n\x1a\n' and header[12:16] == b'IHDR', 'Invalid native PNG')
    require(list(struct.unpack('>II', header[16:24])) == dimensions, 'PNG dimensions differ from native capture')


def validate_layout(geometry, viewport):
    require(isinstance(geometry, dict) and geometry.get('ok') is True
            and geometry.get('noOverflow') is True and geometry.get('viewport') == viewport,
            'Missing successful stage geometry')
    rows = geometry.get('expectedRows')
    require(isinstance(rows, list) and rows and geometry.get('actualRows') == rows,
            'Sidebar projection differs from actual DOM')
    require(all(isinstance(row, dict) and set(row) == {'name', 'section'}
                and all(isinstance(value, str) and value for value in row.values()) for row in rows),
            'Missing sidebar name or classification')
    require(len({row['name'] for row in rows}) == len(rows), 'Ambiguous fixture channel name')
    focus, heading = geometry.get('focus'), geometry.get('heading')
    require(isinstance(focus, dict) and focus.get('visible') is True and focus.get('labelsFit') is True,
            'Focused channel hidden or text clipped')
    require({key: focus.get(key) for key in ('name', 'section')} in rows, 'Wrong captured focus channel')
    require(isinstance(heading, dict) and heading.get('visible') is True
            and heading.get('text') == focus['section'], 'Channel classification heading not visible')
    rectangle(focus.get('rect'), viewport)
    rectangle(heading.get('rect'), viewport)
    dimensions = geometry.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2
            and all(type(n) is int and n > 0 for n in dimensions), 'Invalid PNG dimensions')


EXTRAS = ['cache_timestamps', 'features', 'salesforce_features', 'misc', 'subteams',
          'team_prefs', 'stats', 'default_workspace', 'plan_info', 'feature_usage_data']
NAMES = ('warm-harbor', 'warm-harbor', 'refreshed-harbor', 'refreshed-harbor', 'socket-harbor', 'foreign-harbor')
PROOFS = {
    'warm': None,
    'init': dict(bootRegistered=True, acceptedConnectionStarted=True, bootStartedOnce=True,
                 noInitialAuthority=True, missingInitRejected=True, failureDidNotInitialize=True, initAdopted=True),
    'channels': dict(channelsPublished=True),
    'rejected': dict(failureRetainedWarm=True, initialRetired=True, noReplay=True),
    'socket': dict(olderQueuePending=True, acceptedSocket=True, staleSocketRejected=True,
                   nativeName='socket-harbor', sourceName='socket-harbor'),
    'replacement': dict(accountReplaced=True, sameTeam=True, foreignPublicationRejected=True,
                        oldOwnerRetired=True, nativeName='foreign-harbor'),
}


def validate_requests(requests, count):
    require(isinstance(requests, list) and len(requests) == count, 'Wrong prepared request count or replay')
    for index, request in enumerate(requests):
        branch, boot = index % 3, index < 3
        endpoint = ('client.init', 'client.channels', 'client.extras')[branch]
        reason = ('fetch-client-init-data', 'deferred-data', 'fetch-client-extras-data')[branch] if boot else 'resynced-topics'
        expected = {'token': 'synthetic-channel-token', '_x_reason': reason}
        if branch == 0:
            expected['include_relevant_onboarding'] = 'true' if boot else 'false'
        elif branch == 1:
            expected.update(version_all_channels='false', return_all_relevant_mpdms='true')
        else:
            expected['extras'] = json.dumps(EXTRAS, separators=(',', ':'))
        require(isinstance(request, dict) and request.get('method') == 'POST'
                and request.get('endpoint') == f'https://example.test/api/{endpoint}', 'Wrong prepared request route')
        form = request.get('form')
        require(isinstance(form, list) and all(isinstance(pair, list) and len(pair) == 2
                and all(isinstance(v, str) for v in pair) for pair in form), 'Invalid prepared request form')
        require(len(form) == len(expected) and dict(form) == expected, 'Wrong boot/resync request policy')


def validate(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(fixture in FIXTURES, 'Unknown channel initialization fixture')
    require(isinstance(receipt, dict) and receipt.get('ok') is True and receipt.get('fixture') == fixture,
            'Wrong or failed native receipt')
    require(receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview') == 'PNG inspection required separately', 'Missing capture/review boundary')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Incorrect native viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows)
            and [row.get('stage') for row in rows] == list(STAGES), 'Missing or reordered stages')
    require(isinstance(paths, dict) and set(paths) == set(STAGES), 'Wrong capture inventory')
    previous = None
    for index, row in enumerate(rows):
        stage, model, geometry = row['stage'], row.get('model'), row.get('geometry')
        require(isinstance(model, dict) and model.get('enabled') is True, 'Missing enabled fixture model')
        for key in ('networkCalls', 'epoch', 'socketGeneration'):
            require(type(model.get(key)) is int and model[key] >= 0, 'Missing integer model counters')
        require(model['networkCalls'] == 0 and model['socketGeneration'] == (0 if index in (0, 5) else 1)
                and model.get('queuePending') is (index in (0, 5)), 'Network use or wrong warm/publication custody')
        require(model.get('initialPending') is (index in (1, 4)), 'Wrong initial attempt custody')
        account = '00000000-0000-4000-8000-000000000082' if stage == 'replacement' else '00000000-0000-4000-8000-000000000081'
        require(model.get('account') == account, 'Wrong account identity')
        if previous is not None:
            require(model['epoch'] == previous['epoch'] + (stage == 'replacement'), 'Wrong account epoch transition')
        name = NAMES[index]
        section = 'External connections' if index in (2, 3, 4) else 'Channels'
        expected_rows = [dict(name=name, section=section)]
        require(model.get('channels') == [dict(id='CINIT0001', **expected_rows[0])]
                and model.get('focusChannel') == name, 'Wrong native channel state or classification')
        require(row.get('proof') == PROOFS[stage] and model.get('lastProof') == PROOFS[stage],
                'Missing actual controller transition proof')
        # JSON equality alone treats true and 1 as equal: proof flags must be actual booleans.
        if PROOFS[stage] is not None:
            require(all(type(row['proof'][k]) is type(v) and type(model['lastProof'][k]) is type(v)
                        for k, v in PROOFS[stage].items()), 'Invalid transition proof types')
        kinds = [] if index in (0, 5) else ['Init'] if index == 1 else ['Init', 'Extras']
        expected_settlements = [dict(kind=kind, ok=True, failedKind=None) for kind in kinds]
        require(model.get('settlements') == expected_settlements
                and all(item.get('ok') is True for item in model['settlements']), 'Payload did not actually settle successfully')
        validate_requests(model.get('preparedRequests'), (0, 3, 3, 6, 9, 9)[index])
        if previous is not None:
            require(model['preparedRequests'][:len(previous['preparedRequests'])] == previous['preparedRequests'],
                    'Prepared request history changed')
        validate_layout(geometry, viewport)
        require(geometry['expectedRows'] == expected_rows and geometry['focus']['name'] == name,
                'Captured DOM differs from source/native stage')
        path = Path(paths[stage]) if isinstance(paths[stage], str) else None
        require(path is not None and path.is_absolute()
                and path == captures.resolve() / f'{fixture}-{stage}.png', 'Wrong owned capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing native capture')
            validate_png(path, geometry['pngDimensions'])
        previous = model
    require(receipt.get('final') == rows[-1]['model'], 'Final state changed after captured replacement')


def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    expected = [dict(stage=stage, edge=edge) for stage in STAGES for edge in ('before', 'after')]
    require(native.receipts(EDGE) == expected, 'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures):
    require(fixture in FIXTURES, 'Unknown channel initialization fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'channel-initialization-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_CHANNEL_INITIALIZATION_NONCE': nonce,
                            'SUPER_PLATINUM_CHANNEL_INITIALIZATION_READY_DIR': str(directory)})
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
                    expected = ready_tuple(fixture, nonce, native.process.pid)
                    deadline = time.monotonic() + 25
                    while not (waiting := native.receipts(WAITING)):
                        native.require_running()
                        require(not native.receipts(PREFIX) and not native.receipts(EDGE)
                                and time.monotonic() < deadline, 'Readiness failed or timed out')
                        time.sleep(.05)
                    require(len(waiting) == 1 and not native.receipts(EDGE) and not native.receipts(PREFIX),
                            'Duplicate readiness or interaction before placement')
                    validate_ready(waiting[0], expected)
                    pointer = H['normalize'](native, evidence, stack, process_api, handle)
                    H['publish_ready'](directory, expected)
                    evidence['readiness'] = expected
                    deadline, sampled, count = time.monotonic() + 150, 0., 0
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
                            evidence['native'] = validate_log(native, fixture, captures)
                            validate_owned(evidence['native'], expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'Channel initialization native receipt timed out')
                        time.sleep(.05)
            except BaseException:
                try:
                    path = captures / f'{fixture}-failure.png'
                    native.capture(path)
                    evidence['failureScreenshot'] = str(path)
                except (OSError, RuntimeError, ValueError) as error:
                    evidence['failureCaptureError'] = str(error)
                raise
        require(native.process.poll() is not None and not native.cleanup_errors, 'Fixture process or cleanup did not finish cleanly')
        if sys.platform == 'win32':
            H['validate_pointer_cleanup'](evidence.get('pointer', {}))
        validate_closed(native, expected, captures, evidence['native'], evidence.get('expectedPngDimensions'))
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
        evidence['cleanupErrors'] = native.cleanup_errors
        if handle:
            try:
                evidence['cleanupProcessIdentity'] = process_api.times(handle)
            except OSError as error:
                evidence.update(ok=False, cleanupIdentityError=str(error))
            process_api.kernel.CloseHandle(handle)
        path = captures / f'{fixture}.json'
        path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(dict(receipt=str(path), **evidence)), flush=True)
    require(evidence['ok'], 'Final process identity audit failed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/channel-initialization-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    args = parser.parse_args()
    captures = args.captures.resolve()
    for fixture in args.fixture or FIXTURES:
        require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        run_fixture(fixture, args.binary.resolve(), captures)


if __name__ == '__main__':
    main()
