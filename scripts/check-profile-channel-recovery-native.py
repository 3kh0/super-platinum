#!/usr/bin/env python3
"""Offline Profile conditional channel recovery acceptance, normal and narrow.

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

PREFIX = 'profile-channel-recovery-check: '
EDGE = 'profile-channel-recovery-capture: '
WAITING = 'profile-channel-recovery-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = ('initial', 'opened-stale', 'renamed-mounted', 'reopened', 'unchanged', 'failure-retained', 'malformed-retained', 'socket-retired', 'conditional-pending', 'socket-pending', 'pending-reopened', 'account-replaced')
FIXTURES = ('profile-channel-recovery', 'profile-channel-recovery-narrow')


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


CHANNEL = 'CMENTION01'
DRAFT = 'Recovery draft with '
OLD = 'project-harbor'
NEW = 'recovered-harbor'
QUEUED = 'conditional-harbor'
LATEST = 'socket-latest-harbor'
ACCOUNT = '00000000-0000-4000-8000-000000000081'
FOREIGN = '00000000-0000-4000-8000-000000000082'
REFRESH_FAILURE = 'Channel metadata could not refresh.'


def blocks():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=DRAFT),
        dict(type='channel', channel_id=CHANNEL, fixture_occurrence=1),
        dict(type='channel', channel_id=CHANNEL, fixture_occurrence=2)])])]


def layout(value, viewport, stage, label):
    require(isinstance(value, dict) and value.get('ok') is True and value.get('noOverflow') is True
            and value.get('viewport') == viewport, 'Missing successful measured layout')
    replaced = stage == 'account-replaced'
    require(value.get('dialog') is (not replaced), 'Wrong visible dialog owner')
    if not replaced: rectangle(value.get('dialogRect'), viewport)
    controls, notices, tokens = (value.get(k) for k in ('controls', 'notices', 'tokens'))
    require(all(isinstance(v, list) for v in (controls, notices, tokens)), 'Missing actual visible inventory')
    for row in controls + notices + tokens:
        require(row.get('visible') is True, 'Hidden control, notice or token')
        rectangle(row.get('rect'), viewport)
    required = stage in ('failure-retained', 'malformed-retained')
    feedback = any(n.get('text') == REFRESH_FAILURE and n.get('visible') is True for n in notices)
    require(value.get('feedbackRequired') is required and value.get('feedbackVisible') is feedback,
            'Missing actual scoped feedback measurement')
    require(not required or feedback, 'Missing exact readable Profile refresh failure notice')
    require('globalToast' in value, 'Missing separate global toast measurement')
    toast = value['globalToast']
    if toast is not None:
        require(isinstance(toast, dict) and type(toast.get('positioned')) is bool
                and type(toast.get('visible')) is bool and isinstance(toast.get('text'), str)
                and toast.get('visibility') in ('visible', 'hidden', 'collapse'), 'Invalid global toast diagnostic')
        if toast['visible']:
            require(toast['positioned'] and toast['visibility'] == 'visible', 'Visible toast not safely positioned')
            rectangle(toast.get('rect'), viewport)
            require(isinstance(toast.get('dismiss'), dict) and toast['dismiss'].get('visible') is True,
                    'Visible global toast has unreachable dismissal')
            rectangle(toast['dismiss'].get('rect'), viewport)
    wanted = [] if replaced else ['Back', 'Save section', 'Use description', 'Cancel description edit']
    require(sorted(c.get('text') for c in controls) == sorted(wanted), 'Wrong native actions')
    require(all(c.get('disabled') is (c['text'] == 'Save section') for c in controls), 'Wrong action disabled state')
    require(value.get('richEditable') is (None if replaced else True), 'Wrong mounted editability')
    if replaced:
        require(not tokens and value.get('captured') is None, 'Retired editor remains mounted')
    else:
        require(len(tokens) == 2 and all(t.get('text') == label and t.get('editable') is False for t in tokens),
                'Mounted canonical label custody changed')
        for token in tokens:
            fragments = token.get('fragments')
            require(isinstance(fragments, list) and fragments, 'Missing actual inline fragments')
            for fragment in fragments: rectangle(fragment, viewport)
        native = value.get('captured')
        require(isinstance(native, dict) and native.get('snapshot', {}).get('blocks') == blocks(),
                'Native unsaved text or opaque duplicate tokens changed')
        snapshot = native['snapshot']
        require(snapshot.get('composing') is False and type(snapshot.get('revision')) is int
                and snapshot['revision'] >= 0 and isinstance(snapshot.get('generation'), str)
                and snapshot['generation'].isdigit(), 'Missing native capture generation')
        require(native.get('tokens') == [dict(value=v, label=label) for v in blocks()[0]['elements'][0]['elements'][1:]],
                'Native token witness order or metadata changed')
    dimensions = value.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2 and all(type(v) is int and v > 0 for v in dimensions),
            'Invalid PNG dimensions')


def validate(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(fixture in FIXTURES and isinstance(receipt, dict) and receipt.get('fixture') == fixture,
            'Wrong native receipt owner')
    require(receipt.get('ok') is True, f"Native assertion failed: {receipt.get('error', 'missing error')}")
    require(receipt.get('transport') == 'closed entered requests; no HTTP polling', 'Missing synthetic transport boundary')
    require(receipt.get('captureChecksPassed') is True and receipt.get('visualReview') == 'PNG inspection required separately',
            'Missing capture/visual review boundary')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Wrong native viewport')
    rows, paths, requests = (receipt.get(k) for k in ('stages', 'captures', 'requests'))
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES), 'Missing or reordered stages')
    require(isinstance(paths, dict) and set(paths) == set(STAGES), 'Wrong capture inventory')
    require(isinstance(requests, list) and len(requests) == 11, 'Wrong conditional request count')
    serials = []
    for index, request in enumerate(requests):
        require(type(request.get('serial')) is int and request['serial'] > 0, 'Invalid held request serial')
        serials.append(request['serial'])
        require(request.get('method') == ('conversations.genericInfo' if index in (4,6) else 'channels.info'),
                'Wrong actual prepared directory/fallback method')
        version = 101 if index < 2 else 103 if index == 10 else 102
        name = OLD if index < 2 else LATEST if index == 10 else NEW
        require(request.get('ids') == [CHANNEL] and request.get('versions') == {CHANNEL:version},
                'Conditional request lost exact selected ID/version predecessor')
        before = request.get('predecessor', {})
        raw = before.get('source', {})
        require(raw.get('id') == CHANNEL and raw.get('updated') == version
                and raw.get('name') == name and raw.get('fixture_metadata') == dict(retained=True)
                and before.get('nativeName') == raw['name'] and before.get('account') == ACCOUNT
                and before.get('socketGeneration') == (2 if index >= 8 else 1),
                'Missing exact raw/native/owner predecessor before response')
    require(len(set(serials)) == len(serials), 'Conditional request replay or duplicate admission')
    predecessor = None
    for index, row in enumerate(rows):
        stage, model, proof, geometry = (row.get(k) for k in ('stage', 'model', 'proof', 'geometry'))
        require(isinstance(model, dict) and isinstance(proof, dict), 'Missing actual model/proof')
        profile = model.get('profile')
        if stage == 'account-replaced':
            require(model.get('account') == FOREIGN and model.get('epoch') == predecessor['epoch'] + 1,
                    'Account replacement not demonstrated')
            require(profile == dict(enabled=False) and model.get('source') is None and model.get('nativeName') is None,
                    'Retired account published or editor survived')
            require(proof.get('oldOpeningRefused') is True and proof.get('oldEditorHidden') is True
                    and proof.get('previousAccount') == ACCOUNT and proof.get('account') == FOREIGN
                    and proof.get('previousBlocks') == blocks() and proof.get('previousAttempts') == [],
                    'Missing actual replacement/old-opening refusal proof')
            require(model.get('idle') is True, 'Retired account worker did not finish')
        else:
            name, version = (OLD, 101) if index < 2 else (LATEST, 103) if stage == 'pending-reopened' else (NEW, 102)
            require(model.get('account') == ACCOUNT and type(model.get('epoch')) is int, 'Wrong account custody')
            if predecessor: require(model['epoch'] == predecessor['epoch'], 'Unexpected account epoch change')
            source = model.get('source')
            pending = stage in ('conditional-pending', 'socket-pending')
            if pending:
                require(source is None and model.get('nativeName') == (NEW if stage == 'conditional-pending' else LATEST),
                        'Pending source incorrectly exposed as canonical or native order lost')
                status = model.get('conditional', {})
                require(status.get('drainHeld') is True and status.get('held') == 0 and status.get('requestCount') == 10,
                        'Missing actual held drain/settled request proof')
                queues = status.get('queues', [])
                require(len(queues) == 1 and queues[0].get('pending') is True
                        and any(r.get('id') == CHANNEL and r.get('name') == QUEUED and r.get('updated') == 103
                                for r in queues[0].get('incoming', [])), 'Missing actual conditional queued predecessor')
            else:
                require(isinstance(source, dict) and source.get('id') == CHANNEL and source.get('name') == name
                        and source.get('updated') == version and source.get('fixture_metadata') == dict(retained=True)
                        and model.get('nativeName') == name, 'Wrong raw/native accepted canonical row')
            if index in (1, 3, 4, 5, 6, 7):
                require(source == predecessor['source'], 'Unchanged/failure/retired response changed predecessor')
            require(profile.get('enabled') is True and profile.get('blocks') == blocks()
                    and profile.get('dirty') is True and profile.get('busy') is False and profile.get('uncertain') is False,
                    'Unsaved Profile draft or token custody changed')
            require(profile.get('attempts') == [] and profile.get('reads') == 0, 'Recovery sent or read Profile data')
            if stage in ('failure-retained', 'malformed-retained'):
                require(profile.get('notice') == REFRESH_FAILURE, 'Current Profile editor lacks owned failure notice')
            require(profile.get('choiceValue') == 'Morning' and profile.get('countryValue') == 'US'
                    and profile.get('managedValue') == 'Preserved organization value', 'Untouched sibling changed')
            require(model.get('idle') is (stage not in ('opened-stale', 'conditional-pending', 'socket-pending')), 'Wrong held/settled conditional state')
            if stage == 'initial': require(proof == dict(initialConditionalRead=True), 'Initial refresh not exercised')
            elif stage == 'opened-stale':
                require(proof == dict(topicRetainedPredecessor=True, topicDidNotFetch=True), 'Topic changed row or fetched without hit')
            elif stage == 'renamed-mounted': require(proof == dict(mountedLabelRetained=True), 'Missing mounted-label stability')
            elif stage == 'reopened': require(proof == dict(newCanonicalLabel=True), 'Missing reopened canonical label')
            elif stage in ('unchanged', 'failure-retained', 'malformed-retained'):
                require(proof == dict(predecessorRetained=True, fallbackAttempted=stage!='unchanged', topic=dict(topicRetainedPredecessor=True, topicDidNotFetch=True)),
                        'Missing unchanged/failure/topic retention proof')
            elif stage == 'socket-retired':
                socket = proof.get('socket', {})
                require(proof.get('latePublicationRefused') is True and proof.get('successorRead') is True and socket.get('before') == predecessor['socketGeneration']
                        and socket.get('after') == socket['before'] + 1 and model.get('socketGeneration') == socket['after'],
                        'Retired socket publication not demonstrated')
            elif stage == 'conditional-pending':
                require(proof == dict(conditionalQueued=True, canonicalUnavailable=True), 'Missing pending conditional proof')
            elif stage == 'socket-pending':
                topic = proof.get('topic', {})
                require(proof.get('socketAccepted') is True and proof.get('canonicalUnavailable') is True
                        and topic.get('source') is None and topic.get('nativeName') == NEW
                        and topic.get('socketGeneration') == model.get('socketGeneration') == 2,
                        'Missing actual pending topic/socket ordering')
                require(all(CHANNEL not in r.get('freshIds', []) for r in topic.get('conditional', {}).get('managers', []))
                        and topic.get('conditional', {}).get('managers'), 'Topic did not stale pending predecessor')
                require(any(CHANNEL in r.get('freshIds', []) for r in model['conditional'].get('managers', [])),
                        'Accepted pending socket did not restore freshness')
            elif stage == 'pending-reopened':
                status = model.get('conditional', {})
                require(proof == dict(latestSocketLabel=True, noRedundantRead=True, requestCount=10)
                        and status.get('requestCount') == 10 and status.get('drainHeld') is False and status.get('held') == 0,
                        'Pending reopen scheduled redundant conditional request')
                require(status.get('managers') and all(r.get('worker') is False and r.get('active') is False
                        and r.get('pendingIds') == [] and CHANNEL in r.get('freshIds', []) for r in status['managers']),
                        'Pending reopen lacks settled source freshness')
                require(status.get('queues') and all(q.get('pending') is False for q in status['queues']),
                        'Released source drain not complete')
            predecessor = model
        label = '#'+(OLD if index < 3 else LATEST if stage == 'pending-reopened' else NEW)
        layout(geometry, viewport, stage, label)
        path = Path(paths[stage]) if isinstance(paths[stage], str) else None
        require(path is not None and path.is_absolute() and path == captures.resolve()/f'{fixture}-{stage}.png', 'Wrong owned capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing native PNG')
            validate_png(path, geometry['pngDimensions'])


def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    require(native.receipts(EDGE) == [dict(stage=s, edge=e) for s in STAGES for e in ('before','after')],
            'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures):
    require(fixture in FIXTURES, 'Unknown profile channel recovery fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'profile-channel-recovery-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_NONCE': nonce,
                            'SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_READY_DIR': str(directory)})
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
                        require(time.monotonic() < deadline, 'Profile channel recovery native receipt timed out')
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-channel-recovery-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    args = parser.parse_args()
    captures = args.captures.resolve()
    for fixture in args.fixture or FIXTURES:
        require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        run_fixture(fixture, args.binary.resolve(), captures)


if __name__ == '__main__':
    main()
