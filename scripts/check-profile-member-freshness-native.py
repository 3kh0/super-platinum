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

PREFIX = 'profile-member-freshness-check: '
EDGE = 'profile-member-freshness-capture: '
WAITING = 'profile-member-freshness-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = {
    'renamed': ('initial', 'opened-held', 'native-edited', 'read-before-ack', 'renamed', 'reopened'),
    'failure': ('initial', 'opened-held', 'native-edited', 'malformed', 'offline', 'rejected', 'recovered', 'reopened'),
    'account': ('initial', 'opened-held', 'account-replaced'),
    'socket-successor': ('initial','held','native-edited','socket-replaced','retired-replies','successor-read','successor-ack','reopened'),
    'changed-opening': ('initial','healthy','changed-native','opening-held','read-before-ack','continued'),
    'cross-socket-opening': ('initial','changed-native','opening-held','disconnected','successor-held','old-retired','successor-read','continued'),
    'opening-failure-cancel': ('initial','opening-held','failed','retry-held','canceled','shared-finished','reopened','account-replaced'),
}
FIXTURES = ('profile-member-freshness', 'profile-member-freshness-narrow')

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
EDITED = 'Native unsaved description \u4e2d\U0001f642 with '
RENAMED = 'Renamed Cafe\u0301 member'
ACCOUNT = '00000000-0000-4000-8000-000000000081'
FOREIGN = '00000000-0000-4000-8000-000000000082'


def blocks(text):
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=text),
        *[dict(type='user', user_id=MEMBER, fixture_occurrence=i, opaque={'keep': True}) for i in (1, 2)]])])]


def editor_capture(captured, text, label):
    require(isinstance(captured, dict) and captured.get('snapshot', {}).get('blocks') == blocks(text),
            'Native unsaved text or raw tokens changed')
    tokens = captured.get('tokens')
    require(tokens == [dict(value=t, label='@' + label) for t in blocks(text)[0]['elements'][0]['elements'][1:]],
            'Ordered occurrence label custody changed')


def layout(value, viewport, stage, label):
    require(isinstance(value, dict) and value.get('ok') is True and value.get('noOverflow') is True
            and value.get('viewport') == viewport, 'Missing actual layout')
    replaced = stage == 'account-replaced'
    unavailable = False
    editor = stage not in ('initial', 'account-replaced')
    require(value.get('dialog') is (not replaced), 'Wrong dialog owner')
    if not replaced: rectangle(value.get('dialogRect'), viewport)
    controls, notices, tokens = [value.get(k) for k in ('controls', 'notices', 'tokens')]
    require(all(isinstance(v, list) for v in (controls, notices, tokens)), 'Missing native inventory')
    for row in controls + notices + tokens:
        require(row.get('visible') is True, 'Hidden action, feedback or token')
        rectangle(row.get('rect'), viewport)
    wanted = [] if replaced else ['Back', 'Save section'] + (
        ['Use description', 'Cancel description edit'] if editor else
        ['Retry description', 'Cancel description edit'] if unavailable else ['Edit description'])
    require(sorted(c.get('text') for c in controls) == sorted(wanted), 'Wrong action inventory')
    require(all(c.get('disabled') is ((editor or unavailable) and c['text'] == 'Save section') for c in controls),
            'Automatic refresh locked or enabled the wrong action')
    require(value.get('richEditable') is (True if editor else None), 'Editor changed editability')
    if unavailable:
        require(any(n.get('text')=='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.' for n in notices),
                'Missing existing visible Retry guidance')
    require(value.get('globalToast') is None, 'Automatic failure generated an unrelated global toast')
    if editor:
        text = DRAFT if stage == 'opened-held' else EDITED
        editor_capture(value.get('captured'), text, label)
        require(len(tokens) == 2 and all(t.get('text') == '@' + label and t.get('editable') is False for t in tokens),
                'Wrong exact visible member labels')
    else:
        require(value.get('captured') is None and tokens == [], 'Retired editor remained mounted')


