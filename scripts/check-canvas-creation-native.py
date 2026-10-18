#!/usr/bin/env python3
"""Stateful isolated Canvas native exercise + fresh-process durable readback.

Never builds or accesses real session/storage. Geometry is sampled, not an atomic
capture guarantee. Events/IME are synthetic; actual images require human/agent read.
"""
import argparse
from contextlib import ExitStack
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import sqlite3
import sys
sys.dont_write_bytecode = True
import time
from urllib.parse import parse_qsl, urlsplit

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
from native_check import expected_viewport

H = runpy.run_path(str(ROOT / 'scripts/check-multi-external-select-native.py'))
PREFIX = 'canvas-creation-check: '
WAITING = 'canvas-creation-waiting: '
STAGE = 'canvas-creation-stage: '
ENV = 'SUPER_PLATINUM_CANVAS_CREATION_'
FIXTURES = ('canvas-creation-durable', 'canvas-creation-durable-narrow')
STAGES = {
    'departure': ('initial', 'palette', 'returned', 'capture-refused', 'capture-resumed', 'save-refused', 'review', 'chosen'),
    'editing': ('initial', 'edited', 'native-ahead', 'composing', 'reopened'),
    'known-unsent-conflict': ('initial', 'unsent', 'conflict', 'review', 'chosen'),
    'late-result': ('initial', 'pending', 'closed', 'created', 'archived', 'uncertain', 'reopened'),
}
TITLE = 'Native canvas \u65e5\u672c\u8a9e \U0001f680'
BODY = '## Exact body\n\n  \u4e2d\U0001f642 e\u0301  \n- [ ] Keep whitespace\n'
AHEAD = 'Native ahead\n\u65e5\u672c\u8a9e \U0001f680 e\u0301\n'
COMPOSED = 'Composed \u65e5\u672c\u8a9e \U0001f680 e\u0301\n'
OURS = 'Our retained local version \u4e2d\U0001f642 e\u0301\n'
OWNER = {'account': '00000000-0000-4000-8000-0000000000c1', 'team': 'T1', 'user': 'U00000001'}
TABLES = ('identity', 'scopes', 'drafts', 'attempts', 'conflicts', 'decisions', 'dispositions',
          'retired_attempts', 'retired_decisions', 'evidence_dispositions', 'late_evidence')
REQUIRED = {
    'departure': ('oneShortcutDeparture', 'oneNavigation', 'nativeAheadCaptured', 'returnExactSaved', 'captureRefusalRetained', 'saveRefusalRetained', 'intentCanceled', 'noAutoRetry', 'explicitChoice', 'noDefaultChoice'),
    'editing': ('autosave', 'nativeAheadCaptured', 'compositionDeferred', 'closeReopen'),
    'known-unsent-conflict': ('knownUnsentExactBase', 'noDispatchOnRefusal', 'conflictPreserved', 'noDefaultChoice', 'explicitChoice'),
    'late-result': ('heldResponse', 'lateCreatedRetained', 'noNavigationSteal', 'archiveOrthogonal', 'uncertainNoReplay'),
}


def require(condition, text):
    if not condition:
        raise RuntimeError(text)


def ready_tuple(fixture, nonce, pid, scenario, phase):
    require(fixture in FIXTURES and scenario in STAGES and phase in ('exercise', 'restart'), 'Unknown fixture phase')
    require(isinstance(nonce, str) and re.fullmatch('[0-9a-f]{32}', nonce), 'Invalid nonce')
    require(type(pid) is int and pid > 0, 'Invalid PID')
    return dict(version=1, fixture=fixture, nonce=nonce, pid=pid, scenario=scenario, phase=phase)


def validate_ready(actual, expected):
    require(actual == expected and type(actual.get('pid')) is int and type(actual.get('version')) is int,
            'Wrong exact readiness tuple')


def publish_ready(directory, expected):
    path = directory / f"ready-{expected['phase']}.json"
    staged = directory / f"ready-{expected['phase']}.staged"
    with staged.open('x', encoding='utf-8') as output:
        json.dump(expected, output)
        output.flush()
        os.fsync(output.fileno())
    try:
        os.link(staged, path)
    finally:
        staged.unlink()


