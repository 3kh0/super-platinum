"""Strict section-reference evidence; native process/capture guards are reused unchanged."""
import json
import math
from pathlib import Path
import sys
import profile_canvas_section_status as status
from profile_channel_title import ACCOUNT, FOREIGN, FIXTURES, require, same
from profile_canvas_section_source import (blocks, label_units, validate_units, STAGES, LOADING, UNAVAILABLE,
    state_for, source, file_ledger, section_ledger, validate_source, validate_cache)

PRESENTATION = dict(kind='canvas-section', icon='description', disabled=False)


def expected_tokens(reply):
    return [dict(value=raw, labelUnits=units, presentation=status.presentation(reply) if reply in ('private','deleted') else dict(PRESENTATION))
            for raw, units in zip(blocks()[0]['elements'][0]['elements'][1:], label_units(reply))]


def validate_capture(captured, prefix, reply):
    require(isinstance(captured, dict), 'Missing actual section editor capture')
    snapshot = captured.get('snapshot', {})
    require(same(snapshot.get('blocks'), blocks(prefix)) and snapshot.get('composing') is False
            and type(snapshot.get('revision')) is int and snapshot['revision'] >= 0
            and isinstance(snapshot.get('generation'), str) and snapshot['generation'].isdigit(),
            'Wrong exact current draft/raw section identity')
    tokens = captured.get('tokens')
    expected = expected_tokens(reply)
    require(isinstance(tokens, list) and len(tokens) == len(expected), 'Wrong section occurrence inventory')
    for token, wanted in zip(tokens, expected):
        require(isinstance(token, dict), 'Missing section witness')
        validate_units(token.get('labelUnits'), wanted['labelUnits'])
        require(same(token, wanted), 'Wrong exact section witness/raw/presentation; scalar label is not authority')

def validate_model(m, case, stage, previous):
    e = state_for(case, stage)
    if e['retired']:
        require(m == dict(enabled=False), 'Retired Profile model survived')
        return
    require(m.get('enabled') is True and m.get('account') == ACCOUNT and m.get('epoch') == 0
            and all(type(m.get(k)) is int for k in ('visit', 'serial')), 'Wrong Profile visit/account')
    if previous:
        require(m['visit'] == previous['visit']
                and ((m['serial'] != previous['serial']) if e['saved'] else m['serial'] == previous['serial']),
                'Unexpected section serial/visit change')
    require(same(m.get('blocks'), blocks(e['model_prefix'])) and m.get('busy') is False
            and m.get('uncertain') is False and m.get('canSave') is e['can_save'], 'Form draft or write state changed')
    require(m.get('managedValue') == 'Preserved organization value' and m.get('choiceValue') == 'Morning'
            and m.get('countryValue') == 'US', 'Untouched Profile sibling changed')
    require(m.get('reads') == int(e['saved']) and isinstance(m.get('attempts'), list)
            and len(m['attempts']) == int(e['saved']), 'Unexpected Save/readback replay')
    if e['saved']:
        attempt = m['attempts'][0]
        require(isinstance(attempt, list), 'Missing Profile wire')
        values = {v.get('element_id'): v for v in attempt}
        require(len(values) == len(attempt) and {'long', 'locked'} <= set(values)
                and same(json.loads(values['long']['long_text']['blocks']), blocks('Saved with '))
                and values['locked'].get('text', {}).get('text') == 'Preserved organization value',
                'Raw saved Canvas token/source custody changed')


DESCRIPTION_PATH = 'M360-240h240q17 0 28.5-11.5T640-280q0-17-11.5-28.5T600-320H360q-17 0-28.5 11.5T320-280q0 17 11.5 28.5T360-240Zm0-160h240q17 0 28.5-11.5T640-440q0-17-11.5-28.5T600-480H360q-17 0-28.5 11.5T320-440q0 17 11.5 28.5T360-400ZM240-80q-33 0-56.5-23.5T160-160v-640q0-33 23.5-56.5T240-880h287q16 0 30.5 6t25.5 17l194 194q11 11 17 25.5t6 30.5v447q0 33-23.5 56.5T720-80H240Zm280-560v-160H240v640h480v-440H560q-17 0-28.5-11.5T520-640ZM240-800v200-200 640-640Z'


def scalar(units):
    return bytes(sum(([u & 255,u >> 8] for u in units),[])).decode('utf-16-le','replace')


