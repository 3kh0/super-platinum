"""Opt-in native Share custody of unrelated nonempty channel/thread drafts.

This checks DOM-input evidence and local DraftBook custody, not OS typing or
SQLite durability. The ordinary Share validator owns publication/send receipts.
"""
import math
from pathlib import Path

import workflow_management_confirmation_add as A
import workflow_management_first_publish as F
import workflow_management_thread_keyboard as K
from workflow_management_channel_workflow_details import calculated_geometry
from workflow_management_delete import exact, integer, require

CHANNEL = 'CAPPHOME1'
ROOT = '1781524900.000001'
REPLY = '1781524901.000002'
MAIN = 'Unsent channel review \u4e2d\nKeep caf\u00e9 details \U0001f642.'
THREAD = 'Unsent thread review \u4e2d\nKeep caf\u00e9 replies \U0001f642.'
RAW = [dict(type='message', ts=ROOT, channel=CHANNEL, user='UWORKFLOW',
            text='Original workflow Share discussion.', thread_ts=ROOT,
            reply_count=1, latest_reply=REPLY),
       dict(type='message', ts=REPLY, channel=CHANNEL, user='UWORKFLOW',
            text='Keep this discussion while sharing the workflow.', thread_ts=ROOT)]
FIELDS = {
    'conversations.info': [['channel', CHANNEL], ['return_app_home', 'true'],
                           ['_x_reason', 'app-home-open-api']],
    'apps.about.info': [['app_id', 'A_HOME'], ['_x_reason', 'fetch-unknown-apps-about-info']],
    'apps.profile.get': [['app', 'A_HOME'], ['_x_reason', 'fetch-all-apps-by-id']],
    'experiments.getByUser': [['ssb_platform', 'windows'], ['ssb_release_channel', 'prod']],
}


def obj(v, label):
    require(isinstance(v, dict), 'Share drafts missing ' + label)
    return v


def composer(text):
    # Actual field.rs oninput records a UTF-8 byte-end core selection. The
    # separately observed DOM selection is UTF-16 [7,22], not this core value.
    end = len(text.encode('utf-8'))
    return dict(text=text, selection=dict(start=end, end=end), reply_broadcast=False)


def local(text):
    return dict(composer=composer(text), attachments=[])


def rect(v):
    obj(v, 'rectangle')
    for k in ('left', 'top', 'right', 'bottom', 'width', 'height'):
        require(type(v.get(k)) in (int, float) and math.isfinite(v[k]), 'Draft rectangle scalar')
    require(v['width'] > 0 and v['height'] > 0 and
            abs(v['right'] - v['left'] - v['width']) < .1 and
            abs(v['bottom'] - v['top'] - v['height']) < .1, 'Draft rectangle bounds')


def activation(v, kind, label, viewport, *, node=None, text=None, trusted=False):
    obj(v, 'activation')
    if v.get('kind') == 'trusted-keyboard':
        require(trusted and label in ('Finish Up', 'Messages'), 'Unrequested draft keyboard action')
        K.trusted_action(v, label, node)
        return
    editing = kind in ('input', 'metadata')
    require(v.get('ok') is True and v.get('kind') == ('dom-input' if editing else 'dom-click')
            and v.get('label') == label, 'Draft action path/label')
    identity = integer(v.get('nodeId'), 'Draft action node', 1)
    if node is not None:
        require(identity == node, 'Input replaced original composer')
    require(exact(v.get('event'), dict(type='input' if editing else 'click',
                                     isTrusted=False, targetNodeId=identity)), 'Actual draft DOM event')
    rect(v.get('rect'))
    point = obj(v.get('hitPoint'), 'action hit')
    r = v['rect']; x = point.get('x'); y = point.get('y')
    require(type(x) in (int, float) and type(y) in (int, float) and
            max(0, r['left']) <= x < min(viewport[0], r['right']) and
            max(0, r['top']) <= y < min(viewport[1], r['bottom']), 'Draft action not exposed')
    if 'exposure' in v:
        exposure = obj(v['exposure'], 'action exposure')
        require(exact(exposure.get('hitPoint'), point) and exposure.get('occluded') is False
                and any(b['left']<=x<=b['right'] and b['top']<=y<=b['bottom']
                        for b in exposure.get('regions', [])), 'Actual action used covered background')
    require(v.get('messageId') == (ROOT if kind == 'reply' else None), 'Wrong original Reply row')
    if kind == 'input':
        require(v.get('value') == text and
                (v.get('selectionStart'), v.get('selectionEnd'), v.get('selectionDirection')) ==
                (7, 22, 'forward'), 'Original Unicode input/selection changed')
    elif kind == 'metadata':
        require(v.get('value') == text, 'Actual original metadata input changed')


