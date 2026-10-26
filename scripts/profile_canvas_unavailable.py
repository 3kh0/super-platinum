"""Strict unavailable Canvas evidence, using unchanged Profile native guards."""
import json
import math
from pathlib import Path
import sys
from profile_channel_title import FIXTURES, ACCOUNT, blocks, require, same
from profile_canvas_unavailable_source import (STAGES, PRIVATE, RECOVERED, TOKEN, LOADING,
    UNAVAILABLE, state_for, file_ledger, validate_source, validate_cache, validate_exercise)

# Existing typed Google Material Symbols Rounded Lock; independent expected path.
LOCK_PATH = 'M240-80q-33 0-56.5-23.5T160-160v-400q0-33 23.5-56.5T240-640h40v-80q0-83 58.5-141.5T480-920q83 0 141.5 58.5T680-720v80h40q33 0 56.5 23.5T800-560v400q0 33-23.5 56.5T720-80H240Zm0-80h480v-400H240v400Zm240-120q33 0 56.5-23.5T560-360q0-33-23.5-56.5T480-440q-33 0-56.5 23.5T400-360q0 33 23.5 56.5T480-280ZM360-640h240v-80q0-50-35-85t-85-35q-50 0-85 35t-35 85v80ZM240-160v-400 400Z'


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


def expected_tokens(e):
    label = PRIVATE if e['private'] else RECOVERED
    return [dict(value=t, label=label, **({'presentation': TOKEN} if e['private'] else {}))
            for t in blocks()[0]['elements'][0]['elements'][1:]]


def validate_capture(captured, e):
    require(isinstance(captured, dict), 'Missing native resolved capture')
    snap = captured.get('snapshot', {})
    require(same(snap.get('blocks'), blocks(e['dom_prefix'])) and snap.get('composing') is False
            and type(snap.get('revision')) is int and snap['revision'] >= 0
            and isinstance(snap.get('generation'), str) and snap['generation'].isdigit(), 'Raw capture or current generation changed')
    require(same(captured.get('tokens'), expected_tokens(e)), 'Raw token/label/presentation custody changed')


def validate_token_view(value, e, captured):
    require(isinstance(value, dict) and set(value) == {'ok', 'tokens', 'capture'} and value['ok'] is True,
            'Missing actual private atom view')
    require(same(value['capture'], captured), 'Icon proof differs from captured current draft')
    require(isinstance(value['tokens'], list) and len(value['tokens']) == 2, 'Wrong private token count')
    for token in value['tokens']:
        require(set(token) == {'private', 'disabled', 'tag', 'contentEditable', 'href', 'tabindex', 'icon'}
                and token['tag'] == 'SPAN' and token['contentEditable'] == 'false'
                and token['href'] is None and token['tabindex'] is None
                and token['private'] is e['private']
                and token['disabled'] == ('false' if e['private'] else None), 'Wrong enabled opaque atom semantics')
        icon = token['icon']
        if not e['private']:
            require(icon is None, 'Recovered normal title retained private icon')
            continue
        require(isinstance(icon, dict) and set(icon) == {'tag', 'hidden', 'paths', 'width', 'height'}
                and icon['tag'] == 'svg' and icon['hidden'] == 'true' and icon['paths'] == [LOCK_PATH],
                'Wrong original typed status icon')
        require(all(type(icon[k]) in (int, float) and math.isfinite(icon[k]) and icon[k] > 0
                    for k in ('width', 'height')), 'Status icon has no rendered geometry')


