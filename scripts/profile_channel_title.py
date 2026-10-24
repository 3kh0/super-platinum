"""Closed known-public Canvas channel-title evidence; no live server inference."""
import json
from pathlib import Path
import sys

ACCOUNT = '00000000-0000-4000-8000-000000000081'
FOREIGN = '00000000-0000-4000-8000-000000000082'
TEAM, SELF, CHANNEL = 'TINIT0001', 'UINIT0001', 'CMENTION01'
FILE, URL = 'FTITLE0001', 'https://fixture.slack.com/docs/FTITLE0001'
OLD, NEW = 'project-harbor', 'recovered-harbor'
STAGES = {
    'known': ('cold-loading', 'opened', 'initial-settled', 'refresh-held', 'renamed-mounted', 'used', 'confirmed'),
    'failure': ('cold-loading', 'opened', 'initial-settled', 'refresh-held', 'failure-retained', 'retry-held', 'renamed-mounted', 'used', 'confirmed'),
    'cancel': ('cold-loading', 'file-cancelled', 'opened', 'channel-cancelled', 'reopened'),
    'retirement': ('cold-loading', 'opened', 'initial-settled', 'refresh-held', 'pre-mount-retired', 'socket-retired', 'account-replaced'),
}
COUNTS = {'known': (0, 1, 1, 2, 2, 2, 2), 'failure': (0, 1, 1, 2, 3, 4, 4, 4, 4),
          'cancel': (0, 0, 1, 1, 1), 'retirement': (0, 1, 1, 2, 2, 4, 5)}
HIDDEN = {'cold-loading', 'file-cancelled', 'channel-cancelled', 'pre-mount-retired', 'used', 'account-replaced'}
HELD = {'opened', 'refresh-held', 'retry-held'}
FIXTURES = ('profile-sections-mentions', 'profile-sections-mentions-narrow')
LOADING = 'Loading document titles. Your description is unchanged.'
FAILURE = 'Channel metadata could not refresh.'
CHANNEL_PRESENTATION = dict(locale='en-US', namespace='channel_token', source='private-channel',
                            value='private-channel', origin='application-presentation')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def same(a, b):
    return json.dumps(a, sort_keys=True, separators=(',', ':')) == json.dumps(b, sort_keys=True, separators=(',', ':'))


def channel(name=OLD, version=101):
    return dict(id=CHANNEL, name=name, updated=version, is_channel=True, is_member=True,
                fixture_metadata=dict(retained=True))


def projected_channel(name=OLD, version=101):
    """Exact ordinary canonical projection; raw seed/replies stay unexpanded.

    directory_channels/projection.rs merged_before_names/process/internal_names
    adds these fields for this public, member, empty-topic fixture row.
    """
    return dict(channel(name, version), _name_lc=name,
        connectedLimitedTeamIds=[], connectedTeamIds=[], internalTeamIds=[],
        fromAnotherTeam=False, has_fetched_history_after_scrollback=False,
        history_is_being_fetched=False, isNonExistent=False, isUnknown=False,
        is_general=False, is_group=False, is_im=False, is_mpim=False,
        is_org_default=False, is_org_mandatory=False, purpose={}, topic={},
        scroll_top=-1, unread_highlights=[], unreads=[])


def title_blocks():
    return [dict(type='rich_text', opaque=dict(keep=1), elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Notes for '),
        dict(type='channel', channel_id=CHANNEL, style=dict(bold=True), opaque=dict(keep=2))])])]


def raw_file():
    return dict(ok=True, file=dict(id=FILE, mode='quip', filetype='quip', created=101, updated=101,
        title='Misleading Canvas fallback', name='wrong-name-fallback', fixture_opaque=dict(preserve=[FILE, 1, 2]),
        title_blocks=title_blocks()))


def blocks(prefix='Working with '):
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=prefix),
        *[dict(type='canvas', file_id=FILE, text=URL, url=URL, fixture_occurrence=i,
               style={style: True}, opaque=dict(keep=[i, 'original'])) for i, style in ((1, 'bold'), (2, 'italic'))]])])]


def seed():
    user = dict(id=SELF, name='fixture-self', team_id=TEAM, profile={})
    team = dict(id=TEAM, name='Channel title')
    return dict(account=ACCOUNT, team=TEAM, user=SELF,
        boot=dict(ok=True, team=team, self=user, users=[user], workspaces=[dict(id=TEAM)],
                  channels=[dict(id='CINIT0001', name='warm-harbor', is_channel=True, is_member=True), channel()], prefs={}),
        init=dict(ok=True, self=user, team=team, workspaces=[dict(id=TEAM)],
                  onboarding=dict(user_data=dict(onboarding_complete=True)), user_date_created=123))