def control(receipt, row, item, viewport, rectangle):
    """Passive original-thread occlusion is not an actionable hit claim."""
    native = row['proof']['native']
    background = native.get('backgroundThread')
    if background is None:
        require(item.get('visible') is True and item.get('hit') is True, 'Share hidden control')
        rectangle(item['rect'], viewport)
        return
    require(receipt.get('shareDraftCustody') is True and native.get('modal') is None
            and native.get('originalNodes') is True and exact(background, dict(original=True,
            controlsRetained=True, measurement='individual-original-exposed-region-or-owned-thread-occlusion')),
            'Unexpected background observation mode')
    require(item in native.get('controls', []) + native.get('feedback', []), 'Foreign passive item')
    x = obj(item.get('exposure'), 'background exposure')
    r = x.get('rect'); rect(r)
    require(x.get('displayed') is True and x.get('originalThread') is True, 'Background original owner absent')
    pane = native['shareDrafts']['threadPane']['rect']
    require(exact(x.get('occluder'), pane), 'Foreign overlay cannot excuse covered control')
    def box(left, top, right, bottom):
        return dict(left=left, top=top, right=right, bottom=bottom,
                    width=max(0, right-left), height=max(0, bottom-top))
    v = box(max(0,r['left']),max(0,r['top']),min(viewport[0],r['right']),min(viewport[1],r['bottom']))
    require(calculated_geometry(x.get('viewport'), v), 'Background viewport intersection changed')
    left,top=max(v['left'],pane['left']),max(v['top'],pane['top'])
    right,bottom=min(v['right'],pane['right']),min(v['bottom'],pane['bottom'])
    regions=[v] if v['width']>0 and v['height']>0 else []
    if right>left and bottom>top:
        regions=[b for b in [box(v['left'],v['top'],v['right'],top),box(v['left'],bottom,v['right'],v['bottom']),
                 box(v['left'],top,left,bottom),box(right,top,v['right'],bottom)] if b['width']>0 and b['height']>0]
    require(calculated_geometry(x.get('regions'), regions), 'Covered background region fabricated as exposed')
    hit=x.get('hitPoint'); center=x.get('centerHit'); covered=x.get('coveredByThread')
    require(type(center) is bool and type(covered) is bool and x.get('occluded') is (hit is None), 'Background hit witness absent')
    cx,cy=r['left']+r['width']/2,r['top']+r['height']/2
    if center:
        require(any(b['left']<=cx<=b['right'] and b['top']<=cy<=b['bottom'] for b in regions), 'Covered center claimed hit')
    if hit is not None:
        require(any(calculated_geometry(hit,dict(x=b['left']+b['width']*fx,y=b['top']+b['height']*fy))
                    for b in regions for fx in (.5,.1,.9) for fy in (.5,.2,.8)), 'Input hit outside measured exposed region')
    if covered:
        require(not center and 0<=r['left'] and 0<=r['top'] and r['right']<=viewport[0]+1 and r['bottom']<=viewport[1]+1
                and pane['left']<=cx<=pane['right'] and pane['top']<=cy<=pane['bottom'], 'Unrelated/offscreen covering claimed thread')
    require(hit is not None or covered, 'Background control is neither exposed nor owned-thread covered')
    require(type(item.get('hit')) is bool and type(item.get('visible')) is bool, 'Original center observations missing')