def geometry(g, viewport, stage, h, case):
    e = state_for(case, stage)
    require(isinstance(g, dict) and g.get('ok') is True and g.get('noOverflow') is True
            and g.get('viewport') == viewport and g.get('dialog') is (not e['retired']), 'Wrong measured dialog/layout')
    if not e['retired']: h['rectangle'](g.get('dialogRect'), viewport)
    for key in ('controls', 'notices', 'tokens'):
        require(isinstance(g.get(key), list), 'Missing measured inventory')
        for item in g[key]:
            require(item.get('visible') is True, 'Required token/control/feedback hidden')
            h['rectangle'](item.get('rect'), viewport)
            for fragment in item.get('fragments', []): h['rectangle'](fragment, viewport)
    recovering = stage in ('cold-loading', 'malformed', 'offline', 'pre-mount-retired', 'socket-held', 'socket-retired')
    loading = stage in ('cold-loading', 'socket-held')
    controls = [] if e['retired'] else ['Back', 'Save section']
    if e['editor']: controls += ['Use description', 'Cancel description edit']
    if recovering: controls += ['Retry description', 'Cancel description edit']
    require(sorted(c.get('text') for c in g['controls']) == sorted(controls), 'Wrong actual control inventory')
    require(all(c.get('disabled') is ((c['text'] == 'Save section' and (not e['can_save'] or e['editor']))
                or (c['text'] == 'Retry description' and loading)) for c in g['controls']), 'Wrong action disabled state')
    notices = [n.get('text') for n in g['notices']]
    if recovering: require((LOADING if loading else UNAVAILABLE) in notices, 'Missing exact bounded recovery feedback')
    if stage == 'confirmed': require('Profile fields saved.' in notices, 'Missing Save/readback feedback')
    require(g.get('richEditable') is (True if e['editor'] else None), 'Wrong editor state')
    if not e['editor']:
        require(g['tokens'] == [] and g.get('captured') is None, 'Hidden editor retained capture')
        return
    label = PRIVATE if e['private'] else RECOVERED
    require([t.get('text') for t in g['tokens']] == [label, label]
            and all(t.get('editable') is False for t in g['tokens']), 'Wrong mounted/adopted status label')
    require([(t.get('fontWeight'), t.get('fontStyle')) for t in g['tokens']] == [('700', 'normal'), ('400', 'italic')],
            'Outer Canvas occurrence styles changed')
    validate_capture(g.get('captured'), e)


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    require(isinstance(receipt, dict), 'Missing unavailable Canvas receipt')
    require(receipt.get('ok') is True, f"Native unavailable Canvas fixture failed: {receipt.get('error', 'no terminal success')}")
    require(case in STAGES and fixture in FIXTURES and receipt.get('fixture') == fixture
            and receipt.get('case') == case and receipt.get('unavailableCanvas') is True, 'Wrong exact receipt mode/owner')
    require(receipt.get('mentionKind') == 'canvas' and receipt.get('titleBlocks') is True
            and receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview') == 'PNG inspection required separately', 'Missing closed capture boundary')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    viewport = h['expected_viewport'](fixture, platform)
    require(receipt.get('viewport') == viewport and isinstance(rows, list)
            and [r.get('stage') for r in rows] == list(STAGES[case]) and isinstance(paths, dict)
            and set(paths) == set(STAGES[case]), 'Wrong stage/capture inventory')
    require(same(receipt.get('files'), file_ledger(case, STAGES[case][-1])), 'Wrong final files.info ledger')
    previous = None
    for row in rows:
        stage, m, proof, g = row['stage'], row['model'], row['proof'], row['geometry']
        e = state_for(case, stage)
        validate_model(m, case, stage, previous)
        validate_source(proof.get('source'), case, stage)
        validate_cache(proof.get('cache'), case, stage)
        geometry(g, viewport, stage, h, case)
        validate_exercise(proof.get('exercise'), case, stage,
                          lambda value: validate_token_view(value, e, g.get('captured')))
        dims = g.get('pngDimensions')
        require(isinstance(dims, list) and len(dims) == 2 and all(type(n) is int and n > 0 for n in dims), 'Missing PNG dimensions')
        path = Path(captures).resolve() / f'{fixture}-{stage}.png'
        require(isinstance(paths[stage], str) and Path(paths[stage]) == path, 'Foreign capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original PNG')
            h['validate_png'](path, dims)
        previous = m
    return receipt