def validate_token_view(value, captured, e):
    require(isinstance(value,dict) and set(value)=={'ok','capture','units','icons','editorEditable'}
        and value['ok'] is True and value['editorEditable'] is True, 'Missing actual section atom proof')
    require(same(value['capture'],captured) and same(value['units'],label_units(e['label'])),
            'Token-view units or capture not current mounted witness')
    require(isinstance(value['icons'],list) and len(value['icons'])==2, 'Wrong section icon inventory')
    for token in value['icons']:
        require(set(token)=={'section','disabled','editable','icon'} and token['section'] is True
            and token['disabled']=='false' and token['editable']=='false', 'Wrong enabled opaque section atom')
        icon=token['icon']
        require(isinstance(icon,dict) and set(icon)=={'hidden','paths','width','height'}
            and icon['hidden']=='true' and icon['paths']==[DESCRIPTION_PATH], 'Wrong typed section icon')
        require(all(type(icon[k]) in (float,int) and math.isfinite(icon[k]) and icon[k]>0 for k in ('width','height')),
                'Section icon has no actual geometry')


def geometry(g,viewport,stage,h,case):
    e=state_for(case,stage)
    require(isinstance(g,dict) and g.get('ok') is True and g.get('noOverflow') is True
            and g.get('viewport')==viewport and g.get('dialog') is (not e['retired']), 'Wrong actual section dialog/viewport')
    if not e['retired']: h['rectangle'](g.get('dialogRect'),viewport)
    for key in ('controls','notices','tokens'):
        require(isinstance(g.get(key),list), 'Missing measured control/token inventory')
        for item in g[key]:
            require(item.get('visible') is True, 'Required action, feedback or token is hidden')
            h['rectangle'](item.get('rect'),viewport)
            for fragment in item.get('fragments',[]): h['rectangle'](fragment,viewport)
    loading=stage in ('section-loading','socket-held')
    recovering=loading or stage in ('pre-mount-retired','socket-retired')
    controls=[] if e['retired'] else ['Back','Save section']
    if e['editor']: controls+=['Use description','Cancel description edit']
    if recovering: controls+=['Retry description','Cancel description edit']
    require(sorted(x.get('text') for x in g['controls'])==sorted(controls), 'Wrong exact available actions')
    require(all(x.get('disabled') is ((x['text']=='Save section' and (not e['can_save'] or e['editor']))
            or (x['text']=='Retry description' and loading)) for x in g['controls']), 'Wrong actual action/Save lock')
    notices=[x.get('text') for x in g['notices']]
    if recovering: require((LOADING if loading else UNAVAILABLE) in notices, 'Missing exact opening feedback')
    if stage=='confirmed': require('Profile fields saved.' in notices, 'Missing confirmed Save/readback feedback')
    require(g.get('richEditable') is (True if e['editor'] else None), 'Wrong surrounding editor capability')
    if not e['editor']:
        require(g['tokens']==[] and g.get('captured') is None, 'Unmounted editor retained atom capture')
        return
    units=label_units(e['label'])
    require([t.get('text') for t in g['tokens']]==[scalar(u) for u in units]
            and len(g['tokens'])==2 and all(t.get('editable') is False for t in g['tokens']),
            'Wrong rendered scalar projection or opaque token count')
    for token,wanted in zip(g['tokens'],units): validate_units(token.get('labelUnits'),wanted)
    require([(t.get('fontWeight'),t.get('fontStyle')) for t in g['tokens']]==[('700','normal'),('400','italic')],
            'Distinct raw occurrence formatting changed')
    validate_capture(g.get('captured'),e['dom_prefix'],e['label'])


def validate_exercise(value,case,stage,g):
    e=state_for(case,stage)
    if case=='status':
        return status.exercise(value,stage,g.get('captured'),label_units(e['label']))
    require(isinstance(value,dict), 'Missing actual lifecycle proof')
    if stage=='account-replaced':
        expected=source(case,stage)
        before=json.loads(json.dumps(expected));before['sections']=section_ledger(case,'account-held')
        account=dict(oldOpeningRefused=True,oldEditorHidden=True,previousAccount=ACCOUNT,account=FOREIGN,
            previousEpoch=0,epoch=1,previousAttempts=[],previousBlocks=blocks())
        require(same(value,dict(account=account,release=dict(delivered=True,cancelledBeforeReply=False),accepted=False,
                replacementBefore=before,replacementAfter=expected)), 'Late reply changed replacement source/account')
        return
    expected={
        'section-loading':dict(held=True), 'socket-held':dict(held=True),
        'cancelled':dict(release=dict(delivered=False,cancelledBeforeReply=True)),
        'pre-mount-retired':dict(openingRefused=True,draftUnchanged=True,noWrite=True),
        'socket-retired':dict(socket=dict(before=0,after=1),release=dict(delivered=True,cancelledBeforeReply=False),latePublicationRefused=True),
        'used':dict(rawTokensPreserved=True),
    }
    if stage in expected:
        require(same(value,expected[stage]), 'Wrong exact retirement/Cancel/Use proof')
        return
    validate_token_view(value.get('token'),g.get('captured'),e)
    rest={k:v for k,v in value.items() if k!='token'}
    if stage=='opened' and case=='success':
        validate_completion(rest.get('completion'),g.get('captured'))
        rest=dict(rest,completion=None)
    expected={
        'opened':dict(completion=None), 'refreshed-mounted':dict(mountedLabelRetained=True,tamper=dict(ok=True,
            iconMutationRefused=True,equalDisplayUnitMutationRefused=True,descriptorRemovalRefused=True,restored=True)),
        'failed-reopened':dict(errorClearedFile=True), 'reopened-present':dict(newOpeningRetried=True),
        'confirmed':dict(savedAndReadBack=True), 'reopened':dict(newSocket=True), 'account-held':dict(held=True),
    }
    require(stage in expected and same(rest,expected[stage]), 'Wrong exact current/mounted/capture proof')


