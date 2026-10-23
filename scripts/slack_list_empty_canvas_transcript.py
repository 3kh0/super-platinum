"""Independent transcript fallback proof atop the unchanged cold-file custody oracle."""
import copy
import hashlib
import json
from pathlib import Path
import runpy

CASES = {'plain-empty': 'cold-files', 'blocks-empty': 'cold-files-recovery'}
STAGES = {'plain-empty': ('cold-held', 'cold-loaded', 'canvas-open', 'canvas-return', 'list-open', 'list-return'),
          'blocks-empty': ('cold-failed', 'retry-held', 'source-retired', 'cold-recovered', 'visit-held', 'visit-retired')}
DESCRIPTOR = dict(locale='en-US', namespace='canvas', source='Untitled', value='Untitled', origin='application-presentation')
ORIGINAL_HASHES = ('bf1afca6830d39742ed0a16567441d484218a7eda448c56670d6e9a5a9f6bc2d',
                   'a55f136cc5107219eeb87c863ba0f5010f30b79455f10285d81faa02f3ee79a5')
# file_title_source::FIELDS/project, distinct from CandidateObservation's
# sharing/channel/default projection. These fixture receipts are complete
# ordinary positive files.info rows; no missing-field merge is simulated.
TITLE_FIELDS = ('id', 'mode', 'filetype', 'subtype', 'name', 'title', 'title_blocks',
    'isUnknown', 'isNonExistent', 'is_deleted', 'is_tombstoned', 'restriction_type',
    'is_locked', 'is_hidden_by_limit', 'file_access', 'permalink', 'updated',
    'created', 'timestamp', 'is_external', 'external_type', 'is_skill',
    'canvas_template_mode', 'list_template_mode', 'is_huddle_canvas',
    'is_huddle_transcript_canvas', 'huddle_summary_id', 'huddle_transcript_file_id')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def title_blocks(case):
    if case == 'plain-empty':
        return []
    require(case == 'blocks-empty', 'Unknown empty transcript case')
    return [dict(type='rich_text', block_id='empty-transcript-title', opaque=dict(keep='block'), elements=[
        dict(type='rich_text_section', opaque=dict(keep='section'), elements=[dict(type='text', text=' \t ',
            style=dict(italic=True), opaque=dict(keep='whitespace'))])])]


def response_text(original, case):
    marker = '"title":"Alex Canvas"'
    require(original.count(marker) == 1, 'Original Canvas scalar is not unique')
    title = ' \t ' if case == 'plain-empty' else 'Misleading plain Canvas title'
    return original.replace(marker, '"title":' + json.dumps(title) + ',"title_blocks":'
                            + json.dumps(title_blocks(case), sort_keys=True, separators=(',', ':')), 1)


def adapted(case):
    """Only fixture inputs/painted transcript label vary; no receipt is rewritten."""
    require(case in CASES, 'Unknown empty transcript case')
    loaded = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_cold_files.py')))
    base = loaded['validate'].__globals__
    canvas = base['CANVAS']
    require(tuple(hashlib.sha256(base['ORIGINAL_TEXT'][id].encode('utf-8')).hexdigest()
                  for id in (canvas, base['LIST'])) == ORIGINAL_HASHES, 'Original singleton byte fixtures changed')
    texts = dict(base['ORIGINAL_TEXT']); texts[canvas] = response_text(texts[canvas], case)
    base.update(ORIGINAL_TEXT=texts, BASE={id: json.loads(text) for id, text in texts.items()})
    original_plan = base['token_plan']

    def plan(success, index):
        value = original_plan(success, index)
        if value is not None and value[0][0] == 'ready':
            value[0] = (value[0][0], 'Untitled', value[0][2], value[0][3])
        return value

    def click(value, id, viewport):
        n = base['N']
        require(value.get('ok') is True and value.get('id') == id
                and value.get('kind') == ('canvas' if id == canvas else 'file') and value.get('ts') == base['TS'],
                'Click lost original transcript occurrence')
        require(value.get('control') == dict(tag='BUTTON', type='button', href=None, disabled=False)
                and type(value.get('defaultPrevented')) is bool, 'Click is not the original enabled button')
        n['measured'](value.get('before'), 'Untitled' if id == canvas else base['F']['LABELS'][id], viewport)
        require(isinstance(value.get('draft'), dict) and isinstance(value['draft'].get('blocks'), list), 'Missing immediate native click draft')
        if value.get('input') is not None:
            require(value['input'].get('ok') is True and value['input'].get('snapshot') == value['draft'], 'Click/input custody changed')

    base['token_plan'] = plan
    base['N']['click'] = click
    return base


