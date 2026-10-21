#!/usr/bin/env python3
"""Offline About -> Configuration workflow metadata acceptance, normal and narrow.

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
from workflow_management_delete import STAGES as DELETE_STAGES, validate as validate_delete
from workflow_management_unpublish import STAGES as UNPUBLISH_STAGES, validate as validate_unpublish
from workflow_management_publish import STAGES as PUBLISH_STAGES, validate as validate_publish
from workflow_management_first_publish import STAGES as FIRST_PUBLISH_STAGES, validate as validate_first_publish, choose_owned_file, AVATAR_PATH, avatar_path, retain_native_failure, avatar_request_path, avatar_case, chooser_count, chooser_request
from workflow_management_confirmation_add import STAGES as CONFIRMATION_ADD_STAGES, validate as validate_confirmation_add
from workflow_management_channel_workflows import STAGES as CHANNEL_WORKFLOWS_STAGES, validate as validate_channel_workflows
from workflow_management_channel_workflow_details import STAGES as CHANNEL_WORKFLOW_DETAILS_STAGES, validate as validate_channel_workflow_details
FIRST_PUBLISH_STAGES = {**FIRST_PUBLISH_STAGES, **CONFIRMATION_ADD_STAGES, **CHANNEL_WORKFLOWS_STAGES, **CHANNEL_WORKFLOW_DETAILS_STAGES}
from workflow_management_permissions import STAGES as PERMISSIONS_STAGES, validate as validate_permissions
from workflow_management_run_access import STAGES as RUN_ACCESS_STAGES, validate as validate_run_access
from workflow_management_run_access_prerequisites import STAGES as RUN_PREREQUISITE_STAGES, validate as validate_run_prerequisites
RUN_ACCESS_STAGES = {**RUN_ACCESS_STAGES, **RUN_PREREQUISITE_STAGES}
from workflow_management_managers import STAGES as MANAGERS_STAGES, validate as validate_managers
from workflow_management_manager_agents import STAGES as MANAGER_AGENT_STAGES, validate as validate_manager_agents
MANAGERS_STAGES = {**MANAGERS_STAGES, **MANAGER_AGENT_STAGES}

PREFIX = 'workflow-management-check: '
EDGE = 'workflow-management-capture: '
WAITING = 'workflow-management-waiting: '
FLAG = 'SUPER_PLATINUM_WORKFLOW_MANAGEMENT_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = {
    'managed': ('about', 'managed', 'empty-search', 'editor', 'draft', 'saved'),
    'editor': ('about', 'editor', 'draft', 'partial', 'checked', 'checked-again', 'adopted', 'preflight-refused'),
    'reply-loss': ('about', 'editor', 'draft', 'unconfirmed', 'checked', 'checked-again', 'account-replaced'),
}
FIXTURES = tuple('workflow-' + scenario + suffix for scenario in STAGES for suffix in ('', '-narrow'))
TITLE = 'R\u00e9vision de projet'
DESCRIPTION = 'Keep the proposed details.\nReview \u4e2d\U0001f642 e\u0301 together.'
CHANGED_TITLE = 'Retained changed preflight \u65e5\u672c\u8a9e'
MATCHED = 'The current workflow matches your changes, but delivery remains unconfirmed. No update will be replayed.'
DIFFERENT = 'The current workflow differs. Your proposed changes and each update response are retained.'
REFUSED = 'Workflow or permissions changed. No update was sent; your draft is retained.'


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


def validate_closed(native, expected, captures, admitted, dimensions=None, *, platform=sys.platform, png=True, delete_case=None, unpublish_case=None, permissions_case=None, run_access_case=None, managers_case=None, publish_case=None, first_publish_case=None, thread_custody=False):
    waiting = H['closed_receipts'](native.log, WAITING)
    require(len(waiting) == 1, 'Missing or duplicate closed readiness request')
    validate_ready(waiting[0], expected)
    # Strict newline parsing also rejects partial trailing terminal/capture records.
    for prefix in (PREFIX, EDGE):
        require(H['closed_receipts'](native.log, prefix) == native.receipts(prefix), 'Closed log parse changed')
    if first_publish_case is not None:
        prefix = 'workflow-first-publish-chooser: '
        choosers = H['closed_receipts'](native.log, prefix)
        require(choosers == native.receipts(prefix), 'Closed chooser record changed')
        require(len(choosers) == chooser_count(first_publish_case), 'Original file chooser request inventory changed')
        for serial, request in enumerate(choosers,1):
            chooser_request(request,first_publish_case,expected['fixture'],expected['pid'],serial)
    receipt = validate_log(native, expected['fixture'], captures, platform, png, delete_case=delete_case, unpublish_case=unpublish_case, permissions_case=permissions_case, run_access_case=run_access_case, managers_case=managers_case, publish_case=publish_case, first_publish_case=first_publish_case, thread_custody=thread_custody)
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


def validate(fixture, receipt, captures, *, platform=sys.platform, png=True, delete_case=None, unpublish_case=None, permissions_case=None, run_access_case=None, managers_case=None, publish_case=None, first_publish_case=None, thread_custody=False):
    require(fixture in FIXTURES, 'Unknown workflow fixture')
    require(receipt.get('threadCustody', False) is thread_custody, 'Requested thread custody mode differs from native receipt')
    require(not thread_custody or first_publish_case in ('first-details-success', 'first-details-errors', 'first-details-close-held'), 'Thread custody requires an admitted details case')
    require(sum(x is not None for x in (delete_case, unpublish_case, permissions_case, run_access_case, managers_case, publish_case, first_publish_case)) <= 1, 'Conflicting workflow operation cases')
    require(first_publish_case is not None or receipt.get('firstPublishCase') is None, 'Unrequested first-Publish receipt')
    if first_publish_case is not None:
        require(first_publish_case in FIRST_PUBLISH_STAGES and fixture in ('workflow-editor','workflow-editor-narrow'), 'Invalid first-Publish fixture')
        require(all(receipt.get(k) is None for k in ('publishCase','managersCase','runAccessCase','permissionsCase','deleteCase','unpublishCase')), 'Foreign first-Publish operation receipt')
        validator = (validate_channel_workflow_details if first_publish_case in CHANNEL_WORKFLOW_DETAILS_STAGES else
                     validate_channel_workflows if first_publish_case in CHANNEL_WORKFLOWS_STAGES else
                     validate_confirmation_add if first_publish_case in CONFIRMATION_ADD_STAGES else validate_first_publish)
        return validator(fixture, first_publish_case, receipt, captures,
            viewport=expected_viewport(fixture, platform), rectangle=rectangle, validate_png=validate_png, png=png)
    require(publish_case is not None or receipt.get('publishCase') is None, 'Unrequested Publish receipt')
    if publish_case is not None:
        require(publish_case in PUBLISH_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid Publish fixture/case')
        require(all(receipt.get(k) is None for k in ('deleteCase','unpublishCase','permissionsCase','runAccessCase','managersCase')), 'Foreign operation receipt')
        return validate_publish(fixture, publish_case, receipt, captures,
                                expected_viewport(fixture, platform), rectangle, validate_png, png=png)
    if managers_case is not None:
        require(managers_case in MANAGERS_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid managers fixture/case')
        require(all(receipt.get(k) is None for k in ('deleteCase','unpublishCase','permissionsCase','runAccessCase')), 'Foreign operation receipt')
        viewport = expected_viewport(fixture, platform)
        validator = validate_manager_agents if managers_case in MANAGER_AGENT_STAGES else validate_managers
        return validator(fixture, managers_case, receipt, captures,
                                 viewport=viewport, rectangle=rectangle, validate_png=validate_png, png=png)
    if run_access_case is not None:
        require(run_access_case in RUN_ACCESS_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid Run access case or fixture')
        require(all(receipt.get(k) is None for k in ('deleteCase','unpublishCase','permissionsCase')), 'Foreign operation receipt')
        validator = validate_run_prerequisites if run_access_case in RUN_PREREQUISITE_STAGES else validate_run_access
        return validator(fixture, run_access_case, receipt, captures,
                                   viewport=expected_viewport(fixture, platform), rectangle=rectangle,
                                   validate_png=validate_png, png=png)
    require(receipt.get('runAccessCase') is None, 'Unrequested Run access receipt')
    if permissions_case is not None:
        require(permissions_case in PERMISSIONS_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid Permissions case or fixture')
        require(receipt.get('deleteCase') is None and receipt.get('unpublishCase') is None, 'Foreign operation receipt')
        return validate_permissions(fixture, permissions_case, receipt, captures,
                                    viewport=expected_viewport(fixture, platform), rectangle=rectangle,
                                    validate_png=validate_png, png=png)
    require(receipt.get('permissionsCase') is None, 'Unrequested Permissions receipt')
    if unpublish_case is not None:
        require(unpublish_case in UNPUBLISH_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid Unpublish case or fixture')
        require(receipt.get('deleteCase') is None, 'Delete receipt in Unpublish run')
        return validate_unpublish(fixture, unpublish_case, receipt, captures,
                                  viewport=expected_viewport(fixture, platform), rectangle=rectangle,
                                  validate_png=validate_png, png=png)
    require(receipt.get('unpublishCase') is None, 'Unrequested Unpublish receipt')
    if delete_case is not None:
        require(delete_case in DELETE_STAGES and fixture in ('workflow-editor', 'workflow-editor-narrow'),
                'Invalid Delete case or fixture')
        return validate_delete(fixture, delete_case, receipt, captures,
                               viewport=expected_viewport(fixture, platform), rectangle=rectangle,
                               validate_png=validate_png, png=png)
    require(receipt.get('deleteCase') is None, 'Unrequested Delete receipt')
    scenario = fixture.removesuffix('-narrow').removeprefix('workflow-')
    stages = STAGES[scenario]
    require(receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('scenario') == scenario, 'Wrong or failed native receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'syntheticSession', 'noLiveNetwork')),
            'Missing actual entry or synthetic isolation proof')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Incorrect native viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(stages), 'Missing or reordered stages')
    require(isinstance(paths, dict) and set(paths) == set(stages), 'Wrong capture inventory')
    previous_reads = 0
    previous_writes = 0
    for row in rows:
        stage, model, geometry = row['stage'], row.get('model'), row.get('geometry')
        require(isinstance(model, dict) and isinstance(geometry, dict), 'Missing state or geometry')
        for key in ('reads', 'writes', 'networkCalls', 'retainedDocuments', 'retainedAttempts'):
            require(type(model.get(key)) is int and model[key] >= 0, 'Missing integer counters')
        require(model['networkCalls'] == 0 and model.get('busy') is False, 'Network use or unsettled stage')
        require(model['reads'] >= previous_reads and model['writes'] >= previous_writes, 'Counters went backwards')
        if stage in ('checked', 'checked-again'):
            require(model['reads'] > previous_reads, 'Check did not perform a fresh read')
        previous_reads, previous_writes = model['reads'], model['writes']
        writes = 0 if stage in ('about', 'managed', 'empty-search', 'editor', 'draft') else 2
        require(model['writes'] == writes, 'Unexpected mutation count or replay')
        route = 'about' if stage in ('about', 'account-replaced') else 'managed' if stage in ('managed', 'empty-search') else 'editor'
        require(model.get('route') == route, 'Wrong actual UI route')
        require(model['retainedDocuments'] == (0 if stage in ('about', 'managed', 'empty-search') else 1), 'Document custody differs')
        require(model['retainedAttempts'] == (0 if writes == 0 else 2 if stage == 'preflight-refused' else 1), 'Attempt custody differs')
        if route != 'editor':
            require(model.get('title') is None and model.get('description') is None, 'Hidden editor leaked across route')
        pending = stage in ('partial', 'unconfirmed', 'checked', 'checked-again')
        dirty = pending or stage in ('draft', 'preflight-refused')
        require(model.get('pending') is pending and model.get('dirty') is dirty
                and model.get('observed') is pending, 'Wrong draft/delivery custody')
        outcomes = ['Workflow: Unconfirmed', 'Trigger: Unconfirmed'] if scenario == 'reply-loss' else ['Workflow: Acknowledged', 'Trigger: Rejected']
        require(model.get('outcomes') == (outcomes if pending else [] if route != 'about' else None), 'Lost independent slot outcomes')
        notice = {'saved': 'Workflow details saved.', 'partial': DIFFERENT, 'unconfirmed': MATCHED,
                  'checked': MATCHED if scenario == 'reply-loss' else DIFFERENT,
                  'checked-again': MATCHED if scenario == 'reply-loss' else DIFFERENT,
                  'adopted': 'Using the current workflow. Earlier update responses remain retained.',
                  'preflight-refused': REFUSED}.get(stage)
        if notice is not None:
            require(model.get('notice') == notice, 'Wrong outcome guidance')
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True
                and geometry.get('viewport') == viewport, 'Unproven native layout')
        controls, feedback = geometry.get('controls'), geometry.get('feedback')
        require(isinstance(controls, list) and controls and isinstance(feedback, list), 'Missing controls or feedback geometry')
        for item in controls + feedback:
            require(item.get('visible') is True, 'Hidden control or feedback')
            rectangle(item.get('rect'), viewport)
        ids = [item.get('id') for item in controls]
        require(len(ids) == len(set(ids)) and all(isinstance(i, str) and i for i in ids), 'Invalid control identity')
        if route == 'about':
            require(ids == ['workflow-configuration'] and geometry.get('fields') is None, 'About did not expose Configuration')
        elif route == 'managed':
            require('workflow-management-search' in ids and geometry.get('fields') is None, 'Managed list not rendered')
            require(('workflow-management-open-WF_REVIEW' in ids) is (stage == 'managed'), 'Search result not actually filtered')
        else:
            fields = geometry.get('fields')
            require(isinstance(fields, dict), 'Missing actual native editor values')
            title = 'Review requests' if stage == 'editor' else CHANGED_TITLE if stage == 'preflight-refused' else TITLE
            description = 'Collect the details before a team review.' if stage == 'editor' else DESCRIPTION
            require(model.get('title') == title and model.get('description') == description
                    and fields.get('title') == title and fields.get('description') == description, 'Exact proposed/native text changed')
            require(fields.get('titleReadonly') is pending and fields.get('descriptionReadonly') is pending, 'Wrong native editor lock')
            save = geometry.get('save')
            require(isinstance(save, dict) and save.get('visible') is True, 'Save control not visible')
            rectangle(save.get('rect'), viewport)
            if pending:
                require(save.get('disabled') is True and type(save.get('opacity')) in (int, float)
                        and abs(save['opacity'] - .55) < .001, 'Pending Save lacks visible disabled state')
            require('workflow-management-check' in ids and 'workflow-management-managed' in ids, 'Missing recovery controls')
            if pending:
                require('workflow-management-use-current' in ids and 'workflow-management-save' not in ids, 'Unsafe pending actions')
        messages = [item.get('text') for item in feedback]
        expected_messages = ([model['notice']] if model.get('notice') else []) + (model.get('outcomes') or [])
        require(all(message in messages for message in expected_messages), 'Outcome feedback missing from viewport')
        dimensions = geometry.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), 'Missing native PNG dimensions')
        path = captures / f'{fixture}-{stage}.png'
        require(paths[stage] == str(path), 'Capture path escaped stage ownership')
        if png:
            validate_png(path, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Final state differs from final captured state')
    if scenario == 'reply-loss':
        replacement = receipt.get('replacement')
        require(isinstance(replacement, dict) and replacement.get('before') == rows[-2]['model']
                and replacement.get('after') == rows[-1]['model'], 'Missing actual account replacement observations')
        require(type(replacement.get('previousEpoch')) is int and type(replacement.get('currentEpoch')) is int
                and replacement['currentEpoch'] > replacement['previousEpoch'], 'Account did not actually change')
        require(replacement.get('retainedDraftExact') is True and replacement.get('foreignDocumentHidden') is True
                and type(replacement.get('retainedPending')) is int and replacement['retainedPending'] == 1,
                'Account replacement lost unresolved proposal or exposes foreign document')
        for key in ('retainedDocuments', 'retainedAttempts'):
            require(replacement['before'][key] == replacement['after'][key] == 1, 'Retained custody changed on account replacement')
    else:
        require(receipt.get('replacement') is None, 'Unexpected account replacement')


def validate_log(native, fixture, captures, platform=sys.platform, png=True, *, delete_case=None, unpublish_case=None, permissions_case=None, run_access_case=None, managers_case=None, publish_case=None, first_publish_case=None, thread_custody=False):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    require(receipt.get('threadCustody', False) is thread_custody, 'Requested thread custody mode differs from native receipt')
    validate(fixture, receipt, captures, platform=platform, png=png, delete_case=delete_case, unpublish_case=unpublish_case, permissions_case=permissions_case, run_access_case=run_access_case, managers_case=managers_case, publish_case=publish_case, first_publish_case=first_publish_case, thread_custody=thread_custody)
    stages = (FIRST_PUBLISH_STAGES[first_publish_case] if first_publish_case is not None else
              PUBLISH_STAGES[publish_case] if publish_case is not None else
              MANAGERS_STAGES[managers_case] if managers_case is not None else
              RUN_ACCESS_STAGES[run_access_case] if run_access_case is not None else
              PERMISSIONS_STAGES[permissions_case] if permissions_case is not None else
              UNPUBLISH_STAGES[unpublish_case] if unpublish_case is not None else
              DELETE_STAGES[delete_case] if delete_case is not None else STAGES[receipt['scenario']])
    expected = [dict(stage=stage, edge=edge) for stage in stages for edge in ('before', 'after')]
    require(native.receipts(EDGE) == expected, 'Wrong native capture order')
    return receipt


def run_fixture(fixture, binary, captures, delete_case=None, unpublish_case=None, permissions_case=None, run_access_case=None, managers_case=None, publish_case=None, first_publish_case=None, thread_custody=False):
    require(fixture in FIXTURES, 'Unknown workflow fixture')
    require(delete_case is None or delete_case in DELETE_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid Delete run')
    require(sum(x is not None for x in (delete_case, unpublish_case, permissions_case, run_access_case, managers_case, publish_case, first_publish_case)) <= 1, 'Conflicting workflow operation cases')
    require(unpublish_case is None or unpublish_case in UNPUBLISH_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid Unpublish run')
    require(permissions_case is None or permissions_case in PERMISSIONS_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid Permissions run')
    require(run_access_case is None or run_access_case in RUN_ACCESS_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid Run access run')
    require(managers_case is None or managers_case in MANAGERS_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid managers run')
    require(publish_case is None or publish_case in PUBLISH_STAGES
            and fixture in ('workflow-editor', 'workflow-editor-narrow'), 'Invalid Publish run')
    require(first_publish_case is None or first_publish_case in FIRST_PUBLISH_STAGES and fixture in ('workflow-editor','workflow-editor-narrow'), 'Invalid first-Publish run')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'workflow-management-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    checks = {FLAG: '1', 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
              'SUPER_PLATINUM_WORKFLOW_MANAGEMENT_NONCE': nonce,
              'SUPER_PLATINUM_WORKFLOW_MANAGEMENT_READY_DIR': str(directory)}
    require(not thread_custody or first_publish_case in ('first-details-success', 'first-details-errors', 'first-details-close-held'), 'Thread custody requires an admitted details case')
    if thread_custody:
        checks['SUPER_PLATINUM_WORKFLOW_DETAILS_THREAD_CHECK'] = '1'
        evidence['threadCustody'] = True
    if first_publish_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_FIRST_PUBLISH_CASE'] = first_publish_case
        evidence['firstPublishCase'] = first_publish_case
    if delete_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_DELETE_CASE'] = delete_case
        evidence['deleteCase'] = delete_case
    if unpublish_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_UNPUBLISH_CASE'] = unpublish_case
        evidence['unpublishCase'] = unpublish_case
    if permissions_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_PERMISSIONS_CASE'] = permissions_case
        evidence['permissionsCase'] = permissions_case
    if run_access_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_RUN_ACCESS_CASE'] = run_access_case
        evidence['runAccessCase'] = run_access_case
    if publish_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_PUBLISH_CASE'] = publish_case
        evidence['publishCase'] = publish_case
    if managers_case is not None:
        checks['SUPER_PLATINUM_WORKFLOW_MANAGERS_CASE'] = managers_case
        evidence['managersCase'] = managers_case
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary, checks)
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
                        choosers = native.receipts('workflow-first-publish-chooser: ')
                        require(len(choosers)<=chooser_count(first_publish_case), 'Repeated or unrequested file chooser request')
                        completed=evidence.setdefault('fileChoosers',[]) if avatar_case(first_publish_case) else []
                        for serial,request in enumerate(choosers,1):
                            chooser_request(request,first_publish_case,fixture,native.process.pid,serial)
                            if serial<=len(completed): continue
                            reply=choose_owned_file(native.process.pid,request,
                                avatar_path(first_publish_case),native.require_running)
                            completed.append(dict(reply,serial=serial,case=first_publish_case,input=request['input']))
                            if serial==1: evidence['fileChooser']=reply
                        edges = native.receipts(EDGE)
                        if pointer:
                            for edge in edges[count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check('running', require_clear=True)
                                sampled = time.monotonic()
                        count = len(edges)
                        if native.receipts(PREFIX):
                            if avatar_case(first_publish_case):
                                require(len(evidence.get('fileChoosers',[]))==chooser_count(first_publish_case) and all(v.get('dialogClosed') is True for v in evidence['fileChoosers']),
                                        'Original owned file chooser did not complete')
                            evidence['native'] = validate_log(native, fixture, captures, delete_case=delete_case, unpublish_case=unpublish_case, permissions_case=permissions_case, run_access_case=run_access_case, managers_case=managers_case, publish_case=publish_case, first_publish_case=first_publish_case, thread_custody=thread_custody)
                            validate_owned(evidence['native'], expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'Workflow native receipt timed out')
                        time.sleep(.05)
            except BaseException:
                if first_publish_case is not None:
                    retain_native_failure(evidence, native.receipts(PREFIX), fixture, first_publish_case)
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
        validate_closed(native, expected, captures, evidence['native'], evidence.get('expectedPngDimensions'),
                        delete_case=delete_case, unpublish_case=unpublish_case, permissions_case=permissions_case, run_access_case=run_access_case, managers_case=managers_case, publish_case=publish_case, first_publish_case=first_publish_case, thread_custody=thread_custody)
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/workflow-management-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    parser.add_argument('--thread-custody', action='store_true', help='Verify an open thread and unsent draft across workflow details using closed fixture messages')
    cases = parser.add_mutually_exclusive_group()
    cases.add_argument('--delete-case', choices=tuple(DELETE_STAGES))
    cases.add_argument('--unpublish-case', choices=tuple(UNPUBLISH_STAGES))
    cases.add_argument('--publish-case', choices=tuple(PUBLISH_STAGES))
    cases.add_argument('--first-publish-case', choices=tuple(FIRST_PUBLISH_STAGES))
    cases.add_argument('--permissions-case', choices=tuple(PERMISSIONS_STAGES))
    cases.add_argument('--run-access-case', choices=tuple(RUN_ACCESS_STAGES))
    cases.add_argument('--managers-case', choices=tuple(MANAGERS_STAGES))
    args = parser.parse_args()
    captures = args.captures.resolve()
    fixtures = args.fixture or (('workflow-editor', 'workflow-editor-narrow') if args.delete_case or args.unpublish_case or args.permissions_case or args.run_access_case or args.managers_case or args.publish_case or args.first_publish_case else FIXTURES)
    require(not args.delete_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Delete requires the existing editor fixture')
    require(not args.unpublish_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Unpublish requires the existing editor fixture')
    require(not args.permissions_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Permissions requires the existing editor fixture')
    require(not args.run_access_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Run access requires the existing editor fixture')
    require(not args.publish_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Publish requires an editor fixture')
    require(not args.managers_case or all(f in ('workflow-editor', 'workflow-editor-narrow') for f in fixtures),
            'Managers requires the existing editor fixture')
    for fixture in fixtures:
        require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
        run_fixture(fixture, args.binary.resolve(), captures, args.delete_case, args.unpublish_case, args.permissions_case, args.run_access_case, args.managers_case, args.publish_case, args.first_publish_case, args.thread_custody)


if __name__ == '__main__':
    main()
