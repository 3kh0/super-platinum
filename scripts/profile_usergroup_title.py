"""Strict nested known-usergroup evidence over the unchanged owned Profile harness."""
import json
from pathlib import Path
import sys

from profile_channel_title import FIXTURES, LOADING, ACCOUNT, FOREIGN, blocks, require, same
from profile_usergroup_title_source import (STAGES, COUNTS, HIDDEN, state_for, requests,
    reset, validate_source, validate_cache)


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
    controls = [] if e['retired'] else ['Back', 'Save section']
    if e['editor']: controls += ['Use description', 'Cancel description edit']
    if stage == 'cold-loading': controls += ['Retry description', 'Cancel description edit']
    require(sorted(c.get('text') for c in g['controls']) == sorted(controls), 'Wrong original action inventory')
    require(all(c.get('disabled') is ((c['text'] == 'Save section' and (not e['can_save'] or e['editor'])) or c['text'] == 'Retry description')
                for c in g['controls']), 'Wrong action disabled state')
    notices = [n.get('text') for n in g['notices']]
    if stage == 'cold-loading': require(LOADING in notices, 'Missing bounded file loading feedback')
    if stage == 'confirmed': require('Profile fields saved.' in notices, 'Missing actual Save/readback feedback')
    if stage in ('fresh-opened', 'failure-retained', 'fresh-reopened', 'reopened'):
        require(notices == [], 'Quiet freshness read acquired failure/recovery UI')
    require(g.get('richEditable') is (True if e['editor'] else None), 'Wrong editor visibility')
    if not e['editor']:
        require(g['tokens'] == [] and g.get('captured') is None, 'Hidden editor retained token custody')
        return
    label = e['label']
    require([t.get('text') for t in g['tokens']] == [label, label]
            and all(t.get('editable') is False for t in g['tokens']), 'Wrong mounted/adopted nested group label')
    require([(t.get('fontWeight'), t.get('fontStyle')) for t in g['tokens']] == [('700', 'normal'), ('400', 'italic')],
            'Outer Canvas occurrence styles changed')
    captured = g.get('captured', {}); snap = captured.get('snapshot', {})
    require(same(snap.get('blocks'), blocks(e['dom_prefix'])) and snap.get('composing') is False
            and type(snap.get('revision')) is int and snap['revision'] >= 0
            and isinstance(snap.get('generation'), str) and snap['generation'].isdigit(), 'Native raw capture/generation changed')
    require(same(captured.get('tokens'), [dict(value=t, label=label) for t in blocks()[0]['elements'][0]['elements'][1:]]),
            'Token witness id/URL/opaque/style changed')


def delivered(serial):
    return dict(serial=serial, receiverRetired=False, delivered=True)


def account_proof():
    return dict(oldOpeningRefused=True, oldEditorHidden=True, previousAccount=ACCOUNT, account=FOREIGN,
                previousEpoch=0, epoch=1, previousAttempts=[], previousBlocks=blocks())


def exercise(value, case, stage, source):
    expected = {
        'cold-loading': dict(cold=True),
        'fresh-opened': dict(actualSourceSink=True, noGroupRead=True),
        'mounted-held': dict(reset=reset(1), request=2, immediateAdmission=True),
        'failure-retained': dict(firstRelease=delivered(2), fallbackRelease=delivered(3), quiet=True, draftRetained=True),
        'fresh-reopened': dict(noAdditionalRead=True, attemptedFresh=True),
        'retry-held': dict(reset=reset(2), request=5, selectorHitAfterReconnect=True),
        'renamed-mounted': dict(mountedLabelRetained=True),
        'used': dict(rawTokensRetained=True),
        'confirmed': dict(confirmedSave=True, **({} if case == 'cancel' else dict(newOpeningLabel=True))),
        'late-settled': dict(release=delivered(2), cancelledEditorNotResumed=True),
        'reopened': dict(newOpeningLabel=True),
        'pre-mount-retired': dict(oldOpeningRefused=True, draftUnchanged=True),
        'socket-retired': dict(reset=reset(2), socket=reset(3), release=delivered(4), successor=6, latePublicationRefused=True),
        'account-held': dict(reset=reset(4), request=8),
    }
    if stage != 'account-replaced':
        require(same(value, expected[stage]), 'Missing exact lifecycle/source/interaction proof')
        return
    require(isinstance(value, dict) and set(value) == {'account', 'release', 'before', 'after', 'originalWorkerFinished', 'successorUnchanged'}
            and same(value.get('account'), account_proof()) and value.get('release') == delivered(8)
            and value.get('originalWorkerFinished') is True and value.get('successorUnchanged') is True,
            'Missing exact old worker/account retirement proof')
    require(same(value.get('after'), source), 'Account-after source differs from captured stage')
    before = value.get('before', {})
    # The only changes are release of the old receiver and its passive ledger.
    expected_before = dict(source, requests=requests(case, 8, True), pending=[requests(case, 8, True)[-1]])
    require(same(before, expected_before), 'Late callback altered replacement state or fabricated prior custody')


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    require(isinstance(receipt, dict), 'Missing usergroup title receipt')
    require(receipt.get('ok') is True, f"Native usergroup fixture failed: {receipt.get('error', 'no terminal success')}")
    require(case in STAGES and fixture in FIXTURES and receipt.get('fixture') == fixture
            and receipt.get('case') == case and receipt.get('groupTitle') is True, 'Wrong usergroup title case/receipt owner')
    require(receipt.get('mentionKind') == 'canvas' and receipt.get('titleBlocks') is True
            and receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview') == 'Original PNG inspection required separately', 'Missing closed capture boundary')
    rows = receipt.get('stages'); paths = receipt.get('captures'); viewport = h['expected_viewport'](fixture, platform)
    require(receipt.get('viewport') == viewport and isinstance(rows, list)
            and [r.get('stage') for r in rows] == list(STAGES[case]) and isinstance(paths, dict)
            and set(paths) == set(STAGES[case]), 'Wrong stage/capture inventory')
    require(same(receipt.get('requests'), requests(case)), 'Wrong final discovery/directory/fallback ledger')
    previous = None
    for row in rows:
        stage, m, proof, g = row['stage'], row['model'], row['proof'], row['geometry']
        validate_model(m, case, stage, previous)
        validate_source(proof.get('source'), case, stage, m)
        validate_cache(proof.get('cache'), case, stage)
        exercise(proof.get('exercise'), case, stage, proof['source'])
        geometry(g, viewport, stage, h, case)
        dimensions = g.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2 and all(type(n) is int and n > 0 for n in dimensions),
                'Missing original PNG dimensions')
        path = Path(captures).resolve() / f'{fixture}-{stage}.png'
        require(isinstance(paths[stage], str) and Path(paths[stage]) == path, 'Foreign capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original PNG')
            h['validate_png'](path, dimensions)
        previous = m
    return receipt