def current_row(stage):
    if stage == 'account-replaced':
        return None
    return projected_channel(NEW, 102) if stage in ('renamed-mounted', 'used', 'confirmed', 'channel-cancelled', 'reopened', 'pre-mount-retired', 'socket-retired') else projected_channel()


def file_ledger(case, stage):
    cold = stage == 'cold-loading'
    cancelled = case == 'cancel' and not cold
    count = 2 if (case == 'cancel' and stage not in ('cold-loading', 'file-cancelled')) or stage == 'account-replaced' else 1
    rows = [dict(serial=i, method='files.info', args=dict(file=FILE, page='1', count='500', truncate='true')) for i in range(1, count+1)]
    if not cold:
        rows[0]['releasedResponse'] = raw_file()
        if not cancelled:
            rows[0]['response'] = dict(raw=raw_file())
        if count == 2:
            rows[1]['response'] = dict(raw=raw_file())
    return dict(requests=rows, completed=0 if cold else count, cancelled=int(cancelled), held=cold, active=int(cold), networkCalls=0)


def validate_cache(value, stage):
    if stage in ('cold-loading', 'file-cancelled', 'account-replaced'):
        require(value is None, 'Cold/cancelled/foreign file cache must not grant authority')
        return
    require(isinstance(value, dict), 'Missing exact file observation')
    raw = raw_file()['file']; effective = {k: v for k, v in raw.items() if k != 'fixture_opaque'}
    effective.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
    require(same(value.get('source'), raw) and same(value.get('effective'), effective), 'Raw file/title leaf/style/opaque changed')
    current = stage != 'socket-retired'
    require(value.get('current') is current, 'File socket/currentness proof changed')
    require(value.get('titleUsers') is None, 'Channel dependency borrowed user authority')
    require(value.get('quipTitle') == dict(current=True, value='nullish', fileId=FILE), 'Channel-only title bypassed Quip observation')
    expected = dict(ids=[CHANNEL], rows=[current_row(stage)], presentation=CHANNEL_PRESENTATION) if current else None
    require(same(value.get('titleChannels'), expected), 'Nested current canonical channel dependency lost or substituted')


def validate_source(source, case, stage, count, epoch):
    require(isinstance(source, dict) and source.get('case') == case and same(source.get('seed'), seed()), 'Wrong exact seed or title case')
    require(same(source.get('files'), file_ledger(case, stage)), 'File request/cancel/reuse ledger changed')
    require(source.get('networkCalls') == 0, 'Fixture performed network work')
    retired = stage == 'account-replaced'
    require(source.get('account') == (FOREIGN if retired else ACCOUNT)
            and source.get('epoch') == epoch + int(retired), 'Nested source escaped account epoch')
    row = current_row(stage)
    require(same(source.get('channel'), row) and source.get('nativeName') == (row['name'] if row else None), 'Canonical/native channel history changed')
    if not retired:
        require(source.get('socketGeneration') == (2 if stage == 'socket-retired' else 1), 'Unexpected source socket')
    status = source.get('conditional', {})
    require(status.get('requestCount') == count and status.get('held') == int(stage in HELD)
            and status.get('drainHeld') is False, 'Conditional count/hold changed')
    require(source.get('idle') is (stage not in HELD), 'Unsettled conditional work or false idle')
    if not retired:
        require(status.get('installed') is True and status.get('listenerReady') is True
                and status.get('sourcePaused') is False and status.get('paused') is False, 'Shared manager source was not ready')
        require(all(q.get('pending') is False for q in status.get('queues', [])), 'Unpublished queue exposed as canonical')


