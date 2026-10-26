"""Strict finite Profile event acceptance; existing native guards are reused."""
import json
from pathlib import Path
import sys
from profile_channel_title import FIXTURES, ACCOUNT, blocks, require, same
from profile_canvas_deleted import validate_token_view, DELETE_PATH
from profile_canvas_deletion_event_source import (STAGES, NORMAL, DELETED, TOKEN,
    LOADING, UNAVAILABLE, state_for, file_ledger, validate_source, validate_cache, validate_exercise)


def validate_model(m, case, stage, previous, initial):
    e = state_for(case, stage)
    if e['retired']:
        require(m == dict(enabled=False), 'Replacement account retained old Profile model')
        return
    require(m.get('enabled') is (stage != 'team-away') and m.get('account') == ACCOUNT and m.get('epoch') == 0,
            'Wrong current Profile owner')
    require(m.get('busy') is False and m.get('uncertain') is False and m.get('canSave') is e['can_save'],
            'Unexpected write or Save state')
    require(m.get('managedValue') == 'Preserved organization value', 'Managed sibling changed')
    require(m.get('reads') == int(e['saved']) and isinstance(m.get('attempts'), list)
            and len(m['attempts']) == int(e['saved']), 'Unexpected Save/readback replay')
    if e['closed']:
        require(all(m.get(k) is None for k in ('blocks', 'visit', 'serial', 'choiceValue', 'countryValue'))
                and m.get('dirty') is False, 'Closed Profile retained a mounted draft')
        return
    require(all(type(m.get(k)) is int for k in ('visit', 'serial')) and m['visit'] > 0 and m['serial'] > 0,
            'Missing current section visit/serial')
    require(same(m.get('blocks'), blocks(e['model_prefix'])) and m.get('choiceValue') == 'Morning'
            and m.get('countryValue') == 'US', 'Raw draft or untouched sibling changed')
    if initial and case == 'known' and e['index'] >= 3:
        require(m['visit'] != initial['visit'], 'New Profile visit reused retired visit')
    elif initial:
        require(m['visit'] == initial['visit'], 'Unexpected Profile navigation')
    if previous and previous.get('serial') is not None:
        require((m['serial'] != previous['serial']) if e['saved'] else (m['serial'] == previous['serial']),
                'Wrong section serial boundary')
    if e['saved']:
        attempt = m['attempts'][0]
        require(isinstance(attempt, list), 'Missing actual Profile write')
        fields = {x.get('element_id'): x for x in attempt}
        require(len(fields) == len(attempt) and {'long', 'locked'} <= set(fields)
                and same(json.loads(fields['long']['long_text']['blocks']), blocks('Saved with '))
                and fields['locked'].get('text', {}).get('text') == 'Preserved organization value',
                'Saved wire lost opaque raw Canvas identity/style or managed field')


def expected_tokens(e):
    return [dict(value=t, label=DELETED if e['deleted'] else NORMAL,
                 **({'presentation': TOKEN} if e['deleted'] else {}))
            for t in blocks()[0]['elements'][0]['elements'][1:]]


def validate_capture(value, e):
    require(isinstance(value, dict), 'Missing actual resolved editor capture')
    snap = value.get('snapshot', {})
    require(same(snap.get('blocks'), blocks(e['dom_prefix'])) and snap.get('composing') is False
            and type(snap.get('revision')) is int and snap['revision'] >= 0
            and isinstance(snap.get('generation'), str) and snap['generation'].isdigit(),
            'Wrong raw current editor capture')
    require(same(value.get('tokens'), expected_tokens(e)), 'Mounted/adopted label or raw token/presentation changed')


