"""Ordinary-link lifecycle: exact old custody assertions with isolated source/label."""
from pathlib import Path

_path = Path(__file__).with_name('slack_list_empty_canvas_reply.py')
_source = _path.read_text(encoding='utf-8')
_changes = {
    'import slack_list_empty_canvas_reply_source as S': 'import slack_list_link_title_source as S',
    "LIFECYCLE='empty-canvas-lifecycle'": "LIFECYCLE='link-title-lifecycle'",
}
for _before, _after in _changes.items():
    assert _source.count(_before) == 1
    _source = _source.replace(_before, _after, 1)
assert _source.count("'Untitled'") == 3
_source = _source.replace("'Untitled'", 'S.LABEL')
_queued = """            attempts=m.get('titleAcquisition',{}).get('rows',[])
            live=[v for v in attempts if v.get('current') is True and v.get('id')==S.CANVAS]
            require(len(live)==1,'Sent Canvas lacks unique independent transcript receipt')
            S.observed(live[0].get('titleUsers'),[F['token'](S.CANVAS)],json.loads(S.search_text()))
"""
assert _source.count(_queued) == 1
# Nonempty no-user labels use FileTarget's current canonical path. This exact
# branch must not invent the localized async acquisition used by empty titles.
_source = _source.replace(_queued, '            direct_transcript(row)\n', 1)
exec(compile(_source, str(_path), 'exec'), globals())
# No recovery route is admitted by this new runner. Every old guard remains.
STAGES = {LIFECYCLE: STAGES[LIFECYCLE]}
# Exact closed driver: title_users::run sets 7 BEFORE completion::seed, whose
# canonical::fixture_begin accepts Boot then checked_add(1)/events::connected.
# This is not a render default or a socket change on journal reopen.
ACCEPTED_BOOT_SOCKET = 7 + 1


def direct_transcript(row):
    m, p = row['model'], row['proof']
    require(m.get('titleAcquisition') == dict(generation=0, rows=[], running=False),
            'Direct canonical title unexpectedly acquired async transcript authority')
    sources = (p.get('sourceFiles'), m['completionSource']['files']['source'],
               m['coldFiles']['source'])
    require(sources[0] == sources[1] == sources[2],
            'Direct transcript canonical source snapshots diverged')
    for source in sources:
        # Each actual passive observation performs Snapshot::capture; compare
        # exact original source, effective ordered union and response envelope.
        candidate(source, True)
    received = p.get('renderedFiles', {}).get('message')
    require(received == m['composeLedger']['messages'][0],
            'Direct title borrowed another received row')
    tokens = [n for n in R['nodes'](received.get('blocks'))
              if n.get('type') in ('canvas', 'file')]
    require(tokens == [F['token'](S.CANVAS), F['token'](S.LIST)],
            'Direct transcript original occurrence order/raw changed')
    comments = m['comments']
    require(comments.get('active') is True and R['integer'](comments.get('generation'))
            and comments['generation'] > 0
            and comments.get('visibleTs', []).count(received['ts']) == 1,
            'Direct transcript lacks original current received surface')
    original = p.get('beforeReopen', {})
    seed = m['completionSource'].get('seed')
    require(isinstance(seed, dict) and seed.get('accepted') is True
            and seed == original.get('completionSource', {}).get('seed')
            and p.get('runtimeForgotten') is True
            and R['integer'](original.get('comments', {}).get('generation'))
            and comments['generation'] != original['comments']['generation'],
            'Direct transcript lost accepted Boot or actual durable reopen boundary')
    # The former reply job is deliberately retired after Send. Its title may
    # match visually, but it must not be promoted to transcript display owner.
    job = m.get('titleUsers') or {}
    observed = job.get('observed') or {}
    require(job.get('current') is False and observed.get('current') is False,
            'Retired reply job borrowed as transcript owner')
    titles = observed.get('titles', {})
    require(titles.get('current') is False and titles.get('required') == []
            and titles.get('users') is None and observed.get('policy') is None,
            'Direct no-user title borrowed member/policy authority')
    # This is only the old runtime Quip observation, not transcript authority.
    S.fallback(titles.get('canvasFallback', {}).get(S.CANVAS), False, current=False)
    direct = p.get('directTranscript', {})
    before = direct.get('before')
    require(isinstance(before, dict) and before == direct.get('after'),
            'Original direct transcript witness changed across capture')
    require(before.get('current') is True and before.get('sourceCurrent') is True
            and before.get('generation') == comments['generation']
            and before.get('socket') == ACCEPTED_BOOT_SOCKET and before.get('team') == 'T1'
            and before.get('ts') == received['ts'],
            'Direct transcript original source/socket/generation changed')
    rows = before.get('rows')
    require(isinstance(rows, list) and len(rows) == 2,
            'Direct transcript occurrence count changed')
    for ordinal, (actual, raw) in enumerate(zip(rows, tokens)):
        fid = raw['file_id']
        label = S.LABEL if fid == S.CANVAS else 'Alex List'
        require(actual.get('ordinal') == ordinal and actual.get('raw') == raw
                and actual.get('current') is True and actual.get('label') == label,
                'Direct original Token raw/order/currentness changed')
        file = actual.get('file', {})
        require(file.get('current') is True, 'Direct original file source retired')
        S.file_rows([{k: v for k, v in file.items() if k != 'current'}],
                    json.loads(S.search_text()), [fid])
        require(actual.get('quipTitle') == (dict(fileId=fid, current=True, value='nullish')
                if fid == S.CANVAS else None), 'Direct original Quip receipt changed')
    native = direct.get('nativeBefore')
    require(isinstance(native, dict) and native == direct.get('nativeAfter')
            and native.get('ok') is True and native.get('sameRow') is True
            and native.get('ts') == received['ts'],
            'Original enabled transcript DOM changed across capture')
    wanted = [dict(ordinal=i, id=raw['file_id'], kind=raw['type'],
                   label=S.LABEL if raw['file_id'] == S.CANVAS else 'Alex List',
                   tag='BUTTON', connected=True, sameNode=True, disabled=False,
                   state='ready', ariaDisabled='false', ariaBusy='false', title='Open file')
              for i, raw in enumerate(tokens)]
    require(native.get('tokens') == wanted,
            'Direct transcript file node is disabled, replaced or not ready')