def field(v, name, text, *, selected=False):
    obj(v, 'native ' + name)
    require(v.get('id') == name + '-composer' and v.get('connected') is True,
            'Original native composer absent')
    integer(v.get('nodeId'), 'Native composer identity', 1)
    require(v.get('value') == text, 'Native draft text changed')
    require(isinstance(v.get('draftScope'), str) and v['draftScope'] and
            isinstance(v.get('projectionVisit'), str) and v['projectionVisit'], 'Native draft lease absent')
    require(v.get('readonly') is False and v.get('disabled') is False, 'Unrelated composer disabled')
    if selected:
        require((v.get('selectionStart'), v.get('selectionEnd'), v.get('selectionDirection')) ==
                (7, 22, 'forward'), 'Mounted native draft selection changed')


def history(model, baseline=None):
    require(model.get('enabled') is True and model.get('channel') == CHANNEL and
            model.get('root') == ROOT and model.get('reply') == REPLY, 'Draft original source identity')
    require(model.get('historyLoaded') is True and model.get('threadLoaded') is True and
            model.get('historyPending') == [] and model.get('threadPending') == [], 'Original history pending/lost')
    for key, raws in [('history', RAW[:1]), ('thread', RAW)]:
        rows = model.get(key)
        require(isinstance(rows, list) and len(rows) == len(raws), 'Original history inventory')
        for row, raw in zip(rows, raws):
            require(all(exact(row.get(k), value) for k, value in raw.items()), 'Original history content/identity')
        if baseline is not None:
            require(exact(rows, baseline[key]), 'Original typed history changed')
    require(exact(model.get('mainDraftKey'), dict(team='TWORKFLOW', channel=CHANNEL, thread=None)) and
            exact(model.get('threadDraftKey'), dict(team='TWORKFLOW', channel=CHANNEL, thread=ROOT)),
            'Draft key changed original destination')
    require(model.get('mainAttachments') == 0 and model.get('threadAttachments') == 0,
            'Draft attachment state changed')


def observation(v):
    obj(v, 'observation')
    model = obj(v.get('model'), 'model'); native = obj(v.get('native'), 'native')
    require(native.get('ok') is True, 'Native draft observation refused')
    history(model)
    return model, native


def retained(model, baseline, *, destination=False):
    history(model, baseline)
    require(exact(model.get('storedMainDraft'), local(MAIN)) and
            exact(model.get('storedThreadDraft'), local(THREAD)), 'Original DraftBook content/selection lost')
    if destination:
        require(model.get('activeRoot') is None and exact(model.get('composer'), composer('')) and
                exact(model.get('threadComposer'), composer('')), 'Destination consumed/carried unrelated live buffer')
    else:
        require(model.get('activeRoot') == ROOT and model.get('loading') is False and
                exact(model.get('composer'), composer(MAIN)) and
                exact(model.get('threadComposer'), composer(THREAD)), 'Live original drafts changed')


def thread_custody(native, original):
    require(native.get('windowOriginal') is True and native.get('sameThreadNodes') is True and
            native.get('threadOriginalConnected') is True, 'Original thread mount retired during Share')
    require(exact(native.get('original'), original.get('original')), 'Original node witness changed')
    for key in ('thread', 'threadPane', 'threadBody'):
        now = obj(native.get(key), key); old = original[key]
        require(now.get('connected') is True and now.get('nodeId') == old['nodeId'], 'Original thread node replaced')
    field(native['thread'], 'thread', THREAD, selected=True)
    for k in ('draftScope', 'projectionVisit'):
        require(native['thread'][k] == original['thread'][k], 'Original thread lease changed')
    require(exact(native['threadBody'].get('rows'), original['threadBody'].get('rows')), 'Native thread rows changed')