def validate(fixture,receipt,captures,h,*,case,platform=sys.platform,png=True):
    require(isinstance(receipt,dict), 'Missing native section receipt')
    require(receipt.get('ok') is True, f"Native section fixture failed: {receipt.get('error','no terminal success')}")
    require(case in STAGES and fixture in FIXTURES and receipt.get('fixture')==fixture and receipt.get('case')==case
        and receipt.get('canvasSection') is True and receipt.get('mentionKind')=='canvas' and receipt.get('titleBlocks') is True,
        'Wrong explicit section fixture/owner mode')
    require(receipt.get('captureChecksPassed') is True and receipt.get('visualReview')=='PNG inspection required separately',
            'Missing native capture boundary')
    rows,paths=receipt.get('stages'),receipt.get('captures');viewport=h['expected_viewport'](fixture,platform)
    require(receipt.get('viewport')==viewport and isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case])
        and isinstance(paths,dict) and set(paths)==set(STAGES[case]), 'Wrong exact stage/capture inventory')
    require(same(receipt.get('files'),file_ledger(case,STAGES[case][-1]))
        and same(receipt.get('sections'),section_ledger(case,STAGES[case][-1])), 'Wrong final file/section requests')
    previous=None
    for row in rows:
        stage,m,proof,g=row['stage'],row['model'],row['proof'],row['geometry']
        validate_model(m,case,stage,previous)
        validate_source(proof.get('source'),case,stage)
        validate_cache(proof.get('cache'),case,stage)
        geometry(g,viewport,stage,h,case)
        validate_exercise(proof.get('exercise'),case,stage,g)
        dims=g.get('pngDimensions')
        require(isinstance(dims,list) and len(dims)==2 and all(type(n) is int and n>0 for n in dims), 'Missing native PNG dimensions')
        path=Path(captures).resolve()/f'{fixture}-{stage}.png'
        require(isinstance(paths[stage],str) and Path(paths[stage])==path, 'Foreign native capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original capture')
            h['validate_png'](path,dims)
        previous=m
    return receipt


def validate_completion(value,current_capture):
    require(isinstance(value,dict) and set(value)=={'ok','ordinaryCompletion','nativeUndo','rawSnapshotRestored','before','completed','afterUndo','restored'}
        and all(value[k] is True for k in ('ok','ordinaryCompletion','nativeUndo','rawSnapshotRestored')), 'Missing actual completion/Undo custody')
    expected=blocks();expected[0]['elements'][0]['elements'].append(dict(type='text',text=' query'))
    snapshots=[]
    for key in ('before','completed','afterUndo','restored'):
        captured=value[key];snapshot=captured.get('snapshot',{})
        require(same(captured.get('tokens'),expected_tokens('present')) and snapshot.get('composing') is False
            and type(snapshot.get('revision')) is int and snapshot['revision']>=0
            and snapshot.get('generation')==current_capture['snapshot']['generation'], 'Completion changed exact raw tokens, units or generation')
        snapshots.append(snapshot)
    require(same(snapshots[0]['blocks'],expected) and same(snapshots[2]['blocks'],expected)
        and same(snapshots[3]['blocks'],blocks()) and same(value['restored'],current_capture),
        'Actual native Undo/query restoration did not retain the exact draft')
    completed=snapshots[1]['blocks']
    require(isinstance(completed,list) and len(completed)==1 and completed[0].get('type')=='rich_text'
        and len(completed[0].get('elements',[]))==1 and completed[0]['elements'][0].get('type')=='rich_text_section', 'Completion changed block structure')
    leaves=completed[0]['elements'][0].get('elements',[])
    require(same([leaf for leaf in leaves if leaf.get('type')!='text'],blocks()[0]['elements'][0]['elements'][1:])
        and ''.join(leaf.get('text','') for leaf in leaves if leaf.get('type')=='text')=='Working with  completed',
        'Completion did not insert the intended text beside protected section atoms')