def title_source(source):
    raw = source['source']
    effective = {key: copy.deepcopy(raw[key]) for key in TITLE_FIELDS if key in raw}
    effective.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
    return dict(id=source['id'], source=raw, receipt=source['receipt'], effective=effective)


def observed(value, raw_token, source, current, case):
    require(isinstance(value, dict) and value.get('current') is current and value.get('policy') is None,
            'Transcript borrowed policy or wrong current display receipt')
    titles = value.get('titles', {})
    require(titles.get('current') is current and titles.get('tokens') == [raw_token]
            and titles.get('required') == [] and titles.get('users') is None,
            'Transcript title borrowed query/member/draft ownership or changed original token')
    require(titles.get('files') == [title_source(source)], 'Transcript title lost original singleton source/title projection/envelope')
    fid = raw_token['file_id']; is_canvas = raw_token['type'] == 'canvas'
    expected = {}
    if is_canvas:
        expected[fid] = dict(current=current, descriptor=DESCRIPTOR,
            quipTitle=None if case == 'plain-empty' else dict(fileId=fid, current=True, value='nullish'))
    require(titles.get('canvasFallback') == expected, 'Missing/wrong original English/Quip transcript observation')
    if is_canvas:
        require(titles['canvasFallback'][fid].get('current') is current, 'Fallback currentness is not actual boolean')
        if case == 'blocks-empty':
            require(titles['canvasFallback'][fid]['quipTitle'].get('current') is True, 'Quip observation is not current')
    labels = titles.get('labels')
    if current:
        require(isinstance(labels, dict) and len(labels) == 1, 'Missing retained original label map')
        key, label = next(iter(labels.items()))
        require(json.loads(key) == raw_token and label == ('Untitled' if is_canvas else 'Alex List'),
                'Display title replaced original wire token or incorrect branch')
    else:
        require(labels is None, 'Retired transcript retained title authority')


def validate(fixture, receipt, directory, *, case, dimensions=None, png=True):
    base = adapted(case)
    base['validate'](fixture, receipt, directory, expected_case=CASES[case], dimensions=dimensions, png=png)
    received = base['N']['received']()
    tokens = [v for v in received['blocks'][0]['elements'][0]['elements'] if v.get('type') in ('canvas', 'file')]
    require(len(tokens) == 2 and tokens[0]['style'] == {'bold': True} and tokens[1]['style'] == {'italic': True},
            'Original Canvas/List occurrences changed')
    for row in receipt['stages']:
        require(row['proof'].get('emptyCanvas') == dict(case=case, expectedLabel='Untitled'), 'Wrong exact named transcript case')
        model = row['model']; acquisition = model.get('coldAcquisition', {})
        canonical = model['coldFiles']['source']
        sources = {r['id']: r for r in canonical.get('rows', [])}
        attempts = acquisition.get('rows', [])
        for attempt in attempts:
            value = attempt.get('titleUsers')
            ready = attempt.get('current') is True and attempt.get('state') == 'ready'
            if value is None:
                require(not (ready and attempt['id'] == base['CANVAS']), 'Ready empty Canvas lacks original transcript receipt')
                continue
            raw_token = next(t for t in tokens if t['file_id'] == attempt['id'])
            current = ready
            require(attempt.get('ordinal') == tokens.index(raw_token), 'Title receipt borrowed another occurrence ordinal')
            if current:
                require(attempt['id'] in sources, 'Ready transcript has no current canonical singleton')
                observed(value, raw_token, sources[attempt['id']], True, case)
            else:
                # The original controller may retain a completed historical receipt;
                # only current source and current occurrence can authorize display.
                require(value.get('current') is False and value.get('titles', {}).get('current') is False
                        and value.get('titles', {}).get('labels') is None, 'Retired transcript receipt retained authority')
        if row['stage'] in ('cold-loaded', 'canvas-return', 'list-return', 'cold-recovered'):
            require(any(a.get('id') == base['CANVAS'] and a.get('current') is True
                        and a.get('state') == 'ready' and a.get('titleUsers') for a in attempts),
                    'Displayed empty Canvas was not independently acquired by original transcript')
    return receipt


def validator(case):
    require(case in CASES, 'Unknown empty transcript case')
    return lambda fixture, receipt, directory, **options: validate(fixture, receipt, directory, case=case, **options)
