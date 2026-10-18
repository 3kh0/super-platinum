#!/usr/bin/env python3
"""Offline Profile existing channel/user mention acceptance, normal and narrow.

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

PREFIX = 'profile-sections-mentions-check: '
EDGE = 'profile-sections-mentions-capture: '
WAITING = 'profile-sections-mentions-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_MENTIONS_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = ('opened', 'confirmed', 'rejected', 'uncertain', 'checked', 'stale-metadata', 'unavailable', 'retried', 'limit5000', 'limit5001', 'account-replaced')
FIXTURES = ('profile-sections-mentions', 'profile-sections-mentions-narrow')


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
LABEL = '#project-harbor'
MODE = dict(users=False, groups=False)
GROUP = 'SMENTION01'
GROUP_LABEL = '@@ Cafe\u0301 crew  '
GROUP_STAGES = ('group-empty', 'group-unknown', 'group-deactivated')

def stages():
    return STAGES[:8] + GROUP_STAGES + STAGES[8:] if MODE['groups'] else STAGES

def kind():
    return 'usergroup' if MODE['groups'] else 'user' if MODE['users'] else 'channel'

def prefix(stage):
    return {'opened':'Working with ', 'confirmed':'Saved with ', 'rejected':'Rejected with ',
            'uncertain':'Uncertain with ', 'checked':'Uncertain with ', 'stale-metadata':'Saved with ',
            'unavailable':'Saved with ', 'retried':'Saved with ', 'group-empty':'x'*5000,
            'group-unknown':'Saved with ', 'group-deactivated':'Saved with ', 'limit5000':'x'*4990, 'limit5001':'x'*4991}[stage]

def group_raw(handle):
    return dict(id=GROUP, team_id='TINIT0001', handle=handle, name='Fixture crew', date_delete=0, is_visible=True, fixture_metadata=dict(keep=True))
USER = 'UMENTION01'
USER_LABEL = '@  Cafe\u0301 Harbor  '


def label(stage=None):
    if MODE['groups']:
        return '' if stage=='group-empty' else '@unknown-group' if stage=='group-unknown' else GROUP_LABEL
    return USER_LABEL if MODE['users'] else LABEL


def proofs():
    result = dict(PROOFS)
    if MODE['users']:
        result['opened'] = dict(canonicalLabel=True, mountedLabelStable=True,
                                sourceName='renamed-member', nativeName='renamed-member',
                                mountedLabel=USER_LABEL, rawTokenCount=2)
    if MODE['groups']:
        result['opened'] = dict(canonicalLabel=True, mountedLabelStable=True,
                                sourceName='renamed-group', nativeName='renamed-group',
                                mountedLabel=GROUP_LABEL, rawTokenCount=2)
        result['group-empty'] = dict(observedLabel='', emptyContributesZero=True, text5000Accepted=True, text5001Refused=True, rawTokenCount=2)
        unknown=group_raw('ignored-handle');unknown['isUnknown']=True
        deactivated=group_raw(GROUP_LABEL);deactivated.update(date_delete=123,is_visible=False)
        for stage,raw in [('group-unknown',unknown),('group-deactivated',deactivated)]:
            result[stage]=dict(observedLabel=label(stage),observedRow=raw,rawTokenCount=2,originalDraft=True)
    return result

UNAVAILABLE = 'Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'
REFUSED = 'Could not use this description. Your text is still here. Try again.'
SAVED = 'Profile fields saved.'
REJECTED = 'Slack could not save these fields. Your draft is retained.'
UNCERTAIN = 'Slack has not confirmed this change. Check your profile before saving again.'
MISMATCH = 'Slack currently shows different values. Check again, or keep the current section without sending another change.'
PROOFS = {
    'opened': dict(canonicalLabel=True, mountedLabelStable=True, sourceName='renamed-harbor',
                   nativeName='renamed-harbor', mountedLabel=LABEL, rawTokenCount=2),
    'confirmed': dict(confirmedSave=True, duplicateGuard=True),
    'rejected': dict(rejectionRetainsDraft=True),
    'uncertain': dict(uncertainRetainsDraft=True),
    'checked': dict(repeatedCheckNoReplay=True),
    'stale-metadata': dict(staleOpeningRefused=True, draftUnchanged=True, noWrite=True),
    'unavailable': dict(retryRetainsDraft=True, noWrite=True),
    'retried': dict(retryOpened=True, draftRetained=True),
    'limit5000': dict(sourceLength=5000, accepted=True, rawTokenCount=2),
    'limit5001': dict(sourceLength=5001, retainedInvalid=True, saveDisabled=True, reopenedExact=True, rawTokenCount=2),
}


def blocks(text):
    def token(occurrence):
        if MODE['groups']:
            return dict(type='usergroup',usergroup_id=GROUP,fixture_occurrence=occurrence,tab_id='opaque-tab',opaque=dict(keep=True))
        identity = dict(type='user', user_id=USER) if MODE['users'] else dict(type='channel', channel_id=CHANNEL)
        return dict(**identity, fixture_occurrence=occurrence)
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=text), token(1), token(2)])])]


def validate_layout(geometry, viewport, stage):
    require(isinstance(geometry, dict) and geometry.get('ok') is True
            and geometry.get('noOverflow') is True and geometry.get('viewport') == viewport,
            'Missing successful stage geometry')
    dialog = stage != 'account-replaced'
    require(geometry.get('dialog') is dialog, 'Wrong actual dialog ownership')
    rich = stage in ('opened', 'confirmed', 'rejected', 'retried', 'limit5000') or (MODE['groups'] and stage in GROUP_STAGES)
    empty = MODE['groups'] and stage=='group-empty'
    if dialog:
        rectangle(geometry.get('dialogRect'), viewport)
    controls, notices, tokens = (geometry.get(key) for key in ('controls', 'notices', 'tokens'))
    require(all(isinstance(v, list) for v in (controls, notices, tokens)), 'Missing measured UI inventory')
    for item in controls + notices + ([] if empty else tokens):
        require(isinstance(item, dict) and item.get('visible') is True, 'Hidden control, feedback or token')
        rectangle(item.get('rect'), viewport)
    if empty:
        for token in tokens:
            r=token.get('rect',{})
            require(token.get('visible') is False and token.get('text') == '' and r.get('width') == 0,
                    'Empty group invented visible text or width')
            require(all(type(r.get(k)) in (int,float) and math.isfinite(r[k]) for k in ('left','top','right','bottom','width','height'))
                    and r['height']>=0 and r['left']==r['right'] and abs(r['bottom']-r['top']-r['height'])<=1
                    and 0<=r['left']<=viewport[0]+1 and 0<=r['top']<=r['bottom']<=viewport[1]+1,
                    'Empty group anchor escaped editor viewport')
    wanted = ['Back'] if dialog else []
    if stage in ('uncertain', 'checked'):
        wanted += ['Check profile']
        if stage == 'checked': wanted += ['Keep current section']
    elif dialog:
        wanted += ['Save section']
    if rich: wanted += ['Use description', 'Cancel description edit']
    if stage in ('stale-metadata', 'unavailable'): wanted += ['Retry description', 'Cancel description edit']
    require(sorted(c.get('text') for c in controls) == sorted(wanted), 'Wrong visible actions')
    require(all(type(c.get('disabled')) is bool for c in controls), 'Missing actual button state')
    require(all(c['disabled'] is (c['text'] == 'Save section') for c in controls), 'Action incorrectly enabled or disabled')
    require(geometry.get('richEditable') is (True if rich else None), 'Wrong rich editor lock state')
    require(len(tokens) == (2 if rich else 0) and all(t.get('text') == label(stage) and t.get('editable') is False for t in tokens),
            'Canonical duplicate labels missing or editable')
    expected_notice = {'confirmed': SAVED, 'rejected': REJECTED, 'uncertain': UNCERTAIN, 'checked': MISMATCH,
                       'stale-metadata': UNAVAILABLE, 'unavailable': UNAVAILABLE, 'limit5001': "Check this field's value or length."}.get(stage)
    texts = [n.get('text') for n in notices]
    if expected_notice: require(expected_notice in texts, 'Missing exact visible recovery guidance')
    if stage in ('uncertain', 'checked'):
        require(geometry.get('longDisabled') is True and geometry.get('managedFieldPreserved') is True,
                'Unresolved section controls unlocked or managed field changed')
        require('Checking reads your saved section. It does not send the change again.' in texts,
                'Missing no-replay guidance')
    if stage == 'limit5001':
        require(geometry.get('longDisabled') is False and geometry.get('managedFieldPreserved') is True,
                'Invalid description must remain editable with managed sibling preserved')
    if stage in ('uncertain', 'checked', 'limit5001'):
        require(geometry.get('choiceValue') == 'Morning' and geometry.get('countryValue') == 'US',
                'Untouched native selects differ from model')
    captured = geometry.get('captured')
    if rich:
        text = prefix(stage)
        require(isinstance(captured, dict) and captured.get('snapshot', {}).get('blocks') == blocks(text),
                'Native opaque blocks or text changed')
        snapshot = captured['snapshot']
        require(snapshot.get('composing') is False and type(snapshot.get('revision')) is int and snapshot['revision'] >= 0
                and isinstance(snapshot.get('generation'), str) and snapshot['generation'].isdigit(), 'Invalid native snapshot custody')
        require(captured.get('tokens') == [dict(value=v, label=label(stage)) for v in blocks(text)[0]['elements'][0]['elements'][1:]],
                'Native token witnesses changed order, metadata or canonical labels')
    else:
        require(captured is None, 'Hidden editor leaked a native capture')
    dimensions = geometry.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2
            and all(type(n) is int and n > 0 for n in dimensions), 'Invalid PNG dimensions')


def validate(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(fixture in FIXTURES and isinstance(receipt, dict)
            and receipt.get('fixture') == fixture, 'Wrong native receipt owner')
    require(receipt.get('ok') is True,
            f"{fixture}: native assertion failed: {receipt.get('error', 'missing native error')}")
    require(receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview') == 'PNG inspection required separately', 'Missing capture/review boundary')
    require(receipt.get('mentionKind') == kind(), 'Wrong mention mode')
    expected_proofs = proofs()
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Incorrect native viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows)
            and [row.get('stage') for row in rows] == list(stages()), 'Missing or reordered stages')
    require(isinstance(paths, dict) and set(paths) == set(stages()), 'Wrong capture inventory')
    previous = None
    for index, row in enumerate(rows):
        stage, model, proof, geometry = row['stage'], row.get('model'), row.get('proof'), row.get('geometry')
        require(isinstance(model, dict) and isinstance(proof, dict), 'Missing actual state or proof')
        if stage == 'account-replaced':
            require(model == dict(enabled=False), 'Old profile editor remains available after replacement')
            require(proof.get('oldOpeningRefused') is True and proof.get('oldEditorHidden') is True,
                    'Foreign account admitted stale opening')
            require(proof.get('previousAccount') == previous['account']
                    and proof.get('account') == '00000000-0000-4000-8000-000000000082'
                    and type(proof.get('epoch')) is int and proof['epoch'] == previous['epoch'] + 1
                    and proof.get('previousEpoch') == previous['epoch'], 'Account replacement not demonstrated')
            require(proof.get('previousAttempts') == previous['attempts'] and proof.get('previousBlocks') == previous['blocks'],
                    'Replacement predecessor custody differs')
        else:
            require(model.get('enabled') is True, 'Closed disconnected fixture identity not current')
            require(model.get('account') == '00000000-0000-4000-8000-000000000081'
                    and type(model.get('epoch')) is int, 'Wrong profile account identity')
            if previous: require(model['epoch'] == previous['epoch'], 'Unexpected account epoch change')
            require(model.get('blocks') == blocks(prefix(stage)), 'Section lost text or exact duplicate tokens')
            require(model.get('managedValue') == 'Preserved organization value', 'Managed sibling changed')
            require(model.get('choiceValue') == 'Morning' and model.get('countryValue') == 'US',
                    'Untouched model selects changed')
            require(model.get('busy') is False and model.get('uncertain') is (stage in ('uncertain', 'checked')),
                    'Wrong pending/lock state')
            require(model.get('dirty') is (stage in ('rejected','uncertain','checked','group-empty','limit5000','limit5001')), 'Draft dirtiness changed')
            require(model.get('canSave') is (stage in ('rejected','group-empty','limit5000')) and model.get('canCheck') is (stage in ('uncertain','checked')),
                    'Unsafe write or readback availability')
            require(proof == expected_proofs[stage] and all(type(proof[k]) is type(v) for k, v in expected_proofs[stage].items()),
                    'Missing exact production transition proof')
            attempts = model.get('attempts')
            require(isinstance(attempts, list) and len(attempts) == min(index, 3), 'Wrong write count or replay')
            require(type(model.get('reads')) is int and model['reads'] == (0 if index == 0 else 1 if index < 4 else 3),
                    'Wrong actual readback count')
            for write, text in zip(attempts, ('Saved with ', 'Rejected with ', 'Uncertain with ')):
                require(isinstance(write, list), 'Missing prepared section elements')
                values = {item.get('element_id'): item for item in write}
                require(len(values) == len(write) and 'long' in values and 'locked' in values, 'Invalid section write inventory')
                raw = values['long'].get('long_text', {}).get('blocks')
                require(isinstance(raw, str) and json.loads(raw) == blocks(text), 'Wire lost duplicate tokens or used displayed label as ID')
                require(values['locked'].get('text', {}).get('text') == 'Preserved organization value', 'Wire changed managed sibling')
            if previous: require(attempts[:len(previous['attempts'])] == previous['attempts'], 'Write history changed')
            previous = model
        validate_layout(geometry, viewport, stage)
        path = Path(paths[stage]) if isinstance(paths[stage], str) else None
        require(path is not None and path.is_absolute() and path == captures.resolve() / f'{fixture}-{stage}.png',
                'Wrong owned capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing native capture')
            validate_png(path, geometry['pngDimensions'])


def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    expected = [dict(stage=stage, edge=edge) for stage in stages() for edge in ('before', 'after')]
    require(native.receipts(EDGE) == expected, 'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures):
    require(fixture in FIXTURES, 'Unknown profile mentions fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'profile-sections-mentions-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), mentionKind=kind(), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK': '1' if MODE['users'] else '0',
                            'SUPER_PLATINUM_PROFILE_GROUP_MENTIONS_CHECK': '1' if MODE['groups'] else '0',
                            'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_PROFILE_MENTIONS_NONCE': nonce,
                            'SUPER_PLATINUM_PROFILE_MENTIONS_READY_DIR': str(directory)})
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
                        require(time.monotonic() < deadline, 'Profile mentions native receipt timed out')
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-sections-mentions-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--users', action='store_true', help='Exercise existing user tokens with exact member labels')
    mode.add_argument('--groups', action='store_true', help='Exercise existing usergroup tokens with observed row labels')
    args = parser.parse_args()
    MODE['users'] = args.users
    MODE['groups'] = args.groups
    captures = args.captures.resolve()
    for fixture in args.fixture or FIXTURES:
        require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        run_fixture(fixture, args.binary.resolve(), captures)


if __name__ == '__main__':
    main()
