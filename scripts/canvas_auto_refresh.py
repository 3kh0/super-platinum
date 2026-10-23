"""Actual CanvasDialog debounce, stale-pair refusal, error latch and explicit Retry."""
import copy
import canvas_return_body as B
from canvas_special_roots import require, same

R = B.R
PREFIX = 'canvas-auto-refresh'
FIXTURES, CASES = B.FIXTURES, B.CASES
TITLE = 'Latest Project handbook'
CONTENT = 'Latest automatic body. Original rich reply stays saved.'
OLD_TITLE = 'Superseded Project handbook'
OLD_CONTENT = 'This older automatic body must never replace the current view.'
HTML = ("<div class='quip-canvas-content'><h1>Latest Project handbook</h1>"
        '<p>Latest automatic body. Original rich reply stays saved.</p>'
        '<p>Read-only acquired body &amp; \u4e2d\U0001f98a</p></div>')
OLD_HTML = ("<div class='quip-canvas-content'><h1>Superseded Project handbook</h1>"
            '<p>This older automatic body must never replace the current view.</p></div>')
INITIAL_BODY = B.TITLE+B.CONTENT+'Read-only acquired body & \u4e2d\U0001f98a'
LATEST_BODY = TITLE+CONTENT+'Read-only acquired body & \u4e2d\U0001f98a'


def stages(case):
    require(case in CASES, 'Unknown automatic-refresh case')
    return (*B.stages('success')[:9], 'auto-held', 'newer-event', 'latest-pending',
            *(('auto-failed', 'failure-latched') if case == 'failure' else ()),
            'auto-ready', 'reply-restored')


def requests(serial, case):
    result = B.requests(serial)
    title = OLD_TITLE if serial == 1 else TITLE
    for row in result:
        if row['path'] == '/api/files.info':
            row['response']['file'].update(title=title, name=title)
    result[-1]['response'] = ({'error': 'fixture_auto_refresh_failed'}
                             if case == 'failure' and serial == 2 else
                             {'html': OLD_HTML if serial == 1 else HTML})
    return result


def plan(name, case):
    if name == 'auto-held': return 10, 1, 1, ['pending']
    if name == 'newer-event': return 20, 1, 1, ['pending']
    if name == 'latest-pending': return 20, 2, 2, ['stale-success', 'pending']
    if name in ('auto-failed', 'failure-latched'):
        return (30 if name == 'failure-latched' else 20), 2, None, ['stale-success', 'failed']
    return ((30, 3, None, ['stale-success', 'failed', 'ready']) if case == 'failure'
            else (20, 2, None, ['stale-success', 'ready']))


def millis(value):
    require(R.integer(value), 'Actual scheduler time absent or invalid')
    return value