def source(row, previous=None):
    server = row['proof']['server']
    value = obj(server.get('confirmationAdd', {}).get('share', {}).get('drafts'), 'closed source ledger')
    require(value.get('networkCalls') == 0, 'Draft fixture used network')
    seed = obj(value.get('seed'), 'raw history seed')
    require(exact(seed.get('owner'), F.OWNER) and seed.get('channel') == CHANNEL and
            seed.get('root') == ROOT and seed.get('reply') == REPLY and
            seed.get('composerStateSeeded') is False and exact(seed.get('raw'), RAW), 'Raw source fabricated draft/history')
    integer(seed.get('channelGeneration'), 'Original channel generation')
    rows = value.get('about')
    require(isinstance(rows, list) and 1 <= len(rows) <= 12, 'Fresh About acquisition absent/unbounded')
    for index, read in enumerate(rows, 1):
        method = read.get('method')
        require(method in FIELDS and type(read.get('serial')) is int and read['serial'] == index,
                'Unexpected About read/serial')
        require(exact(read.get('owner'), F.OWNER) and read.get('channel') == CHANNEL and
                read.get('channelGeneration') == seed['channelGeneration'] + 2, 'About read owner retired')
        require(exact(read.get('body'), FIELDS[method]) and read.get('retrySafe') is (method == 'conversations.info') and
                read.get('endpoint') == 'https://example.test/api/' + method, 'About exact request factory')
        raw = obj(read.get('raw'), 'About response')
        if method == 'conversations.info':
            require(set(raw) == {'ok', 'channel', 'app_home'} and raw['ok'] is True and
                    isinstance(raw['channel'], dict) and raw['channel'].get('id') == CHANNEL and
                    exact(raw['app_home'], dict(app_id='A_HOME', app_installed_team_id='TWORKFLOW',
                         home_tab_enabled=True, messages_tab_enabled=True, home_view_id='V_HOME')),
                    'Original App Home response')
            # first_publish::seed changes the canonical app DM label after
            # retaining the original boot receipt; the fresh read uses that row.
            boot = dict(server['seed']['boot']['channels'][0], name='Review requests')
            require(all(exact(raw['channel'].get(k), val) for k, val in boot.items()), 'About channel changed boot source')
        else:
            expected = {
                'apps.about.info': dict(ok=True, app_id='A_HOME', name='Review requests', owner_ids=['UWORKFLOW']),
                'apps.profile.get': dict(ok=True, app_profile=dict(id='A_HOME', team_id='TWORKFLOW', is_installed=True, is_workflow_app=True)),
                'experiments.getByUser': dict(ok=True, assignments=dict(agent_about_page=dict(group='on'), flexpane_content_parity=dict(group='on'))),
            }[method]
            require(exact(raw, expected), 'About original response changed')
    if previous is not None:
        require(exact(value['seed'], previous['seed']) and
                exact(rows[:len(previous['about'])], previous['about']), 'About ledger was rewritten')
    return value


def capture(receipt, value, label, fixture, directory, validate_png, png):
    obj(value, label + ' capture')
    require(exact(value.get('before'), value.get('after')), 'Draft screenshot changed observation')
    observation(value['before'])
    path = Path(value.get('path', ''))
    require(path.resolve().parent == Path(directory).resolve() and path.name == f'{fixture}-{label}.png'
            and receipt['shareDrafts']['captures'].get(label) == str(path), 'Draft screenshot path/ownership')
    dimensions = value.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2 and
            all(type(x) is int and x > 0 for x in dimensions), 'Draft physical PNG dimensions')
    stages = receipt.get('stages')
    require(isinstance(stages, list) and stages and all(
        exact(row.get('geometry', {}).get('pngDimensions'), dimensions) for row in stages),
        'Draft PNG dimensions differ from original base window')
    require(value['before']['native'].get('windowOriginal') is True,
            'Draft screenshot belongs to another window')
    if png:
        validate_png(path, dimensions)