def validate_request(request, body, executed):
    require(request.get('method') == 'POST' and request.get('retrySafe') is False and request.get('executed') is executed,
            'Wrong mutation/dispatch policy')
    url = urlsplit(request.get('url', ''))
    require((url.scheme, url.netloc, url.path, url.fragment) == ('https', 'fixture.slack.com', '/api/files.createCanvas', ''),
            'Wrong actual prepared endpoint')
    pairs = parse_qsl(url.query, keep_blank_values=True, strict_parsing=True)
    query = dict(pairs)
    expected = {'_x_desktop_ia': 'true', '_x_foreground': 'true', '_x_frontend_build_type': 'current',
                '_x_gantry': 'true', '_x_num_retries': '0', '_x_version_ts': 'capture-required', 'slack_route': 'default'}
    require(len(pairs) == 10 and set(query) == set(expected) | {'_x_id', '_x_csid', 'fp'}, 'Wrong exact xparams keys')
    require(all(query[k] == v for k, v in expected.items()), 'Changed xparams contract')
    require(all(re.fullmatch('[0-9a-f]{32}', query[k]) for k in ('_x_id', '_x_csid', 'fp')), 'Malformed xparams identity')
    require(request.get('form') == [['token', 'canvas-fixture-not-a-token'], ['title', TITLE], ['markdown', body]],
            'Actual request did not preserve exact ordered content')
    headers = request.get('headers')
    require(isinstance(headers, list) and len(headers) == len(dict(headers)), 'Missing/duplicate prepared headers')
    header = {k.lower(): v for k, v in headers}
    require(header.get('content-type') == 'application/x-www-form-urlencoded', 'Wrong form content type')
    require('authorization' not in header and 'cookie' not in header, 'Unexpected credential header')


def expected_body(scenario):
    return {'editing': COMPOSED, 'known-unsent-conflict': OURS, 'late-result': AHEAD, 'departure': OURS}[scenario]


def validate_inventory(receipt):
    inventory = receipt['inventory']
    require(isinstance(inventory, dict) and set(inventory) == {'active', 'drafts', 'attempts', 'conflicts', 'lateResults', 'decisions', 'retryAt'},
            'Incomplete durable inventory')
    active = inventory['active']
    require(active['owner'] == OWNER and active['content'] == {'title': TITLE, 'markdown': expected_body(receipt['scenario'])},
            'Wrong durable canonical content/owner')
    require(not active['archived'] and inventory['retryAt'] == 0, 'Wrong active/cooldown state')
    require(inventory['conflicts'] == [] and inventory['lateResults'] == [], 'Unexpected unresolved text/result evidence')
    drafts, attempts = inventory['drafts'], inventory['attempts']
    require(all(d['owner'] == OWNER for d in drafts) and all(a['owner'] == OWNER for a in attempts), 'Inventory owner mismatch')
    require(sum(d == active for d in drafts) == 1, 'Active draft is not exactly inventoried')
    if receipt['scenario'] == 'late-result':
        require(len(drafts) == 2 and len(attempts) == 2 and len(inventory['decisions']) == 1, 'Wrong retained delivery inventory')
        created = [a for a in attempts if a['delivery'] == {'kind': 'Created', 'file_id': 'FCANVASFIXTURE1'}]
        uncertain = [a for a in attempts if a['delivery'] == {'kind': 'Uncertain'}]
        require(len(created) == len(uncertain) == 1 and active['attempt'] == uncertain[0]['id'], 'Wrong exact created/uncertain associations')
        require(created[0]['frozen'] == {'title': TITLE, 'markdown': BODY} and uncertain[0]['frozen'] == active['content'], 'Frozen attempt content differs')
        require(any(d['archived'] and d['id'] == created[0]['draft_id'] and d['attempt'] == created[0]['id'] for d in drafts), 'Archived positive result not retained')
    else:
        require(len(drafts) == 1 and attempts == [] and inventory['decisions'] == [] and active['attempt'] is None,
                'Unexpected send authority in local-only case')


def rectangle(value, viewport):
    require(isinstance(value, dict), 'Missing measured rectangle')
    keys = ('left', 'top', 'right', 'bottom', 'width', 'height')
    require(all(type(value.get(k)) in (int, float) and math.isfinite(value[k]) for k in keys), 'Nonfinite/missing rectangle coordinate')
    require(value['width'] > 0 and value['height'] > 0 and abs(value['right'] - value['left'] - value['width']) <= 1
            and abs(value['bottom'] - value['top'] - value['height']) <= 1, 'Invalid rectangle arithmetic')
    require(value['left'] >= 0 and value['top'] >= 0 and value['right'] <= viewport[0] + 1
            and value['bottom'] <= viewport[1] + 1, 'Measured control/dialog outside viewport')


