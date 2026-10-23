"""Returned-A current-owner body acquisition, independent of rich reply custody."""
import copy
import canvas_replacement_custody as R
from canvas_special_roots import require, same

PREFIX = 'canvas-return-body'
FIXTURES = R.FIXTURES
CASES = ('success', 'failure')
FILE = 'Ffixture'
THREAD = 'returned-a-thread'
DOCUMENT = 'returned-a-document'
TITLE = 'Returned Project handbook'
CONTENT = 'Current returned-account body. Original rich reply stays saved.'
EXPORT = 'https://files.slack.com/files-tmb/Ffixture/returned-a.html'
HTML = ("<div class='quip-canvas-content'><h1>Returned Project handbook</h1>"
        '<p>Current returned-account body. Original rich reply stays saved.</p>'
        '<p>Read-only acquired body &amp; \u4e2d\U0001f98a</p></div>')
ERROR = 'Canvas state could not refresh. Try again or open it in Slack.'
# Ffixture's initial Reader explicitly parses canvas_fixture.html. This preview
# remains visible before Refresh and after a failed export; it is not a current
# canonical snapshot or a successful body acquisition.
STATIC_BODY = ('\nProject handbook\n'
    'A shared home for decisions, work in progress, and the details that matter.\n'
    'This week\n'
    'Keep the conversation clear, the interface quiet, and the feedback loop short. '
    'Read the design notes before your next review.\n'
    'Agree on the scope and review the first prototypeCheck keyboard navigation and narrow layouts'
    'Collect feedback and publish the next iteration\n'
    'Decisions\n'
    'AreaDecisionNext stepTypographyGeist, compact and readableReview long messages'
    'NavigationPredictable keyboard focusExercise dialogs\n'
    'Make the frequent actions effortless. Keep the rest within reach.\n'
    'Developer notes\ncargo test --locked\ncargo fmt --check\n'
    'Unicode stays intact: caf\u00e9, \u65e5\u672c\u8a9e, \U0001f98a. '
    'Old decision \u2192 revised after review.\n')


def stages(case):
    require(case in CASES, 'Unknown return-body case')
    return (*R.STAGES, 'back-to-canvas', *(('body-failed',) if case == 'failure' else ()),
            'body-ready', 'reply-restored')


def file():
    return dict(id=FILE, mode='quip', filetype='quip', title=TITLE, name=TITLE,
                editable=False, quip_thread_id=THREAD, url_private=EXPORT)


def requests(pair, failed=False):
    """Independent exact original factory forms and original closed response bytes."""
    pb, encoded = R.D.pb, R.D.encoded
    options = pb(1, 'returned-quip-user') + pb(33, 'U0') + pb(34, 'T1')
    payload = pb(3, pb(1, THREAD) + pb(2, 102) + pb(10, DOCUMENT))
    payload += pb(6, pb(1, DOCUMENT) + pb(2, 102) + pb(6, THREAD))
    load = pb(2, pb(1, 1) + pb(2, payload) + pb(3, pb(1, 'Section/' + DOCUMENT)))
    query = pb(1, THREAD) + pb(3, pb(1, 1) + pb(2, THREAD))
    shapes = [
        ('POST', '/api/files.info', {'file': FILE}, {'ok': True, 'file': file()}),
        ('POST', '/api/quip.getControllerInitData', {'_x_reason': 'canvas-controller-init'},
         {'ok': True, 'quip_user_id': 'returned-quip-user', 'init_options': encoded(options)}),
        ('POST', '/canvas/-/load-data', {'request_binary': encoded(query)}, {'base64': encoded(load)}),
        ('POST', '/api/files.info', {'file': FILE}, {'ok': True, 'file': file()}),
        ('GET', EXPORT, {}, {'error': 'fixture_return_body_failed'} if failed else {'html': HTML})]
    return [dict(serial=5*(pair-1)+i+1, pair=pair, target=FILE, method=m, path=p, args=a, response=r)
            for i, (m, p, a, r) in enumerate(shapes)]


def replacement_prefix(receipt, fixture):
    """Run the accepted unchanged seven-stage oracle on its exact prefix."""
    prefix = copy.copy(receipt)
    prefix['stages'] = receipt['stages'][:len(R.STAGES)]
    prefix['layout'] = prefix['stages'][-1]['layout']
    prefix['requestCount'] = 4
    R.validate(prefix, fixture)


