#!/usr/bin/env python3
"""Offline Profile conditional group recovery acceptance, normal and narrow.

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
from profile_group_absent_member import CASES as ABSENT_MEMBER_CASES
from profile_group_known_freshness import CASES as KNOWN_FRESHNESS_CASES

PREFIX = 'profile-group-recovery-check: '
EDGE = 'profile-group-recovery-capture: '
WAITING = 'profile-group-recovery-waiting: '
FLAG = 'SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_CHECK'
H = runpy.run_path(str(Path(__file__).with_name('check-multi-external-select-native.py')))
STAGES = {
    'legacy': ('initial','ordinary-open','held','malformed','offline','success','saved','reopened'),
    'absent': ('initial','held','partial','fallback-held','success','saved','reopened'),
    'retirement': ('initial','held','cancelled','source-refused','account-replaced'),
    'automatic': ('initial','held','success','edited','saved','reopened'),
    'pending': ('initial','held','pending','success','edited','saved','reopened'),
    'automatic-offline': ('initial','held','offline','success','edited','saved','reopened'),
    'automatic-malformed': ('initial','held','malformed','success','edited','saved','reopened'),
    'ready': ('initial','success','edited','saved','reopened'),
    'mixed-member': ('initial','held','success','edited','saved','reopened'),
    'mixed-file-group': ('initial','file-held','group-held','offline','retry-file-held','success','edited','saved','reopened'),
    'rate-limit': ('initial','held','direct-held','limited','cancelled','reopened-limited',
                   'expired','success','edited','saved','reopened'),
}
AUTOMATIC = ('automatic', 'pending', 'automatic-offline', 'automatic-malformed', 'ready', 'mixed-member', 'rate-limit', 'mixed-file-group')
STAGES.update(ABSENT_MEMBER_CASES)
STAGES.update(KNOWN_FRESHNESS_CASES)
FIXTURES = ('profile-group-recovery', 'profile-group-recovery-narrow')

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



# Scenario/model/geometry validation lives in this wrapper and is exercised without native processes.
GROUP = 'SMISSING01'
OTHER = 'SMISSING02'
LABEL = '@@ Cafe\u0301 crew  '
OTHER_LABEL = '@Review crew'
DRAFT = 'Keep my unsaved description with '
EDITED = 'Native edited \u4e2d\U0001f98a & description with '
MEMBER = 'UMIXED001'
MEMBER_LABEL = '@Known mixed member'
LOADING = 'Loading mention names. Your description is unchanged.'
MIXED_LOADING = 'Loading reference details. Your description is unchanged.'
MIXED_GUIDANCE = ('Reference details are unavailable or changed while opening. Your description is unchanged. '
                  'Retry to load document and mention details.')
GUIDANCE = ('Mention names are unavailable or changed while opening. Your description is unchanged. '
            'Retry to load member, channel, and group details.')
LIMITED = 'Slack asked us to wait before loading group names. Your description is unchanged.'
RETRY_READY = 'You can retry loading group names now. Your description is unchanged.'
ACCOUNT = '00000000-0000-4000-8000-000000000081'
FOREIGN = '00000000-0000-4000-8000-000000000082'


def ids(case):
    return [GROUP] if case == 'legacy' else [GROUP, OTHER]


def blocks(case, edited=False):
    if case in ABSENT_MEMBER_CASES:
        from profile_group_absent_member import original_blocks
        return original_blocks(EDITED if edited else DRAFT)
    tokens = [GROUP, GROUP] + ([] if case == 'legacy' else [OTHER])
    value = [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text=EDITED if edited else DRAFT), *[dict(type='usergroup', usergroup_id=g,
        fixture_occurrence=i + 1, tab_id='opaque-tab', opaque={'keep': True}) for i,g in enumerate(tokens)]])])]
    if case in ('mixed-member','mixed-file-group'):
        value[0]['elements'][0]['elements'].append(dict(type='user',user_id=MEMBER,fixture_occurrence=4,
            tab_id='opaque-member-tab',opaque={'keep':True}))
    if case=='mixed-file-group':
        from profile_group_mixed_file import file_tokens
        value[0]['elements'][0]['elements'].extend(file_tokens())
    return value


def member_raw():
    return dict(id=MEMBER,team_id='TINIT0001',name='username-must-not-win',real_name='Real fallback',
                profile=dict(display_name='Known mixed member',real_name='Profile fallback'),updated=101)


def member_source():
    # The actual BootRead drain runs member_hydration/profile.rs::effective/derive.
    # Its canonical source is distinct from the original boot wire and typed User.
    value=member_raw()
    value['profile'].update(first_name='',last_name='',display_name_normalized='Known mixed member',
                            real_name_normalized='Profile fallback')
    value.update(_name_lc='username-must-not-win',_first_name_lc='',_last_name_lc='',
                 _real_name_lc='profile fallback',_display_name_lc='known mixed member',
                 _real_name_normalized_lc='profile fallback',
                 _display_name_normalized_lc='known mixed member',_email_normalized_lc='')
    return value


def raw(group):
    return dict(id=group, team_id='TINIT0001', handle=LABEL if group == GROUP else 'Review crew',
                name='Fixture crew', date_delete=0, is_visible=True, fixture_metadata={'keep': True})


def layout(value, viewport, stage, case):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('noOverflow') is True
            and value.get('viewport') == viewport, 'Missing successful measured layout')
    replaced = stage == 'account-replaced'
    editor = stage in ('success','edited','reopened')
    waiting = stage in ('ordinary-open','held','malformed','offline','pending','partial','fallback-held','source-refused',
                        'direct-held','limited','reopened-limited','expired','file-held','group-held','retry-file-held')
    loading = stage in ('held','fallback-held','direct-held','file-held','group-held','retry-file-held')
    require(value.get('dialog') is (not replaced), 'Wrong dialog owner')
    if not replaced: rectangle(value.get('dialogRect'),viewport)
    controls,notices,tokens = [value.get(k) for k in ('controls','notices','tokens')]
    require(all(isinstance(v,list) for v in (controls,notices,tokens)), 'Missing native inventory')
    for row in controls + notices + tokens:
        require(row.get('visible') is True, 'Hidden action, feedback or token')
        rectangle(row.get('rect'),viewport)
    notice = LOADING if loading else LIMITED if stage in ('limited','reopened-limited') else RETRY_READY if stage=='expired' else GUIDANCE
    if case=='mixed-file-group': notice = MIXED_LOADING if loading else MIXED_GUIDANCE
    feedback = any(n.get('text') == notice and n.get('visible') is True for n in notices)
    require(value.get('feedbackRequired') is waiting and value.get('feedbackVisible') is feedback
            and (not waiting or feedback), 'Missing exact readable Retry guidance')
    wanted = [] if replaced else ['Back','Save section'] + (
        ['Use description','Cancel description edit'] if editor else
        ['Retry description','Cancel description edit'] if waiting else ['Edit description'])
    require(sorted(c.get('text') for c in controls) == sorted(wanted), 'Wrong action inventory')
    require(all(c.get('disabled') is ((c['text']=='Save section' and (editor or waiting or stage in ('initial','cancelled','saved')))
                or (c['text']=='Retry description' and (loading or stage in ('limited','reopened-limited')))) for c in controls), 'Wrong action disabled state')
    require(value.get('richEditable') is (True if editor else None), 'Wrong editor editability')
    if editor:
        labels = [LABEL,LABEL] + ([] if case=='legacy' else [OTHER_LABEL])
        if case in ('mixed-member','mixed-file-group'): labels.append(MEMBER_LABEL)
        if case=='mixed-file-group': labels.extend(['List & Plan']*2)
        require([t.get('text') for t in tokens] == labels and all(t.get('editable') is False for t in tokens),
                'Wrong exact group labels or token count')
        for token in tokens:
            require(isinstance(token.get('fragments'),list) and token['fragments'], 'Missing inline fragments')
            for fragment in token['fragments']: rectangle(fragment,viewport)
        if case=='mixed-file-group':
            require([(t.get('fontWeight'),t.get('fontStyle')) for t in tokens[-2:]]
                    == [('700','normal'),('400','italic')], 'Mixed file computed styles changed')
        captured=value.get('captured')
        require(isinstance(captured,dict) and captured.get('snapshot',{}).get('blocks')
                == blocks(case, case in AUTOMATIC and stage in ('edited','reopened')),
                'Original native draft or opaque duplicates changed')
        snapshot=captured['snapshot']
        require(snapshot.get('composing') is False and type(snapshot.get('revision')) is int
                and isinstance(snapshot.get('generation'),str) and snapshot['generation'], 'Invalid renderer capture')
        require(captured.get('tokens') == [dict(value=t,label=l) for t,l in zip(blocks(case)[0]['elements'][0]['elements'][1:],labels)],
                'Captured group token witnesses changed')
    else:
        require(not tokens and value.get('captured') is None, 'Editor mounted before complete labels or after retirement')
    if stage=='saved':
        require(any(n.get('text')=='Profile fields saved.' for n in notices), 'Missing visible save/readback notice')
    require('globalToast' in value, 'Missing independent toast measurement')
    toast=value['globalToast']
    if toast is not None:
        require(isinstance(toast,dict) and type(toast.get('visible')) is bool and type(toast.get('positioned')) is bool,
                'Invalid toast diagnostic')
        if toast['visible']:
            require(toast['positioned'] and toast.get('visibility')=='visible', 'Unsafe visible toast')
            rectangle(toast.get('rect'),viewport)
            require(toast.get('dismiss',{}).get('visible') is True, 'Visible toast cannot be dismissed')
            rectangle(toast['dismiss'].get('rect'),viewport)


def request_reads(rows):
    require(isinstance(rows,list), 'Missing request ledger')
    reads=[]
    for i,row in enumerate(rows):
        require(row.get('serial')==i+1 and type(row.get('released')) is bool, 'Wrong serial or release state')
        require(row.get('account')==ACCOUNT and row.get('team')=='TINIT0001' and row.get('epoch')==0,
                'Request escaped original owner')
        kind=row.get('kind');body=row.get('body')
        require(isinstance(body,dict), 'Missing exact request body')
        if kind=='discovery':
            require(row.get('method')=='api.getFlannelHttpUrl' and body=={
                'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'}, 'Wrong discovery body')
        elif kind=='directory':
            require(row.get('method')=='usergroups/info' and set(body)=={'ids'} and isinstance(body['ids'],list)
                    and body['ids'] and len(body['ids'])==len(set(body['ids'])) and all(g in (GROUP,OTHER) for g in body['ids']),
                    'Wrong group directory IDs, dedup or flags')
            reads.append(('directory',body['ids']))
        elif kind=='direct':
            require(row.get('method')=='subteams.info' and set(body)=={
                'include_users','include_count','include_disabled','usergroups','_x_reason'}
                and body['include_users']=='false' and body['include_count']=='true' and body['include_disabled']=='true'
                and body['_x_reason']=='fallback:fetchRawUserGroupsById', 'Wrong direct fallback flags')
            groups=body['usergroups'].split(',')
            require(groups and len(groups)==len(set(groups)) and all(g in (GROUP,OTHER) for g in groups), 'Wrong direct fallback subset')
            reads.append(('direct',groups))
        else: raise RuntimeError('Unknown request kind')
    return reads


def expected_reads(case):
    if case=='legacy': return [('directory',[GROUP]),('direct',[GROUP]),('directory',[GROUP]),('direct',[GROUP]),('directory',[GROUP])]
    if case=='absent': return [('directory',[GROUP,OTHER]),('directory',[OTHER]),('direct',[OTHER])]
    if case=='ready': return []
    if case in ('automatic','mixed-member'): return [('directory',[GROUP,OTHER])]
    if case=='pending': return [('directory',[GROUP,OTHER])]*2
    if case in ('automatic-offline','automatic-malformed','rate-limit','mixed-file-group'):
        return [('directory',[GROUP,OTHER]),('direct',[GROUP,OTHER]),('directory',[GROUP,OTHER])]
    return [('directory',[GROUP,OTHER]),('directory',[GROUP,OTHER]),('directory',[OTHER])]


def stage_counts(case):
    counts={'initial':(0,0),'ordinary-open':(0,0),'held':(1,0),'malformed':(2,1),'offline':(4,2),
        'partial':(1,1),'fallback-held':(3,1),'cancelled':(1,1),'source-refused':(2,2),'account-replaced':(3,3),
        'success':(5,3) if case=='legacy' else (3,2),'saved':(5,3) if case=='legacy' else (3,2),
        'reopened':(5,3) if case=='legacy' else (3,2)}
    if case in AUTOMATIC:
        final={'automatic':(1,1),'pending':(2,2),'automatic-offline':(3,2),
               'automatic-malformed':(3,2),'ready':(0,0),'mixed-member':(1,1),'rate-limit':(3,2),'mixed-file-group':(3,2)}[case]
        counts.update(pending=(1,1),offline=(2,1),malformed=(2,1))
        counts.update({stage:final for stage in ('success','edited','saved','reopened')})
        if case=='rate-limit':
            counts.update({'direct-held':(2,0), **{s:(2,1) for s in ('limited','cancelled','reopened-limited','expired')}})
    if case=='mixed-file-group': counts.update({'file-held':(0,0),'group-held':(1,0),'retry-file-held':(2,1)})
    return counts


def retirement_deliveries(rows):
    reads=[r for r in rows if r['kind']!='discovery']
    for i,row in enumerate(reads):
        if row['released']:
            # Original Cancel and install_loaded drop their owned receivers;
            # the intervening source-change read really delivers before rejection.
            require(row.get('receiverRetired') is (i in (0,2))
                    and row.get('delivered') is (i==1),
                    'Retired/source-change callback delivery ownership differs')


def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(isinstance(receipt,dict) and receipt.get('ok') is True,
            f"Native assertion failed: {receipt.get('error','missing successful receipt') if isinstance(receipt,dict) else receipt}")
    require(fixture in FIXTURES and receipt.get('fixture')==fixture, 'Wrong native fixture')
    case=receipt.get('case');require(case in STAGES,'Unknown case')
    viewport=expected_viewport(fixture,platform)
    require(receipt.get('viewport')==viewport and receipt.get('captureChecksPassed') is True,'Wrong viewport or incomplete captures')
    stages=receipt.get('stages')
    require(isinstance(stages,list) and [s.get('stage') for s in stages]==list(STAGES[case]),'Wrong stage order')
    require(isinstance(receipt.get('captures'),dict) and set(receipt['captures'])==set(STAGES[case]),'Incomplete captures')
    if case in KNOWN_FRESHNESS_CASES:
        from profile_group_known_freshness import validate_known
        return validate_known(receipt, captures, viewport, require, rectangle, validate_png, png)
    if case in ABSENT_MEMBER_CASES:
        from profile_group_absent_member import validate_absent
        return validate_absent(receipt, captures, viewport, require, rectangle, validate_png, png)
    require(request_reads(receipt.get('requests'))==expected_reads(case),'Wrong actual fallback/read sequence')
    require(all(r['released'] for r in receipt['requests']),'Unreleased final callback')
    if case=='retirement': retirement_deliveries(receipt['requests'])
    counts=stage_counts(case)
    original=stages[0]['model']
    if case=='rate-limit':
        from profile_group_rate_limit import validate_rate
        validate_rate(receipt, require)
    if case=='mixed-file-group':
        from profile_group_mixed_file import validate_mixed
        validate_mixed(receipt, require)
    for row in stages:
        stage=row['stage'];model=row.get('model');geometry=row.get('geometry');proof=row.get('proof',{})
        require(isinstance(model,dict),'Missing model')
        replaced=stage=='account-replaced';held=stage in ('held','fallback-held','direct-held','group-held');saved=stage in ('saved','reopened')
        wanted_proofs={
            'initial':('originalDraft',),'ordinary-open':('noForcedRead',),'held':('explicitRetryRead','duplicateIdsCoalesced'),
            'malformed':('sourceRetained','draftRetained','actualFallback'),'offline':('sourceRetained','draftRetained','actualFallback'),
            'partial':('actualPartialRetained','noPlaceholder','draftRetained'),'fallback-held':('onlyUnresolvedRequested','failedIdsFallback'),
            'success':('actualReadAndAdoption','originalDraft','membershipPreserved'),'saved':('actualProfileSaveReadback','rawDuplicatesPreserved'),
            'reopened':('noRedundantRead','originalDraft'),'cancelled':('lateResultDidNotOpen','draftRetained'),
            'source-refused':('actualGroupEvent','oldSourceResultRefused','draftRetained'),'account-replaced':('actualInstallLoaded',)}
        if stage=='held' and case!='legacy':
            wanted_proofs['held']=('automaticRead','duplicateIdsCoalesced')
            require(proof.get('explicitRetryRead') in (None,False),'Initial absent read required explicit Retry')
        if case in AUTOMATIC:
            wanted_proofs['edited']=('actualNativeEdit',)
            wanted_proofs['success']=(('readyWithoutRead',) if case=='ready' else ('actualReadAndAdoption',))+('originalDraft',)
            if stage=='held':
                require(proof.get('duplicateOpen') is True,'Repeated original open not exercised')
            if stage in ('held','pending','offline','malformed'):
                require(proof.get('noAutomaticLoop') is True and proof.get('quietObservationMs')==500,
                        'Missing bounded no-auto-loop observation')
                if stage=='pending':
                    wanted_proofs['pending']=('pendingOnly','sourceRetained','draftRetained')
        if case=='rate-limit':
            wanted_proofs.update({
                'direct-held':('independentFallback',),
                'limited':('sourceRetained','draftRetained','noDispatch'),
                'cancelled':('draftRetained','debtRetained'),
                'reopened-limited':('draftRetained','debtRetained','duplicateOpen','noDispatch'),
                'expired':('realExpiry','noAutomaticRequest'),
                'success':('actualReadAndAdoption','originalDraft','explicitRetry'),
            })
        if case=='mixed-file-group':
            wanted_proofs.update({'file-held':('automaticRead','duplicateOpen'),
                'group-held':('automaticRead','fileReady','draftRetained'),
                'retry-file-held':('explicitRetryRead','duplicateRetry','draftRetained')})
        require(all(proof.get(k) is True for k in wanted_proofs[stage]),'Missing executed stage proof')

        require(model.get('account')==(FOREIGN if replaced else ACCOUNT) and model.get('epoch')==(1 if replaced else 0),'Wrong stage owner')
        if case in ('mixed-member','mixed-file-group'):
            mixed=model.get('mixedMember',{})
            require(mixed==original.get('mixedMember') and mixed.get('observation',{}).get('current') is True
                    and mixed['observation'].get('source')==member_source(),
                    'Mixed original canonical member source/currentness changed')
            native=mixed['observation'].get('native',{})
            require(native.get('id')==MEMBER and native.get('team_id')=='TINIT0001'
                    and native.get('profile',{}).get('display_name')=='Known mixed member',
                    'Mixed original typed member label changed')
            require(all(mixed.get(key)==dict(started=0,finished=0,active=0)
                        for key in ('absentDriver','knownDriver'))
                    and mixed.get('automaticDriver')==dict(pending=[],subscribed=[],stale=[],worker=False,
                        started=0,finished=0,http_finished=0,acknowledgments=0,failures=0),
                    'Mixed known member unexpectedly acquired or scheduled')
        profile=model.get('profile',{})
        if not replaced:
            expected_blocks=blocks(case,case in AUTOMATIC and saved)
            require(profile.get('enabled') is True and profile.get('blocks')==expected_blocks
                    and profile.get('dirty') is (not saved),'Draft custody changed')
            require(profile.get('reads')==int(saved) and isinstance(profile.get('attempts'),list)
                    and len(profile['attempts'])==int(saved),'Unexpected profile writes/readback')
            require(profile.get('choiceValue')=='Morning' and profile.get('countryValue')=='US','Sibling fields changed')
            if saved:
                attempt=profile['attempts'][0]
                require(isinstance(attempt,list),'Missing actual section wire')
                long=[e for e in attempt if e.get('element_id')=='long']
                require(len(long)==1 and isinstance(long[0].get('long_text',{}).get('blocks'),str),'Missing long-text wire')
                try: wire=json.loads(long[0]['long_text']['blocks'])
                except (ValueError,TypeError) as error: raise RuntimeError('Invalid saved blocks wire') from error
                require(wire==expected_blocks,'Save wire changed opaque duplicate tokens or actual edit')

        else:
            require(profile.get('enabled') is False and proof.get('actualInstallLoaded') is True,'Missing real account replacement')
            require(proof.get('before',{}).get('profile',{}).get('blocks')==blocks(case),'Replacement lost original draft evidence')
        groups=model.get('groups');require(isinstance(groups,dict) and set(groups)==set(ids(case)),'Wrong group inventory')
        successful=stage in ('success','edited','saved','reopened') or case=='ready'
        for group in ids(case):
            value=groups[group];require(isinstance(value,dict),'Missing group observation')
            if successful or (case=='absent' and stage in ('partial','fallback-held') and group==GROUP):
                require(value.get('source')==raw(group) and value.get('label')==(LABEL if group==GROUP else OTHER_LABEL),
                        'Actual raw metadata or exact label not adopted')
                require(value.get('native',{}).get('label_source')==raw(group),'Typed/native metadata lacks exact witness')
            elif case=='retirement' and stage=='source-refused' and group==GROUP:
                changed=raw(GROUP);changed['handle']='Socket crew'
                require(value.get('source')==changed and value.get('label')=='@Socket crew','Stale callback overwrote actual event')
            elif case=='legacy' and not replaced:
                require(value==original['groups'][group] and value.get('source') is None and value.get('label') is None,
                        'Legacy failure fabricated source authority')
            else:
                require(value.get('native') is None and value.get('source') is None and value.get('label') is None,
                        'Unresolved or retired group gained fabricated metadata')
        if case=='legacy' and successful:
            require(groups[GROUP]['native'].get('users')==original['groups'][GROUP]['native']['users']
                    and groups[GROUP]['native'].get('members_loaded') is True,'Metadata recovery erased membership')
        if stage=='edited':
            edit=proof.get('nativeEdit',{})
            before,after=edit.get('before',{}),edit.get('after',{})
            require(edit.get('ok') is True and before.get('snapshot',{}).get('blocks')==blocks(case)
                    and after.get('snapshot',{}).get('blocks')==blocks(case,True)
                    and before.get('tokens')==after.get('tokens')==geometry['captured']['tokens'],
                    'Actual native edit/opaque token proof missing')
            a,b=before['snapshot'],after['snapshot']
            require(a.get('generation')==b.get('generation')==geometry['captured']['snapshot']['generation']
                    and type(a.get('revision')) is int and type(b.get('revision')) is int
                    and b['revision']>a['revision'] and a.get('composing') is False and b.get('composing') is False,
                    'Native edit did not advance original editor revision')
        read_count,finished=counts[stage]
        require(request_reads(model.get('requests'))==expected_reads(case)[:read_count],'Stage request order changed')
        require(model.get('driver')==dict(started=finished+held,finished=finished,active=int(held)),'Driver completion not observed')
        pending=model.get('pending');require(isinstance(pending,list) and len(pending)==int(held),'Wrong pending state')
        if pending: require(pending[0]==model['requests'][-1] and pending[0]['released'] is False,'Wrong held reply')
        if case=='retirement':
            retirement_deliveries(model['requests'])
            if stage in ('cancelled','account-replaced'):
                released=[r for r in model['requests'] if r['kind']!='discovery'][-1]
                require(proof.get('retiredRelease')==dict(serial=released['serial'],
                    receiverRetired=True,delivered=False),'Missing actual closed original receiver proof')
        layout(geometry,viewport,stage,case)
        path=Path(receipt['captures'][stage]);require(path.resolve()==(captures/f'{fixture}-{stage}.png').resolve(),'Capture escaped expected path')
        dimensions=geometry.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(v) is int and v>0 for v in dimensions),'Missing PNG dimensions')
        if png: validate_png(path,dimensions)


def validate_log(native, fixture, captures, platform=sys.platform, png=True):
    receipts = native.receipts(PREFIX)
    require(len(receipts) == 1, 'Missing or duplicate terminal receipt')
    receipt = receipts[0]
    validate(fixture, receipt, captures, platform=platform, png=png)
    require(native.receipts(EDGE) == [dict(stage=s, edge=e) for s in STAGES[receipt['case']] for e in ('before','after')],
            'Wrong native capture order')
    return receipt

def run_fixture(fixture, binary, captures, case):
    require(fixture in FIXTURES, 'Unknown profile group recovery fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'profile-group-recovery-{nonce}'
    directory.mkdir(exist_ok=False)
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    case=case, readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    environment = {FLAG: '1', 'SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_CASE': case,
                   'SUPER_PLATINUM_UI_CAPTURE_DIR': str(captures),
                   'SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_NONCE': nonce,
                   'SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_READY_DIR': str(directory)}
    if case in ABSENT_MEMBER_CASES:
        environment['SUPER_PLATINUM_PROFILE_MEMBER_RECOVERY_CHECK'] = '1'
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary, environment)
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
                        require(time.monotonic() < deadline, 'Profile group recovery native receipt timed out')
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
    parser.add_argument('--captures', type=Path, default=ROOT / 'tmp/profile-group-recovery-native')
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