def geometry(g, viewport, stage, h, case):
    e = state_for(case, stage)
    dialog = not (e['retired'] or e['closed'])
    require(isinstance(g, dict) and g.get('ok') is True and g.get('noOverflow') is True
            and g.get('viewport') == viewport and g.get('dialog') is dialog, 'Wrong actual dialog/layout')
    if dialog: h['rectangle'](g.get('dialogRect'), viewport)
    else: require(g.get('dialogRect') is None, 'Closed dialog retained geometry')
    for key in ('controls', 'notices', 'tokens'):
        require(isinstance(g.get(key), list), 'Missing measured inventory')
        for item in g[key]:
            require(item.get('visible') is True, 'Required action/token/feedback hidden')
            h['rectangle'](item.get('rect'), viewport)
            for fragment in item.get('fragments', []): h['rectangle'](fragment, viewport)
    controls = ['Back', 'Save section'] if dialog else []
    if e['editor']: controls += ['Use description', 'Cancel description edit']
    if stage in ('cold-loading', 'pre-mount-retired'): controls += ['Retry description', 'Cancel description edit']
    require(sorted(x.get('text') for x in g['controls']) == sorted(controls), 'Wrong exact control inventory')
    require(all(x.get('disabled') is ((x['text'] == 'Save section' and (not e['can_save'] or e['editor']))
                or (x['text'] == 'Retry description' and stage == 'cold-loading')) for x in g['controls']),
            'Wrong action disabled state or Save enabled while editor mounted')
    notices = [x.get('text') for x in g['notices']]
    if stage == 'cold-loading': require(LOADING in notices, 'Missing cold metadata feedback')
    if stage == 'pre-mount-retired': require(UNAVAILABLE in notices, 'Missing stale opening feedback')
    if stage == 'confirmed': require('Profile fields saved.' in notices, 'Missing Save/readback feedback')
    require(g.get('richEditable') is (True if e['editor'] else None), 'Wrong surrounding editor state')
    if not e['editor']:
        require(g['tokens'] == [] and g.get('captured') is None, 'Unmounted editor retained captured atoms')
        return
    label = DELETED if e['deleted'] else NORMAL
    require([t.get('text') for t in g['tokens']] == [label, label]
            and all(t.get('editable') is False for t in g['tokens']), 'Wrong mounted label/canonical adoption')
    require([(t.get('fontWeight'), t.get('fontStyle')) for t in g['tokens']] == [('700', 'normal'), ('400', 'italic')],
            'Distinct opaque occurrence styles changed')
    validate_capture(g.get('captured'), e)


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    require(isinstance(receipt, dict), 'Missing event receipt')
    require(receipt.get('ok') is True, f"Native deletion-event fixture failed: {receipt.get('error', 'no terminal success')}")
    require(case in STAGES and fixture in FIXTURES and receipt.get('fixture') == fixture
            and receipt.get('case') == case and receipt.get('deletionEvent') is True, 'Wrong exact event fixture mode')
    require(receipt.get('mentionKind') == 'canvas' and receipt.get('titleBlocks') is True
            and receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview') == 'PNG inspection required separately', 'Missing closed native capture boundary')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    viewport = h['expected_viewport'](fixture, platform)
    require(receipt.get('viewport') == viewport and isinstance(rows, list)
            and [x.get('stage') for x in rows] == list(STAGES[case]) and isinstance(paths, dict)
            and set(paths) == set(STAGES[case]), 'Wrong finite stages/captures')
    require(same(receipt.get('files'), file_ledger(case, STAGES[case][-1])), 'Unexpected files.info request or wrong final ledger')
    initial, previous = None, None
    for row in rows:
        stage, m, proof, g = row['stage'], row['model'], row['proof'], row['geometry']
        e = state_for(case, stage)
        validate_model(m, case, stage, previous, initial)
        validate_source(proof.get('source'), case, stage)
        validate_cache(proof.get('cache'), case, stage)
        geometry(g, viewport, stage, h, case)
        validate_exercise(proof.get('exercise'), case, stage,
            lambda token: validate_token_view(token, e, g.get('captured')), m, previous)
        dimensions = g.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), 'Missing PNG dimensions')
        path = Path(captures).resolve() / f'{fixture}-{stage}.png'
        require(isinstance(paths[stage], str) and Path(paths[stage]) == path, 'Foreign capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original PNG')
            h['validate_png'](path, dimensions)
        if initial is None: initial = m
        previous = m
    if case == 'known':
        away = next(x for x in rows if x['stage'] == 'team-away')['proof']['exercise']['navigation']
        returned = next(x for x in rows if x['stage'] == 'team-returned')['proof']['exercise']['navigation']
        require(returned['profileGenerationBefore'] == away['profileGenerationAfter'], 'Navigation return borrowed a different Profile generation')
    return receipt