def control(value, label, viewport, painted=True, disabled=False):
    require(isinstance(value, dict) and R.integer(value.get('node'), 1)
            and value.get('text') == label and value.get('disabled') is disabled
            and value.get('connected') is True, 'Original body control missing or disabled')
    if painted:
        R.D.visible(value, viewport)


def finish_record(value, original, blocks):
    require(isinstance(value, dict) and R.owner(value) == original and value.get('closed') is True,
            'Returned mount final capture did not close')
    commands, captures, acks = (value.get(k) for k in ('commands', 'captures', 'acks'))
    require(all(isinstance(v, list) and len(v) == 1 for v in (commands, captures, acks)),
            'Final capture/command/ACK inventory differs')
    command, capture, ack = commands[0], captures[0], acks[0]
    require(R.canonical_uuid(command.get('request')) and command == dict(
        kind='finish', scope=original['scope'], mount=original['mount'], request=command['request']),
        'Back did not use the exact original finish command')
    require(R.owner(capture) == original and capture.get('kind') == 'capture'
            and capture.get('request') == command['request'] and capture.get('finalCapture') is True
            and capture.get('detached') is False and capture.get('composing') is False
            and R.integer(capture.get('sequence'), 1) and R.integer(capture.get('revision'))
            and same(capture.get('blocks'), blocks), 'Back final envelope lost original rich content')
    require(ack == dict(kind='ack', scope=original['scope'], mount=original['mount'],
                       sequence=capture['sequence']), 'Original final ACK does not match capture')


def body_source(value, case, pair_count, failed, owner, armed):
    require(isinstance(value, dict) and value.get('case') == case
            and value.get('networkCalls') == 0 and type(value.get('networkCalls')) is int
            and value.get('httpCapability') is False and value.get('armed') is armed
            and value.get('current') is armed, 'Returned body source/current isolation differs')
    if not armed:
        require(value.get('owner') is None and value.get('pairs') == [] and value.get('requests') == [],
                'Body was acquired before explicit returned-A Refresh')
        return
    require(same(value.get('owner'), owner), 'Body reader adopted an old or foreign owner')
    expected_pairs = [dict(serial=n, owner=owner, read=n,
                           status='failed' if case == 'failure' and n == 1 else 'ready')
                      for n in range(1, pair_count+1)]
    expected_requests = [row for n in range(1, pair_count+1)
                         for row in requests(n, case == 'failure' and n == 1)]
    require(same(value.get('pairs'), expected_pairs)
            and same(value.get('requests'), expected_requests),
            'Original current-owner canonical/export pair or raw responses differ')
    presentation = value.get('presentation')
    require(isinstance(presentation, dict) and R.integer(presentation.get('visit'))
            and type(presentation.get('read')) is int and presentation['read'] == pair_count
            and presentation.get('loading') is False
            and presentation.get('stale') is (pair_count == 0 or failed)
            and presentation.get('error') == (ERROR if failed else None)
            and same(presentation.get('snapshot'), None if pair_count == 0 or failed else
                     dict(file=FILE, thread=THREAD, document=DOCUMENT, sections=0)),
            'Whole paired result failed, but canonical authority or readiness was fabricated')


def validate(receipt, fixture=None, case='success'):
    _validate(receipt, fixture, case, False)


def validate_loaded_prefix(receipt, fixture=None):
    """Check the actual first nine stages, without inventing a final reply stage."""
    _validate(receipt, fixture, 'success', True)