def expected_feedback(scenario, phase, stage):
    saved = ('status', 'Draft saved on this device.')
    uncertain = ('alert', 'Slack may have created this canvas. Its exact draft and delivery receipt are retained. Checking Files does not authorize a retry.')
    composing = ('alert', 'Finish composing text, then try again.')
    if phase == 'restart':
        return (uncertain,) if scenario == 'late-result' else (saved,)
    return {
        'editing': {'edited': (saved,), 'native-ahead': (saved,), 'composing': (composing,), 'reopened': (saved,)},
        'departure': {'returned': (saved,), 'capture-refused': (composing,), 'capture-resumed': (saved,),
                      'save-refused': (('alert', 'Your text is retained. Save or review it before navigating.'),), 'chosen': (saved,)},
        'known-unsent-conflict': {'unsent': (saved,), 'conflict': (('alert', 'Your text is retained here. Retry its local save before leaving.'),), 'chosen': (saved,)},
        'late-result': {'pending': (('status', 'Retaining your canvas\u2026'),), 'created': (('status', 'Your canvas is ready.'),),
                        'archived': (saved,), 'uncertain': (uncertain,), 'reopened': (uncertain,)},
    }[scenario].get(stage, ())


def inside_rectangle(inner, outer):
    return inner['left'] >= outer['left'] - 1 and inner['top'] >= outer['top'] - 1 \
        and inner['right'] <= outer['right'] + 1 and inner['bottom'] <= outer['bottom'] + 1


def validate_geometry(value, viewport, required_feedback=()):
    require(value.get('ok') is True and value.get('noOverflow') is True and value.get('footerHitTested') is True,
            'Covered or overflowing controls')
    require(value.get('viewport') == dict(width=viewport[0], height=viewport[1]), 'Geometry viewport mismatch')
    rectangle(value.get('dialog'), viewport)
    client, scroll = value.get('clientWidth'), value.get('scrollWidth')
    require(type(client) in (int, float) and type(scroll) in (int, float) and math.isfinite(client) and math.isfinite(scroll)
            and client > 0 and scroll > 0 and scroll <= client + 1, 'Measured horizontal overflow')
    controls = value.get('controls')
    require(isinstance(controls, list) and len(controls) > 0, 'Missing measured active controls')
    for control in controls:
        require(isinstance(control.get('label'), str) and control['label'] and control.get('hit') is True, 'Unproven control hit target')
        rectangle(control.get('rect'), viewport)
    if value.get('kind') == 'palette':
        return
    footer, fields, dialog = value.get('footer'), value.get('fields'), value['dialog']
    rectangle(footer, viewport)
    rectangle(fields, viewport)
    require(inside_rectangle(footer, dialog) and inside_rectangle(fields, dialog)
            and fields['bottom'] <= footer['top'] + 1, 'Editor/footer layout overlaps or escapes dialog')
    feedback = value.get('feedback')
    require(isinstance(feedback, dict) and 'region' in feedback and isinstance(feedback.get('messages'), list), 'Missing feedback geometry')
    region, messages = feedback['region'], feedback['messages']
    if region is None:
        require(not messages and not required_feedback, 'Required feedback absent')
        return
    rectangle(region, viewport)
    require(messages and inside_rectangle(region, dialog) and region['bottom'] <= footer['top'] + 1
            and region['top'] >= fields['bottom'] - 1, 'Feedback region overlaps editor/footer or escapes dialog')
    observed = []
    for message in messages:
        require(isinstance(message, dict) and message.get('role') in ('alert', 'status')
                and isinstance(message.get('text'), str) and message['text'].strip(), 'Missing feedback copy/role')
        require(message.get('visible') is True and message.get('unclipped') is True, 'Feedback is hidden, covered or clipped')
        rectangle(message.get('rect'), viewport)
        require(inside_rectangle(message['rect'], region), 'Feedback text outside fixed region')
        observed.append((message['role'], message['text']))
    require(all(item in observed for item in required_feedback), 'Required feedback absent or changed')