def validate_setup(receipt, fixture, captures, viewport, rectangle, validate_png, *, png=True):
    require(receipt.get('shareDraftCustody') is True, 'Explicit draft custody mode')
    evidence = obj(receipt.get('shareDrafts'), 'setup/return')
    require(set(evidence.get('captures', {})) == {'draft-setup', 'draft-returned'}, 'Draft PNG inventory')
    setup = obj(evidence.get('setup'), 'setup')
    states = {k: observation(setup.get(k)) for k in
              ('before', 'messages', 'channelDrafted', 'opened', 'threadDrafted', 'after')}
    baseline = states['before'][0]
    require(exact(baseline.get('composer'), composer('')) and exact(baseline.get('threadComposer'), composer(''))
            and baseline.get('storedMainDraft') is None and baseline.get('storedThreadDraft') is None,
            'Draft buffers were privately seeded')
    for model, _ in states.values():
        history(model, baseline)
    activation(setup.get('messagesActivation'), 'tab', 'Messages', viewport)
    main = states['messages'][1]['channel']; field(main, 'channel', '')
    activation(setup.get('channelInput'), 'input', 'channel-composer', viewport, node=main['nodeId'], text=MAIN)
    field(states['channelDrafted'][1]['channel'], 'channel', MAIN, selected=True)
    require(exact(states['channelDrafted'][0]['composer'], composer(MAIN)) and
            exact(states['channelDrafted'][0]['storedMainDraft'], local(MAIN)), 'Actual main input was not recorded')
    activation(setup.get('replyActivation'), 'reply', 'Reply', viewport)
    opened = states['opened']; field(opened[1]['thread'], 'thread', '')
    require(opened[0]['activeRoot'] == ROOT and opened[0]['loading'] is False, 'Actual Reply destination')
    activation(setup.get('threadInput'), 'input', 'thread-composer', viewport,
               node=opened[1]['thread']['nodeId'], text=THREAD)
    drafted, native = states['threadDrafted']; retained(drafted, baseline)
    field(native['channel'], 'channel', MAIN, selected=True)
    thread_custody(native, native)
    ids = native['original']
    require(ids == dict(channelNodeId=main['nodeId'], threadPaneNodeId=native['threadPane']['nodeId'],
                        threadBodyNodeId=native['threadBody']['nodeId'], threadComposerNodeId=opened[1]['thread']['nodeId']),
            'Original native bindings are not input nodes')
    require([r.get('id') for r in native['threadBody']['rows']] == [ROOT, REPLY], 'Original native thread rows')
    capture(receipt, setup.get('capture'), 'draft-setup', fixture, captures, validate_png, png)
    require(exact(setup['capture']['before'], setup['threadDrafted']), 'Setup screenshot not original drafted state')
    activation(setup.get('aboutActivation'), 'tab', 'About', viewport)
    retained(states['after'][0], baseline); thread_custody(states['after'][1], native)
    require(states['after'][1]['channel'] is None and states['after'][1]['channelOriginalConnected'] is False,
            'About must unmount channel editor without clearing its draft')
    previous = None
    for row in receipt.get('stages', []):
        if row.get('stage') in A.PREFIX:
            _stage(receipt, row)
        previous = source(row, previous)
    if any(r['proof']['native'].get('backgroundThread') is not None for r in receipt['stages']):
        draft = next(r for r in receipt['stages'] if r['stage'] == 'draft')
        inputs = obj(draft['proof']['exercise'].get('draftEdit'), 'original metadata inputs')
        for name, value in [('title', F.TITLE), ('description', F.DESCRIPTION)]:
            activation(inputs.get(name), 'metadata', 'workflow-management-'+name, viewport,
                       node=draft['proof']['native']['fields'][name]['nodeId'], text=value)
        finish = next(r for r in receipt['stages'] if r['stage'] == 'finish')
        button = next(v for v in draft['proof']['native']['controls'] if v['id']=='workflow-management-finish-up')
        activation(finish['proof']['exercise'].get('draftOpen'), 'button', 'Finish Up', viewport,
                   node=button['nodeId'], trusted=fixture=='workflow-editor-narrow')
    require(previous is not None, 'Missing original stage ledger')
    generation = previous['seed']['channelGeneration']
    # Actual Messages then About tab selection each advances channel_generation;
    # thread_scope includes it, whereas the mounted editor lease stays original.
    for key, step in [('before', 0), ('messages', 1), ('channelDrafted', 1),
                      ('opened', 1), ('threadDrafted', 1), ('after', 2)]:
        model = states[key][0]
        thread_generation = integer(model.get('threadGeneration'), 'Thread visit generation')
        root = model.get('activeRoot') or ''
        expected = f"{F.OWNER['epoch']}-TWORKFLOW-{CHANNEL}-{generation + step}-{root}-{thread_generation}"
        require(model.get('scope') == expected, 'Original tab transition scope changed')