def validate(fixture, receipt, captures, platform=sys.platform, png=True):
    require(fixture in FIXTURES and isinstance(receipt, dict) and receipt.get('ok') is True
            and receipt.get('fixture') == fixture and receipt.get('noLiveNetwork') is True,
            'Missing successful closed freshness receipt')
    case = receipt.get('case')
    require(case in STAGES, 'Unknown freshness case')
    viewport = list(expected_viewport(fixture, platform))
    require(receipt.get('viewport') == viewport, 'Wrong actual viewport')
    stages = receipt.get('stages')
    require(isinstance(stages, list) and [s.get('stage') for s in stages] == list(STAGES[case]), 'Wrong stages')
    require(set(receipt.get('captures', {})) == set(STAGES[case]), 'Wrong capture inventory')
    if case in ('cross-socket-opening','opening-failure-cancel'):
        return validate_continuation(fixture,receipt,captures,viewport,png)
    if case in ('socket-successor','changed-opening'):
        return validate_lifetime(fixture,receipt,captures,viewport,png)
    member_count = dict(initial=0, **{'opened-held':1,'native-edited':1,'read-before-ack':1,
        'renamed':1,'reopened':4 if case=='failure' else 1,'malformed':1,'offline':2,
        'rejected':3,'recovered':4,'account-replaced':1})
    original = stages[0]['model']
    for row in stages:
        stage, model = row['stage'], row.get('model', {})
        replaced = stage == 'account-replaced'
        published = stage in ('renamed', 'recovered', 'reopened')
        label = RENAMED if stage == 'reopened' else LABEL
        require(model.get('networkCalls') == 0 and model.get('account') == (FOREIGN if replaced else ACCOUNT),
                'Wrong account or real network work')
        require(model.get('epoch') == (1 if replaced else 0), 'Wrong account epoch')
        reqs = model.get('requests')
        require(isinstance(reqs,list), 'Missing request ledger')
        reads = [r for r in reqs if r.get('kind') == 'member']
        require(len(reads) == member_count[stage], 'Wrong actual conditional read count')
        for req in reqs:
            require(req.get('account') == ACCOUNT and req.get('team') == 'TINIT0001' and req.get('epoch') == 0,
                    'Foreign request ownership')
            if req['kind'] == 'member':
                require(req.get('method') == 'users/info' and req.get('body') == {
                    'updated_ids': {MEMBER:101},'check_interaction':True,'include_profile_only_users':True},
                    'Forced/missing request substituted for conditional read')
            else:
                require(req.get('kind') == 'discovery' and req.get('method') == 'api.getFlannelHttpUrl', 'Unknown request')
        frames = model.get('subscriptions')
        operation_count = max(0, 2*member_count[stage]-1) if case=='failure' else member_count[stage]
        require(isinstance(frames,list) and len(frames) == operation_count, 'Wrong socket operation count')
        ids=[]
        for position, frame in enumerate(frames):
            unsubscribe = case=='failure' and position%2==1
            request=frame.get('request',{})
            require(frame.get('account') == ACCOUNT and request.get('type')=='subscription_update'
                    and request.get('subtype')==('user_change_unsubscribe_request' if unsubscribe else 'user_change_subscribe_request') and request.get('ids')==[MEMBER]
                    and type(request.get('id')) is int, 'Fabricated or wrong member subscription')
            if unsubscribe:
                require(frame.get('reply',{}).get('ok') is True and frame.get('accepted') is True,
                        'Unsubscribe did not settle its original waiter')
            ids.append(request['id'])
            if frame.get('reply') is not None:
                require(frame['reply'].get('reply_to')==request['id'], 'ACK answered another request')
        require(len(ids)==len(set(ids)), 'Socket ID reused')
        profile=model.get('profile',{})
        if replaced:
            require(profile.get('enabled') is False and model.get('native') is None and model.get('sourcePresent') is False,
                    'Retired account adopted old member')
            require(row.get('proof',{}).get('replacement',{}).get('actualInstallLoaded') is True,
                    'Account retirement bypassed real install_loaded')
            before=row['proof']['replacement'].get('before',{}).get('profile',{})
            require(before.get('enabled') is True and before.get('attempts')==[] and before.get('reads')==0
                    and 'attempts' not in profile, 'Missing original empty write ledger at retirement')
        else:
            require(profile.get('enabled') is True and profile.get('attempts')==[] and profile.get('reads')==0
                    and profile.get('dirty') is True, 'Unexpected Profile mutation or write')
            form_text = EDITED if stage in ('offline','rejected','recovered','reopened') else DRAFT
            require(profile.get('blocks')==blocks(form_text), 'Original Rust form changed before explicit capture')
            source,native=model.get('source',{}),model.get('native',{})
            expected_label=RENAMED if published else LABEL
            require(model.get('sourcePresent') is True and source.get('id')==MEMBER
                    and source.get('updated')==(102 if published else 101)
                    and source.get('profile',{}).get('display_name')==expected_label
                    and native.get('profile',{}).get('display_name')==expected_label,
                    'Conditional source/native publication mismatch')
            if not published: require(source==original['source'], 'Failure or held ACK changed source')
            automatic=model.get('automatic',{})
            require(all(type(automatic.get(k)) is int for k in ('started','finished','http_finished','acknowledgments','failures')),
                    'Missing actual worker counters')
            expected_failures = {'malformed':1,'offline':2,'rejected':3,'recovered':3,'reopened':3 if case=='failure' else 0}.get(stage,0)
            require(automatic['failures']==expected_failures, 'Wrong aggregate failure count')
            if stage=='rejected':
                require(frames[-1]['reply'].get('ok') is False, 'Rejected ACK was replaced with success')
            if stage=='read-before-ack':
                require(automatic['http_finished']==1 and automatic['finished']==0 and automatic['acknowledgments']==0
                        and frames[0].get('reply') is None, 'Queue acceptance mistaken for aggregate completion')
            if stage in ('renamed','malformed','offline','rejected','recovered','reopened'):
                require(automatic['finished']==operation_count and automatic.get('worker') is False,
                        'Captured before aggregate settlement')
            if stage in ('renamed','malformed','offline','rejected','recovered'):
                proof=row.get('proof',{}).get('native',{})
                require(proof.get('ok') is True and proof.get('sameNode') is True and proof.get('sameSelection') is True
                        and proof.get('noToast') is True, 'Original native editor custody lost')
        layout(row.get('geometry'),viewport,stage,label)
        path=Path(receipt['captures'][stage])
        require(path.resolve()==(captures/f'{fixture}-{stage}.png').resolve(), 'Capture escaped expected path')
        dimensions=row['geometry'].get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(v)is int and v>0 for v in dimensions),
                'Missing native PNG dimensions')
        if png: validate_png(path,dimensions)
    require(receipt.get('final')==stages[-1]['model'], 'Terminal state differs from final captured state')



