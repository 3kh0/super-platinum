#!/usr/bin/env python3
"""Offline About -> metadata -> Activity acceptance, normal and narrow.

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
from workflow_activity_surfaces import STAGES as SURFACE_STAGES, validate as validate_surfaces
from workflow_activity_source_replacement import STAGES as REPLACEMENT_STAGES, validate as validate_replacement

PREFIX = 'workflow-activity-check: '
EDGE = 'workflow-activity-capture: '
WAITING = 'workflow-activity-waiting: '
FLAG = 'SUPER_PLATINUM_WORKFLOW_ACTIVITY_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = {
    'user-refresh': ('about','draft','list','detail-real','event-display','warm-return','warm-reentry','editor-invalidated','reentry-real','offline','offline-retried','malformed','malformed-retried','raw-refused','composition-refused','held','latest-display','returned','account-held','account-replaced'),
    'user-real': ('about','draft','list','policy-failed','detail','returned','held','account-replaced'),
    'user-display': ('about','draft','list','policy-failed','detail','returned','held','account-replaced'),
    'browse': ('about', 'draft', 'uncommitted-refused', 'composition-refused', 'list', 'filtered', 'next', 'previous', 'detail', 'nested', 'error', 'retried', 'empty', 'returned'),
    'channel': ('about', 'draft', 'detail', 'filter-refused', 'search-refused', 'composition-refused', 'known', 'returned', 'malformed', 'offline', 'held', 'retired', 'retried', 'final-return'),
    'retirement': ('about', 'draft', 'unconfirmed', 'list', 'held', 'account-replaced'),
    'picker': ('about', 'draft', 'local-empty', 'remote', 'raw-refused', 'option-refused',
               'composition-refused', 'selected', 'query-retired', 'filter-retired',
               'malformed', 'failure', 'retried', 'empty', 'returned'),
}
STAGES['user-deactivated'] = STAGES['user-refresh']
STAGES['export'] = ('about','draft','gate-missing','gate-off','ready','raw-refused','composition-refused','preflight-refused','held','initiated','rejected','retried','offline','offline-retried','malformed','unknown-blocked','returned')
STAGES['export-retirement'] = ('about','draft','list','route-held','route-retired','account-held','account-replaced')
STAGES['execution'] = ('transcript','opened','closed','filtered','paged','metadata-draft','returned','reopened','missing-target')
STAGES['execution-failure'] = ('transcript','policy-denied','policy-retried','read-failed','read-retried','returned')
STAGES['execution-retirement'] = ('transcript','route-held','route-retired','account-held','account-replaced')
STAGES.update(SURFACE_STAGES)
STAGES.update(REPLACEMENT_STAGES)
BASE_FIXTURES = ('workflow-activity', 'workflow-activity-narrow')
PICKER_FIXTURES = ('workflow-activity-members', 'workflow-activity-members-narrow')
FIXTURES = BASE_FIXTURES + PICKER_FIXTURES
TITLE = 'R\u00e9vision retained for Activity'
DESCRIPTION = 'Keep these unsaved details.\nReview \u4e2d\U0001f642 e\u0301 together.'
REFUSAL = 'Finish editing the current text before opening Activity. Your draft is unchanged.'
FAILURE = 'Activity could not be loaded. Check the filters and try again.'
CURSOR = 'activity+/page=2'
EMOJI_TRIGGER = 'Prefix :wave: \U0001f44b  :activity_cold:'
EMOJI_STEP = 'L<@U1><b>x</b>replacement.value{no}{zero} \u4e2d\U0001f642 <@U1><i>y</i> \U0001f44b'

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


def validate_closed(native, expected, captures, admitted, scenario, dimensions=None, *, platform=sys.platform, png=True):
    waiting = H['closed_receipts'](native.log, WAITING)
    require(len(waiting) == 1, 'Missing or duplicate closed readiness request')
    validate_ready(waiting[0], expected)
    # Strict newline parsing also rejects partial trailing terminal/capture records.
    for prefix in (PREFIX, EDGE):
        require(H['closed_receipts'](native.log, prefix) == native.receipts(prefix), 'Closed log parse changed')
    receipt = validate_log(native, expected['fixture'], captures, scenario, platform, png)
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


def validate_requests(requests, *, picker=False, selected_user='UACTADA01'):
    require(isinstance(requests, list), 'Missing actual Activity request ledger')
    for serial, request in enumerate(requests, 1):
        require(request.get('serial') == serial, 'Activity request ordering changed')
        method, args = request.get('method'), request.get('args')
        require(method in ('workflows.activity.list', 'workflows.activity.get') and isinstance(args, dict),
                'Unknown Activity endpoint')
        require(args.get('workflow_id') == 'WF_REVIEW' and 'token' not in args, 'Wrong workflow or secret in ledger')
        if method.endswith('.get'):
            require(args.get('workflow_execution_id') in ('E_REVIEW', 'E_SECOND', 'E_CHILD'), 'Foreign detail request')
            require(args.get('_x_reason') == 'Load instance steps for activity tab', 'Wrong detail request reason')
            require(set(args) == {'workflow_id', 'workflow_execution_id', '_x_reason'}, 'Unexpected detail request fields')
        else:
            require(args.get('_x_reason') == 'multi-click-activity-list-fetcher', 'Wrong list request reason')
            require(args.get('limit') in ('20', '50', '75', '100'), 'Invalid Activity limit')
            try:
                filters = json.loads(args['filters'])
            except (KeyError, TypeError, ValueError) as error:
                raise RuntimeError('Malformed request filters') from error
            allowed = {'workflow_instance_status', 'from_timestamp', 'to_timestamp'} | ({'user_id'} if picker else set())
            require(isinstance(filters, dict) and set(filters) <= allowed,
                    'Unexpected filter or unimplemented Started-by claim')
            require('user_id' not in filters or filters['user_id'] == selected_user, 'Unadmitted Started-by filter')
            require(set(args) <= {'workflow_id', 'limit', 'filters', 'cursor', 'parent_workflow_execution_id', '_x_reason'},
                    'Unexpected list request fields')
            require('cursor' not in args or args['cursor'] == CURSOR, 'Opaque cursor changed')
            require('parent_workflow_execution_id' not in args or args['parent_workflow_execution_id'] == 'E_REVIEW',
                    'Wrong nested parent')


def validate_emoji(value, viewport):
    require(isinstance(value, dict) and value.get('ok') is True and value.get('viewport') == viewport,
            'Missing Activity emoji DOM observation')
    require(value.get('measurement') == 'individual-scroll-samples' and value.get('paragraphCount') == 2,
            'Wrong Activity message measurement')
    paragraphs = value.get('paragraphs')
    require(isinstance(paragraphs, list) and len(paragraphs) == 2, 'Missing message paragraphs')
    for p, identity, text in zip(paragraphs,
            ('workflow-activity-trigger-message', 'workflow-activity-step-message-0'), (EMOJI_TRIGGER, EMOJI_STEP)):
        require(p.get('id') == identity and p.get('text') == text, 'Activity literal text or placeholder semantics changed')
        require(p.get('visible') is True and p.get('scrollSample') is True, 'Activity emoji text clipped')
        rectangle(p.get('rect'), viewport)
        spans = p.get('spans')
        require(isinstance(spans, list) and spans, 'Missing actual inline span styles')
        for span in spans:
            require(span.get('display') in ('inline', 'inline-block') and span.get('margins') == ['0px'] * 4,
                    'Activity inline span gained paragraph margin or block layout')
    require(value.get('inlineMargins') is True and value.get('forbiddenNodes') == 0,
            'Activity text became HTML or actionable entity')
    require(value.get('standardGlyphs') == ['\U0001f44b', '\U0001f44b'], 'Missing explicit or trailing standard emoji')
    require(value.get('explicitGlyphCount') == value.get('trailingGlyphCount') == 1
            and 'explicitTitle' in value and value['explicitTitle'] is None and value.get('trailingTitle') == ':wave:',
            'Explicit and trailing emoji tooltip behavior changed')
    require(value.get('coldFallback') == ':activity_cold:' and value.get('coldImages') == 0,
            'Cold emoji lost readable shortcode fallback')
    images = value.get('images')
    require(isinstance(images, list) and len(images) == 1, 'Wrong custom emoji image count')
    picture = images[0]
    require(picture.get('alt') == ':activity_ready:' and picture.get('complete') is True
            and picture.get('naturalWidth') == picture.get('naturalHeight') == 16,
            'Ready emoji image did not decode')
    require(isinstance(picture.get('src'), str) and picture['src'].startswith(
            ('super-platinum-media://emoji/', 'http://super-platinum-media.emoji/')),
            'Emoji image bypassed opaque native media')


def validate(fixture, receipt, captures, scenario, *, platform=sys.platform, png=True):
    require(fixture in FIXTURES and scenario in STAGES, 'Unknown Activity fixture or case')
    require((fixture in PICKER_FIXTURES) is (scenario == 'picker'), 'Fixture/case mismatch')
    if scenario in REPLACEMENT_STAGES:
        return validate_replacement(fixture, receipt, captures, scenario, globals(), platform=platform, png=png)
    if scenario in SURFACE_STAGES:
        return validate_surfaces(fixture, receipt, captures, scenario, globals(), platform=platform, png=png)
    if scenario.startswith('execution'):
        return validate_execution(fixture, receipt, captures, scenario, platform=platform, png=png)
    if scenario in ('export','export-retirement'):
        return validate_export(fixture, receipt, captures, scenario, platform=platform, png=png)
    if scenario in ('user-refresh','user-deactivated'):
        return validate_refresh(fixture, receipt, captures, scenario=scenario, platform=platform, png=png)
    if scenario in ('user-real','user-display'):
        return validate_users(fixture,receipt,captures,scenario,platform=platform,png=png)
    if scenario == 'channel':
        return validate_channel(fixture, receipt, captures, platform=platform, png=png)
    if scenario == 'picker':
        return validate_picker(fixture, receipt, captures, platform=platform, png=png)
    require(receipt.get('ok') is True, f"Native Activity assertion failed: {receipt.get('error', receipt)}")
    require(receipt.get('fixture') == fixture and receipt.get('case') == scenario, 'Wrong native receipt identity')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'actualActivityEntry', 'noLiveNetwork')),
            'Missing real entry or isolation proof')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Wrong viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[scenario]), 'Missing or reordered stages')
    require(isinstance(paths, dict) and set(paths) == set(STAGES[scenario]), 'Wrong capture inventory')
    request_counts = {'about': 0, 'draft': 0, 'uncommitted-refused': 0, 'composition-refused': 0,
                      'unconfirmed': 0, 'list': 1, 'filtered': 2, 'next': 3, 'previous': 4, 'detail': 5,
                      'nested': 6, 'error': 8, 'retried': 9, 'empty': 10, 'returned': 10,
                      'held': 2, 'account-replaced': 2}
    previous_requests = []
    for row in rows:
        stage, model, geometry = row['stage'], row.get('model'), row.get('geometry')
        require(isinstance(model, dict) and isinstance(geometry, dict), 'Missing model/geometry')
        metadata, activity, ledger = model.get('metadata'), model.get('activity'), model.get('ledger')
        require(isinstance(metadata, dict) and isinstance(ledger, dict), 'Missing metadata or closed ledger')
        require(metadata.get('networkCalls') == 0 and ledger.get('networkCalls') == 0, 'Live network used')
        requests = ledger.get('requests')
        validate_requests(requests)
        require(len(requests) == request_counts[stage] and requests[:len(previous_requests)] == previous_requests,
                'Unexpected Activity read count/replay or rewritten request history')
        previous_requests = requests
        require(ledger.get('finished') == len(requests) - (1 if stage == 'held' else 0), 'Unsettled Activity callback')
        require(ledger.get('cancelled') == (1 if stage == 'account-replaced' else 0), 'Unexpected cancelled read count')
        require((type(ledger.get('held')) is int) is (stage == 'held'), 'Wrong held read state')
        require(metadata.get('busy') is (stage == 'held'), 'Wrong active read state')
        pending = scenario == 'retirement' and stage not in ('about', 'draft', 'account-replaced')
        writes = 2 if scenario == 'retirement' and stage not in ('about', 'draft') else 0
        require(metadata.get('writes') == writes, 'Activity replayed or changed metadata writes')
        require(metadata.get('pending') is pending, 'Uncertain write custody changed')
        if stage in ('about', 'account-replaced'):
            require(metadata.get('route') == 'about' and activity is None, 'Foreign Activity/editor remains visible')
        else:
            require(metadata.get('title') == TITLE and metadata.get('description') == DESCRIPTION
                    and metadata.get('dirty') is True, 'Exact metadata draft changed')
            require(isinstance(activity, dict), 'Missing owned Activity state')
            active = stage not in ('draft', 'uncommitted-refused', 'composition-refused', 'unconfirmed', 'returned')
            require(activity.get('active') is active, 'Wrong Activity route')
            if pending:
                require(metadata.get('outcomes') == ['Workflow: Unconfirmed', 'Trigger: Unconfirmed']
                        and metadata.get('observed') is True, 'Activity settled or lost uncertain write outcome')
            if stage in ('uncommitted-refused', 'composition-refused'):
                proof = row.get('proof', {})
                require(metadata.get('notice') is None and proof.get('draftRetained') is True
                        and proof.get('noActivityRead') is True and proof.get('unchangedRustState') is True
                        and proof.get('sameRevision') is True
                        and type(proof.get('beforeRevision')) is int and proof['beforeRevision'] > 0
                        and proof.get('afterRevision') == proof['beforeRevision'],
                        'Missing completed native navigation refusal')
                require(type(proof.get('beforeRefusals')) is int and proof['beforeRefusals'] >= 0
                        and proof.get('afterRefusals') == proof['beforeRefusals'] + 1,
                        'Missing native refusal counter advance')
                expected_buffer = 'Uncommitted native title 未確定' if stage == 'uncommitted-refused' else TITLE
                require(proof.get('probe', {}).get('value') == expected_buffer
                        and proof.get('native', {}).get('value') == expected_buffer
                        and proof['native'].get('notice') == REFUSAL,
                        'Original native buffer was not preserved through refusal')
                require(proof.get('composition') is (stage == 'composition-refused')
                        and proof.get('uncommitted') is (stage == 'uncommitted-refused'), 'Wrong native refusal exercise')
            if stage == 'filtered':
                q, dates = activity.get('query', {}), row.get('proof', {})
                require(q.get('limit') == 20 and q.get('filters') == dict(workflow_instance_status='SUCCESS',
                        from_timestamp=dates.get('from'), to_timestamp=dates.get('to')),
                        'Filters or actual renderer local dates changed')
                require(type(dates.get('from')) is int and type(dates.get('to')) is int and dates['from'] < dates['to'],
                        'Missing actual local date conversion')
            if stage == 'next':
                require(activity.get('query', {}).get('cursor') == CURSOR and len(activity.get('previous', [])) == 1,
                        'Next lost opaque cursor or previous query')
            if stage == 'previous':
                require('cursor' not in activity.get('query', {}) and activity.get('previous') == [], 'Previous did not restore query')
            if stage == 'detail':
                validate_emoji(row.get('proof', {}).get('emoji'), viewport)
                event = activity.get('detail', {}).get('workflow_event', {})
                require(event.get('workflow_execution_id') == 'E_REVIEW',
                        'Missing actual decoded run detail')
                require(event.get('workflow_status') == 'PENDING'
                        and activity.get('detailRow', {}).get('workflow_instance_status') == 'SUCCESS',
                        'Detail status is not distinguished from selected list row')
                require(geometry.get('detailStatus') == 'Processing', 'Detail header borrowed list status')
                status = geometry.get('detailStatusGeometry', {})
                last = geometry.get('lastStepStatuses')
                require(status.get('text') == 'Processing' and status.get('visible') is True
                        and status.get('scrollSample') is True, 'Detail header is not visibly measured')
                rectangle(status.get('rect'), viewport)
                require(isinstance(last, list) and len(last) == 1 and last[0].get('text') == 'Completed'
                        and last[0].get('visible') is True and last[0].get('scrollSample') is True,
                        'Final step lost original selected row completion')
                rectangle(last[0].get('rect'), viewport)
            if stage == 'nested':
                require(activity.get('query', {}).get('parent_workflow_execution_id') == 'E_REVIEW'
                        and len(activity.get('breadcrumbs', [])) == 1, 'Nested route lost original parent')
            if stage == 'error':
                require(activity.get('notice') == FAILURE and activity.get('page') is not None, 'Malformed page replaced warm page or feedback')
            if stage == 'retried':
                require(activity.get('notice') is None and activity.get('page', {}).get('workflow_events'), 'Retry failed to recover')
            if stage == 'empty':
                require(activity.get('page', {}).get('workflow_events') == [] and activity.get('notice') is None, 'Empty success inferred from failure')
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True and geometry.get('viewport') == viewport,
                'Unproven native geometry')
        require(geometry.get('activity') is (bool(activity) and activity.get('active') is True), 'Rendered Activity differs from owner')
        require(geometry.get('detail') is (stage == 'detail'), 'Rendered detail route changed')
        controls, notices = geometry.get('controls'), geometry.get('notices')
        require(isinstance(controls, list) and controls and isinstance(notices, list), 'Missing controls/feedback')
        require(geometry.get('controlMeasurement') == 'individual-scroll-samples'
                and all(c.get('scrollSample') is True for c in controls), 'Missing honest scroll accessibility measurements')
        for element in controls + notices:
            require(element.get('visible') is True, 'Hidden control or feedback')
            rectangle(element.get('rect'), viewport)
        ids = [c.get('id') for c in controls]
        require(len(ids) == len(set(ids)) and all(isinstance(i, str) and i for i in ids), 'Invalid control identities')
        if geometry['activity']:
            require({'workflow-activity-back', 'workflow-activity-status', 'workflow-activity-from', 'workflow-activity-to',
                     'workflow-activity-limit', 'workflow-activity-apply'} <= set(ids), 'Missing Activity filter/navigation controls')
        if stage == 'error':
            require(FAILURE in [n.get('text') for n in notices], 'Failure guidance not visibly captured')
        if stage in ('uncommitted-refused', 'composition-refused'):
            require(REFUSAL in [n.get('text') for n in notices], 'Entry refusal not visibly captured')
        fields = geometry.get('fields')
        if stage in ('draft', 'uncommitted-refused', 'composition-refused', 'unconfirmed', 'returned'):
            require(isinstance(fields, dict) and fields.get('description') == DESCRIPTION, 'Native description lost')
            require(fields.get('title') == ('Uncommitted native title 未確定' if stage == 'uncommitted-refused' else TITLE), 'Native title changed')
            require(fields.get('titleReadonly') is pending and fields.get('descriptionReadonly') is pending, 'Native metadata lock changed')
        dims = geometry.get('pngDimensions')
        require(isinstance(dims, list) and len(dims) == 2 and all(type(n) is int and n > 0 for n in dims), 'Missing PNG dimensions')
        expected_path = captures / f'{fixture}-{stage}.png'
        require(Path(paths[stage]) == expected_path, 'Foreign capture path')
        if png:
            validate_png(expected_path, dims)
    require(receipt.get('final') == rows[-1]['model'], 'Final state differs from captured state')
    if scenario == 'retirement':
        proof = receipt.get('replacement', {})
        require(proof.get('foreignDocumentHidden') is True and proof.get('retainedDraftExact') is True
                and proof.get('retainedPending') == 1 and proof.get('currentEpoch', 0) > proof.get('previousEpoch', 0),
                'Real account replacement lost unresolved metadata')
        require(proof.get('lateReply') == dict(delivered=False, cancelledBeforeReply=True),
                'Retired read cancellation confused with accepted late decode')
        require(proof.get('after', {}).get('writes') == 2
                and proof.get('after', {}).get('retainedAttempts') == proof.get('before', {}).get('retainedAttempts') == 1,
                'Account replacement changed attempt custody')
    return receipt


def validate_picker(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(receipt.get('ok') is True, f"Native picker assertion failed: {receipt.get('error', receipt)}")
    require(receipt.get('fixture') == fixture and receipt.get('case') == 'picker', 'Wrong picker identity')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration', 'actualActivityEntry', 'noLiveNetwork')),
            'Missing actual picker entry/isolation')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Wrong picker viewport')
    stages, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(stages, list) and [r.get('stage') for r in stages] == list(STAGES['picker']),
            'Missing or reordered picker stages')
    require(isinstance(paths, dict) and set(paths) == set(STAGES['picker']), 'Wrong picker captures')
    counts = dict(zip(STAGES['picker'], (0, 0, 0, 1, 1, 1, 1, 2, 3, 5, 6, 7, 8, 9, 9)))
    eligible = [['UACTADA01', 'Ada Lovelace'], ['UACTZOE01', 'Zo\u00eb Alvarez']]
    # The installed fallback model penalizes Ada's full-name prefix when the
    # source has no first_name. Query "a" ranks Zoe's surname match first.
    ranked = [eligible[1], eligible[0]]
    prior_reads, prior_members = [], []
    failure = 'People could not be loaded. Try again.'
    offline = 'People are unavailable while offline. Try again when connected.'
    allowed = {'client.userBoot', 'client.gantryBoot', 'users.prefs.get', 'team.prefs.get',
               'experiments.getByUser', 'drafts.list', 'drafts.listActive', 'api.getFlannelHttpUrl',
               'search.autocomplete.model', 'search.autocomplete.offlineFeatures', 'users/search'}
    for index, row in enumerate(stages):
        stage, model, g, proof = row['stage'], row.get('model', {}), row.get('geometry', {}), row.get('proof', {})
        metadata, activity, ledger = model.get('metadata', {}), model.get('activity'), model.get('ledger', {})
        members = proof.get('members', {})
        ml, picker = members.get('ledger', {}), members.get('picker')
        require(metadata.get('networkCalls') == ledger.get('networkCalls') == ml.get('networkCalls') == 0,
                'Picker used live network')
        require(metadata.get('writes') == 0 and metadata.get('pending') is False and metadata.get('busy') is False,
                'Picker changed metadata write custody')
        active = stage not in ('about', 'draft', 'returned')
        if stage == 'about':
            require(metadata.get('route') == 'about' and activity is None and picker is None, 'Wrong About entry')
        else:
            require(metadata.get('route') == 'editor' and metadata.get('title') == TITLE
                    and metadata.get('description') == DESCRIPTION and metadata.get('dirty') is True,
                    'Picker lost exact unsaved metadata')
            require(isinstance(activity, dict) and activity.get('active') is active and isinstance(picker, dict),
                    'Wrong picker route')
        reads = ledger.get('requests')
        validate_requests(reads, picker=True)
        expected_reads = 0 if stage in ('about', 'draft') else 1 if index < STAGES['picker'].index('selected') else 2
        require(len(reads) == expected_reads and reads[:len(prior_reads)] == prior_reads
                and ledger.get('finished') == len(reads) and ledger.get('held') is None
                and ledger.get('cancelled') == 0, 'Picker replayed or changed Activity reads')
        prior_reads = reads
        if expected_reads == 2:
            require(json.loads(reads[-1]['args']['filters']).get('user_id') == 'UACTADA01',
                    'Selected actor was not sent in Activity filters')
        requests = ml.get('requests')
        require(isinstance(requests, list) and requests[:len(prior_members)] == prior_members,
                'Picker rewrote original request history')
        prior_members = requests
        for request in requests:
            method, args = request.get('method'), request.get('args')
            require(method in allowed and isinstance(args, dict) and 'token' not in args, 'Foreign picker endpoint or secret')
            if method == 'users/search':
                require(set(args) == {'count', 'fuzz', 'enable_workspace_ranking', 'filter', 'include_profile_only_users', 'query'}
                        and type(args['count']) is int and args['count'] == 20 and args['fuzz'] == 1
                        and args['enable_workspace_ranking'] is True and args['include_profile_only_users'] is True
                        and args['filter'] == 'NOT deactivated' and args['query'] in ('a', 'ad', 'zz-no-match'),
                        'Changed actual member search request')
        require(ml.get('searches') == counts[stage] == sum(r['method'] == 'users/search' for r in requests)
                and ml.get('completed') == counts[stage] and ml.get('held') is False,
                'Wrong member request count or unfinished callback')
        cancelled = 0 if index < STAGES['picker'].index('query-retired') else 1 if stage == 'query-retired' else 2
        require(ml.get('cancelled') == cancelled, 'Retired member callback was not cancelled')
        if active:
            native = proof.get('native', {})
            require(picker.get('pending') is False and native.get('ok') is True
                    and native.get('query') == picker.get('query') and native.get('selected') == picker.get('filterUser')
                    and native.get('options') == [['', 'Anyone']] + picker.get('rows', []),
                    'Native picker options differ from current ranked result')
            rows = picker['rows']
            require(all(r in eligible for r in rows) and len(rows) == len({r[0] for r in rows}),
                    'Self, bot, duplicate, or unobserved option admitted')
            if stage in ('remote', 'raw-refused', 'option-refused', 'retried'):
                require(rows == ranked and picker.get('generation', 0) > 0, 'Missing ordered actual ranked members')
            elif stage == 'selected':
                require(rows == eligible[:1] and picker.get('selection') == eligible[0]
                        and picker.get('filterUser') == 'UACTADA01', 'Actor selection lost exact admitted identity')
            else:
                require(rows == [], 'Failed, retired, or empty lookup exposed stale options')
            require(picker.get('notice') == (offline if stage == 'failure' else failure if stage == 'malformed' else None),
                    'Failure confused with successful empty lookup')
            if stage.endswith('-refused'):
                ex = proof.get('exercise', {})
                require(ex.get('kind') == stage and ex.get('before', {}).get('filterUser') == ''
                        and ex.get('after', {}).get('filterUser') == ''
                        and ex['before'].get('filterRevision') == ex['after'].get('filterRevision')
                        and ex.get('searchesBefore') == ex.get('searchesAfter') == counts[stage],
                        'Missing actual native admission refusal')
                attempt = ex.get('nativeAttempt', {})
                require(attempt.get('selected') == ('UACTSTALE' if stage == 'option-refused' else 'UACTADA01'),
                        'Refusal never attempted option selection')
                if stage == 'raw-refused':
                    require(attempt.get('query') == 'Uncommitted native query', 'Raw input mismatch not exercised')
            if stage in ('query-retired', 'filter-retired'):
                release = proof.get('exercise', {}).get('release', {})
                require(release == dict(query='ad' if stage == 'query-retired' else 'a', delivered=False, cancelledBeforeReply=True),
                        'Retired reply remained publishable')
                require(picker.get('filterUser') == '', 'Retired response selected an actor')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport') == viewport
                and g.get('activity') is active and g.get('detail') is False, 'Invalid picker layout')
        controls, notices = g.get('controls'), g.get('notices')
        require(isinstance(controls, list) and controls and isinstance(notices, list)
                and g.get('controlMeasurement') == 'individual-scroll-samples', 'Missing picker measurements')
        for element in controls + notices:
            require(element.get('visible') is True, 'Hidden picker control or feedback')
            rectangle(element.get('rect'), viewport)
        ids = [c.get('id') for c in controls]
        require(all(ids) and len(set(ids)) == len(ids) and all(c.get('scrollSample') is True for c in controls),
                'Invalid picker control measurements')
        if active:
            require({'workflow-activity-user', 'workflow-activity-user-search', 'workflow-activity-find-people',
                     'workflow-activity-apply', 'workflow-activity-back'} <= set(ids), 'Missing real picker controls')
        if stage in ('malformed', 'failure'):
            require((offline if stage == 'failure' else failure) in [n.get('text') for n in notices], 'Picker failure guidance not visible')
        if stage in ('malformed', 'failure', 'retried', 'empty', 'returned'):
            feedback = proof.get('feedback', {})
            require(feedback.get('ok') is True and feedback.get('globalToastCount') == 0,
                    'Picker generated duplicate toast or failed scoped feedback proof')
            expected_notice = offline if stage == 'failure' else failure if stage == 'malformed' else ''
            require(feedback.get('notice') == expected_notice, 'Picker feedback is stale or unsafe')
            if expected_notice:
                require(feedback.get('outsideScroll') is True and feedback.get('stable') is True,
                        'Picker feedback is inside scrolling content or moves with it')
                samples = feedback.get('samples', [])
                require(isinstance(samples, list) and [x.get('edge') for x in samples] == ['start', 'end'],
                        'Picker feedback was not checked at both scroll edges')
                for sample in samples:
                    require(sample.get('visible') is True, 'Picker fixed feedback is hidden or occluded')
                    rectangle(sample.get('rect'), viewport)
                require(abs(samples[0]['rect']['top']-samples[1]['rect']['top']) < 1
                        and abs(samples[0]['rect']['bottom']-samples[1]['rect']['bottom']) < 1,
                        'Picker feedback moved with content scroll')
            else:
                require(feedback.get('samples') == [] and feedback.get('outsideScroll') is False,
                        'Cleared picker feedback retained a mounted notice')
            actions = feedback.get('actions')
            expected_actions = ['workflow-management-activity', 'workflow-management-managed',
                                'workflow-management-check', 'workflow-management-save'] if stage == 'returned' else []
            require(isinstance(actions, list) and [x.get('id') for x in actions] == expected_actions,
                    'Returned metadata actions were not measured')
            require(feedback.get('actionMeasurement') == 'individual-scroll-samples',
                    'Missing returned action measurement scope')
            for action in actions:
                require(action.get('visible') is True and action.get('disabled') is False
                        and action.get('scrollSample') is True, 'Returned metadata action is occluded or disabled')
                rectangle(action.get('rect'), viewport)
        if stage in ('draft', 'returned'):
            require(g.get('fields') == dict(title=TITLE, description=DESCRIPTION, titleReadonly=False, descriptionReadonly=False),
                    'Native unsaved metadata changed')
        dimensions = g.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2 and all(type(n) is int and n > 0 for n in dimensions),
                'Missing picker PNG dimensions')
        path = captures / f'{fixture}-{stage}.png'
        require(Path(paths[stage]) == path, 'Foreign picker capture')
        if png:
            validate_png(path, dimensions)
    require(receipt.get('final') == stages[-1]['model'], 'Final picker state changed after capture')
    return receipt


def validate_users(fixture, receipt, captures, scenario, *, platform=sys.platform, png=True):
    require(receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('case')==scenario, 'Wrong USER fixture receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')), 'Missing actual USER entry')
    viewport=expected_viewport(fixture,platform)
    require(receipt.get('viewport')==viewport,'Wrong USER viewport')
    rows,paths=receipt.get('stages'),receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[scenario]),'Missing USER stages')
    require(isinstance(paths,dict) and set(paths)==set(STAGES[scenario]),'Missing USER captures')
    previous=[]; activity_previous=[]; original_epoch=None
    prefs_counts={'about':0,'draft':0,'list':0,'policy-failed':1,'detail':2,'returned':2,'held':3,'account-replaced':3}
    for row in rows:
        stage,model,g,proof=row['stage'],row.get('model',{}),row.get('geometry',{}),row.get('proof',{})
        policy=proof.get('policy',{}); ledger=policy.get('ledger',{}); metadata=model.get('metadata',{}); activity=model.get('activity') or {}
        require(policy.get('override')==(1 if scenario=='user-real' else -1),'Wrong observed policy case')
        require(ledger.get('networkCalls')==ledger.get('searches')==0,'USER policy depends on picker search or live request')
        boot=ledger.get('initialBoot',{}); require(boot.get('method')=='client.userBoot' and boot.get('applied') is True and 'token' not in boot.get('args',{}),'Missing actual initial raw BootRead adoption')
        require(metadata.get('networkCalls')==metadata.get('writes')==0,'USER scenario wrote metadata or used live transport')
        activity_ledger=model.get('ledger',{}); activity_requests=activity_ledger.get('requests')
        validate_requests(activity_requests)
        require(activity_ledger.get('networkCalls')==0 and activity_ledger.get('held') is None and activity_ledger.get('cancelled')==0
                and activity_ledger.get('finished')==len(activity_requests) and activity_requests[:len(activity_previous)]==activity_previous,'USER Activity response custody changed')
        activity_previous=activity_requests
        detail_count={'about':0,'draft':0,'list':0,'policy-failed':1,'detail':2,'returned':2,'held':3,'account-replaced':3}[stage]
        require(sum(r['method']=='workflows.activity.get' for r in activity_requests)==detail_count,'USER detail retry request missing or replayed')
        requests=ledger.get('requests')
        require(isinstance(requests,list) and requests[:len(previous)]==previous,'USER policy request custody changed')
        previous=requests
        methods=[v.get('method') for v in requests]
        require(all(m in ('client.userBoot','client.gantryBoot','users.prefs.get') for m in methods),'Unexpected policy acquisition route')
        require(methods.count('client.userBoot')<=1 and ('client.userBoot' not in methods or methods[0]=='client.userBoot'),'Boot authority replay')
        without_boot=[m for m in methods if m!='client.userBoot']
        require(without_boot==['client.gantryBoot','users.prefs.get']*prefs_counts[stage],'Missing cold policy, retry or invalidation reads')
        for request in requests:
            args=request.get('args',{})
            require(isinstance(args,dict) and 'token' not in args,'Secret in USER receipt')
            if request['method']=='users.prefs.get': require(args=={},'Wrong user preference request')
            if request['method']=='client.gantryBoot': require(args=={'include_channels':'false','_x_reason':'fetch-client-data'},'Wrong gantry request')
        require(ledger.get('completed')==len(requests)-(stage=='held') and ledger.get('cancelled')==int(stage=='account-replaced') and ledger.get('held') is (stage=='held'),'Policy failure or cancellation accounting changed')
        if original_epoch is None: original_epoch=model.get('epoch')
        if stage!='account-replaced':
            require(model.get('account')=='synthetic-workflow-activity-user' and model.get('epoch')==original_epoch,'Original USER owner replaced early')
            if stage!='about':require(metadata.get('title')==TITLE and metadata.get('description')==DESCRIPTION and metadata.get('dirty') is True,'USER detail lost dirty metadata')
        if stage in ('about','draft','list','policy-failed','held'):
            require(not activity.get('detail'),'Detail published before successful policy')
        exercise=proof.get('exercise') or {}
        if stage=='policy-failed':
            require(activity.get('notice')=='Run details could not be loaded. Close the details and try again.' and exercise.get('users')==0,'Policy failure was hidden or displayed guessed names')
            require(activity['notice'] in [n.get('text') for n in g.get('notices',[])],'Missing visible USER policy failure')
        if stage=='detail':
            names=exercise.get('rows'); suffix='profile real' if scenario=='user-real' else 'display'
            require(exercise.get('ok') is True and exercise.get('measurement')=='individual-scroll-samples' and isinstance(names,list) and [n.get('text') for n in names]==[f'Ada {suffix}',f'Ada {suffix}',f'Zoe {suffix}'],'USER policy label precedence or duplicate occurrence changed')
            require([n.get('user') for n in names]==['UACTADA01','UACTADA01','UACTZOE01']
                    and [n.get('part') for n in names]==['1','3','1']
                    and [n.get('message') for n in names]==['workflow-activity-trigger-message']*2+['workflow-activity-step-message-0'],'USER occurrence identity changed')
            source={v.get('id'):v.get('raw') for v in policy.get('users',[])}
            for id,first in [('UACTADA01','Ada'),('UACTZOE01','Zoe')]:
                raw=source.get(id) or {};profile=raw.get('profile') or {}
                require(raw.get('name')==first.lower()+'_username' and raw.get('real_name')==first+' top real'
                        and profile.get('real_name')==first+' profile real' and profile.get('display_name')==first+' display','USER source names are not distinct observed fields')
            for name in names:
                require(name.get('tag')=='STRONG' and all(name.get(k) is True for k in ('bold','inert','visible')),'USER label must be bold and inert')
                rectangle(name.get('rect'),viewport)
            require(activity.get('detail') and metadata.get('busy') is False,'USER detail never published')
        if stage=='returned':
            require(activity.get('active') is False and g.get('fields')==dict(title=TITLE,description=DESCRIPTION,titleReadonly=False,descriptionReadonly=False),'Native metadata draft did not return')
        if stage=='held':
            inv=exercise.get('invalidation',{})
            require(type(inv.get('before')) is int and inv.get('after')==inv['before']+1 and policy.get('namesRevision')==inv['after'],'Actual preference event did not retire policy')
            require(exercise.get('native',{}).get('users')==0 and metadata.get('busy') is True,'Held policy allowed stale USER detail')
        if stage=='account-replaced':
            require(model.get('epoch')==original_epoch+1 and model.get('account')!='synthetic-workflow-activity-user' and not activity,'Foreign account received original detail')
            require(exercise.get('foreignDocumentHidden') is True and exercise.get('lateReply')==dict(delivered=False,cancelledBeforeReply=True),'Late policy reply survived account retirement')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,'Invalid USER layout')
        controls,notices=g.get('controls'),g.get('notices')
        require(isinstance(controls,list) and controls and isinstance(notices,list),'Missing USER control geometry')
        for item in controls+notices:
            require(item.get('visible') is True,'Hidden USER control or error');rectangle(item.get('rect'),viewport)
        dimensions=g.get('pngDimensions');require(isinstance(dimensions,list) and len(dimensions)==2,'Missing USER PNG dimensions')
        path=captures/f'{fixture}-{stage}.png';require(Path(paths[stage])==path,'Foreign USER capture')
        if png:validate_png(path,dimensions)
    require(receipt.get('final')==rows[-1]['model'],'USER final model changed')
    return receipt


def validate_deactivated_names(rows, users, suffix, viewport):
    require(isinstance(users,list),'Missing actual member source')
    source={u.get('id'):u.get('raw') for u in users}
    require(source.get('UACTADA01',{}).get('deleted') is True and source.get('UACTZOE01',{}).get('deleted') is False,
            'Deactivated marker is not backed by actual member deleted field')
    for i,row in enumerate(rows):
        deleted=i<2; expected=f"{'Ada' if deleted else 'Zoe'} {suffix}"
        require(row.get('rawName')==expected and row.get('nameTag')=='SPAN' and row.get('nameCount')==1
                and row.get('nameVisible') is True and row.get('shape') is True,
                'Member label must remain a separate exact raw span without literal separator')
        rectangle(row.get('nameRect'),viewport)
        for key in ('fragments','nameFragments','indicatorFragments'):
            pieces=row.get(key)
            require(isinstance(pieces,list) and (bool(pieces) if key!='indicatorFragments' or deleted else pieces==[]),
                    'Missing actual wrapped member fragment measurements')
            for piece in pieces:
                require(piece.get('hit') is True,'Wrapped member fragment is occluded')
                rectangle(piece.get('rect'),viewport)
        require(row.get('deactivated')==str(deleted).lower() and row.get('deactivatedClass') is deleted
                and row.get('opacity')==1,'Wrong deactivated marker/class or opacity')
        if deleted:
            require(row.get('indicator')=='(deactivated)' and row.get('indicatorTag')=='SPAN'
                    and row.get('indicatorCount')==1 and row.get('indicatorVisible') is True,
                    'Missing separate visible deactivated indicator')
            require(row.get('indicatorMargin')==4 and row.get('indicatorWeight')==400,
                    'Deactivated indicator lost native spacing/weight')
            muted=row.get('mutedColor'); ordinary=row.get('ordinaryColor')
            require(isinstance(muted,list) and len(muted)==3 and isinstance(ordinary,list) and len(ordinary)==3
                    and row.get('parentColor')==row.get('indicatorColor')==muted and muted!=ordinary,
                    'Deactivated names must use resolved muted color, distinct from ordinary names')
            rectangle(row.get('indicatorRect'),viewport)
        else:
            require(row.get('indicator') is None and row.get('indicatorTag') is None
                    and row.get('indicatorCount')==0 and row.get('indicatorRect') is None,
                    'Ordinary member acquired deactivated indicator')
            require(row.get('parentColor')==row.get('ordinaryColor'),'Ordinary member was dimmed')


def validate_refresh(fixture, receipt, captures, *, scenario='user-refresh', platform=sys.platform, png=True):
    require(receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('case')==scenario, 'Wrong refresh receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')), 'Missing actual refresh entry')
    viewport=expected_viewport(fixture,platform)
    require(receipt.get('viewport')==viewport,'Wrong refresh viewport')
    rows,paths=receipt.get('stages'),receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES['user-refresh']),'Missing refresh stages')
    require(isinstance(paths,dict) and set(paths)==set(STAGES['user-refresh']),'Missing refresh captures')
    counts=[0,0,0,1,2,2,2,2,3,4,5,6,7,7,7,8,9,9,10,10]
    revisions=[0,0,0,0,1,1,1,2,2,3,3,4,4,5,6,7,8,8,9,None]
    previous=[]; activity_previous=[]; detail=None; epoch=rows[0]['model']['epoch']
    failure='Names could not be refreshed. Close the run details and reopen them to try again.'
    real_stages={'detail-real','reentry-real','offline-retried','malformed-retried'}
    display_stages={'event-display','warm-reentry','latest-display'}
    for index,row in enumerate(rows):
        stage=row['stage']; model=row.get('model',{}); g=row.get('geometry',{}); proof=row.get('proof',{})
        policy=proof.get('policy',{}); ledger=policy.get('ledger',{}); activity=model.get('activity') or {}; meta=model.get('metadata',{}); ex=proof.get('exercise') or {}
        expected_override=-1 if stage in ('event-display','warm-return','warm-reentry','latest-display','returned') else 1
        require(policy.get('override')==expected_override,'Wrong refresh raw override')
        require(ledger.get('networkCalls')==ledger.get('searches')==0 and meta.get('networkCalls')==meta.get('writes')==0,'Refresh sent live/picker/write request')
        boot=ledger.get('initialBoot',{});require(boot.get('method')=='client.userBoot' and boot.get('applied') is True,'Missing actual raw boot')
        requests=ledger.get('requests');require(isinstance(requests,list) and requests[:len(previous)]==previous,'Refresh rewrote policy request history');previous=requests
        methods=[r.get('method') for r in requests]
        require(methods==['client.gantryBoot','users.prefs.get']*counts[index],'Refresh policy requests missing or replayed')
        for r in requests:
            require(r.get('args')==({'include_channels':'false','_x_reason':'fetch-client-data'} if r['method']=='client.gantryBoot' else {}),'Changed refresh policy request')
        held=stage in ('held','account-held'); cancelled=2 if stage=='account-replaced' else 1 if index>=16 else 0
        require(ledger.get('held') is held and ledger.get('completed')==len(requests)-held and ledger.get('cancelled')==cancelled,'Refresh response/cancellation accounting changed')
        reads=model.get('ledger',{}); reqs=reads.get('requests'); validate_requests(reqs)
        require(reqs[:len(activity_previous)]==activity_previous and reads.get('finished')==len(reqs) and reads.get('held') is None and reads.get('cancelled')==0 and reads.get('networkCalls')==0,'Refresh changed Activity read custody');activity_previous=reqs
        get_count=0 if index<3 else 1 if index<10 else 2 if index<12 else 3
        require(sum(r['method']=='workflows.activity.get' for r in reqs)==get_count and sum(r['method']=='workflows.activity.list' for r in reqs)==int(index>=2),'Automatic refresh refetched raw Activity')
        if stage!='account-replaced':
            require(model.get('account')=='synthetic-workflow-activity-user' and model.get('epoch')==epoch,'Refresh retired owner early')
            require(policy.get('namesRevision')==revisions[index],'Actual preference event revision changed')
            if stage!='about':require(meta.get('title')==TITLE and meta.get('description')==DESCRIPTION and meta.get('dirty') is True,'Refresh lost dirty draft')
            if index>=3:
                if detail is None:detail=activity.get('detail')
                require(isinstance(detail,dict) and activity.get('detail')==detail,'Refresh replaced retained raw detail')
                require(activity.get('nameRefreshPending') is held and meta.get('busy') is held,'Refresh pending ownership changed')
            require(activity.get('nameNotice')==(failure if stage in ('offline','malformed') else None),'Refresh notice stale/missing')
        if stage in real_stages|display_stages:
            names=ex.get('names',ex);suffix='profile real' if stage in real_stages else 'display';found=names.get('rows')
            require(names.get('ok') is True and names.get('measurement')=='individual-scroll-samples' and isinstance(found,list) and [n.get('text') for n in found]==[f'Ada {suffix}'+('(deactivated)' if scenario=='user-deactivated' else '')]*2+[f'Zoe {suffix}'],'Refresh did not publish exact observed names')
            require([n.get('user') for n in found]==['UACTADA01','UACTADA01','UACTZOE01'] and [n.get('part') for n in found]==['1','3','1'],'Refresh changed occurrence custody')
            if scenario=='user-deactivated':
                validate_deactivated_names(found,policy.get('users'),suffix,viewport)
            for n in found:
                require(n.get('tag')=='STRONG' and all(n.get(k) is True for k in ('bold','inert','visible')),'Refresh names not bold inert visible');rectangle(n.get('rect'),viewport)
        if 'invalidation' in ex:
            inv=ex['invalidation'];require(inv.get('after')==inv.get('before',-2)+1 and inv['after']==policy['namesRevision'],'Missing actual event revision')
        if stage in ('offline','malformed'):
            footer=ex.get('footer',{});require(footer.get('ok') is True and footer.get('notice')==failure and footer.get('visible') is True and footer.get('outsideScroll') is True and footer.get('globalToastCount')==0,'Refresh failure footer hidden or duplicated');rectangle(footer.get('rect'),viewport)
            require(failure in [x.get('text') for x in g.get('notices',[])],'Missing captured refresh failure')
        if stage in ('raw-refused','composition-refused'):
            native=ex.get('native',{});require(ex.get('requestsUnchanged') is True and native.get('ok') is True,'Missing native refresh refusal')
            require(native.get('value')==('2026-06-01' if stage=='raw-refused' else '') and native.get('composing') is (stage=='composition-refused'),'Refusal changed native buffer or did not compose')
            before,after=ex.get('before',{}),ex.get('after',{});require(before.get('activity',{}).get('detail')==after.get('activity',{}).get('detail')==detail and before['activity']['filters']==after['activity']['filters'],'Refused refresh changed detail/filter')
        if stage=='latest-display':require(ex.get('release')==dict(delivered=False,cancelledBeforeReply=True),'Superseded policy reply remained deliverable')
        if stage=='account-replaced':
            require(model.get('epoch')==epoch+1 and model.get('account')!='synthetic-workflow-activity-user' and not activity and ex.get('foreignDocumentHidden') is True and ex.get('lateReply')==dict(delivered=False,cancelledBeforeReply=True),'Account retirement accepted held policy')
        if stage in ('draft','warm-return','editor-invalidated','returned'):
            require(activity.get('active') is False and g.get('fields')==dict(title=TITLE,description=DESCRIPTION,titleReadonly=False,descriptionReadonly=False),'Retained editor draft missing')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,'Invalid refresh geometry')
        controls,notices=g.get('controls'),g.get('notices');require(isinstance(controls,list) and controls and isinstance(notices,list),'Missing refresh controls')
        for item in controls+notices:require(item.get('visible') is True,'Hidden refresh control');rectangle(item.get('rect'),viewport)
        dims=g.get('pngDimensions');require(isinstance(dims,list) and len(dims)==2,'Missing refresh PNG dimensions')
        path=captures/f'{fixture}-{stage}.png';require(Path(paths[stage])==path,'Foreign refresh capture')
        if png:validate_png(path,dims)
    require(receipt.get('final')==rows[-1]['model'],'Refresh final model changed')
    return receipt


def validate_channel_labels(proof, viewport, fetched=False):
    require(isinstance(proof,dict) and proof.get('ok') is True and proof.get('fetched') is fetched
            and proof.get('measurement') == 'individual-scroll-samples', 'Missing channel icon/name proof')
    rows=proof.get('rows')
    ids=['workflow-activity-channel-trigger-1','workflow-activity-channel-trigger-3','workflow-activity-channel-step-0-1']
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids, 'Channel label occurrence custody changed')
    for i,row in enumerate(rows):
        known=i<2 or fetched
        name='activity-known' if i<2 else 'activity-fetched' if fetched else 'Channel unavailable'
        require(row.get('channel')==('CACTKNOWN01' if i<2 else 'CACTMISS01')
                and row.get('text')==row.get('name')==name and row.get('tag')=='BUTTON'
                and row.get('labelTag')==row.get('nameTag')=='SPAN', 'Channel raw name or separate label shape changed')
        require(row.get('ok') is True and row.get('visible') is True and row.get('scrollSample') is True, 'Channel label hidden or clipped')
        rectangle(row.get('nameRect'),viewport); rectangle(row.get('buttonRect'),viewport)
        require(row.get('iconCount')==row.get('svgCount')==int(known), 'Wrong channel icon count')
        icon=row.get('icon')
        if known:
            require(isinstance(icon,dict) and icon.get('tag')=='svg' and icon.get('ariaHidden')=='true'
                    and icon.get('viewBox')=='0 -960 960 960' and all(icon.get(k) is True for k in ('material','pathMatches','visible')),
                    'Channel icon is not the decorative Material Tag SVG')
            rectangle(icon.get('rect'),viewport)
            a,b=icon['rect'],row['nameRect']
            require(row.get('inline') is True and a['right']<=b['left']+1
                    and abs((a['top']+a['bottom']-b['top']-b['bottom'])/2)<=3, 'Channel icon/name alignment changed')
        else:
            require(icon is None and row.get('inline') is None, 'Unavailable channel acquired a fake icon')


def validate_channel(fixture, receipt, captures, *, platform=sys.platform, png=True):
    require(receipt.get('ok') is True, f"Native CHANNEL assertion failed: {receipt.get('error', receipt)}")
    require(receipt.get('fixture') == fixture and receipt.get('case') == 'channel', 'Wrong CHANNEL identity')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')), 'Missing CHANNEL entry proof')
    viewport = expected_viewport(fixture, platform)
    require(receipt.get('viewport') == viewport, 'Wrong CHANNEL viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows] == list(STAGES['channel']), 'Missing CHANNEL stages')
    require(isinstance(paths,dict) and set(paths) == set(STAGES['channel']), 'Wrong CHANNEL captures')
    counts = dict.fromkeys(STAGES['channel'], 0)
    counts.update(malformed=1, offline=2, held=3, retired=3, retried=4, **{'final-return':4})
    original = document = None
    activity_reads, channel_reads, about_reads = [], [], []
    metadata_reads = None
    source_baseline = None
    for row in rows:
        stage, model, g, proof = row['stage'], row.get('model',{}), row.get('geometry',{}), row.get('proof',{})
        require(model.get('account') == 'synthetic-workflow-activity-channel', 'Missing bound channel fixture account')
        channel = proof.get('channel',{})
        ledger = channel.get('ledger',{})
        metadata, activity = model.get('metadata',{}), model.get('ledger',{})
        require(ledger.get('networkCalls') == metadata.get('networkCalls') == activity.get('networkCalls') == 0
                and metadata.get('writes') == 0, 'CHANNEL live request or metadata write')
        require(ledger.get('original') and (original is None or ledger['original'] == original), 'Original app DM changed')
        original = ledger['original']
        requests = ledger.get('requests')
        require(isinstance(requests,list) and len(requests) == counts[stage] and requests[:len(channel_reads)] == channel_reads,
                'CHANNEL endpoint replay or rewritten request')
        channel_reads = requests
        require(all(r == dict(method='conversations.info',args={'channel':'CACTMISS01','include_num_members':'false'}) for r in requests), 'Wrong CHANNEL request')
        require(ledger.get('completed') == counts[stage] - (stage == 'held') and ledger.get('held') is (stage == 'held')
                and ledger.get('cancelled') == (1 if stage in ('retired','retried','final-return') else 0), 'CHANNEL callback settlement changed')
        about = ledger.get('aboutRequests')
        total_about = 8 if stage == 'final-return' else 4 if STAGES['channel'].index(stage) >= STAGES['channel'].index('returned') else 0
        require(isinstance(about,list) and len(about) == total_about and about[:len(about_reads)] == about_reads, 'About did not freshly acquire on return')
        about_reads = about
        require([r.get('method') for r in about] == ['conversations.info','apps.about.info','apps.profile.get','experiments.getByUser'] * (total_about//4), 'Wrong fresh About endpoint sequence')
        require(all('token' not in r.get('args',{}) for r in about), 'Secret in About receipt')
        if about:
            require(about[0]['args'] == {'channel':original,'return_app_home':'true','_x_reason':'app-home-open-api'}, 'About read did not name original DM')
        reads = activity.get('requests')
        validate_requests(reads)
        require(reads[:len(activity_reads)] == activity_reads and activity.get('finished') == len(reads)
                and activity.get('cancelled') == 0 and activity.get('held') is None, 'Activity response custody changed')
        activity_reads = reads
        docs = channel.get('documents')
        if stage == 'about':
            require(docs == [], 'Document installed before Configuration')
        else:
            require(isinstance(docs,list) and len(docs)==1, 'Original document duplicated or lost')
            d=docs[0]
            require(d.get('title') == TITLE and d.get('description') == DESCRIPTION and d.get('dirty') is True and d.get('pending') is None, 'Exact dirty document lost')
            require(document is None or document == d.get('id'), 'Reentry installed another document')
            document=d.get('id')
            require(type(document) is int, 'Missing document identity')
        selected = 'CACTKNOWN01' if stage=='known' else 'CACTMISS01' if stage=='retried' else original
        require(channel.get('selected') == selected and channel.get('missingPresent') is (stage in ('retried','final-return')), 'Foreign or premature channel publication')
        sidebar=channel.get('sidebar',{})
        sidebar_rows,sections=sidebar.get('rows'),sidebar.get('sections')
        require(isinstance(sidebar_rows,list) and isinstance(sections,list) and sections, 'Missing actual sidebar projection')
        expected_ids={original,'CACTKNOWN01'} | ({'CACTMISS01'} if stage in ('retried','final-return') else set())
        require({r.get('id') for r in sidebar_rows} == expected_ids and len(sidebar_rows)==len(expected_ids), 'Missing, duplicate or premature sidebar channel')
        require(len({r.get('index') for r in sidebar_rows}) == len(sidebar_rows)
                and all(type(r.get('index')) is int and r['index']>=0 for r in sidebar_rows), 'Invalid sidebar channel index')
        by_id={r['id']:r for r in sidebar_rows}
        names={original:'Review requests','CACTKNOWN01':'activity-known','CACTMISS01':'activity-fetched'}
        require(all(r.get('name') == names[r['id']] for r in sidebar_rows), 'Sidebar label changed identity')
        require(sidebar.get('activeId') == selected and sidebar.get('activeIndex') == by_id[selected]['index']
                and [r['id'] for r in sidebar_rows if r.get('active') is True] == [selected], 'Sidebar active row does not map current index')
        flattened=[(section.get('id'),index) for section in sections for index in section.get('indices',[])]
        require(flattened == [(r.get('section'),r['index']) for r in sidebar_rows], 'Sidebar sections lost exact index mapping')
        native=proof.get('sidebarNative',{})
        require(native.get('ok') is True and native.get('rows') == [dict(name=r['name'],active=r['active']) for r in sidebar_rows]
                and native.get('headings') == [section.get('title') for section in sections]
                and native.get('activeId') == selected and native.get('activeIndex') == sidebar['activeIndex']
                and native.get('activePosition') == next(i for i,r in enumerate(sidebar_rows) if r['id']==selected), 'Native sidebar differs from exact projection')
        if stage in ('known','retried'):
            require(native.get('measured') is True and native.get('visible') is True and native.get('hit') is True, 'Selected sidebar row is hidden or covered')
            rectangle(native.get('rect'),viewport)
        source=channel.get('sourceRows')
        require(isinstance(source,list) and {r.get('id') for r in source}==expected_ids and len(source)==len(expected_ids), 'Unexpected source channel mutation')
        source={r['id']:r for r in source}
        if source_baseline is None: source_baseline=source
        require(all(source.get(id)==row for id,row in source_baseline.items()), 'Sidebar projection changed original membership or count facts')
        if 'CACTMISS01' in source:
            require(source['CACTMISS01']==dict(id='CACTMISS01',name='activity-fetched',isMember=True,isOpen=None,unreadCount=None,unreadDisplay=None,mentionCount=None,hasUnreads=False),
                    'Fetched source membership/counts differ from actual info response')
        exercise = proof.get('exercise')
        if stage in ('detail','filter-refused','search-refused','composition-refused','malformed','offline','held'):
            validate_channel_labels(proof.get('channelLabels'),viewport)
        if stage in ('known','retried'):
            name='activity-known' if stage=='known' else 'activity-fetched'
            owner=proof.get('conversationOwner',{})
            timeline=f"{model.get('epoch')}-TWORKFLOW-{selected}-{channel.get('generation')}"
            require(g.get('conversation') is True and g.get('timelineScope') == owner.get('timeline') == timeline
                    and isinstance(owner.get('composer'),str) and owner['composer']
                    and g.get('composerScope') == owner['composer'], 'Conversation DOM does not belong to selected visit')
            require(g.get('headerText') == '#'+name and g.get('placeholder') == 'Message #'+name,
                    'Selected conversation header/composer differs')
            require({c.get('id') for c in g.get('controls',[])} == {'channel-details-trigger','message-timeline','channel-composer'},
                    'Missing actual conversation geometry')
        if stage == 'detail':
            require(exercise.get('rows') == [dict(id='workflow-activity-channel-trigger-1',channel='CACTKNOWN01',text='activity-known',tag='BUTTON'),dict(id='workflow-activity-channel-trigger-3',channel='CACTKNOWN01',text='activity-known',tag='BUTTON'),dict(id='workflow-activity-channel-step-0-1',channel='CACTMISS01',text='Channel unavailable',tag='BUTTON')], 'Duplicate CHANNEL occurrences lost')
        if stage.endswith('-refused'):
            require(exercise.get('unchangedRustState') is True and exercise.get('before') == exercise.get('after') == channel, 'Refusal mutated Rust custody')
            require(exercise.get('native',{}).get('notice') == 'Finish editing the current text before opening this conversation. Your draft is unchanged.'
                    and exercise['native'].get('refusals') == exercise.get('attempt',{}).get('refusals',-1)+1, 'Missing actual native refusal')
            if stage=='filter-refused': require(exercise['attempt'].get('value') == exercise['native'].get('value') == 'ERROR', 'Filter buffer was not retained')
            if stage=='search-refused': require(exercise['attempt'].get('value') == exercise['native'].get('value') == 'Uncommitted native search', 'Search buffer was not retained')
            if stage=='composition-refused': require(exercise['attempt'].get('composing') is True and exercise['native'].get('composing') is True, 'Composition refusal missing')
        if stage in ('returned','final-return'):
            validate_channel_labels(exercise.get('reentryLabels'),viewport,stage=='final-return')
            before, after = exercise.get('before',{}), exercise.get('after',{})
            click = exercise.get('nativeAboutClick',{})
            require(click.get('ok') is True and click.get('selected') == 'Review requests'
                    and isinstance(click.get('scope'),str) and f'-TWORKFLOW-{original}-' in click['scope'], 'About click did not bind current conversation DOM')
            require(after == channel and before.get('documents') == after.get('documents')
                    and before.get('aboutVisit') != after.get('aboutVisit') and before.get('managementVisit') != after.get('managementVisit'), 'Return did not retain document under fresh authority')
            require(metadata_reads is not None and metadata.get('reads',0)>metadata_reads, 'Return did not reacquire workflow policy')
            require(g.get('fields') == dict(title=TITLE,description=DESCRIPTION,titleReadonly=False,descriptionReadonly=False), 'Returned native draft differs')
        metadata_reads=metadata.get('reads')
        if stage in ('malformed','offline'):
            expected='Could not open this conversation. It may no longer be accessible.' if stage=='malformed' else 'Connect to Slack to open this conversation.'
            native=exercise.get('native',{})
            require(native.get('detail') is True and native.get('notice') == expected and native.get('globalToastCount') == 0
                    and native.get('visible') is True and native.get('outsideScroll') is True, 'Missing visible scoped channel failure feedback or duplicate toast')
            rectangle(native.get('rect'),viewport)
            require(exercise.get('modelBefore') == exercise.get('modelAfter') == model, 'Channel failure changed retained model')
            before,after=exercise.get('before',{}),exercise.get('after',{})
            require(after == channel and all(before.get(k)==after.get(k) for k in ('documents','selected','generation','aboutVisit','managementVisit','missingPresent')),
                    'Channel failure changed source or document custody')
            require(expected in [n.get('text') for n in g.get('notices',[])], 'Failure footer absent from layout capture')
        if stage=='retired': require(exercise == dict(delivered=False,cancelledBeforeReply=True), 'Late channel reply remained publishable')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport') == viewport, 'Invalid CHANNEL layout')
        controls, notices = g.get('controls'),g.get('notices')
        require(isinstance(controls,list) and controls and isinstance(notices,list), 'Missing CHANNEL controls')
        for item in controls+notices:
            require(item.get('visible') is True, 'Hidden CHANNEL control or feedback')
            rectangle(item.get('rect'),viewport)
        dimensions=g.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(n) is int and n>0 for n in dimensions), 'Missing CHANNEL PNG dimensions')
        path=captures/f'{fixture}-{stage}.png'
        require(Path(paths[stage])==path,'Foreign CHANNEL capture')
        if png: validate_png(path,dimensions)
    require(receipt.get('final')==rows[-1]['model'],'CHANNEL final model changed')
    return receipt


EXPORT_NOTICES = {
    'preparing': 'Checking the current filters before exporting…',
    'sent': 'The export request is being sent.',
    'initiated': 'The export request was accepted. The CSV has not been confirmed ready yet.',
    'unknown': 'The export request may have reached Slack. It has not been sent again.',
    'rejected': 'Slack did not accept the export request.',
    'not-sent': 'The export request was not sent. Your draft is unchanged.',
}
EXPORT_REFUSAL = 'Finish editing the current filters before exporting. Your draft is unchanged.'


def validate_export_requests(requests, arguments):
    require(isinstance(requests,list),'Missing export ledger')
    ids=[]
    for serial,r in enumerate(requests,1):
        require(r.get('serial')==serial and type(r.get('attempt')) is int and r['attempt']>0,'Wrong export request identity')
        require(r.get('method')=='workflows.activity.export','Foreign export endpoint')
        args=r.get('args',{})
        require(set(args)=={'workflow_id','filters','_x_reason'} and args['workflow_id']=='WF_REVIEW'
                and args['_x_reason']=='export_workflow_activity_csv','Export field leakage or wrong reason')
        try: filters=json.loads(args['filters'])
        except (ValueError,TypeError) as e: raise RuntimeError('Invalid export filters JSON') from e
        require(isinstance(filters,dict) and set(filters)=={'user_id','from_timestamp','to_timestamp'}
                and filters['user_id']=='UWORKFLOW' and type(filters['from_timestamp']) is int
                and type(filters['to_timestamp']) is int,'Export filter/status/user contract changed')
        require(r['attempt'] in arguments and arguments[r['attempt']]==dict(workflow_id='WF_REVIEW',filters=filters),'Plan and actual export request differ')
        require(r.get('dispatch') in ('synthetic-entered','not-sent'),'Missing synthetic dispatch distinction')
        ids.append(r['attempt'])
    require(len(set(ids))==len(ids),'Export attempt replayed')


def validate_export(fixture,receipt,captures,scenario,*,platform=sys.platform,png=True):
    require(receipt.get('ok') is True,f"Native export failed: {receipt.get('error',receipt)}")
    require(receipt.get('fixture')==fixture and receipt.get('case')==scenario,'Wrong export receipt owner')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')),'Missing export entry/isolation')
    rows,paths=receipt.get('stages'),receipt.get('captures'); viewport=expected_viewport(fixture,platform)
    require(receipt.get('viewport')==viewport,'Wrong export viewport')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[scenario]),'Export stages missing/reordered')
    require(isinstance(paths,dict) and set(paths)==set(STAGES[scenario]),'Export capture inventory changed')
    counts=([0,0,0,0,0,0,0,0,1,1,2,3,4,5,6,6,6] if scenario=='export' else [0,0,0,1,1,2,2])
    phases=({'preflight-refused':'not-sent','held':'sent','initiated':'initiated','rejected':'rejected','retried':'initiated','offline':'not-sent','offline-retried':'initiated','malformed':'unknown','unknown-blocked':'unknown','returned':'unknown'} if scenario=='export' else {'route-held':'sent','route-retired':'unknown','account-held':'sent','account-replaced':'initiated'})
    previous=[]; activity_previous=[]; old_attempts={}; dates=None; selected=None
    for i,row in enumerate(rows):
        stage=row['stage']; m=row.get('model',{}); p=row.get('proof',{}); g=row.get('geometry',{}); ex=p.get('exercise') or {}
        meta=m.get('metadata',{}); activity=m.get('activity'); ledger=p.get('ledger',{}); book=p.get('export',{}); native=p.get('native',{})
        require(book==m.get('export') and ledger==m.get('exportFixture'),'Export proof differs from actual model')
        require(ledger.get('networkCalls')==meta.get('networkCalls')==meta.get('writes')==m.get('ledger',{}).get('networkCalls')==0,'Export used live IO or wrote metadata')
        require(meta.get('pending') is False and meta.get('busy') is (stage in ('held','route-held','account-held')),'Export changed metadata custody')
        activity_reads=m.get('ledger',{}).get('requests');validate_requests(activity_reads,picker=True,selected_user='UWORKFLOW')
        require(activity_reads[:len(activity_previous)]==activity_previous,'Export rewrote Activity request history');activity_previous=activity_reads
        attempts=book.get('attempts'); require(isinstance(attempts,list),'Missing retained export attempts')
        ids=[a.get('id') for a in attempts];require(all(type(v) is int and v>0 for v in ids) and len(ids)==len(set(ids)),'Invalid export attempt IDs')
        for a in attempts:
            require(a.get('phase') in EXPORT_NOTICES and a.get('initiated') is (a['phase']=='initiated'),'ACK/completion classification changed')
            if a['id'] in old_attempts:
                prior=old_attempts[a['id']]
                require(a['arguments']==prior['arguments'],'Retained export arguments changed')
                if prior['phase'] not in ('preparing','sent'): require(a==prior,'Terminal export receipt overwritten')
        require(set(ids)>=set(old_attempts),'Export history lost')
        old_attempts={a['id']:a.copy() for a in attempts}
        require(len(attempts)==counts[i]+int(scenario=='export' and i>=7),'Wrong export attempt count')
        args={a['id']:a.get('arguments') for a in attempts}
        requests=ledger.get('requests');validate_export_requests(requests,args)
        require(len(requests)==counts[i] and requests[:len(previous)]==previous,'Export sent/replayed unexpected request')
        previous=requests
        held=stage in ('held','route-held','account-held')
        require(ledger.get('finished')==len(requests)-int(held) and ledger.get('cancelled')==0,'Export callback cancelled or unfinished')
        require((type(ledger.get('held')) is int) is held,'Wrong held export state')
        hidden=stage in ('about','account-replaced'); active=not hidden and stage not in ('draft','returned','route-retired')
        if hidden:
            require(activity is None and meta.get('route')=='about','Retired export pane remains visible')
        else:
            require(meta.get('title')==TITLE and meta.get('description')==DESCRIPTION and meta.get('dirty') is True,'Export lost exact dirty metadata')
            require(isinstance(activity,dict) and activity.get('active') is active,'Wrong export route')
        if stage=='held': require(activity.get('query',{}).get('parent_workflow_execution_id')=='E_REVIEW','Export did not consume actual nested view')
        if stage=='rejected': require(activity.get('query',{}).get('cursor')==CURSOR,'Export did not consume actual cursor view')
        if stage=='draft':
            selected=ex
            require(selected.get('user')=='UWORKFLOW' and isinstance(selected.get('label'),str) and selected['label']
                    and selected.get('initialFixtureSelection') is True and selected.get('pickerSelectionClaimed') is False,'Missing honest selected-user setup')
        if active:
            require(native.get('user')=='UWORKFLOW' and native.get('userLabel')==selected['label'],'Native selected user changed')
            gated=stage in ('gate-missing','gate-off','preflight-refused')
            require((native.get('button') is None) is gated,'Export experiment button gate changed')
            if not gated:
                require(native['button'].get('disabled') is (held or stage in ('malformed','unknown-blocked')),'Export disabled state changed')
        if stage in ('ready','list'):
            dates=ex.get('dates',{})
            require(type(dates.get('from')) is int and type(dates.get('to')) is int and dates['from']<dates['to'],'Missing renderer-local dates')
            require(native.get('from')=='2026-06-15' and native.get('to')=='2026-06-16' and native.get('status')=='SUCCESS' and native.get('limit')=='20','Native export source filters changed')
        for index,r in enumerate(requests):
            f=json.loads(r['args']['filters'])
            expected_from=dates['from']-(86400 if scenario=='export-retirement' and index==1 else 0)
            require(f==dict(user_id='UWORKFLOW',from_timestamp=expected_from,to_timestamp=dates['to']),'Local dates or uppercase status omission changed')
            require(r['dispatch']==('not-sent' if scenario=='export' and index==3 else 'synthetic-entered'),'Offline dispatch classification changed')
        if stage in phases:
            require(attempts and attempts[-1]['phase']==phases[stage],'Wrong export phase')
            a=attempts[-1]
            if active:
                notice=native.get('notice') or {}
                require(notice.get('text')==EXPORT_NOTICES[a['phase']],'Wrong exact export notice')
            if stage in ('initiated','retried','offline-retried','account-replaced'):
                require(a.get('receipt')==dict(ok=True,fixture_receipt='initiation-only',retained_unknown=17),'Raw ACK lost or claimed file completion')
            if stage=='rejected':require(a.get('receipt')==dict(ok=False,error='not_allowed'),'Rejection receipt changed')
            if stage=='offline':require(a.get('receipt')=={'failure':'offline'},'Offline became unknown')
            if stage=='malformed':require(a.get('receipt')=={'ok':'malformed'},'Malformed ACK accepted or lost')
            if stage=='route-retired':require(a.get('receipt')=={'failure':'transport'},'Reply loss receipt not retained')
        if stage in ('raw-refused','composition-refused'):
            require(ex.get('unchangedRustState') is True and not attempts and not requests,'Native refusal mutated Rust or dispatched')
            require(ex.get('composition') is (stage=='composition-refused'),'Wrong native refusal exercise')
            before,after=ex.get('before',{}),ex.get('after',{})
            value='2026-06-01' if stage=='raw-refused' else '2026-06-15'
            require(type(before.get('refusals')) is int and after.get('refusals')==before['refusals']+1
                    and before.get('value')==after.get('value')==ex.get('rawValue')==value,'Refusal did not preserve original native buffer/ack')
            require((native.get('nativeNotice') or {}).get('text')==after.get('notice')==EXPORT_REFUSAL,'Missing exact native refusal feedback')
        if stage in ('initiated','route-retired','account-replaced'):
            release=ex.get('release',{}) if stage=='account-replaced' else ex
            require(release.get('delivered') is True and release.get('cancelledBeforeReply') is False,'Late export result dropped before receipt')
        if stage=='account-replaced':
            require(ex.get('foreignDocumentHidden') is True and ex.get('currentEpoch')==ex.get('previousEpoch',-1)+1,'Account retirement not actual')
            require(book.get('current',{}).get('shown') is False,'Old export published into successor')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,'Export layout failed')
        require(g.get('activity') is active,'Export capture route mismatch')
        controls=g.get('controls',[]);require(controls,'No measured export controls')
        for item in controls+g.get('notices',[])+[native[k] for k in ('notice','nativeNotice') if native.get(k)]:
            require(item.get('visible') is True,'Export feedback/control hidden');rectangle(item.get('rect'),viewport)
        dims=g.get('pngDimensions');require(isinstance(dims,list) and len(dims)==2,'Missing export capture dimensions')
        path=captures/f'{fixture}-{stage}.png';require(Path(paths[stage])==path,'Foreign export capture path')
        if png:validate_png(path,dims)
    require(receipt.get('final')==rows[-1]['model'],'Export final state changed')
    return receipt


EXECUTION_COMPOSER = 'Uncommitted conversation draft \u4e2d\U0001f642'
ENTRY_FAILURE = 'The workflow or its permissions could not load. Your draft is retained.'
ENTRY_BACK_REFUSAL = 'Finish editing the current text before returning. Your draft is unchanged.'
ENTRY_RAW_TITLE = 'Uncommitted workflow title'


def validate_execution_source(source):
    require(isinstance(source,list) and len(source)==1 and source[0].get('ts')=='1781524800.000001','Missing canonical execution source')
    blocks=source[0].get('blocks');require(isinstance(blocks,list),'Missing raw message blocks')
    actions=[e for b in blocks if b.get('type')=='actions' for e in b.get('elements',[])]
    require(len(actions)==3,'Execution action count changed')
    labels=['View workflow execution','View missing execution','Unavailable execution action']
    ids=['E_REVIEW','E_ABSENT',7]
    for row,label,execution in zip(actions,labels,ids):
        require(row.get('type')=='button' and row.get('action_id')=='workflow_error_view_logs'
                and row.get('accessibility_label')==label,'Not actual workflow client action')
        try:value=json.loads(row['value'])
        except (KeyError,TypeError,ValueError) as e:raise RuntimeError('Wrong raw execution action value') from e
        require(value==dict(type='wfb',wf_id='WF_REVIEW',execution_id=execution),'Execution action target changed')


def validate_execution(fixture,receipt,captures,scenario,*,platform=sys.platform,png=True):
    require(receipt.get('ok') is True,f"Native execution failed: {receipt.get('error',receipt)}")
    require(receipt.get('fixture')==fixture and receipt.get('case')==scenario,'Wrong execution receipt identity')
    require(receipt.get('actualMessageAction') is True and receipt.get('aboutConfiguration') is False
            and receipt.get('actualActivityEntry') is True and receipt.get('noLiveNetwork') is True,'Missing direct entry/isolation proof')
    rows,paths=receipt.get('stages'),receipt.get('captures');viewport=expected_viewport(fixture,platform)
    require(receipt.get('viewport')==viewport,'Wrong execution viewport')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[scenario]),'Missing execution stages')
    require(isinstance(paths,dict) and set(paths)==set(STAGES[scenario]),'Missing execution captures')
    counts={'execution':[0,2,2,3,4,4,4,6,7],'execution-failure':[0,0,2,3,4,4],'execution-retirement':[0,1,1,2,2]}[scenario]
    methods={'execution':['workflows.activity.list','workflows.activity.get','workflows.activity.list','workflows.activity.list','workflows.activity.list','workflows.activity.get','workflows.activity.list'],
             'execution-failure':['workflows.activity.list','workflows.activity.get','workflows.activity.list','workflows.activity.list'],
             'execution-retirement':['workflows.activity.list','workflows.activity.list']}[scenario]
    previous=[];source=None;composer=None;composer_model=None;dirty_id=None
    for i,row in enumerate(rows):
        stage=row['stage'];m=row.get('model',{});g=row.get('geometry',{});p=row.get('proof',{});ex=p.get('exercise') or {}
        entry=m.get('entry',{});meta=m.get('metadata',{});activity=m.get('activity');ledger=m.get('ledger',{});origin=p.get('source',{})
        require(meta.get('networkCalls')==meta.get('writes')==ledger.get('networkCalls')==0 and meta.get('pending') is False,'Execution used network or wrote metadata')
        requests=ledger.get('requests');validate_requests(requests)
        require([r['method'] for r in requests]==methods[:counts[i]] and len(requests)==counts[i]
                and requests[:len(previous)]==previous,'Execution reread/reopened/auto-paged unexpectedly');previous=requests
        for request in requests:
            if request['method'].endswith('.get'):require(request['args']['workflow_execution_id']=='E_REVIEW','Missing target triggered GET')
        held=stage in ('route-held','account-held');cancelled=2 if stage=='account-replaced' else 1 if stage in ('route-retired','account-held') else 0
        require(ledger.get('finished')==len(requests)-int(held) and ledger.get('cancelled')==cancelled,'Retired execution request not cancelled')
        require((type(ledger.get('held')) is int) is held,'Wrong execution held read')
        direct=stage not in ('transcript','returned','route-retired','account-replaced')
        active=direct and stage not in ('metadata-draft','policy-denied')
        detail=stage in ('opened','reopened','policy-retried')
        require(entry.get('active') is direct and entry.get('busy') is held and entry.get('autoPending') is False,'Wrong direct entry state or unreleased intent')
        require(meta.get('busy') is held,'Wrong metadata read custody')
        if direct:
            require(entry.get('workflow')=='WF_REVIEW' and entry.get('execution')==('E_ABSENT' if stage=='missing-target' else 'E_REVIEW'),'Wrong direct workflow target')
            require(type(entry.get('visit')) is int and entry['visit']>0,'Missing direct visit')
        if active:
            require(isinstance(activity,dict) and activity.get('active') is True,'Missing direct Activity panel')
            require(isinstance(activity.get('detail'),dict) is detail,'Execution reopened after close/filter/page or lost target')
            if detail:
                event=activity['detail'].get('workflow_event',{})
                require(event.get('workflow_execution_id')=='E_REVIEW' and event.get('workflow_status')=='PENDING','Wrong execution detail')
                require(g.get('detailStatus')=='Processing','Detail status borrowed list state')
            if stage=='paged':require(activity.get('query',{}).get('cursor')==CURSOR,'Missing actual opaque pagination')
            if stage in ('filtered','paged'):require(activity.get('query',{}).get('limit')==20 and activity['query'].get('filters',{}).get('workflow_instance_status')=='SUCCESS','Missing actual filter edits')
        if stage=='policy-denied':require(entry.get('notice')==ENTRY_FAILURE and activity is None,'Policy denial changed or exposed editor')
        if stage=='read-failed':require(activity.get('notice')==FAILURE,'Read failure guidance changed')
        if stage=='missing-target':require(ex.get('readCountAfter')==ex.get('readCountBefore',-2)+1==len(requests),'Missing target fabricated read or auto-pagination')
        if stage!='account-replaced':
            require(origin.get('team')=='TWORKFLOW' and origin.get('channel')=='C2' and origin.get('ordinaryChannel') is True
                    and origin.get('messagesTab') is True and origin.get('aboutPresent') is False and origin.get('homePresent') is False,'Direct entry relied on fake About/bot DM')
            validate_execution_source(origin.get('source'))
            if source is None:source=origin['source'];composer_model=origin.get('composerModel')
            require(origin.get('source')==source and origin.get('composerModel')==composer_model and origin.get('appModal') is False,'Execution mutated source/composer or opened generic modal')
            native=p.get('composer',{})
            require(native.get('sameNode') is True and native.get('value')==EXECUTION_COMPOSER
                    and native.get('start')==3 and native.get('end')==10 and native.get('visible') is (not direct),'Original native composer lost or exposed behind panel')
            require(isinstance(native.get('visit'),str) and native['visit'] and isinstance(native.get('scope'),str) and native['scope'],'Missing composer mount witness')
            if composer is None:composer=(native['visit'],native['scope'])
            require((native['visit'],native['scope'])==composer,'Composer remounted during direct entry')
        if scenario=='execution' and i>=5:
            docs=origin.get('documents',[]);matches=[d for d in docs if d.get('title')==TITLE and d.get('description')==DESCRIPTION and d.get('dirty') is True]
            require(len(matches)==1,'Dirty metadata lost on conversation return/reopen')
            if dirty_id is None:dirty_id=matches[0]['id']
            require(matches[0]['id']==dirty_id,'Dirty metadata cloned/replaced')
            if direct:require(meta.get('title')==TITLE and meta.get('description')==DESCRIPTION and meta.get('dirty') is True,'Reopened editor lost exact draft')
        if stage=='transcript':
            bad=ex.get('malformed',{})
            require(ex.get('unchangedAfterMalformed') is True and bad.get('disabled') is True
                    and bad.get('label')=='Unavailable execution action','Malformed client action dispatched or enabled')
        if stage in ('transcript','returned','route-retired'):
            header=p.get('header',{})
            require(isinstance(header,dict),'Missing original conversation header proof')
            require(not direct and all(header.get(k) is True for k in ('ok','sameHeader','sameName','sameConversation','visible'))
                    and header.get('text')=='#ship' and header.get('display')=='flex',
                    'Conversation header hidden, replaced or incorrect after entry')
            require(type(header.get('scrollTop')) in (int,float) and header['scrollTop']==0
                    and type(header.get('originalScrollTop')) in (int,float) and header['originalScrollTop']==0,
                    'Conversation scrolled away from original header')
            rectangle(header.get('rect'),viewport)
        else:require(p.get('header') is None,'Claimed original conversation header in owned or replacement panel')
        if stage=='metadata-draft':
            refusal=ex.get('backRefusal',{});native=refusal.get('native',{});restored=refusal.get('restored',{})
            require(refusal.get('before')==refusal.get('after')==m
                    and refusal.get('sourceBefore')==refusal.get('sourceAfter')==origin,
                    'Refused Back changed model or retained document')
            require(all(native.get(k) is True for k in ('ok','sameRoot','sameTitle','sameNotice','visible'))
                    and native.get('value')==ENTRY_RAW_TITLE and native.get('notice')==ENTRY_BACK_REFUSAL,
                    'Refused Back lost original native buffer or readable notice')
            rectangle(native.get('rect'),viewport)
            require(restored.get('ok') is True and restored.get('sameTitle') is True and restored.get('value')==TITLE,
                    'Refused Back did not restore original native title')
            require(any(n.get('text')==ENTRY_BACK_REFUSAL for n in g.get('notices',[])),
                    'Missing captured Back refusal guidance')
        if stage in ('route-retired','account-replaced'):
            release=ex.get('release',{}) if stage=='account-replaced' else ex
            require(release.get('cancelledBeforeReply') is True and release.get('delivered') is False,'Late retired execution result delivered')
        if stage=='account-replaced':
            require(ex.get('foreignDocumentHidden') is True and ex.get('currentEpoch')==ex.get('previousEpoch',-1)+1,'Account was not actually replaced')
            require(activity is None and p.get('composer') is None,'Old detail/composer claimed on replacement account')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport
                and g.get('activity') is active and g.get('detail') is detail,'Execution geometry/route failed')
        controls=g.get('controls',[]);require(controls,'Missing execution controls')
        for item in controls+g.get('notices',[]):require(item.get('visible') is True,'Execution control/feedback hidden');rectangle(item.get('rect'),viewport)
        if stage in ('policy-denied','read-failed'):
            expected=ENTRY_FAILURE if stage=='policy-denied' else FAILURE
            require(any(n.get('text')==expected for n in g.get('notices',[])),'Missing readable failure guidance')
        dims=g.get('pngDimensions');require(isinstance(dims,list) and len(dims)==2,'Missing execution PNG dimensions')
        path=captures/f'{fixture}-{stage}.png';require(Path(paths[stage])==path,'Foreign execution capture')
        if png:validate_png(path,dims)
    require(receipt.get('final')==rows[-1]['model'],'Execution final receipt changed')
    return receipt


def validate_log(native, fixture, captures, scenario, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = validate(fixture, receipts[0], captures, scenario, platform=platform, png=png)
    expected = [dict(stage=stage, edge=edge) for stage in STAGES[scenario] for edge in ('before', 'after')]
    require(native.receipts(EDGE) == expected, 'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures, scenario):
    require(fixture in FIXTURES, 'Unknown workflow fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'workflow-activity-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE': scenario, 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_WORKFLOW_ACTIVITY_MEMBERS_CHECK': '1' if scenario == 'picker' else '0',
                            'SUPER_PLATINUM_WORKFLOW_ACTIVITY_NONCE': nonce,
                            'SUPER_PLATINUM_WORKFLOW_ACTIVITY_READY_DIR': str(directory)})
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
                            evidence['native'] = validate_log(native, fixture, captures, scenario)
                            validate_owned(evidence['native'], expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'Workflow native receipt timed out')
                        time.sleep(.05)
            except BaseException:
                try:
                    # Keep the named "failure" stage intact if outer receipt
                    # validation fails after the native scenario completes.
                    path = captures / f'{fixture}-harness-failure.png'
                    native.capture(path)
                    evidence['failureScreenshot'] = str(path)
                except (OSError, RuntimeError, ValueError) as error:
                    evidence['failureCaptureError'] = str(error)
                raise
        require(native.process.poll() is not None and not native.cleanup_errors, 'Fixture process or cleanup did not finish cleanly')
        if sys.platform == 'win32':
            H['validate_pointer_cleanup'](evidence.get('pointer', {}))
        validate_closed(native, expected, captures, evidence['native'], scenario, evidence.get('expectedPngDimensions'))
        if handle:
            evidence['exitIdentity'] = process_api.times(handle)
            H['validate_exit'](evidence['processCreationFiletime'], evidence['exitIdentity'])
        require(H['binary_sha'](binary) == digest, 'Binary changed during native run')
        evidence.update(ok=True, closedLogRevalidated=True)
    except BaseException as error:
        evidence['error'] = str(error)
        evidence['terminalReceipts'] = native.receipts(PREFIX)
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/workflow-activity-native')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    parser.add_argument('--case', action='append', choices=STAGES)
    args = parser.parse_args()
    for scenario in args.case or ('browse', 'retirement'):
        captures = args.captures.resolve() / scenario
        for fixture in args.fixture or (PICKER_FIXTURES if scenario == 'picker' else BASE_FIXTURES):
            require((fixture in PICKER_FIXTURES) is (scenario == 'picker'), 'Fixture/case mismatch')
            require(not (captures / f'{fixture}.log').exists(), 'Use a fresh capture directory')
            run_fixture(fixture, args.binary.resolve(), captures, scenario)

if __name__ == '__main__':
    main()