def validate_departure_inventory(inventory, body, conflicts):
    require(isinstance(inventory, dict) and set(inventory) == {
        'active', 'drafts', 'attempts', 'conflicts', 'lateResults', 'decisions', 'retryAt'},
        'Incomplete intermediate departure inventory')
    active = inventory.get('active')
    require(isinstance(active, dict) and active.get('owner') == OWNER
            and isinstance(active.get('id'), str) and active['id']
            and active.get('content') == dict(title=TITLE, markdown=body)
            and active.get('archived') is False and 'attempt' in active and active['attempt'] is None,
            'Wrong intermediate canonical identity/content or delivery authority')
    require(inventory['drafts'] == [active], 'Intermediate canonical row not exactly inventoried')
    require(inventory['attempts'] == [] and inventory['lateResults'] == [] and inventory['decisions'] == []
            and type(inventory['retryAt']) is int and inventory['retryAt'] == 0,
            'Unexpected intermediate delivery/history/cooldown authority')
    require(inventory['conflicts'] == conflicts, 'Intermediate conflict inventory differs')
    for conflict in conflicts:
        require(isinstance(conflict, dict) and set(conflict) == {'id', 'owner', 'draft_id', 'base_revision', 'writer', 'content'}
                and conflict.get('owner') == OWNER and conflict.get('draft_id') == active['id']
                and isinstance(conflict.get('id'), str) and conflict['id']
                and isinstance(conflict.get('writer'), str) and conflict['writer']
                and type(conflict.get('base_revision')) is int and conflict['base_revision'] > 0
                and conflict.get('content') == dict(title=TITLE, markdown=OURS),
                'Intermediate conflict lost exact owner/draft/content association')


def validate_departure_palette(palette, viewport):
    validate_geometry(palette, viewport)
    require(palette.get('kind') == 'palette' and palette.get('focused') is True
            and palette.get('canvasAbsent') is True and type(palette.get('shortcutCount')) is int
            and palette['shortcutCount'] == 1, 'Actual focused Palette is unproven')
    require(len(palette['controls']) == 1 and palette['controls'][0].get('label') == 'Channel or person',
            'Palette capture does not identify its exact input control')