def validate_lifetime(fixture, receipt, captures, viewport, png):
    case=receipt['case']; stages=receipt['stages']; socket_case=case=='socket-successor'
    original=stages[0]['model']; first_generation=original.get('socketGeneration')
    require(type(first_generation) is int, 'Missing accepted original socket')
    counts=({'initial':0,'held':1,'native-edited':1,'socket-replaced':2,'retired-replies':2,'successor-read':2,'successor-ack':2,'reopened':2}
            if socket_case else {'initial':0,'healthy':1,'changed-native':1,'opening-held':2,'read-before-ack':2,'continued':2})
    waiting_opening=None
    for row in stages:
        stage=row['stage']; model=row.get('model',{}); proof=row.get('proof',{})
        published=stage in ('successor-ack','reopened','continued')
        native_changed=not socket_case and stage in ('changed-native','opening-held','read-before-ack','continued')
        replacement=socket_case and stage in ('socket-replaced','retired-replies','successor-read','successor-ack','reopened')
        require(model.get('networkCalls')==0 and model.get('account')==ACCOUNT and model.get('epoch')==0,
                'Lifetime owner or network boundary changed')
        require(model.get('socketGeneration')==first_generation+int(replacement),'Wrong accepted socket generation')
        profile=model.get('profile',{})
        require(profile.get('enabled') is True and profile.get('dirty') is True and profile.get('attempts')==[] and profile.get('reads')==0,
                'Lifetime case changed Profile write state')
        formtext=EDITED if (not socket_case and stage!='initial') or stage=='reopened' else DRAFT
        require(profile.get('blocks')==blocks(formtext),'Original Rust blocks changed outside actual Use')
        raw=model.get('source',{}); native=model.get('native',{})
        require(model.get('sourcePresent') is True and raw.get('id')==MEMBER
                and raw.get('updated')==(102 if published else 101)
                and raw.get('profile',{}).get('display_name')==(RENAMED if published else LABEL)
                and native.get('profile',{}).get('display_name')==(RENAMED if published or native_changed else LABEL),
                'Source and typed member custody mismatch')
        if not published: require(raw==original['source'],'Event or retired response adopted raw source')
        requests=model.get('requests',[]); reads=[r for r in requests if r.get('kind')=='member']
        require(len(reads)==counts[stage],'Wrong lifetime conditional read count')
        for request in requests:
            require(request.get('account')==ACCOUNT and request.get('team')=='TINIT0001' and request.get('epoch')==0,
                    'Foreign lifetime request')
            if request.get('kind')=='member':
                require(request.get('method')=='users/info' and request.get('body')=={'updated_ids':{MEMBER:101},'check_interaction':True,'include_profile_only_users':True},
                        'Lifetime request bypassed conditional source')
            else: require(request.get('kind')=='discovery' and request.get('method')=='api.getFlannelHttpUrl','Unknown lifetime endpoint')
        frames=model.get('subscriptions',[])
        operations=counts[stage] if socket_case else max(0,counts[stage]*2-1)
        require(len(frames)==operations,'Wrong lifetime socket operations')
        identities=[]
        for i,frame in enumerate(frames):
            req=frame.get('request',{}); unsubscribe=not socket_case and i==1
            expected_socket=i if socket_case else 0
            require(frame.get('socket')==expected_socket and frame.get('generation')==first_generation+expected_socket
                    and frame.get('account')==ACCOUNT and req.get('type')=='subscription_update'
                    and req.get('subtype')==('user_change_unsubscribe_request' if unsubscribe else 'user_change_subscribe_request')
                    and req.get('ids')==[MEMBER] and type(req.get('id')) is int,'Wrong original socket operation')
            identities.append((frame['socket'],req['id']))
            if frame.get('reply') is not None:
                require(frame['reply'].get('reply_to')==req['id'] and frame['reply'].get('ok') is True,'ACK identity/outcome changed')
            if unsubscribe: require(frame.get('accepted') is True and frame.get('reply') is not None,'Unsubscribe waiter not settled')
        require(len(identities)==len(set(identities)),'ID reused on same socket')
        auto=model.get('automatic',{})
        require(all(type(auto.get(k)) is int for k in ('started','finished','http_finished','acknowledgments','failures')),'Missing actual lifetime counters')
        require(auto['failures']==0,'Unexpected failure in successful lifetime path')
        if published:
            require(auto.get('worker') is False and auto['finished']==operations and auto['acknowledgments']==(1 if socket_case else 3),
                    'Successor/continuation captured before aggregate success')
        if socket_case and replacement:
            require(frames[1].get('reply') is None if not published else frames[1].get('accepted') is True,'Successor ACK state changed')
        if stage=='socket-replaced':
            require(proof.get('before')==stages[2]['model'] and proof.get('connection')=={'before':first_generation,'after':first_generation+1,'socket':1},
                    'Replacement bypassed original same-account source')
        if stage=='retired-replies':
            require(proof.get('oldHttpDelivered') is False and proof.get('oldAck',{}).get('routed') is False
                    and frames[0].get('accepted') is False,'Retired waiter accepted an old reply')
        if socket_case and stage in ('socket-replaced','retired-replies','successor-read','successor-ack'):
            custody=proof.get('native',{})
            require(all(custody.get(k) is True for k in ('ok','sameNode','sameSelection','noToast')),'Mounted native custody lost at socket boundary')
        if stage=='successor-read':
            require(auto['http_finished']>=1 and auto['acknowledgments']==0 and auto['finished']==1,'HTTP mistaken for successor ACK')
        opening=model.get('opening',{})
        if not socket_case and stage in ('opening-held','read-before-ack'):
            require(opening.get('mounted') is False and opening.get('reading') is True and proof.get('opening')==opening,'Missing actual waiting opening')
            if waiting_opening is None: waiting_opening=opening
            else: require(waiting_opening==opening,'Original missing opening retired before response')
        if stage=='continued':
            require(waiting_opening is not None and proof.get('opening')==waiting_opening and proof.get('afterOpening')==opening
                    and proof.get('noExtraOpen') is True and opening.get('mounted') is True and opening.get('reading') is False
                    and opening.get('visit')==waiting_opening.get('visit') and opening.get('serial')==waiting_opening.get('serial')
                    and type(opening.get('identity')) is int and opening['identity']>waiting_opening['identity'],
                    'Continuation did not retain original guarded draft')
        if stage=='changed-native':
            require(proof.get('before',{}).get('source')==proof.get('after',{}).get('source')==original['source']
                    and proof.get('acceptedGeneration')==first_generation,'Changed-native proof fabricated raw authority')
        geometry=row.get('geometry',{})
        if socket_case:
            mapped='initial' if stage=='initial' else 'opened-held' if stage=='held' else 'reopened' if stage=='reopened' else 'native-edited'
            layout(geometry,viewport,mapped,RENAMED if stage=='reopened' else LABEL)
        elif stage in ('opening-held','read-before-ack'):
            waiting_layout(geometry,viewport)
        else:
            mapped='reopened' if stage=='continued' else 'initial'
            layout(geometry,viewport,mapped,RENAMED)
        path=Path(receipt['captures'][stage])
        require(path.resolve()==(captures/f'{fixture}-{stage}.png').resolve(),'Lifetime capture escaped directory')
        dimensions=geometry.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(v)is int and v>0 for v in dimensions),'Missing lifetime PNG dimensions')
        if png: validate_png(path,dimensions)
    require(receipt.get('final')==stages[-1]['model'],'Lifetime final state changed')