def ledger(value, case, name, visit):
    require(isinstance(value, dict) and value.get('case') == case
            and value.get('networkCalls') == 0 and type(value.get('networkCalls')) is int
            and value.get('httpCapability') is False, 'Automatic fixture gained transport capability')
    if name in stages(case)[:9]:
        require(value.get('armed') is False and value.get('current') is False
                and value.get('visit') == 0 and value.get('events') == []
                and value.get('scheduling') == [] and value.get('pairs') == []
                and value.get('requests') == [] and value.get('held') is None and value.get('quiet') is None,
                'Automatic reads occurred before original loaded body/event ingress')
        return
    event_count, pair_count, held, statuses = plan(name, case)
    require(value.get('armed') is True and value.get('current') is True and value.get('visit') == visit
            and value.get('held') == held, 'Automatic original owner/held pair differs')
    events = value.get('events')
    require(isinstance(events, list) and len(events) == event_count, 'Event burst count differs')
    for i, event in enumerate(events):
        expected = dict(serial=i+1, team='T1', frame={'type': 'file_change', 'file_id': B.FILE},
                        visit=visit, readBefore=1+i//10, millis=millis(event.get('millis')),
                        accepted=True, ingress='protected-post-authentication')
        require(same(event, expected), 'Original protected file event or read generation differs')
        if i: require(event['millis'] >= events[i-1]['millis'], 'Event time reversed')
    schedule = value.get('scheduling')
    cycles = min(pair_count, 2)
    require(isinstance(schedule, list) and len(schedule) == 2*cycles,
            'Burst did not coalesce or explicit Retry masqueraded as scheduler')
    for i in range(cycles):
        start, claim = schedule[2*i:2*i+2]
        require(start == dict(kind='scheduled', visit=visit, millis=millis(start.get('millis')))
                and claim == dict(kind='claimed', visit=visit, millis=millis(claim.get('millis')),
                                  admitted=True, read=i+2)
                and claim['millis']-start['millis'] >= 249,
                'Actual 250ms component debounce/claim not observed')
        require(start['millis'] >= events[9+10*i]['millis'], 'Read scheduled before its event burst')
    pairs = value.get('pairs')
    require(isinstance(pairs, list) and len(pairs) == pair_count
            and same(value.get('requests'), [r for n in range(1, pair_count+1) for r in requests(n, case)]),
            'Original automatic request/response pair inventory changed')
    for i, (pair, status) in enumerate(zip(pairs, statuses)):
        require(pair.get('serial') == i+1 and pair.get('read') == i+2 and pair.get('visit') == visit
                and pair.get('status') == status and pair.get('readCurrentAtStart') is True,
                'Original automatic pair did not retain its read owner')
        started = millis(pair.get('startedMillis'))
        if i < 2: require(started >= schedule[2*i+1]['millis'], 'Request preceded actual scheduler claim')
        if status == 'pending':
            require(pair.get('releasedMillis') is None
                    and not any(k in pair for k in ('finishedMillis', 'ownerCurrentAtFinish', 'readCurrentAtFinish')),
                    'Held original response settled early')
        else:
            finished = millis(pair.get('finishedMillis'))
            require(finished >= started and pair.get('ownerCurrentAtFinish') is True
                    and pair.get('readCurrentAtFinish') is (i != 0),
                    'Stale original response was treated as fresh or lost owner')
            if i < 2:
                released = millis(pair.get('releasedMillis'))
                require(started <= released <= finished, 'Original release time differs')
            else:
                require(pair.get('releasedMillis') is None, 'Explicit Retry invented a held response')
        if i:
            require(started >= pairs[i-1]['finishedMillis'], 'Automatic paired requests overlapped')
    if pair_count >= 2:
        require(pairs[0]['releasedMillis'] >= events[19]['millis']
                and schedule[2]['millis'] >= pairs[0]['finishedMillis'],
                'Newer event did not supersede the actual held response before followup')
    quiet = value.get('quiet')
    latched = case == 'failure' and name in ('failure-latched', 'auto-ready', 'reply-restored')
    require(isinstance(quiet, dict) if latched else quiet is None, 'Failure quiet-period evidence differs')
    if latched:
        require(millis(quiet.get('elapsedMillis')) >= 750 and same(quiet.get('before'), quiet.get('after')),
                'Failure latch changed while waiting without user action')
        before = quiet['before']
        require(before.get('quiet') is None and before.get('held') is None
                and len(before.get('events', [])) == 30 and len(before.get('pairs', [])) == 2
                and before['pairs'][-1]['status'] == 'failed', 'Quiet witness is not the actual failed pair')
        if name == 'failure-latched':
            require(same({**value, 'quiet': None}, before), 'Quiet period adopted a hidden successor')


