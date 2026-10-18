#!/usr/bin/env python3
"""Offline native Profile ordinary-channel completion acceptance.

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

SCOPE = 'ordinary-channel-completion'
PREFIX = 'profile-channel-completion-check: '
EDGE = 'profile-channel-completion-capture: '
WAITING = 'profile-sections-mentions-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_CHANNEL_COMPLETION_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = ('opened', 'local', 'refreshed', 'keyboard', 'mouse', 'saved', 'query-retired', 'malformed', 'failure', 'empty', 'cancelled', 'stale-refused')
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



FAILURE = 'Channel suggestions could not load. Edit your query to try again.'
LABELS = ['#project-harbor', '#project-completion', '#project-cove']
METHODS = {'channels/search','users.prefs.get','experiments.getByUser','client.gantryBoot',
           'api.getFlannelHttpUrl','drafts.list','drafts.listActive','search.autocomplete.model','search.autocomplete.offlineFeatures'}

def initial():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text',text='Working with ',style=dict(bold=True)),
        dict(type='channel',channel_id='CMENTION01',fixture_occurrence=1,style=dict(italic=True)),
        dict(type='channel',channel_id='CMENTION01',fixture_occurrence=2)])])]

def elements(blocks):
    require(isinstance(blocks,list) and len(blocks)==1 and blocks[0].get('type')=='rich_text', 'Wrong rich block shape')
    sections=blocks[0].get('elements')
    require(isinstance(sections,list) and len(sections)==1 and sections[0].get('type')=='rich_text_section', 'Wrong native section shape')
    values=sections[0].get('elements')
    require(isinstance(values,list) and values[:3]==initial()[0]['elements'][0]['elements'], 'Original text style or duplicate opaque token changed')
    return values

def validate_native(native,count):
    require(isinstance(native,dict), 'Missing actual native resolved capture')
    snapshot=native.get('snapshot',{})
    require(snapshot.get('composing') is False and type(snapshot.get('revision')) is int and snapshot['revision']>=0
            and isinstance(snapshot.get('generation'),str) and snapshot['generation'].isdigit(), 'Invalid native snapshot custody')
    values=elements(snapshot.get('blocks'))
    tokens=[v for v in values if v.get('type')=='channel']
    expected=initial()[0]['elements'][0]['elements'][1:]
    if count>=3: expected=expected+[dict(type='channel',channel_id='CCOMPLETE1')]
    if count>=4: expected=expected+[dict(type='channel',channel_id='CCOMPLETE2')]
    require(tokens==expected and len(tokens)==count, 'Completion token IDs, order, style or opaque fields changed')
    labels=[LABELS[0],LABELS[0]]+LABELS[1:count-1]
    require(native.get('tokens')==[dict(value=v,label=l) for v,l in zip(tokens,labels)], 'Token display/source witnesses differ')
    return snapshot['blocks']

def validate_source(source):
    require(isinstance(source,dict) and source.get('networkCalls')==0, 'Live network or missing closed producer')
    requests=source.get('requests')
    require(isinstance(requests,list), 'Missing actual request ledger')
    for i,row in enumerate(requests,1):
        require(row.get('serial')==i and row.get('method') in METHODS and isinstance(row.get('args'),dict), 'Request ledger identity or endpoint changed')
        require(not {'token','enterprise_token','cookie','cookies'}.intersection(row['args']), 'Secret in request evidence')
        if row['method']=='channels/search':
            args=row['args']
            require(isinstance(args.get('query'),str) and not args['query'].startswith('#') and args.get('count')==25
                    and args.get('fuzz')==1 and args.get('filter')=='xws' and args.get('include_record_channels') is True
                    and args.get('check_membership') is True and args.get('top_channels')==[] and args.get('default_workspace')=='TINIT0001', 'Wrong source channel request arguments')
    searches=sum(row['method']=='channels/search' for row in requests)
    require(source.get('searches')==searches and all(type(source.get(k)) is int and source[k]>=0 for k in ('completed','active','cancelled')),
            'Invalid search lifecycle counters')
    require(source['completed']<=searches and source['active']==searches-source['completed'] and source['cancelled']<=source['completed'], 'Search accounting mismatch')
    require(isinstance(source.get('heldQueries'),list) and source.get('held') is bool(source['heldQueries']), 'Held sender inventory mismatch')
    return requests

def validate_geometry(geometry,viewport,stage):
    require(geometry.get('ok') is True and geometry.get('noOverflow') is True and geometry.get('viewport')==viewport, 'Invalid or overflowing viewport')
    rectangle(geometry.get('dialog'),viewport)
    for key in ('controls','notices','rows','tokens'):
        require(isinstance(geometry.get(key),list), 'Missing measured '+key)
    controls=geometry['controls']
    wanted=['Back','Save section']+([] if stage=='cancelled' else ['Use description','Cancel description edit'])
    require(sorted(c.get('text') for c in controls)==sorted(wanted), 'Missing actual action controls')
    for row in controls:
        require(type(row.get('disabled')) is bool and row['disabled'] is (row['text']=='Save section'), 'Wrong action disabled state')
    for row in controls+geometry['notices']+geometry['rows']:
        require(row.get('visible') is True, 'Control, popup or notice is not readable')
        rectangle(row.get('rect'),viewport)
    # Result stages deliberately frame the popup. Other stages must show tokens.
    if not geometry['rows']:
        for row in geometry['tokens']:
            require(row.get('visible') is True, 'Existing token clipped')
            rectangle(row.get('rect'),viewport)
    if stage in ('malformed','failure'):
        require(FAILURE in [n.get('text') for n in geometry['notices']], 'Missing visible exact failure feedback')
    if stage=='empty':require('No matching channels' in [n.get('text') for n in geometry['notices']], 'Missing empty feedback')
    dims=geometry.get('pngDimensions')
    require(isinstance(dims,list) and len(dims)==2 and all(type(n) is int and n>0 for n in dims), 'Invalid capture dimensions')

def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(fixture in FIXTURES and isinstance(receipt,dict) and receipt.get('fixture')==fixture, 'Wrong native receipt owner')
    require(receipt.get('ok') is True, f"Native completion assertion failed: {receipt.get('error','missing terminal success')}")
    require(receipt.get('initialBlocks')==initial(), 'Wrong initial fixture contract')
    saved=receipt.get('savedBlocks');elements(saved)
    stages=receipt.get('stages');paths=receipt.get('captures')
    require(isinstance(stages,list) and [r.get('stage') for r in stages]==list(STAGES), 'Missing or reordered acceptance stages')
    require(isinstance(paths,dict) and set(paths)==set(STAGES), 'Missing or extra native captures')
    viewport=expected_viewport(fixture,platform);previous_requests=[];previous_attempts=[]
    for index,row in enumerate(stages):
        stage=row['stage'];model=row.get('model',{});geometry=row.get('geometry',{});proof=row.get('proof',{})
        validate_geometry(geometry,viewport,stage)
        require(model.get('enabled') is True and model.get('busy') is False and model.get('uncertain') is False,
                'Wrong profile ownership/write state')
        require(model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning' and model.get('countryValue')=='US', 'Unrelated field changed')
        require(model.get('blocks')==(initial() if index<5 else saved) and model.get('dirty') is False, 'Stored draft changed outside Use/save')
        attempts=model.get('attempts')
        require(isinstance(attempts,list) and len(attempts)==(0 if index<5 else 1) and model.get('reads')==(0 if index<5 else 1), 'Unrequested write/readback or replay')
        require(attempts[:len(previous_attempts)]==previous_attempts, 'Write evidence changed');previous_attempts=attempts
        if attempts:
            values={v.get('element_id'):v for v in attempts[0]}
            require(len(values)==len(attempts[0]), 'Duplicate section fields')
            raw=values.get('long',{}).get('long_text',{}).get('blocks')
            require(isinstance(raw,str) and json.loads(raw)==saved, 'Actual prepared wire differs from accepted native blocks')
            require(values.get('locked',{}).get('text',{}).get('text')=='Preserved organization value', 'Managed wire field changed')
        requests=validate_source(row.get('source'))
        require(requests[:len(previous_requests)]==previous_requests, 'Request history changed');previous_requests=requests
        count=2 if index<3 else 3 if stage=='keyboard' else 4
        if stage=='cancelled':require(geometry.get('native') is None and not geometry['tokens'], 'Cancelled editor retained native authority')
        else:
            blocks=validate_native(geometry.get('native'),count)
            if stage in ('mouse','saved'):require(blocks==saved, 'Inserted/readback blocks changed')
        if stage=='local':
            require([v['text'] for v in geometry['rows']]==[LABELS[0]] and proof.get('held') is True and row['source']['active']==1, 'Missing held local-first results')
        if stage=='refreshed':
            require(set(v['text'] for v in geometry['rows'])==set(LABELS) and proof.get('sequence')==stages[1]['proof'].get('sequence'), 'Remote refresh changed query identity or lost candidates')
            require(proof.get('release',{}).get('delivered') is True and proof['release'].get('cancelledBeforeReply') is False, 'Initial reply did not arrive through held transport')
        if stage in ('keyboard','mouse'):require(proof.get('keyboard') is (stage=='keyboard') and proof.get('channel')==('CCOMPLETE1' if stage=='keyboard' else 'CCOMPLETE2'), 'Wrong actual selection route')
        if stage=='saved':require(proof.get('exactReadback') is True and proof.get('expectedBlocks')==saved, 'Missing exact readback proof')
        if stage in ('query-retired','cancelled'):
            release=proof.get('release',{})
            require(release.get('delivered') is False and release.get('cancelledBeforeReply') is True and release.get('query')==('project-held' if stage=='query-retired' else 'project-cancel'), 'Late response not refused for exact retired query')
            require(row['source']['cancelled']>=(1 if stage=='query-retired' else 2), 'Cancellation task did not finish')
        if stage in ('malformed','failure'):require(proof.get('noWrite') is True and proof.get('retainedBlocks')==saved, 'Failure lost stored draft')
        if stage=='stale-refused':
            require(proof.get('refused') is True and type(proof.get('previousSequence')) is int and type(proof.get('nextSequence')) is int
                    and proof['nextSequence']>proof['previousSequence'] and proof.get('exactNativeBlocks')==geometry['native']['snapshot']['blocks'], 'Stale choice did not retire exact native query')
        path=Path(paths[stage]) if isinstance(paths[stage],str) else None
        require(path is not None and path.is_absolute() and path==captures.resolve()/f'{fixture}-{stage}.png','Wrong owned PNG path')
        if png:
            require(path.is_file() and not path.is_symlink(),'Missing original PNG')
            validate_png(path,geometry['pngDimensions'])
    require(receipt.get('model')==stages[-1]['model'],'Terminal model drift')
    final=validate_source(receipt.get('source'))
    require(final[:len(previous_requests)]==previous_requests,'Terminal request history drift')
    queries={r['args']['query'] for r in final if r['method']=='channels/search'}
    require({'project','project-held','no-malformed','no-offline','zz-no-matching-channel','project-cancel'}<=queries,'Missing exercised transport outcomes')

def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    expected = [dict(stage=stage, edge=edge) for stage in STAGES for edge in ('before', 'after')]
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
                    readyDirectory=str(directory), scope=SCOPE, limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_PROFILE_MENTIONS_CHECK': '1',
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
                    deadline, sampled, count = time.monotonic() + 180, 0., 0
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-channel-completion-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    args = parser.parse_args()
    captures = args.captures.resolve()
    for fixture in args.fixture or FIXTURES:
        require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        run_fixture(fixture, args.binary.resolve(), captures)


if __name__ == '__main__':
    main()