def validate_departure(result, geometry, viewport):
    shortcuts = result.get('shortcuts')
    require(isinstance(shortcuts, list) and len(shortcuts) == 3, 'Wrong departure shortcut inventory')
    for ordinal, event in enumerate(shortcuts, 1):
        require(event.get('count') == ordinal and type(event.get('count')) is int
                and event.get('key') == 'k' and event.get('ctrl') is True and event.get('meta') is False
                and event.get('trusted') is False and event.get('prevented') is True,
                'Shortcut was repeated, bypassed, or not observed by the real bridge')
    before, after = result.get('beforeGeneration'), result.get('afterGeneration')
    require(type(before) is int and type(after) is int and before >= 0 and after == before + 1,
            'Palette did not navigate exactly once')
    generation = result.get('refusalGeneration')
    require(type(generation) is int and generation == after + 1 and result.get('finalRefusalGeneration') == generation,
            'Refused departure navigated')
    palette = result.get('palette', {})
    validate_departure_palette(palette, (viewport['width'], viewport['height']))
    validate_departure_palette(geometry[1], (viewport['width'], viewport['height']))
    first, returned = result.get('firstVisit'), result.get('returned', {})
    require(isinstance(first, str) and first and returned.get('visit') and returned['visit'] != first
            and shortcuts[0].get('visit') == first, 'Canvas did not actually unmount/remount')
    require(returned.get('title') == TITLE and returned.get('markdown') == AHEAD,
            'Reopened exact captured text differs')
    saved = result.get('savedInventory', {})
    validate_departure_inventory(saved, AHEAD, [])
    for name, text, error in (
            ('captureRefusal', COMPOSED, 'Finish composing text, then try again.'),
            ('saveRefusal', OURS, 'Your text is retained. Save or review it before navigating.')):
        refused = result.get(name, {})
        native = refused.get('native', {})
        require(refused.get('sameEditor') is True and refused.get('pending') is False
                and refused.get('closeRequested') is False and refused.get('error') == error,
                'Refusal lost the editor or retained navigation authority')
        require(native.get('title') == TITLE and native.get('markdown') == text
                and native.get('visit') == returned['visit'] and native.get('error') == error,
                'Refusal did not retain exact original native input/visit')
        require(native.get('codePoints') == list(map(ord, text))
                and native.get('utf16Length') == len(text.encode('utf-16-le')) // 2,
                'Refused native Unicode differs')
        require(all(native.get(k) is False for k in ('titleReadonly', 'bodyReadonly', 'titleDisabled', 'bodyDisabled')),
                'Refusal left native input locked')
    require(shortcuts[1].get('visit') == returned['visit'] == shortcuts[2].get('visit'), 'Refusal source visit changed')
    require(result['captureRefusal'].get('inventory') == saved and result['captureRefusal'].get('editorMarkdown') == AHEAD,
            'Provisional composition changed durable or accepted content')
    refused = result['saveRefusal']
    require(refused.get('retainedSlots') == 1 and type(refused.get('retainedSlots')) is int
            and refused.get('editorFailed') is True, 'Exact retained failed save custody missing')
    inventory = refused.get('inventory', {})
    require(refused.get('stableInventory') == inventory, 'Refused save automatically mutated its inventory')
    require(inventory.get('active', {}).get('content') == dict(title=TITLE, markdown='Other isolated writer; retain this until explicit review.')
            and inventory['active'].get('owner') == OWNER and inventory.get('attempts') == [],
            'Save refusal changed competing canonical or admitted delivery')
    conflicts = inventory.get('conflicts')
    require(isinstance(conflicts, list) and len(conflicts) == 1 and conflicts[0].get('content') == dict(title=TITLE, markdown=OURS)
            and conflicts[0].get('owner') == OWNER
            and result.get('reviewedInventory', {}).get('conflicts') == conflicts,
            'Exact refused text conflict was not preserved through explicit review')
    validate_departure_inventory(inventory, 'Other isolated writer; retain this until explicit review.', conflicts)
    validate_departure_inventory(result['reviewedInventory'], 'Other isolated writer; retain this until explicit review.', conflicts)
    require(result['reviewedInventory'] == inventory, 'Explicit review changed the retained refused inventory')