def custody(stage, baseline, original, restored):
    require(same(stage.get('shell'), baseline['shell']) and stage.get('originalDestinationCurrent') is False
            and same(stage.get('successor'), baseline['successor']), 'Automatic read changed shell/B/old authority')
    old = baseline['retainedEntry']['runtime'];actual = stage.get('retainedEntry', {}).get('runtime', {})
    require(same(stage['retainedEntry'].get('journal'), baseline['retainedEntry']['journal'])
            and same({k:v for k,v in actual.items() if k not in ('owner','nativePresent')},
                     {k:v for k,v in old.items() if k not in ('owner','nativePresent')})
            and actual.get('nativePresent') is restored, 'Automatic body load consumed or altered original reply')
    layout = stage.get('layout', {})
    require(layout.get('ok') is True and layout.get('viewport') == baseline['layout']['viewport']
            and layout.get('canvasPresent') is True and same(layout.get('bridge'), baseline['layout']['bridge'])
            and same(layout.get('original'), baseline['layout']['original']), 'Retired original node/bridge changed')
    if restored:
        fresh = R.owner(actual.get('owner'))
        require(fresh['scope'].split(':')[0] == old['id'] and fresh['scope'] != original['scope']
                and fresh['mount'] != original['mount'] and fresh['mount'] != layout['original']['mount']
                and actual['owner'].get('failed') is False and actual['owner'].get('originalCompletion') is False
                and R.integer(actual['owner'].get('sequence'), 1), 'Final reply adopted a finished native lease')
        editor = layout.get('editor', {})
        require(R.owner(editor) == fresh and same(editor.get('blocks'), old['blocks'])
                and editor.get('running') is True and editor.get('composing') is False
                and editor.get('sameOriginalNode') is False and editor.get('sameOriginalMount') is False,
                'Final native rich reply differs')
    else:
        require(actual.get('owner') is None and layout.get('editor') is None,
                'A hidden reply editor remained active during automatic body refresh')
    reads = stage.get('reads', {});count = 6 if restored else 4
    require(isinstance(reads.get('requests'), list) and len(reads['requests']) == count
            and same(reads['requests'][:4], baseline['reads']['requests'])
            and same(reads.get('responses'), [R.D.response(R.D.CANVAS, thread=n%2 == 1) for n in range(count)]),
            'Automatic body refresh changed original comment request ledger')
    for n, wire in enumerate(reads['requests']):
        R.request(wire, 'replies' if n%2 else 'history', R.D.D.ROOT if n%2 else None)


