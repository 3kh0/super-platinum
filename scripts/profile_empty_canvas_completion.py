"""Query-owned empty Canvas projection, original selection and Save acceptance."""
import copy
import json
from collections import Counter
from pathlib import Path
import sys
import profile_empty_canvas_title as E
from profile_user_title_completion import native

require = E.require
ACCOUNT = '00000000-0000-4000-8000-000000000081'
CANVAS = 'FCOMPLETE1'
FAMILIES = {'users/search', 'usergroups/search', 'search.autocomplete.files'}


def source(value, stage, h, previous):
    requests = h['source'](value)
    require(requests[:len(previous)] == previous, 'Original query request history changed')
    expected = Counter() if stage == 'opened' else Counter((method, 'alex') for method in FAMILIES)
    require(Counter((r['method'], r['args'].get('query')) for r in requests if r['method'] in FAMILIES) == expected,
            'Unexpected query families, repetition or unrequested search')
    require(value.get('active') == 0 and value.get('completed') == sum(expected.values())
            and value.get('cancelled') == 0 and value.get('held') is False and value.get('heldQueries') == [],
            'Original query callbacks did not settle')
    info = [r for r in requests if r['method'] == 'files.info']
    require(len(info) == int(stage == 'saved'), 'Unexpected canonical file read/replay')
    for row in requests:
        require('raw' in row, 'Original response receipt missing')
        if row['method'] == 'search.autocomplete.files':
            require(row['raw'] == E.completion_bytes(), 'Original ordered file response bytes changed')
        elif row['method'] == 'files.info':
            require(row['args']['file'] == CANVAS and row['raw'] == dict(ok=True, file=json.loads(E.completion_bytes())['files'][0]),
                    'Fresh reopen borrowed or replaced original canonical file')
        else:
            require(isinstance(row['raw'], dict) and row['raw'].get('ok') is True,
                    'Missing successful original metadata/search response')
    if stage == 'opened':
        require(requests == [], 'Query dispatched before original input')
    return copy.deepcopy(requests)


def catalog(choices, labels, token):
    require(isinstance(choices, dict) and type(choices.get('sequence')) is int and choices['sequence'] > 0
            and type(choices.get('revision')) is int and choices['revision'] > 0,
            'Missing current original query catalog')
    rows = choices.get('values')
    require(isinstance(rows, list) and [r.get('label') for r in rows] == labels, 'Original native/catalog order differs')
    people = [r for r in rows if r.get('value', {}).get('type') == 'user']
    files = [r for r in rows if r.get('value', {}).get('type') in ('canvas', 'file')]
    require(len(rows) == 7 and len(people) == 5 and len(files) == 2
            and {r['value'].get('user_id') for r in people} == {'UMENTION01', 'UCOMPLETE1', 'UCOMPLETE2', 'UCOMPLETE3', 'UINIT0001'},
            'Original mixed candidate identities changed')
    wanted = [dict(value=token, label='Untitled'),
              dict(value=dict(type='file', file_id='FCOMPLETE2', text='https://fixture.invalid/files/FCOMPLETE2',
                              url='https://fixture.invalid/files/FCOMPLETE2'), label='Alex List')]
    require(sorted(files, key=lambda r: r['value']['file_id']) == wanted, 'Fallback substituted wire identity or borrowed List title')
    require(not choices.get('titleUsers'), 'Text-only title borrowed member authority')
    fallback = choices.get('canvasFallback')
    require(isinstance(fallback, dict) and set(fallback) == {CANVAS}, 'Fallback escaped original Canvas choice')
    E.fallback(fallback[CANVAS], True, quip_file=CANVAS)
    return rows