def validate_native(expected, directory, receipt, edges, dimensions=None, platform=sys.platform, png=True):
    require(receipt.get('ok') is True, f'Native assertion failed: {receipt}')
    for k in ('nonce', 'pid', 'fixture', 'scenario', 'phase'):
        require(receipt.get(k) == expected[k] and (k != 'pid' or type(receipt[k]) is int), f'Wrong receipt {k}')
    for k in ('noSession', 'noTransport', 'noLiveNetwork', 'syntheticInput'):
        require(receipt.get(k) is True, f'Missing isolation assertion {k}')
    require(receipt.get('isolatedDatabase') == str(directory / 'fixture.sqlite'), 'Wrong isolated DB path')
    viewport = receipt.get('viewport', {})
    require([viewport.get('width'), viewport.get('height')] == expected_viewport(expected['fixture'], platform), 'Wrong viewport')
    phase, scenario = expected['phase'], expected['scenario']
    stages = ('restart',) if phase == 'restart' else STAGES[scenario]
    require(edges == [dict(phase=phase, stage=s, edge=e) for s in stages for e in ('before', 'after')], 'Wrong capture edge sequence')
    captures = receipt.get('captures')
    require(isinstance(captures, dict) and set(captures) == set(stages), 'Wrong capture inventory')
    for stage in stages:
        path = directory / f"{expected['fixture']}-{phase}-{stage}.png"
        require(captures[stage] == str(path), 'Wrong exact capture path')
        if png:
            H['png_signature'](path, dimensions)
    geometry = receipt.get('geometry')
    require(isinstance(geometry, list) and len(geometry) == len(stages) - int('closed' in stages), 'Missing stage geometry')
    for stage, g in zip((stage for stage in stages if stage != 'closed'), geometry):
        validate_geometry(g, expected_viewport(expected['fixture'], platform), expected_feedback(scenario, phase, stage))
    result = receipt.get('result', {})
    for k in (('restoredFromStore', 'noReplay') if phase == 'restart' else REQUIRED[scenario]):
        require(result.get(k) is True, f'Missing workflow proof {k}')
    native = result.get('native', {})
    body = expected_body(scenario)
    locked = scenario == 'late-result'
    require(receipt.get('model', {}).get('uncertain') is locked and receipt.get('model', {}).get('editable') is (not locked)
            and receipt['model'].get('delivery') == ({'kind': 'Uncertain'} if locked else None),
            'Wrong restored model delivery/editability')
    for key in ('titleReadonly', 'bodyReadonly', 'titleDisabled', 'bodyDisabled'):
        require(native.get(key) is locked, 'Wrong actual DOM editability')
    require(native.get('title') == TITLE and native.get('markdown') == body, 'Native restored text differs')
    require(native.get('codePoints') == list(map(ord, body)) and native.get('utf16Length') == len(body.encode('utf-16-le')) // 2,
            'Unicode scalar/UTF16 evidence differs')
    require(receipt.get('syntheticIME') is (scenario in ('editing', 'departure') and phase == 'exercise'), 'Wrong IME evidence scope')
    request_bodies = [] if phase == 'restart' or scenario in ('editing', 'departure') else ([BODY] if scenario == 'known-unsent-conflict' else [BODY, AHEAD])
    executed = scenario == 'late-result' and phase == 'exercise'
    require(type(receipt.get('preparedRequests')) is int and receipt['preparedRequests'] == len(request_bodies), 'Wrong prepared count')
    require(type(receipt.get('executedRequests')) is int and receipt['executedRequests'] == (len(request_bodies) if executed else 0), 'Wrong execute count')
    requests = receipt.get('requests')
    require(isinstance(requests, list) and len(requests) == len(request_bodies), 'Missing actual request records')
    for request, body in zip(requests, request_bodies):
        validate_request(request, body, executed)
    validate_inventory(receipt)
    if scenario == 'departure' and phase == 'exercise':
        validate_departure(result, geometry, viewport)
    if scenario in ('known-unsent-conflict', 'departure') and phase == 'exercise':
        reviewed = result.get('reviewedNative', {})
        for key, text in [('canonical', 'Other isolated writer; retain this until explicit review.'), ('retained', OURS)]:
            require(reviewed.get(key, {}).get('markdown') == text and reviewed[key].get('readonly') is True,
                    'Complete review disclosure was not read from actual DOM')
        review = result.get('reviewedInventory', {})
        require(len(review.get('conflicts', [])) == 1 and review['conflicts'][0]['content'] == dict(title=TITLE, markdown=OURS),
                'Wrong exact conflicting text inventory')


def private_snapshot(directory):
    path = directory / 'fixture.sqlite'
    require(path.is_file() and not path.is_symlink(), 'Missing isolated closed database')
    require(path.stat().st_size <= 128 * 1024 * 1024, 'Unexpected fixture database size')
    with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True) as db:
        require(db.execute('PRAGMA quick_check').fetchall() == [('ok',)], 'Isolated DB integrity failure')
        tables = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        require(tables == set(TABLES), 'Unexpected fixture schema')
        result = {table: db.execute(f'SELECT * FROM {table} ORDER BY 1').fetchall() for table in TABLES}
        require(result['identity'] == [(1, OWNER['account'])], 'Wrong isolated DB owner')
        result['user_version'] = db.execute('PRAGMA user_version').fetchone()[0]
    return result


def validate_snapshot(snapshot, receipt):
    inventory = receipt['inventory']
    mapping = {'drafts': 'drafts', 'attempts': 'attempts', 'conflicts': 'conflicts',
               'decisions': 'decisions', 'late_evidence': 'lateResults'}
    for table, field in mapping.items():
        actual = sorted((json.loads(row[1]) for row in snapshot[table]), key=lambda v: json.dumps(v, sort_keys=True))
        expected = sorted(inventory[field], key=lambda v: json.dumps(v, sort_keys=True))
        require(actual == expected, f'Closed SQLite {table} differs from native inventory')
    scopes = [json.loads(row[1]) for row in snapshot['scopes']]
    require(scopes == [dict(owner=OWNER, active=inventory['active']['id'], retry_at=inventory['retryAt'])], 'Wrong closed scope association')
    require(all(snapshot[t] == [] for t in ('retired_attempts', 'retired_decisions', 'evidence_dispositions')), 'Unexpected history retirement')
    require(len(snapshot['dispositions']) == int(receipt['scenario'] in ('known-unsent-conflict', 'departure')), 'Wrong durable conflict disposition count')
    if receipt['scenario'] in ('known-unsent-conflict', 'departure') and receipt['phase'] == 'exercise':
        conflict = receipt['result']['reviewedInventory']['conflicts'][0]
        expected = {k: conflict[k] for k in ('id', 'owner', 'draft_id', 'base_revision', 'writer')}
        require(snapshot['dispositions'][0][0] == expected['id'] and json.loads(snapshot['dispositions'][0][1]) == expected,
                'Disposition does not retire the exact reviewed conflict')