def _validate(receipt, fixture, case, prefix_only):
    require(case in CASES, 'Unknown return-body case')
    rows = receipt.get('stages')
    if prefix_only:
        require(isinstance(rows, list) and len(rows) >= 9, 'Loaded body prefix missing')
        rows = rows[:9]
    names = stages(case)[:-1] if prefix_only else stages(case)
    require(isinstance(rows, list) and len(rows) == len(names)
            and tuple(s.get('stage') for s in rows) == names
            and (prefix_only or type(receipt.get('requestCount')) is int and receipt['requestCount'] == 6),
            'Return-body stage/read inventory differs')
    replacement_prefix(receipt, fixture)
    baseline = rows[6]
    runtime = baseline['retainedEntry']['runtime']
    returned = R.owner(runtime['owner'])
    viewport = baseline['layout']['viewport']
    owner = {k: baseline['shell'][k] for k in ('account', 'epoch', 'team', 'user', 'visit')}
    owner['file'] = FILE
    original_bridge = baseline['layout']['bridge']
    for s in rows[:7]:
        body = s.get('returnBody', {})
        body_source(body.get('source'), case, 0, False, owner, False)
        native = body.get('native', {})
        require(native.get('finishOwner') is None and native.get('actions') == [],
                'Returned mount remembered or body action dispatched before remount')
        for record in native.get('captures', []):
            require(record.get('commands') == [] and record.get('captures') == []
                    and record.get('acks') == [], 'Final capture emitted before actual Back')
    finish = None
    for i, s in enumerate(rows[7:], 7):
        name = s['stage']
        restored = name == 'reply-restored'
        failed = name == 'body-failed'
        ready = name in ('body-ready', 'reply-restored')
        pair_count = (2 if case == 'failure' else 1) if ready else 1 if failed else 0
        require(type(s.get('ordinal')) is int and s['ordinal'] == i
                and all(s.get(k) is True for k in ('ok', 'noSession', 'noTransport')),
                'Return-body stage order/isolation differs')
        require(same(s.get('shell'), baseline['shell']) and s.get('originalDestinationCurrent') is False,
                'Body controls replaced returned shell or revived old destination')
        require(same(s.get('successor'), baseline['successor']), 'Body refresh changed independent B')
        retained = s.get('retainedEntry', {})
        actual = retained.get('runtime', {})
        require(same(retained.get('journal'), baseline['retainedEntry']['journal']),
                'Unedited Back/body/reply reopen changed original durable revision or rich draft')
        require(same({k: v for k, v in actual.items() if k not in ('owner', 'nativePresent')},
                     {k: v for k, v in runtime.items() if k not in ('owner', 'nativePresent')})
                and actual.get('nativePresent') is restored,
                'Body load changed retained rich runtime or pending custody')
        if restored:
            fresh = R.owner(actual.get('owner'))
            require(fresh['mount'] != returned['mount']
                    and fresh['mount'] != R.owner(rows[0]['retainedEntry']['runtime']['owner'])['mount']
                    and fresh['scope'] != returned['scope']
                    and fresh['scope'].split(':')[0] == runtime['id']
                    and actual['owner'].get('originalCompletion') is False
                    and actual['owner'].get('failed') is False
                    and R.integer(actual['owner'].get('sequence'), 1),
                    'Restored reply reused a finished owner')
        else:
            require(actual.get('owner') is None, 'Body view retained a live reply lease')
        reads = s.get('reads', {})
        count = 6 if restored else 4
        require(isinstance(reads.get('requests'), list) and len(reads['requests']) == count
                and same(reads['requests'][:4], baseline['reads']['requests'])
                and same(reads.get('responses'), [R.D.response(R.D.CANVAS, thread=n % 2 == 1)
                                                for n in range(count)]),
                'Body loading fabricated or changed original comment reads')
        for n, wire in enumerate(reads['requests']):
            R.request(wire, 'replies' if n % 2 else 'history', R.D.D.ROOT if n % 2 else None)
        layout = s.get('layout', {})
        require(layout.get('ok') is True and layout.get('viewport') == viewport
                and layout.get('canvasPresent') is True
                and same(layout.get('bridge'), original_bridge)
                and same(layout.get('original'), baseline['layout']['original']),
                'Original retired node or closed bridge changed during body work')
        editor = layout.get('editor')
        if restored:
            require(isinstance(editor, dict) and R.owner(editor) == fresh
                    and same(editor.get('blocks'), runtime['blocks'])
                    and editor.get('running') is True and editor.get('composing') is False
                    and editor.get('sameOriginalNode') is False and editor.get('sameOriginalMount') is False,
                    'Restored reply native content or owner differs')
        else:
            require(editor is None, 'Reply editor remained mounted behind returned body')
        body = s.get('returnBody', {})
        body_source(body.get('source'), case, pair_count, failed, owner, True)
        if i > 7:
            require(body['source']['presentation']['visit'] == rows[7]['returnBody']['source']['presentation']['visit'],
                    'Body result changed its captured view generation')
        native = body.get('native', {})
        require(native.get('ok') is True and native.get('viewport') == viewport
                and native.get('commentsOpen') is restored
                and native.get('error') == (ERROR if failed else None)
                and native.get('title') == (TITLE if ready else 'Project handbook')
                and native.get('status') == ([] if failed or ready else
                    ['This canvas changed. Refresh to continue editing.']),
                'Body/error/readiness visible state differs')
        actions = native.get('actions')
        expected = ['back'] + (['refresh'] if pair_count else [])
        if ready and case == 'failure':
            expected.append('retry')
        if restored:
            expected.append('threads')
        require(isinstance(actions, list) and [a.get('action') for a in actions] == expected,
                'Original Back/Refresh/Retry/threads action order differs')
        labels = dict(back='Back to canvas', refresh='Refresh', retry='Retry', threads='Show all threads')
        for a in actions:
            require(a.get('event') == {'type': 'click', 'isTrusted': False},
                    'Actual synthetic original control click evidence missing')
            control(a.get('control'), labels[a['action']], viewport)
            if a['action'] in ('back', 'refresh', 'threads'):
                expected_node = baseline['returnBody']['native']['controls'][a['action']]['node']
            else:
                expected_node = next(x for x in rows if x['stage'] == 'body-failed')['returnBody']['native']['controls']['retry']['node']
            require(a['control']['node'] == expected_node, 'Body action adopted a replacement control')
        if i > 7:
            previous = rows[i-1]['returnBody']['native']['actions']
            require(same(actions[:len(previous)], previous), 'Original action receipt rewritten')
        if name == 'body-ready':
            text = native.get('body')
            require(isinstance(text, str) and all(piece in text for piece in
                    (TITLE, CONTENT, 'Read-only acquired body & \u4e2d\U0001f98a')),
                    'Acquired export body is missing or still stale')
        elif restored:
            require(native.get('body') is None, 'Body rendered through reopened comments')
        else:
            require(native.get('body') == STATIC_BODY,
                    'Original fixture preview changed before successful current paired acquisition')
        controls = native.get('controls', {})
        control(controls.get('refresh'), 'Refresh', viewport, disabled=restored)
        control(controls.get('threads'), 'Show all threads', viewport, disabled=restored)
        if restored:
            control(controls.get('back'), 'Back to canvas', viewport)
        else:
            require(controls.get('back') is None, 'Back control survived finished comments')
        if failed:
            control(controls.get('retry'), 'Retry', viewport)
        else:
            require(controls.get('retry') is None, 'Stale Retry/error survived successful current pair')
        remembered = native.get('finishOwner')
        require(isinstance(remembered, dict) and R.owner(remembered) == returned
                and same(remembered.get('blocks'), runtime['blocks'])
                and all(remembered.get(k) is False for k in ('connected', 'running', 'holderSame')),
                'Original returned editor was not finished and removed')
        records = native.get('captures')
        require(isinstance(records, list) and len(records) == (3 if restored else 2)
                and len({r.get('mount') for r in records}) == len(records),
                'Passive original capture registry was replaced')
        wanted = [r for r in records if r.get('mount') == returned['mount']]
        require(len(wanted) == 1, 'Original returned finish receipt missing')
        finish_record(wanted[0], returned, runtime['blocks'])
        require(wanted[0]['captures'][0]['sequence'] > runtime['owner']['sequence'],
                'Final capture did not advance original snapshot sequence')
        if finish is None:
            finish = wanted[0]
        require(same(wanted[0], finish), 'Original finish/ACK receipt rewritten after body result')
        for record in records:
            if record['mount'] == returned['mount']:
                continue
            require(record.get('commands') == [] and record.get('captures') == [] and record.get('acks') == [],
                    'Another original mount received the finish request')
        current = native.get('currentEditor')
        if restored:
            require(isinstance(current, dict) and R.integer(current.get('node'), 1)
                    and current.get('mount') == fresh['mount'] and current.get('sameFinishedNode') is False
                    and same(current.get('blocks'), runtime['blocks']), 'Returned rich editor is not fresh')
        else:
            require(current is None, 'Body result retained a hidden native editor')
    if not prefix_only:
        require(same(receipt.get('layout'), rows[-1]['layout']), 'Final restored layout differs')