def validate(fixture, receipt, captures, h, *, platform=sys.platform, png=True):
    base = h['B']; initial = h['initial'](); token = h['token'](CANVAS)
    require(fixture in base['FIXTURES'] and isinstance(receipt, dict) and receipt.get('ok') is True
            and receipt.get('fixture') == fixture and receipt.get('case') == 'completion', 'Wrong original completion terminal')
    require(receipt.get('initialBlocks') == initial and receipt.get('emptyCanvas') == E.snapshot('completion'),
            'Original blocks or empty-title response fixture changed')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(E.COMPLETION_STAGES)
            and isinstance(paths, dict) and set(paths) == set(E.COMPLETION_STAGES), 'Missing/reordered original stages')
    viewport = base['expected_viewport'](fixture, platform)
    previous = None; requests = []; chosen = None; inserted = None
    for row in rows:
        stage = row['stage']; model = row.get('model', {}); proof = row.get('proof', {}); g = row.get('geometry', {})
        exercise = proof.get('exercise', {}); saved = stage == 'saved'
        require(model.get('enabled') is True and model.get('account') == ACCOUNT and type(model.get('epoch')) is int
                and type(model.get('visit')) is int and type(model.get('serial')) is int, 'Original Profile owner missing')
        require(model.get('busy') is False and model.get('uncertain') is False and model.get('dirty') is False
                and model.get('managedValue') == 'Preserved organization value' and model.get('choiceValue') == 'Morning'
                and model.get('countryValue') == 'US', 'Unrelated original field/state changed')
        if previous:
            require((model['epoch'], model['visit']) == (previous['epoch'], previous['visit'])
                    and ((model['serial'] != previous['serial']) if saved else model['serial'] == previous['serial']),
                    'Original section ownership changed outside Save/readback')
        require(model.get('blocks') == (inserted if saved else initial) and model.get('reads') == int(saved)
                and isinstance(model.get('attempts'), list) and len(model['attempts']) == int(saved), 'Unexpected write/readback')
        if saved:
            fields = {v.get('element_id'): v for v in model['attempts'][0]}
            require(len(fields) == len(model['attempts'][0]) and json.loads(fields.get('long', {}).get('long_text', {}).get('blocks', 'null')) == inserted
                    and fields.get('locked', {}).get('text', {}).get('text') == 'Preserved organization value', 'Save wire changed original rich draft')
        require(proof.get('emptyCanvas') == E.snapshot('completion'), 'Wrong retained source vector')
        requests = source(row.get('source'), stage, h, requests)
        base['validate_geometry'](g, viewport, stage)
        labels = h['popup_geometry'](g, viewport)
        blocks = native(g.get('native'), initial, token, 'Untitled', stage, '@alex')
        require([t.get('text') for t in g.get('tokens', [])] == ['@Alex Local', '@Alex Local']
                + (['Untitled'] if stage in ('inserted', 'saved') else []), 'Original painted token labels changed')
        choices = proof.get('choices')
        if stage == 'opened':
            require(exercise == dict(initialBlocks=initial) and choices is None and not labels, 'Unrequested initial query')
        elif stage == 'query-ready':
            values = catalog(choices, labels, token); chosen = copy.deepcopy(choices)
            observed = exercise.get('native', {})
            require(exercise.get('choices') == choices and observed.get('ok') is True and observed.get('sequence') == choices['sequence']
                    and observed.get('serial') == str(model['serial']) and observed.get('native') == g['native'],
                    'Original query/catalog/native ownership differs')
            require(len(g.get('resultRows', [])) == len(values) and observed.get('rows') == [
                dict(id=r['id'], label=r['text'], value=v['value']) for r, v in zip(g['resultRows'], values)],
                'Original option node/order/value proof changed')
            h['ordered_source'](dict(fileResponseBytes=E.completion_bytes(), sourceFiles=proof.get('sourceFiles')))
            first = proof['sourceFiles']['rows'][0]
            require(first['source']['title_blocks'] == E.whitespace_blocks() and first['effective'].get('title_blocks') == E.whitespace_blocks()
                    and first['effective'].get('title') == 'Misleading plain title', 'Raw extraction/rank metadata changed')
        elif stage == 'inserted':
            choice = exercise.get('choice', {}); index = choice.get('index'); values = chosen['values']
            require(choice.get('ok') is True and choice.get('id') == CANVAS and choice.get('type') == 'canvas'
                    and choice.get('keyboard') is True and choice.get('sequence') == chosen['sequence'] and choice.get('values') == values
                    and type(index) is int and 0 <= index < len(values) and values[index] == dict(value=token, label='Untitled')
                    and choice.get('label') == 'Untitled', 'Original Use choice identity or wire changed')
            require(choices is None and not labels and exercise.get('expectedBlocks') == blocks, 'Consumed query or native insertion changed')
            inserted = copy.deepcopy(blocks)
        else:
            require(choices is None and not labels and exercise.get('expectedBlocks') == blocks == inserted
                    and exercise.get('exactReadback') is True, 'Original Save/readback changed wire tokens')
            reopened = exercise.get('reopened', {})
            require(reopened.get('ok') is True and reopened.get('native') == g['native'], 'Fresh reopened native label/custody differs')
        path = Path(captures).resolve() / f'{fixture}-{stage}.png'
        require(isinstance(paths[stage], str) and Path(paths[stage]) == path, 'Foreign original screenshot path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original PNG')
            base['validate_png'](path, g['pngDimensions'])
        previous = model
    require(receipt.get('model') == rows[-1]['model'] and receipt.get('source') == rows[-1]['source'], 'Terminal original model/source drift')
    return receipt