def validate_closed_log(path, expected, directory, admitted, dimensions=None, png=True, platform=sys.platform):
    waiting = H['closed_receipts'](path, WAITING)
    receipts = H['closed_receipts'](path, PREFIX)
    require(len(waiting) == len(receipts) == 1, 'Closed log has extra/missing receipts')
    validate_ready(waiting[0], expected)
    require(receipts[0] == admitted, 'Terminal receipt changed during shutdown')
    validate_native(expected, directory, receipts[0], H['closed_receipts'](path, STAGE), dimensions, platform=platform, png=png)


def audit_cleanup(evidence, native, process_api, handle, platform=sys.platform):
    # Always called after context cleanup, including failed readiness/workflows.
    errors = evidence.setdefault('cleanupErrors', [])
    try:
        evidence['processExited'] = native.process is not None and native.process.poll() is not None
        if native.process is not None and not evidence['processExited']:
            errors.append('Owned process did not exit')
    except Exception as error:
        evidence['processExited'] = False
        errors.append('Process status: ' + str(error))
    if platform == 'win32' and 'pointer' in evidence:
        try:
            H['validate_pointer_cleanup'](evidence['pointer'])
            evidence['pointerCleanupValidated'] = True
        except Exception as error:
            errors.append('Pointer: ' + str(error))
    if handle:
        try:
            identity = process_api.times(handle)
            evidence['cleanupProcessIdentity'] = identity
            H['validate_exit'](evidence['processCreationFiletime'], identity)
            evidence['exitIdentity'] = identity
            evidence['exitIdentityValidated'] = True
        except Exception as error:
            errors.append('Process: ' + str(error))
        finally:
            try:
                process_api.kernel.CloseHandle(handle)
            except Exception as error:
                errors.append('Handle: ' + str(error))
    if errors:
        evidence['ok'] = False


def publish_evidence(path, evidence, emit=True):
    # Reporting is secondary to the actual workflow exception. Keep both.
    def write():
        path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    try:
        write()
    except Exception as error:
        evidence['ok'] = False
        evidence.setdefault('receiptErrors', []).append('Write: ' + str(error))
    if emit:
        try:
            print(json.dumps({'receipt': str(path), **evidence}), flush=True)
        except Exception as error:
            evidence['ok'] = False
            evidence.setdefault('receiptErrors', []).append('Print: ' + str(error))
            try:
                write()
            except Exception:
                pass