def validate_requests(requests, case, epoch):
    expected_count = COUNTS[case][-1]
    require(isinstance(requests, list) and len(requests) == expected_count, 'Wrong conditional request count')
    for index, request in enumerate(requests):
        serial = index + 1
        method = 'conversations.genericInfo' if case == 'failure' and serial == 3 else 'channels.info'
        row = projected_channel(NEW, 102) if case == 'retirement' and serial >= 3 else projected_channel()
        require(request.get('serial') == serial and request.get('method') == method
                and request.get('ids') == [CHANNEL] and request.get('versions') == {CHANNEL: row['updated']}, 'Wrong nested conditional factory/version/replay')
        before = request.get('predecessor', {})
        require(before.get('account') == ACCOUNT and before.get('epoch') == epoch
                and before.get('socketGeneration') == (2 if case == 'retirement' and serial >= 4 else 1)
                and same(before.get('channel'), row) and before.get('nativeName') == row['name'], 'Wrong conditional source predecessor')
        require(same(before.get('seed'), seed()) and before.get('networkCalls') == 0, 'Conditional request lost source scope')
        if case == 'failure' and serial in (2, 3):
            expected = dict(error='closed_failure')
        else:
            changed = (case in ('known', 'retirement') and serial == 2) or case == 'cancel' or (case == 'failure' and serial == 4)
            name, version = ('retired-socket-name', 999) if case == 'retirement' and serial == 3 else ('retired-account-name', 1000) if case == 'retirement' and serial == 5 else (NEW, 102)
            changed = changed or (case == 'retirement' and serial in (3, 5))
            expected = dict(raw=dict(results=[channel(name, version)] if changed else [], member_channels=[CHANNEL] if changed else []))
        require(same(request.get('response'), expected), 'Exact shared response/raw failure changed')


def geometry(g, viewport, stage, h, case):
    retired = stage == 'account-replaced'; rich = stage not in HIDDEN
    require(isinstance(g, dict) and g.get('ok') is True and g.get('noOverflow') is True
            and g.get('viewport') == viewport and g.get('dialog') is (not retired), 'Wrong measured dialog/layout')
    if not retired: h['rectangle'](g.get('dialogRect'), viewport)
    for key in ('controls', 'notices', 'tokens'):
        require(isinstance(g.get(key), list), 'Missing measured inventory')
        for item in g[key]:
            require(item.get('visible') is True, 'Required token/control/feedback hidden')
            h['rectangle'](item.get('rect'), viewport)
            for fragment in item.get('fragments', []): h['rectangle'](fragment, viewport)
    controls = [] if retired else ['Back', 'Save section']
    if rich: controls += ['Use description', 'Cancel description edit']
    if stage == 'cold-loading': controls += ['Retry description', 'Cancel description edit']
    require(sorted(c.get('text') for c in g['controls']) == sorted(controls), 'Wrong original action inventory')
    require(all(c.get('disabled') is ((c['text'] == 'Save section' and stage != 'used') or c['text'] == 'Retry description') for c in g['controls']), 'Wrong action disabled state')
    required = LOADING if stage == 'cold-loading' else FAILURE if stage == 'failure-retained' else 'Profile fields saved.' if stage == 'confirmed' else None
    if required: require(required in [n.get('text') for n in g['notices']], 'Required scoped feedback missing')
    require(g.get('richEditable') is (True if rich else None), 'Wrong editor visibility')
    if not rich:
        require(g['tokens'] == [] and g.get('captured') is None, 'Hidden editor retained token custody')
        return
    label = 'Notes for #' + (NEW if stage in ('confirmed', 'reopened', 'socket-retired') else OLD)
    require([t.get('text') for t in g['tokens']] == [label, label] and all(t.get('editable') is False for t in g['tokens']), 'Wrong mounted/adopted label')
    require([(t.get('fontWeight'), t.get('fontStyle')) for t in g['tokens']] == [('700', 'normal'), ('400', 'italic')], 'Outer token style changed')
    captured = g.get('captured', {}); snap = captured.get('snapshot', {})
    prefix = 'Saved with ' if stage == 'confirmed' else 'Retained draft with ' if stage == 'failure-retained' or (case == 'failure' and stage in ('retry-held', 'renamed-mounted')) else 'Working with '
    require(same(snap.get('blocks'), blocks(prefix)) and snap.get('composing') is False
            and type(snap.get('revision')) is int and snap['revision'] >= 0
            and isinstance(snap.get('generation'), str) and snap['generation'].isdigit(), 'Native raw capture/generation changed')
    require(same(captured.get('tokens'), [dict(value=t, label=label) for t in blocks()[0]['elements'][0]['elements'][1:]]), 'Token witness id/URL/opaque/style changed')


