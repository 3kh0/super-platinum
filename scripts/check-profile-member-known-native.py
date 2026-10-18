#!/usr/bin/env python3
"""Offline Profile conditional member recovery acceptance, normal and narrow.

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

PREFIX = 'profile-member-known-check: '
EDGE = 'profile-member-known-capture: '
WAITING = 'profile-member-known-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_MEMBER_RECOVERY_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = {
    'stale': ('initial', 'ordinary-open', 'held', 'malformed', 'offline', 'repair-held', 'success', 'reopened'),
    'terminal': ('initial', 'ordinary-open', 'held', 'success', 'reopened'),
    'account': ('initial', 'ordinary-open', 'held', 'account-replaced'),
}
FIXTURES = ('profile-member-known', 'profile-member-known-narrow')

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



MEMBER = 'UMISSING01'
LABEL = '  Cafe\u0301 Harbor  '
DRAFT = 'Keep my unsaved description with '
LOADING = 'Loading mention names. Your description is unchanged.'
ACCOUNT = '00000000-0000-4000-8000-000000000081'
FOREIGN = '00000000-0000-4000-8000-000000000082'
GUIDANCE = ('Mention names are unavailable or changed while opening. Your description is unchanged. '
            'Retry to load member, channel, and group details.')


def blocks():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=DRAFT),
        *[dict(type='user', user_id=MEMBER, fixture_occurrence=i, opaque={'keep': True}) for i in (1, 2)]])])]


def layout(value, viewport, stage):
    require(isinstance(value, dict) and value.get('ok') is True and value.get('noOverflow') is True
            and value.get('viewport') == viewport, 'Missing successful measured layout')
    replaced = stage == 'account-replaced'
    editor = stage in ('success', 'reopened')
    waiting = stage in ('ordinary-open', 'held', 'malformed', 'offline', 'repair-held')
    loading = stage in ('held', 'repair-held')
    require(value.get('dialog') is (not replaced), 'Wrong dialog owner')
    if not replaced: rectangle(value.get('dialogRect'), viewport)
    controls, notices, tokens = [value.get(k) for k in ('controls', 'notices', 'tokens')]
    require(all(isinstance(v, list) for v in (controls, notices, tokens)), 'Missing native inventory')
    for row in controls + notices + tokens:
        require(row.get('visible') is True, 'Hidden action, feedback or token')
        rectangle(row.get('rect'), viewport)
    feedback = any(n.get('text') == (LOADING if loading else GUIDANCE) and n.get('visible') is True for n in notices)
    require(value.get('feedbackRequired') is waiting and value.get('feedbackVisible') is feedback
            and (not waiting or feedback), 'Missing exact readable Retry guidance')
    wanted = [] if replaced else ['Back', 'Save section'] + (
        ['Use description', 'Cancel description edit'] if editor else
        ['Retry description', 'Cancel description edit'] if waiting else ['Edit description'])
    require(sorted(c.get('text') for c in controls) == sorted(wanted), 'Wrong action inventory')
    require(all(c.get('disabled') is ((c['text'] == 'Save section' and (editor or waiting)) or (c['text'] == 'Retry description' and loading)) for c in controls),
            'Wrong action disabled state')
    require(value.get('richEditable') is (True if editor else None), 'Wrong editor editability')
    if editor:
        require(len(tokens) == 2 and all(t.get('text') == '@' + LABEL and t.get('editable') is False for t in tokens),
                'Wrong exact canonical member label')
        for token in tokens:
            require(isinstance(token.get('fragments'), list) and token['fragments'], 'Missing token fragments')
            for fragment in token['fragments']: rectangle(fragment, viewport)
        captured = value.get('captured')
        require(isinstance(captured, dict) and captured.get('snapshot', {}).get('blocks') == blocks(),
                'Original native draft or opaque duplicate tokens changed')
        snapshot = captured['snapshot']
        require(snapshot.get('composing') is False and type(snapshot.get('revision')) is int
                and isinstance(snapshot.get('generation'), str) and snapshot['generation'], 'Invalid renderer capture')
        require(captured.get('tokens') == [dict(value=b, label='@' + LABEL)
                for b in blocks()[0]['elements'][0]['elements'][1:]], 'Captured token witnesses changed')
    else:
        require(not tokens and value.get('captured') is None, 'Editor mounted before source admission or after retirement')
    require('globalToast' in value, 'Missing independent toast measurement')
    toast = value['globalToast']
    if toast is not None:
        require(isinstance(toast, dict) and type(toast.get('visible')) is bool
                and type(toast.get('positioned')) is bool, 'Invalid toast diagnostic')
        if toast['visible']:
            require(toast['positioned'] and toast.get('visibility') == 'visible', 'Unsafe visible toast')
            rectangle(toast.get('rect'), viewport)
            require(toast.get('dismiss', {}).get('visible') is True, 'Visible toast cannot be dismissed')
            rectangle(toast['dismiss'].get('rect'), viewport)


def requests(rows, count):
    require(isinstance(rows, list) and len(rows) == count, 'Wrong production request count')
    for i, row in enumerate(rows):
        member = i % 2 == 1
        require(row.get('serial') == i + 1 and row.get('kind') == ('member' if member else 'discovery')
                and row.get('method') == ('users/info' if member else 'api.getFlannelHttpUrl'), 'Wrong request sequence')
        require(row.get('account') == ACCOUNT and row.get('team') == 'TINIT0001' and row.get('epoch') == 0,
                'Request escaped original owner')
        wanted = dict(ids=[MEMBER], check_interaction=True, include_profile_only_users=True) if member else {
            'include_external_workspaces': 'true', '_x_reason': 'client-needs-valid-flannel-url'}
        require(row.get('body') == wanted, 'Wrong Read IDs, flags or leaked credential field')
        require(type(row.get('released')) is bool, 'Missing actual held callback state')


def validate(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(isinstance(receipt, dict) and receipt.get('ok') is True,
            f"Native assertion failed: {receipt.get('error', 'missing successful receipt') if isinstance(receipt, dict) else receipt}")
    require(fixture in FIXTURES and receipt.get('fixture') == fixture, 'Wrong native fixture')
    case = receipt.get('case')
    require(case in STAGES, 'Unknown scenario case')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport and receipt.get('captureChecksPassed') is True,
            'Wrong viewport or incomplete measured capture checks')
    stages = receipt.get('stages')
    require(isinstance(stages, list) and [s.get('stage') for s in stages] == list(STAGES[case]), 'Wrong stage order')
    require(isinstance(receipt.get('captures'), dict) and set(receipt['captures']) == set(STAGES[case]), 'Incomplete captures')
    final_count = 6 if case == 'stale' else 2
    requests(receipt.get('requests'), final_count)
    require(all(r['released'] for r in receipt['requests']), 'Unreleased final callback')
    counts = {'initial': (0, 0), 'ordinary-open': (0, 0), 'held': (2, 0), 'malformed': (2, 1),
              'offline': (4, 2), 'repair-held': (6, 2), 'success': (final_count, final_count // 2),
              'reopened': (final_count, final_count // 2), 'account-replaced': (2, 1)}
    proofs = {'initial': ('initialRawKnown', 'originalDraft'), 'ordinary-open': ('noForcedRead', 'originalDraft'),
              'held': ('explicitRetryRead', 'duplicateIdsCoalesced'), 'malformed': ('sourceRetained', 'draftRetained'),
              'offline': ('sourceRetained', 'draftRetained'), 'repair-held': ('sourceUnchanged', 'nativeDriftBefore'),
              'success': ('actualReadPageAndPrivateAdoption', 'sameVersionPositive', 'originalDraft'),
              'reopened': ('noRedundantRead', 'originalDraft'), 'account-replaced': ('actualInstallLoaded',)}
    original = stages[0]['model']
    version = 0 if case == 'terminal' else 101
    for row in stages:
        stage, model, geometry = row['stage'], row.get('model'), row.get('geometry')
        require(isinstance(model, dict), 'Missing actual model')
        require(all(row.get('proof', {}).get(k) is True for k in proofs[stage]), 'Missing executed stage proof')
        replaced = stage == 'account-replaced'
        editor = stage in ('success', 'reopened')
        held = stage in ('held', 'repair-held')
        require(model.get('account') == (FOREIGN if replaced else ACCOUNT) and model.get('epoch') == (1 if replaced else 0),
                'Wrong stage owner')
        profile = model.get('profile', {})
        if not replaced:
            require(profile.get('enabled') is True and profile.get('blocks') == blocks()
                    and profile.get('dirty') is True and profile.get('attempts') == [] and profile.get('reads') == 0,
                    'Draft custody changed or Profile write/readback occurred')
            require(model.get('sourceObserved') is True and model.get('sourcePresent') is True,
                    'Known source record disappeared')
            source = model.get('source')
            require(isinstance(source, dict) and source.get('id') == MEMBER and source.get('updated') == version,
                    'Source version changed or target lost')
            require((source.get('isNonExistent') is True) is (case == 'terminal' and not editor),
                    'Terminal version-zero record was not replaced by positive source')
            require(model.get('invalidated') is (case != 'terminal' and not editor), 'Wrong stale source admission')
            native = model.get('native')
            require(isinstance(native, dict) and native.get('id') == MEMBER, 'Missing actual native member')
            require(model.get('absentDriver') == dict(started=0, finished=0, active=0), 'Known target entered absent-member driver')
            if editor:
                require(source.get('profile', {}).get('display_name') == LABEL
                        and native.get('profile', {}).get('display_name') == LABEL,
                        'Same-version canonical/native member repair failed')
            else:
                require(source == original['source'] and model.get('interaction') == original.get('interaction')
                        and model.get('disconnected') == original.get('disconnected'), 'Failure changed source or membership observations')
                if stage == 'repair-held':
                    require(native.get('profile', {}).get('display_name') == 'Native drift must be repaired'
                            and native != original['native'], 'Missing deliberate existing native drift')
                else:
                    require(native == original['native'], 'Failure changed original native member')
        else:
            require(profile.get('enabled') is False and model.get('sourcePresent') is False
                    and model.get('native') is None and model.get('source') is None, 'Retired account adopted old result')
            proof = row['proof']
            require(proof.get('before', {}).get('profile', {}).get('blocks') == blocks()
                    and proof.get('before', {}).get('source') == original['source']
                    and proof.get('after', {}).get('account') == FOREIGN, 'Missing actual replacement custody')
        count, finished = counts[stage]
        requests(model.get('requests'), count)
        require(model.get('driver') == dict(started=finished + held, finished=finished, active=int(held)),
                'Known completion not observed after actual adoption/refusal')
        pending = model.get('pending')
        require(isinstance(pending, list) and len(pending) == int(held), 'Wrong held request state')
        if pending: require(pending[0] == model['requests'][-1] and pending[0]['released'] is False, 'Wrong held member request')
        layout(geometry, viewport, stage)
        path = Path(receipt['captures'][stage])
        require(path.resolve() == (captures / f'{fixture}-{stage}.png').resolve(), 'Capture escaped expected path')
        dimensions = geometry.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(v) is int and v > 0 for v in dimensions), 'Missing PNG dimensions')
        if png: validate_png(path, dimensions)
def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    require(native.receipts(EDGE) == [dict(stage=s, edge=e) for s in STAGES[receipt['case']] for e in ('before','after')],
            'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures, case):
    require(fixture in FIXTURES, 'Unknown profile member recovery fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'profile-member-known-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    case=case, readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_CASE': case, 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_NONCE': nonce,
                            'SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_READY_DIR': str(directory)})
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
                            require(evidence['native']['case'] == case, 'Wrong native scenario case')
                            validate_owned(evidence['native'], expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'Profile member recovery native receipt timed out')
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-member-known-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    parser.add_argument('--case', action='append', choices=tuple(STAGES))
    args = parser.parse_args()
    captures = args.captures.resolve()
    for case in args.case or tuple(STAGES):
        for fixture in args.fixture or FIXTURES:
            target = captures / case
            require(not (target / f'{fixture}.log').exists(), 'Use a fresh capture directory')
            run_fixture(fixture, args.binary.resolve(), target, case)


if __name__ == '__main__':
    main()