def _stage(receipt, row):
    setup = receipt['shareDrafts']['setup']; original = setup['threadDrafted']
    model = obj(row.get('model', {}).get('shareDrafts'), 'stage model')
    native = obj(row.get('proof', {}).get('native', {}).get('shareDrafts'), 'stage native')
    require(native.get('ok') is True, 'Stage native draft probe failed')
    destination = row.get('stage') == 'destination'
    retained(model, original['model'], destination=destination)
    require(native.get('windowOriginal') is True and exact(native.get('original'), original['native']['original']),
            'Stage original window/binding changed')
    if destination:
        require(native.get('thread') is None and native.get('threadPane') is None and
                native.get('threadBody') is None and native.get('sameThreadNodes') is False and
                native.get('threadOriginalConnected') is False, 'Destination retained foreign mounted thread')
        field(native.get('channel'), 'channel', '')
        require(native['channel']['nodeId'] != original['native']['channel']['nodeId'], 'Destination reused original field')
    else:
        thread_custody(native, original['native'])
        require(model.get('threadGeneration') == original['model']['threadGeneration'] and
                model.get('scope') == setup['after']['model']['scope'], 'About/Share replaced original thread scope')
        require(native.get('channel') is None and native.get('channelOriginalConnected') is False,
                'About/Share falsely claims mounted channel editor')


def stage(receipt, row, checked):
    require(checked is not None, 'Missing checked original Share owner')
    _stage(receipt, row)


def validate_return(receipt, fixture, captures, viewport, rectangle, validate_png, *, png=True, after_cancel=None, after_resend=None, after_posting=None):
    returned = obj(receipt['shareDrafts'].get('returned'), 'returned')
    original = receipt['shareDrafts']['setup']['threadDrafted']
    before, before_native = observation(returned.get('before'))
    last = receipt['stages'][-1]
    require(after_resend is None or receipt.get('shareResendCheck') is True and after_cancel is None,'Unrequested Resend return baseline')
    require(after_posting is None or receipt.get('sharePostingCheck') is True and after_cancel is None and after_resend is None,'Unrequested posting return baseline')
    baseline=after_posting if after_posting is not None else after_resend if after_resend is not None else after_cancel if after_cancel is not None else dict(model=last['model'],native=last['proof']['native'])
    require(after_cancel is None or receipt.get('shareCancelCheck') is True,'Unrequested Cancel return baseline')
    require(exact(before, baseline['model']['shareDrafts']) and
            exact(before_native, baseline['native']['shareDrafts']), 'Return does not start at final original stage')
    navigated = last['stage'] == 'destination'
    if navigated:
        activation(returned.get('sidebarActivation'), 'sidebar', 'Review requests', viewport)
        activation(returned.get('replyActivation'), 'reply', 'Reply', viewport)
    else:
        require(returned.get('sidebarActivation') is None and returned.get('replyActivation') is None,
                'Unnecessary private return navigation')
    activation(returned.get('messagesActivation'), 'tab', 'Messages', viewport,
               trusted=fixture=='workflow-editor-narrow')
    after, after_native = observation(returned.get('after'))
    history(after, original['model']); field(after_native.get('channel'), 'channel', MAIN)
    require(exact(after.get('storedMainDraft'), local(MAIN)) and
            exact(after.get('storedThreadDraft'), local(THREAD)), 'Return lost stored drafts')
    restored, native = observation(returned.get('restored'))
    retained(restored, original['model'])
    require(native.get('windowOriginal') is True and exact(native.get('original'), original['native']['original']),
            'Return silently rebound original witness')
    field(native.get('channel'), 'channel', MAIN); field(native.get('thread'), 'thread', THREAD)
    require(native['channel']['nodeId'] != original['native']['channel']['nodeId'] and
            native.get('channelOriginalConnected') is False, 'Channel return did not prove unmount/remount recovery')
    if navigated:
        require(native.get('sameThreadNodes') is False and native.get('threadOriginalConnected') is False and
                native['thread']['nodeId'] != original['native']['thread']['nodeId'] and
                restored['threadGeneration'] > original['model']['threadGeneration'], 'Retired thread was not restored via new mount')
    else:
        thread_custody(native, original['native'])
    require([r.get('id') for r in native['threadBody']['rows']] == [ROOT, REPLY], 'Returned thread lost original messages')
    capture(receipt, returned.get('capture'), 'draft-returned', fixture, captures, validate_png, png)
    require(exact(returned['capture']['before'], returned['restored']), 'Returned screenshot not restored native drafts')