def validate(receipt, fixture=None, case='success'):
    rows = receipt.get('stages');names = stages(case)
    require(isinstance(rows, list) and len(rows) == len(names)
            and tuple(s.get('stage') for s in rows) == names
            and type(receipt.get('requestCount')) is int and receipt['requestCount'] == 6,
            'Automatic stage/read inventory differs')
    B.validate_loaded_prefix(receipt, fixture)
    baseline = rows[8];runtime = baseline['retainedEntry']['runtime']
    returned = R.owner(rows[6]['retainedEntry']['runtime']['owner'])
    viewport = baseline['layout']['viewport'];visit = baseline['returnBody']['source']['owner']['visit']
    manual = baseline['returnBody']['source']
    for i, stage in enumerate(rows):
        require(stage.get('ordinal') == i and type(stage.get('ordinal')) is int
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')), 'Automatic stage isolation differs')
        value = stage.get('autoRefresh');name = stage['stage']
        ledger(value, case, name, visit)
        if i < 9: continue
        restored = name == 'reply-restored';ready = name in ('auto-ready','reply-restored')
        failed = name in ('auto-failed','failure-latched')
        pending = name in ('auto-held','newer-event','latest-pending')
        _, pair_count, _, _ = plan(name, case)
        custody(stage, rows[6], returned, restored)
        source = stage['returnBody']['source'];native = stage['returnBody']['native']
        require(same({k:v for k,v in source.items() if k!='presentation'},
                     {k:v for k,v in manual.items() if k!='presentation'}),
                'Automatic query rewrote original explicit acquisition receipts')
        view = source.get('presentation', {})
        require(view.get('visit') == manual['presentation']['visit'] and view.get('read') == pair_count+1
                and view.get('loading') is (name in ('auto-held','latest-pending'))
                and view.get('stale') is (not ready) and view.get('error') == (B.ERROR if failed else None)
                and same(view.get('snapshot'), manual['presentation']['snapshot']),
                'Current paired canonical/readiness state differs')
        expected_status = (['Refreshing canvas state\u2026'] if name in ('auto-held','latest-pending') else
                           ['This canvas changed. Refresh to continue editing.'] if name=='newer-event' else [])
        require(native.get('ok') is True and native.get('viewport') == viewport
                and native.get('commentsOpen') is restored and native.get('title') == (TITLE if ready else B.TITLE)
                and native.get('body') == (None if restored else LATEST_BODY if ready else INITIAL_BODY)
                and native.get('error') == (B.ERROR if failed else None) and native.get('status') == expected_status,
                'Superseded body adopted, duplicate error, or latest body/error not painted')
        controls = native.get('controls', {})
        B.control(controls.get('refresh'), 'Refresh', viewport, disabled=pending or restored)
        B.control(controls.get('threads'), 'Show all threads', viewport, disabled=restored)
        if failed: B.control(controls.get('retry'), 'Retry', viewport)
        else: require(controls.get('retry') is None, 'Error Retry survived success or appeared before failure')
        if restored: B.control(controls.get('back'), 'Back to canvas', viewport)
        else: require(controls.get('back') is None, 'Back remained after original native finish')
        expected_actions = ['back','refresh'] + (['retry'] if ready and case=='failure' else []) + (['threads'] if restored else [])
        actions = native.get('actions')
        require(isinstance(actions,list) and [a.get('action') for a in actions] == expected_actions
                and same(actions[:2], baseline['returnBody']['native']['actions']),
                'Automatic refresh was manually driven or original actions changed')
        for action in actions[2:]:
            kind = action['action'];label = 'Retry' if kind=='retry' else 'Show all threads'
            B.control(action.get('control'), label, viewport)
            require(action.get('event') == {'type':'click','isTrusted':False}, 'Explicit recovery click missing')
            previous = next(s for s in rows if s['stage']==('failure-latched' if kind=='retry' else 'auto-ready'))
            require(action['control']['node'] == previous['returnBody']['native']['controls'][kind]['node'],
                    'Retry/Threads adopted a replacement control')
        previous = rows[i-1]
        require(same(actions[:len(previous['returnBody']['native']['actions'])], previous['returnBody']['native']['actions']),
                'Original action prefix changed')
        require(same(native.get('finishOwner'), baseline['returnBody']['native']['finishOwner']),
                'Finished returned mount changed during automatic refresh')
        captures = native.get('captures')
        initial_captures = baseline['returnBody']['native']['captures']
        require(isinstance(captures,list) and len(captures)==len(initial_captures)+int(restored)
                and same(captures[:len(initial_captures)],initial_captures), 'Original finish command/capture/ACK rewritten')
        current = native.get('currentEditor')
        if restored:
            fresh = R.owner(stage['retainedEntry']['runtime']['owner'])
            require(R.owner(captures[-1])==fresh and captures[-1].get('closed') is False
                    and all(captures[-1].get(k)==[] for k in ('commands','captures','acks'))
                    and isinstance(current,dict) and R.integer(current.get('node'),1)
                    and current.get('mount')==fresh['mount'] and current.get('sameFinishedNode') is False
                    and same(current.get('blocks'),runtime['blocks']), 'Restored rich native reply not fresh')
        else: require(current is None,'Hidden reply editor replaced body controls')
        if i > 9:
            old = previous['autoRefresh']
            require(same(value['events'][:len(old['events'])],old['events'])
                    and same(value['requests'][:len(old['requests'])],old['requests'])
                    and same(value['scheduling'][:len(old['scheduling'])],old['scheduling']),
                    'Original ingress/scheduler/raw receipt prefix rewritten')
            for before, after in zip(old['pairs'],value['pairs']):
                if before['status']!='pending': require(same(before,after),'Settled paired receipt rewritten')
                else:
                    require(all(same(before[k],after.get(k)) for k in ('serial','read','visit','startedMillis','readCurrentAtStart')),
                            'Pending pair replaced original owner or request time')
            if old['quiet'] is not None: require(same(value['quiet'],old['quiet']),'Original quiet receipt rewritten')
    require(same(receipt.get('layout'),rows[-1]['layout']),'Automatic terminal native layout differs')
