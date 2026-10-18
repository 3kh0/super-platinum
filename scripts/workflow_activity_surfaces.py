"""Closed Thread/Pins execution-entry receipt validation; no process or IO runner."""
from pathlib import Path
import workflow_activity_surface_retirement as retirement

STAGES = {case: ('source', 'opened', 'metadata-draft', 'returned', 'reopened')
          for case in ('execution-thread', 'execution-pins', 'execution-focused')}
STAGES.update(retirement.STAGES)
ROOT = '1781524700.000001'
TS = '1781524800.000001'
THREAD_COMPOSER = 'Uncommitted thread reply \u4e2d\U0001f642'


def validate(fixture, receipt, captures, scenario, h, *, platform, png=True):
    require, rectangle = h['require'], h['rectangle']
    viewport = h['expected_viewport'](fixture, platform)
    focused = scenario == 'execution-focused'
    retiring = scenario in retirement.STAGES
    thread = scenario in ('execution-thread', 'execution-focused', 'execution-thread-retirement')
    require(receipt.get('ok') is True, f"Native surface entry failed: {receipt.get('error')}")
    require(receipt.get('fixture') == fixture and receipt.get('case') == scenario,
            'Wrong surface entry receipt identity')
    require(receipt.get('actualMessageAction') is True and receipt.get('actualActivityEntry') is True
            and receipt.get('aboutConfiguration') is False and receipt.get('noLiveNetwork') is True,
            'Surface entry lacks actual action or isolation')
    require(receipt.get('viewport') == viewport, 'Wrong surface entry viewport')
    rows, paths = receipt.get('stages'), receipt.get('captures')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[scenario]),
            'Missing surface entry stages')
    require(isinstance(paths, dict) and set(paths) == set(STAGES[scenario]), 'Missing surface captures')
    original = action_key = editors_original = dirty_id = None
    previous = []
    counts = (0, 2, 2, 3, 3, 5) if retiring else (0, 2, 2, 2, 4)
    methods = (['workflows.activity.list', 'workflows.activity.get', 'workflows.activity.list',
                'workflows.activity.list', 'workflows.activity.get'] if retiring else
               ['workflows.activity.list', 'workflows.activity.get'] * 2)
    for index, row in enumerate(rows):
        stage = row['stage']
        model, proof, geometry = row.get('model', {}), row.get('proof', {}), row.get('geometry', {})
        source, native = proof.get('source', {}), proof.get('nativeSurface', {})
        metadata, entry, activity, ledger = (model.get(k, {}) for k in ('metadata', 'entry', 'activity', 'ledger'))
        direct = stage not in ('source', 'returned', 'dirty', 'retired')
        detailed = stage in ('opened', 'reopened')
        held = retiring and stage == 'held'
        retired = retiring and stage == 'retired'
        successor = retiring and index >= 4
        require(metadata.get('writes') == metadata.get('networkCalls') == ledger.get('networkCalls') == 0
                and metadata.get('pending') is False
                and (type(metadata.get('busy')) is bool if retired else metadata.get('busy') is held),
                'Surface entry wrote metadata, used network or retained busy state')
        requests = ledger.get('requests')
        h['validate_requests'](requests)
        require(len(requests) == counts[index] and [r['method'] for r in requests] == methods[:counts[index]]
                and requests[:len(previous)] == previous, 'Surface entry replayed or changed request history')
        previous = requests
        require(ledger.get('finished') == counts[index] - int(held)
                and ledger.get('cancelled') == int(successor)
                and (ledger.get('held') == 3 if held else ledger.get('held') is None), 'Surface request did not settle')
        for request in requests:
            args = request['args']
            if request['method'].endswith('.get'):
                require(args.get('workflow_execution_id') == 'E_REVIEW', 'Wrong surface execution GET')
            else:
                require(args.get('filters') == '{}' and args.get('limit') == '100'
                        and 'cursor' not in args and 'parent_workflow_execution_id' not in args,
                        'Surface entry fabricated filters or pagination')
        require(entry.get('active') is direct
                and (type(entry.get('busy')) is bool if retired else entry.get('busy') is held)
                and entry.get('autoPending') is False, 'Wrong surface entry ownership')
        if direct:
            require(entry.get('workflow') == 'WF_REVIEW' and entry.get('execution') == 'E_REVIEW'
                    and type(entry.get('visit')) is int and entry['visit'] > 0,
                    'Wrong direct surface target')
        if detailed:
            require(isinstance(activity, dict) and activity.get('active') is True,
                    'Missing surface Activity')
            event = (activity.get('detail') or {}).get('workflow_event', {})
            require(event.get('workflow_execution_id') == 'E_REVIEW'
                    and event.get('workflow_status') == 'PENDING'
                    and geometry.get('detailStatus') == 'Processing', 'Wrong surface detail/status')
        elif held:
            require(isinstance(activity, dict) and activity.get('active') is True
                    and activity.get('page') is None and activity.get('detail') is None
                    and activity.get('detailRow') is None, 'Held first list fabricated Activity data')
        elif retired:
            require(activity is None, 'Retired source still exposes Activity')
        elif activity is not None:
            require(activity.get('active') is False, 'Activity remained active outside detail')
        if retiring:
            retirement.held_or_reopened(rows, row, require)
        require(source.get('team') == 'TWORKFLOW' and source.get('channel') == 'C2'
                and source.get('ordinaryChannel') is True and source.get('mainView') == ('Activity' if focused else 'Home')
                and source.get('surface') == ('thread' if thread else 'pins')
                and source.get('messagesTab') is thread
                and source.get('aboutPresent') is False and source.get('homePresent') is False
                and source.get('appModal') is False, 'Wrong source surface or fake About/modal')
        if focused:
            require(source.get('detailOpen') is True and source.get('conversationVisible') is False
                    and source.get('selectedSurface') == dict(channel='C2', threadRoot=ROOT),
                    'Focused thread lost actual selected Activity surface or manufactured conversation')
        require(type(source.get('channelGeneration')) is int and type(source.get('threadGeneration')) is int
                and isinstance(source.get('threadScope'), str) and source['threadScope'], 'Missing original source generation')
        require(source.get('source') == [dict(ts=ROOT, blocks=[])], 'Action leaked into timeline fallback')
        if thread:
            require(source.get('threadRoot') == ROOT, 'Wrong canonical thread root')
            require(source.get('threadComposerModel') == THREAD_COMPOSER + '.',
                    'Thread model must retain actual autosize input distinct from native draft')
            bag = source.get('threadSource')
            require(isinstance(bag, list) and len(bag) == 2
                    and bag[0].get('ts') == ROOT and bag[0].get('blocks') == [], 'Missing exact canonical thread bag')
            message = bag[1]
        else:
            require(source.get('threadRoot') is None, 'Pins fabricated an open thread')
            pins = source.get('pins', {})
            require(pins.get('scopeCurrent') is True and pins.get('channel') == 'C2'
                    and pins.get('loaded') is True and type(pins.get('generation')) is int,
                    'Pins source lacks current scoped observation')
            items = pins.get('items')
            require(isinstance(items, list) and len(items) == 1, 'Wrong canonical Pins items')
            item = items[0]
            require(item.get('key') == 'message:' + TS and item.get('kind') == 'message'
                    and item.get('channel') == 'C2', 'Wrong Pins item identity')
            message = item.get('message', {})
        h['validate_execution_source']([message])
        require(message.get('channel') == 'C2' and message.get('thread_ts') == ROOT,
                'Reply action lost canonical channel/root')
        stable = {k: v for k, v in source.items() if k != 'documents'}
        if retired:
            retirement.transition(rows, row, thread, require)
            original = stable
            action_key = None
            editors_original = None
        if original is None:
            original = stable
        require(stable == original, 'Canonical source, generations or model drafts changed')
        if index >= 2:
            documents = source.get('documents', [])
            matching = [d for d in documents if d.get('title') == h['TITLE']
                        and d.get('description') == h['DESCRIPTION'] and d.get('dirty') is True]
            require(len(matching) == 1 and type(matching[0].get('id')) is int,
                    'Exact dirty surface document lost')
            if dirty_id is None:
                dirty_id = matching[0]['id']
            require(matching[0]['id'] == dirty_id, 'Dirty surface document replaced')
            if direct:
                require(metadata.get('title') == h['TITLE'] and metadata.get('description') == h['DESCRIPTION']
                        and metadata.get('dirty') is True, 'Reopened surface lost metadata draft')
        require(all(native.get(k) is True for k in ('ok', 'samePanel', 'sameBody', 'sameAction'))
                and native.get('sameCard') is (None if thread else True)
                and native.get('actionCount') == 1, 'Original surface panel/body/action/card replaced')
        require(isinstance(native.get('actionKey'), str) and native['actionKey'], 'Missing canonical action key')
        if action_key is None:
            action_key = native['actionKey']
        require(native['actionKey'] == action_key, 'Source action key changed')
        require(native.get('visible') is (not direct)
                and native.get('display') == ('none' if thread and direct else 'flex'),
                'Original surface hidden or exposed at wrong stage')
        header = native.get('header', {})
        require(header.get('sameNode') is True and header.get('display') == ('none' if not thread and direct else 'flex')
                and header.get('text') == ('Thread' if thread else '#ship')
                and header.get('channelLabel') == ('#ship' if thread else None)
                and header.get('visible') is (not direct), 'Original surface header not restored')
        if not direct:
            rectangle(header.get('rect'), viewport)
        editors = native.get('editors')
        require(native.get('channelComposerCount') == int(thread and not focused)
                and native.get('threadComposerCount') == int(thread)
                and isinstance(editors, list) and len(editors) == (1 if focused else 2 if thread else 0),
                'Wrong native composer count for source surface')
        expected = ({'thread-composer': THREAD_COMPOSER} if focused else
                    {'channel-composer': retirement.SUCCESSOR_CHANNEL if successor else h['EXECUTION_COMPOSER'],
                     'thread-composer': retirement.SUCCESSOR_THREAD if successor else THREAD_COMPOSER} if thread else {})
        require({e.get('id') for e in editors} == set(expected), 'Missing or duplicate surface editor')
        for editor in editors:
            require(editor.get('sameNode') is True and editor.get('value') == expected[editor['id']]
                    and editor.get('start') == 3 and editor.get('end') == 10
                    and editor.get('visible') is (not direct)
                    and isinstance(editor.get('visit'), str) and editor['visit']
                    and isinstance(editor.get('scope'), str) and editor['scope'], 'Surface native draft/selection lost')
            if editor['id'] == 'thread-composer':
                if not direct:
                    client, scroll = editor.get('clientHeight'), editor.get('scrollHeight')
                    require(type(client) in (int, float) and type(scroll) in (int, float)
                            and 0 < client < float('inf') and 0 < scroll < float('inf')
                            and scroll <= client + 1 and editor.get('fits') is True,
                            'Visible thread draft is clipped or lacks measured autosize bounds')
                else:
                    require(editor.get('fits') is False, 'Hidden thread editor falsely claims fit')
        witnesses = {e['id']: (e['visit'], e['scope']) for e in editors}
        if editors_original is None:
            editors_original = witnesses
        require(witnesses == editors_original, 'Surface editor remounted')
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True
                and geometry.get('viewport') == viewport and geometry.get('activity') is (detailed or held)
                and geometry.get('detail') is detailed, 'Surface geometry/route failed')
        controls = geometry.get('controls', [])
        require(controls, 'No measured surface controls')
        for item in controls + geometry.get('notices', []):
            require(item.get('visible') is True, 'Surface control/notice invisible')
            rectangle(item.get('rect'), viewport)
        dimensions = geometry.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2, 'Missing surface PNG dimensions')
        path = captures / f'{fixture}-{stage}.png'
        require(Path(paths[stage]) == path, 'Foreign surface capture path')
        if png:
            h['validate_png'](path, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Surface terminal model changed')
    return receipt