def exercise(value, stage, count, epoch):
    expected = {'cold-loading': dict(cold=True), 'file-cancelled': dict(release=dict(delivered=False, cancelledBeforeReply=True)),
        'opened': dict(conditional=count), 'initial-settled': dict(initialConditionalRead=True),
        'refresh-held': dict(conditional=count), 'retry-held': dict(conditional=count),
        'failure-retained': dict(nestedFeedback=True, draftRetained=True), 'renamed-mounted': dict(mountedLabelRetained=True),
        'used': dict(rawTokensRetained=True), 'confirmed': dict(confirmedSave=True, newOpeningLabel=True),
        'channel-cancelled': dict(lateSharedPublication=True, retiredEditorNotResumed=True), 'reopened': dict(newOpening=True),
        'pre-mount-retired': dict(oldOpeningRefused=True),
        'socket-retired': dict(socket=dict(before=1, after=2), latePublicationRefused=True, successor=4),
        'account-replaced': dict(oldOpeningRefused=True, oldEditorHidden=True, previousAccount=ACCOUNT, account=FOREIGN,
            previousEpoch=epoch, epoch=epoch+1, previousAttempts=[], previousBlocks=blocks())}[stage]
    require(same(value, expected), 'Missing actual source/interaction/retirement proof')


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    require(case in STAGES and fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture') == fixture
            and receipt.get('case') == case and receipt.get('channelTitle') is True, 'Wrong title case/receipt owner')
    require(receipt.get('mentionKind') == 'canvas' and receipt.get('titleBlocks') is True
            and receipt.get('captureChecksPassed') is True and receipt.get('visualReview') == 'Original PNG inspection required separately', 'Missing closed capture boundary')
    rows = receipt.get('stages'); paths = receipt.get('captures'); viewport = h['expected_viewport'](fixture, platform)
    require(receipt.get('viewport') == viewport and isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[case])
            and isinstance(paths, dict) and set(paths) == set(STAGES[case]), 'Wrong stage/capture inventory')
    epoch = rows[0]['model'].get('epoch'); require(type(epoch) is int, 'Missing owner epoch')
    validate_requests(receipt.get('requests'), case, epoch)
    previous = None
    for index, row in enumerate(rows):
        stage = row['stage']; m = row['model']; proof = row['proof']; g = row['geometry']; saved = stage == 'confirmed'
        if stage == 'account-replaced':
            require(m == dict(enabled=False), 'Retired Profile model survived')
        else:
            require(m.get('enabled') is True and m.get('account') == ACCOUNT and m.get('epoch') == epoch
                    and all(type(m.get(k)) is int for k in ('visit', 'serial')), 'Wrong Profile visit/account')
            if previous:
                require(m['visit'] == previous['visit'] and ((m['serial'] != previous['serial']) if saved else m['serial'] == previous['serial']), 'Unexpected section serial/visit change')
            require(same(m.get('blocks'), blocks('Saved with ' if stage in ('used', 'confirmed') else 'Retained draft with ' if case == 'failure' and stage in ('retry-held', 'renamed-mounted') else 'Working with '))
                    and m.get('busy') is False and m.get('uncertain') is False and m.get('canSave') is (stage == 'used' or (case == 'failure' and stage in ('retry-held', 'renamed-mounted'))), 'Form draft or write state changed')
            require(m.get('managedValue') == 'Preserved organization value' and m.get('choiceValue') == 'Morning' and m.get('countryValue') == 'US', 'Untouched sibling changed')
            require(m.get('reads') == int(saved) and isinstance(m.get('attempts'), list) and len(m['attempts']) == int(saved), 'Unexpected Save/readback replay')
            if saved:
                attempt = m['attempts'][0]; values = {v.get('element_id'): v for v in attempt}
                require(len(values) == len(attempt) and {'long', 'locked'} <= set(values)
                        and same(json.loads(values['long']['long_text']['blocks']), blocks('Saved with '))
                        and values['locked'].get('text', {}).get('text') == 'Preserved organization value', 'Raw saved token/source custody changed')
        count = COUNTS[case][index]
        validate_source(proof.get('source'), case, stage, count, epoch)
        validate_cache(proof.get('cache'), stage)
        exercise(proof.get('exercise'), stage, count, epoch)
        geometry(g, viewport, stage, h, case)
        dimensions = g.get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2 and all(type(n) is int and n > 0 for n in dimensions), 'Missing PNG dimensions')
        path = Path(captures).resolve() / f'{fixture}-{stage}.png'
        require(isinstance(paths[stage], str) and Path(paths[stage]) == path, 'Foreign capture path')
        if png:
            require(path.is_file() and not path.is_symlink(), 'Missing original PNG')
            h['validate_png'](path, dimensions)
        previous = m
    return receipt