def waiting_layout(value,viewport,loading=True):
    require(value.get('ok') is True and value.get('noOverflow') is True and value.get('viewport')==viewport
            and value.get('dialog') is True,'Missing waiting layout')
    rectangle(value.get('dialogRect'),viewport)
    controls=value.get('controls',[]);notices=value.get('notices',[])
    require(sorted(c.get('text') for c in controls)==sorted(['Back','Save section','Retry description','Cancel description edit']), 'Wrong waiting actions')
    require(all(c.get('disabled') is (c['text']=='Save section' or loading and c['text']=='Retry description') for c in controls),'Waiting action eligibility changed')
    text='Loading mention names. Your description is unchanged.' if loading else 'Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'
    require(any(n.get('text')==text for n in notices),'Missing actual loading/retry notice')
    for row in controls+notices:
        require(row.get('visible') is True,'Waiting action or notice hidden');rectangle(row.get('rect'),viewport)
    require(value.get('tokens')==[] and value.get('captured') is None and value.get('richEditable') is None
            and value.get('globalToast') is None,'Waiting opening exposed stale editor or toast')


def validate_continuation(fixture,receipt,captures,viewport,png):
    cross=receipt['case']=='cross-socket-opening';rows=receipt['stages'];original=rows[0]['model'];generation=original.get('socketGeneration')
    require(type(generation) is int,'Missing original accepted connection')
    counts=({'initial':0,'changed-native':1,'opening-held':2,'disconnected':2,'successor-held':3,'old-retired':3,'successor-read':3,'continued':3} if cross else
            {'initial':0,'opening-held':2,'failed':2,'retry-held':3,'canceled':3,'shared-finished':3,'reopened':3,'account-replaced':4})
    original_opening=None;canceled=None
    for row in rows:
        stage=row['stage'];m=row['model'];proof=row.get('proof',{});count=counts[stage]
        replaced=stage=='account-replaced';fresh=stage in ('continued','shared-finished','reopened')
        successor=cross and stage in ('successor-held','old-retired','successor-read','continued')
        waiting=stage in ('opening-held','disconnected','successor-held','old-retired','successor-read','retry-held')
        require(m.get('networkCalls')==0 and m.get('account')==(FOREIGN if replaced else ACCOUNT) and m.get('epoch')==int(replaced),'Consumer owner/network changed')
        profile=m.get('profile',{});opening=m.get('opening',{})
        if replaced:
            before=proof.get('replacement',{}).get('before',{});prior=before.get('profile',{})
            require(proof.get('replacement',{}).get('actualInstallLoaded') is True and prior.get('enabled') is True
                    and prior.get('attempts')==[] and prior.get('reads')==0 and prior.get('blocks')==blocks(EDITED)
                    and before.get('opening',{}).get('reading') is True and before.get('opening',{}).get('lease') is True,
                    'Account replaced without original pending consumer custody')
            require(profile.get('enabled') is False and 'attempts' not in profile and opening is None
                    and m.get('native') is None and m.get('sourcePresent') is False,'Old consumer adopted into replacement')
        else:
            require(m.get('socketGeneration')==generation+int(successor),'Wrong consumer socket generation')
            require(profile.get('enabled') is True and profile.get('dirty') is True and profile.get('attempts')==[] and profile.get('reads')==0
                    and profile.get('blocks')==blocks(DRAFT if stage=='initial' else EDITED),'Consumer draft/write custody changed')
            raw=m.get('source',{});native=m.get('native',{})
            require(m.get('sourcePresent') is True and raw.get('updated')==(102 if fresh else 101)
                    and raw.get('profile',{}).get('display_name')==(RENAMED if fresh else LABEL)
                    and native.get('profile',{}).get('display_name')==(LABEL if stage=='initial' else RENAMED),'Raw/native changed-opening boundary changed')
            if not fresh:require(raw==original['source'],'Unaccepted response adopted raw source')
            require(opening.get('reading') is waiting and opening.get('lease') is waiting
                    and opening.get('mounted') is (stage in ('continued','reopened')),'Wrong original consumer lifecycle')
        requests=m.get('requests',[]);reads=[r for r in requests if r.get('kind')=='member']
        require(len(reads)==count,'Wrong conditional consumer read count')
        for req in requests:
            require(req.get('account')==ACCOUNT and req.get('team')=='TINIT0001' and req.get('epoch')==0,'Foreign consumer request')
            if req.get('kind')=='member':
                version=102 if replaced and req is reads[-1] else 101
                require(req.get('method')=='users/info' and req.get('body')=={'updated_ids':{MEMBER:version},'check_interaction':True,'include_profile_only_users':True},'Consumer substituted forced read')
            else:require(req.get('kind')=='discovery' and req.get('method')=='api.getFlannelHttpUrl','Unknown consumer request')
        operations=(0 if count==0 else 1 if count==1 else 4 if successor else 3) if cross else max(0,2*count-1)
        frames=m.get('subscriptions',[]);require(len(frames)==operations,'Wrong consumer socket operations')
        identities=[]
        for i,f in enumerate(frames):
            sock=1 if cross and i==3 else 0;unsubscribe=i==1 if cross else i%2==1;request=f.get('request',{})
            require(f.get('socket')==sock and f.get('generation')==generation+sock and f.get('account')==ACCOUNT
                    and request.get('type')=='subscription_update' and request.get('subtype')==('user_change_unsubscribe_request' if unsubscribe else 'user_change_subscribe_request')
                    and request.get('ids')==[MEMBER] and type(request.get('id')) is int,'Consumer socket witness changed')
            identities.append((sock,request['id']))
            if f.get('reply') is not None: require(f['reply'].get('reply_to')==request['id'] and f['reply'].get('ok') is True,'Consumer ACK changed identity/outcome')
            if unsubscribe:require(f.get('reply') is not None and f.get('accepted') is True,'Consumer unsubscribe not acknowledged')
        require(len(identities)==len(set(identities)),'Same socket reused ACK identity')
        if not replaced:
            auto=m.get('automatic',{})
            require(all(type(auto.get(k)) is int for k in ('started','finished','http_finished','acknowledgments','failures')),'Missing consumer runtime counters')
            require(auto['failures']==(0 if cross or stage in ('initial','opening-held') else 1),'Wrong aggregate consumer failure')
            if stage in ('opening-held','retry-held'):
                require(proof.get('opening')==opening,'Missing original opening receipt')
            if stage=='opening-held':original_opening=opening
            if cross and waiting:require(opening==original_opening,'Socket change retired original consumer')
            if stage=='disconnected':
                require(auto.get('worker') is False and auto.get('pending')==[MEMBER] and auto['finished']==3,
                        'Disconnected workerNone mistaken for completion')
                require(proof.get('acceptedGeneration')==generation,'Disconnect changed generation')
            if stage=='successor-held':require(proof.get('connection')=={'before':generation,'after':generation+1,'socket':1},'Successor bypassed accepted connection')
            if stage=='old-retired':require(proof.get('oldHttpDelivered') is False and proof.get('oldAck',{}).get('routed') is False and frames[2].get('accepted') is False,'Retired original consumer IO was accepted')
            if stage=='successor-read':require(auto['http_finished']>proof.get('beforeHttp',-1) and auto['acknowledgments']==2 and frames[-1].get('reply') is None,'Consumer resumed on HTTP before ACK')
            if stage=='continued':
                require(proof.get('opening')==original_opening and proof.get('afterOpening')==opening and proof.get('noExtraOpen') is True
                        and opening.get('visit')==original_opening['visit'] and opening.get('serial')==original_opening['serial']
                        and opening.get('identity',0)>original_opening['identity'],'Successor continuation lost original draft')
                require(auto.get('worker') is False and auto['finished']==4 and auto['acknowledgments']==3,'Successor aggregate incomplete')
            if stage=='failed':require(auto.get('worker') is False and auto['finished']==3 and proof.get('quietFailure') is True,'Failed consumer still pending')
            if stage=='retry-held':require(proof.get('ordinaryOpen') is True,'Automatic attempt relabeled forced Retry')
            if stage=='canceled':
                canceled=opening
                require(proof.get('afterOpening')==opening and proof.get('beforeOpening',{}).get('identity')!=opening.get('identity'),'Cancellation did not revoke original opening')
            if stage=='shared-finished':
                require(opening==canceled and proof.get('canceledOpening')==canceled and proof.get('noReopen') is True
                        and auto.get('worker') is False and auto['finished']==5,'Shared completion reopened canceled draft')
        geometry=row['geometry']
        if waiting:waiting_layout(geometry,viewport)
        elif stage=='failed':waiting_layout(geometry,viewport,loading=False)
        else:layout(geometry,viewport,'account-replaced' if replaced else 'reopened' if stage in ('continued','reopened') else 'initial',RENAMED)
        path=Path(receipt['captures'][stage]);require(path.resolve()==(captures/f'{fixture}-{stage}.png').resolve(),'Consumer capture escaped expected directory')
        dims=geometry.get('pngDimensions');require(isinstance(dims,list) and len(dims)==2 and all(type(v)is int and v>0 for v in dims),'Missing consumer PNG dimensions')
        if png:validate_png(path,dims)
    require(receipt.get('final')==rows[-1]['model'],'Consumer final differs from last stage')

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
    directory = captures / f'profile-member-freshness-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    case=case, readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG: '1', 'SUPER_PLATINUM_PROFILE_MEMBER_RECOVERY_CHECK': '1', 'SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_CASE': case, 'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                            'SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_NONCE': nonce,
                            'SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_READY_DIR': str(directory)})
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-member-freshness-native')
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