def run_phase(fixture, scenario, phase, nonce, directory, destination, binary, digest):
    evidence = dict(ok=False, fixture=fixture, scenario=scenario, phase=phase, nonce=nonce,
                    platform=sys.platform, binary=str(binary), binarySha256=digest)
    env = {ENV + 'CHECK': '1', ENV + 'NONCE': nonce, ENV + 'SCENARIO': scenario, ENV + 'PHASE': phase,
           ENV + 'DIRECTORY': str(directory)}
    native = NativeFixture(fixture, destination / f'{phase}.log', binary, env)
    process_api = handle = None
    primary = None
    try:
        require(H['binary_sha'](binary) == digest, 'Binary changed before phase')
        with native:
            evidence['pid'] = native.process.pid
            if sys.platform == 'win32':
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence['processCreationFiletime'] = process_api.times(handle)['creation_filetime']
            try:
                with ExitStack() as stack:
                    expected = ready_tuple(fixture, nonce, native.process.pid, scenario, phase)
                    deadline = time.monotonic() + 25
                    while not (waiting := native.receipts(WAITING)):
                        native.require_running()
                        require(not native.receipts(PREFIX) and time.monotonic() < deadline, 'Readiness failed/timed out')
                        time.sleep(.05)
                    require(len(waiting) == 1, 'Duplicate readiness request')
                    validate_ready(waiting[0], expected)
                    pointer = H['normalize'](native, evidence, stack, process_api, handle)
                    publish_ready(directory, expected)
                    evidence['readiness'] = expected
                    deadline, sampled, count = time.monotonic() + 120, 0., 0
                    while True:
                        native.require_running()
                        edges = native.receipts(STAGE)
                        if pointer:
                            for edge in edges[count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check('running', require_clear=True)
                                sampled = time.monotonic()
                        count = len(edges)
                        receipts = native.receipts(PREFIX)
                        if receipts:
                            require(len(receipts) == 1, 'Duplicate terminal receipt')
                            evidence['native'] = receipts[0]
                            validate_native(expected, directory, receipts[0], edges, evidence.get('expectedPngDimensions'))
                            break
                        require(time.monotonic() < deadline, 'Native workflow timed out')
                        time.sleep(.05)
            except BaseException as error:
                primary = error
                try:
                    path = destination / f'{phase}-failure.png'
                    native.capture(path)
                    evidence['failureScreenshot'] = str(path)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        require(native.process.poll() is not None, 'Owned process did not exit')
        validate_closed_log(native.log, expected, directory, evidence['native'], evidence.get('expectedPngDimensions'))
        evidence['closedLogRevalidated'] = True
        require(H['binary_sha'](binary) == digest, 'Binary changed during phase')
        evidence['database'] = private_snapshot(directory)
        validate_snapshot(evidence['database'], evidence['native'])
        evidence['ok'] = True
    except BaseException as error:
        if primary is None:
            primary = error
        elif error is not primary:
            evidence.setdefault('cleanupErrors', []).append(type(error).__name__ + ': ' + str(error))
        evidence.update(ok=False, error=f'{type(primary).__name__}: {primary}')
    finally:
        audit_cleanup(evidence, native, process_api, handle, platform=sys.platform)
        publish_evidence(destination / f'{phase}.json', evidence)
    if primary is not None:
        raise primary.with_traceback(primary.__traceback__)
    require(evidence['ok'], 'Final phase audit/report failed: ' + json.dumps(evidence.get('cleanupErrors', []) + evidence.get('receiptErrors', [])))
    return evidence


def run(fixture, scenario, binary, captures):
    require(fixture in FIXTURES and scenario in STAGES, 'Unknown fixture/scenario')
    destination = captures / f'{fixture}-{scenario}'
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f'canvas-creation-{nonce}'
    directory.mkdir()
    with (directory / 'owner.json').open('x', encoding='utf-8') as output:
        json.dump(dict(version=1, nonce=nonce, fixture=fixture, scenario=scenario), output)
    digest = H['binary_sha'](binary)
    summary = dict(ok=False, fixture=fixture, scenario=scenario, binarySha256=digest,
                   limitations=['Synthetic input/composition, not trusted OS keyboard/IME.',
                                'Offline exact injected results, not live Slack delivery.',
                                'Sampled geometry/pointer observations, not atomic capture proof.',
                                'Subsequent visual inspection is required.'])
    try:
        first = run_phase(fixture, scenario, 'exercise', nonce, directory, destination, binary, digest)
        second = run_phase(fixture, scenario, 'restart', nonce, directory, destination, binary, digest)
        require(first['database'] == second['database'], 'Restart mutated complete durable dictionaries')
        require(first['native']['inventory'] == second['native']['inventory'], 'Restart inventory differs')
        if sys.platform == 'win32':
            require(second['processCreationFiletime'] > first['exitIdentity']['exit_filetime'], 'Restart process was not created after owned first exit')
        summary.update(ok=True, wholeDatabaseUnchanged=True, inventoryUnchanged=True, separateProcesses=True,
                       exercisePid=first['pid'], restartPid=second['pid'])
    finally:
        publish_evidence(destination / 'summary.json', summary, emit=False)
    require(summary['ok'], 'Summary report failed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_BINARY', DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/agent-ui/canvas-creation-durable')
    parser.add_argument('--fixture', action='append', choices=FIXTURES)
    parser.add_argument('--scenario', action='append', choices=tuple(STAGES))
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        for scenario in args.scenario or STAGES:
            run(fixture, scenario, args.binary.resolve(), args.captures.resolve())
