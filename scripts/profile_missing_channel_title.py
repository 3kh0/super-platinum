"""Source-bound missing selector evidence; terminal markers are not server absence."""
import copy
from pathlib import Path
import runpy
import sys

import profile_channel_title as public

STAGES = {
    'positive': ('cold-loading', 'opened', 'settled-mounted', 'used', 'confirmed'),
    'omission': ('cold-loading', 'opened', 'settled-mounted', 'used', 'confirmed'),
    'failure': ('cold-loading', 'opened', 'failure-retained', 'used', 'confirmed'),
    'cancel': ('cold-loading', 'file-cancelled', 'opened', 'channel-cancelled', 'reopened'),
    'retirement': ('cold-loading', 'opened', 'pre-mount-retired', 'socket-retired', 'account-replaced'),
}
COUNTS = {'positive': (0, 1, 1, 1, 2), 'omission': (0, 1, 1, 1, 2),
          'failure': (0, 1, 2, 2, 3), 'cancel': (0, 0, 1, 1, 2),
          'retirement': (0, 1, 1, 2, 3)}
UNKNOWN = 'Notes for #unknown-channel'
FOUND = 'Notes for #fetched-harbor'
TERMINAL = 'Notes for #private-channel'
PROVENANCE = 'sealed-shared-channel-selector'
OBSERVED_PRESENTATION = dict(locale='en-US', namespace='channel_token', unknown='unknown-channel',
                             nonexistent='private-channel', origin='application-presentation')
require, same = public.require, public.same


def seed():
    value = public.seed()
    value['boot']['channels'] = value['boot']['channels'][:1]
    return value


def terminal_row():
    # Exact projection::process output for the sealed manager's raw marker.
    # This shape is display data; the separate sealed lookup supplies authority.
    row = public.projected_channel()
    for key in ('name', 'updated', 'fixture_metadata', 'is_org_default', 'is_org_mandatory', 'isUnknown'):
        del row[key]
    row.update(is_channel=False, is_member=False, isNonExistent=True, _name_lc='')
    return row


def replacement_snapshot(epoch):
    # Exact fixture::install full Boot ingress before active-account replacement.
    # Saved identity stays account81; it is data, not authority for account82.
    raw = dict(id='CINIT0001', name='warm-harbor', is_channel=True, is_member=True)
    native = dict(raw, is_group=False, is_im=False, is_mpim=False, is_private=False,
        is_archived=False, is_starred=False, is_ext_shared=False, connected_teams=[],
        updated=None, user=None, unread_count=None, unread_count_display=None,
        mention_count=None, has_unreads=False, last_read=None, previous_names=[])
    saved = dict(version=1, identity=dict(account=public.ACCOUNT, team=public.TEAM, user=public.SELF),
        provenance='EmptyNativeIngress', records=[], invalidated=[], overflowed=False,
        pending=[dict(origin='Boot', rows=[raw], boot_in_flight=True, receipt_facts=None)])
    workspace = dict(team=public.TEAM, user=public.SELF, generation=0, pending=True,
                     saved=saved, native={'CINIT0001': native})
    return dict(account=public.FOREIGN, epoch=epoch+1, team=public.TEAM,
                targetAbsent=True, workspaces={public.TEAM: workspace})


def current_row(case, stage):
    if stage in ('cold-loading', 'file-cancelled', 'opened', 'pre-mount-retired', 'socket-retired', 'account-replaced'):
        return None
    return terminal_row() if case in ('omission', 'failure') else public.projected_channel('fetched-harbor', 102)


def selected(kind, *, current=False):
    value = dict(kind=kind, transient=kind == 'Unknown', provenance=PROVENANCE)
    if current:
        value['current'] = True
    return value


def file_ledger(case, stage):
    result = public.file_ledger(case, stage)
    if case == 'retirement' and stage in ('socket-retired', 'account-replaced'):
        count = 2 if stage == 'socket-retired' else 3
        result = dict(requests=[dict(serial=i, method='files.info', args=dict(file=public.FILE,
            page='1', count='500', truncate='true'), response=dict(raw=public.raw_file()))
            for i in range(1, count + 1)], completed=count, cancelled=0, held=False, active=0, networkCalls=0)
        result['requests'][0]['releasedResponse'] = public.raw_file()
    return result


def selector(case, stage):
    if stage == 'opened':
        return selected('Unknown', current=True)
    # The fresh terminal opening is certified in its conditional request
    # predecessor. That request's owned publication retires the Selection;
    # diagnostics after settlement must refuse it while Catalog stays mounted.
    return None


def source(value, case, stage, count, epoch, *, request=False, conditional=False, request_serial=0):
    require(isinstance(value, dict) and value.get('case') == case and same(value.get('seed'), seed()),
            'Wrong missing-source seed/case')
    require(value.get('networkCalls') == 0, 'Missing selector made live network work')
    retired = stage == 'account-replaced'
    require(value.get('account') == (public.FOREIGN if retired else public.ACCOUNT)
            and value.get('epoch') == epoch + int(retired), 'Missing selector escaped account epoch')
    if retired:
        require(value.get('socketGeneration') == 0, 'Replacement socket changed')
    if not retired:
        socket = 3 if stage == 'socket-retired' else 2 if stage == 'pre-mount-retired' else 1
        if request and case == 'retirement': socket = request_serial
        require(value.get('socketGeneration') == socket, 'Missing selector socket owner changed')
    file_stage = stage
    if request and case == 'retirement' and request_serial >= 2:
        file_stage = 'socket-retired' if request_serial == 2 else 'account-replaced'
    require(same(value.get('files'), file_ledger(case, file_stage)), 'Missing-source file read/cancel/reuse changed')
    row = current_row(case, stage)
    if request and not conditional: row = None
    require(same(value.get('channel'), row), 'Absent/canonical/terminal row changed or fabricated')
    expected_selector = selector(case, stage)
    if request: expected_selector = (selected('NonExistent', current=True)
        if conditional and case in ('omission', 'failure') else None if conditional else selected('Unknown', current=True))
    require(same(value.get('selector'), expected_selector), 'Missing exact sealed selector provenance')
    known_count = int(stage in ('confirmed', 'reopened'))
    unknown_count = count - known_count
    held = int(stage == 'opened')
    if request:
        known_count = int(conditional)
        unknown_count = count - known_count
        held = int(not conditional)
    unknown = value.get('unknown', {})
    require(unknown.get('requestCount') == unknown_count and unknown.get('held') == held
            and unknown.get('idle') is (not held and not retired) and unknown.get('networkCalls') == 0,
            'Unknown acquisition request/hold/cleanup changed')
    managers = unknown.get('managers')
    require(isinstance(managers, list), 'Missing unknown manager inventory')
    retired_source = retired or stage in ('pre-mount-retired', 'socket-retired')
    expected_managers = [] if count == 0 else [dict(current=not retired_source,
        worker=bool(held), active=bool(held),
        pendingIds=[public.CHANNEL] if held or retired_source else [])]
    if request: expected_managers = [dict(current=True, worker=bool(held), active=bool(held), pendingIds=[public.CHANNEL] if held else [])]
    require(same(managers, expected_managers), 'Unknown manager ownership/pending state changed')
    c = value.get('conditional', {})
    require(c.get('requestCount') == known_count and c.get('held') == int(request and conditional)
            and c.get('drainHeld') is False, 'Conditional follow-up count or hold changed')
    if not retired:
        require(c.get('installed') is True and c.get('listenerReady') is True
                and c.get('sourcePaused') is False and c.get('paused') is False, 'Shared source listener not ready')
        require(all(q.get('pending') is False for q in c.get('queues', [])), 'Unpublished queue used as canonical source')


def cache(value, case, stage):
    if stage in ('cold-loading', 'file-cancelled', 'account-replaced'):
        require(value is None, 'Cold/cancelled/foreign cache granted authority')
        return
    require(isinstance(value, dict), 'Missing current file receipt')
    raw = public.raw_file()['file']
    effective = {k: v for k, v in raw.items() if k != 'fixture_opaque'}
    effective.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
    require(same(value.get('source'), raw) and same(value.get('effective'), effective), 'Raw Canvas title source changed')
    current = not (stage in ('pre-mount-retired', 'socket-retired') or case in ('omission', 'failure') and stage in ('settled-mounted', 'failure-retained', 'used', 'confirmed'))
    require(value.get('current') is current, 'Stale transient/file observation treated as current')
    require(value.get('titleUsers') is None and value.get('quipTitle') == dict(current=True, value='nullish', fileId=public.FILE),
            'Missing channel borrowed user authority or bypassed Quip')
    expected = None
    if current:
        row = current_row(case, stage)
        expected = dict(ids=[public.CHANNEL], rows=[] if row is None else [row], presentation=public.CHANNEL_PRESENTATION)
        if stage == 'opened' or case in ('omission', 'failure'):
            expected.update(observed=[dict(id=public.CHANNEL, **selected('Unknown' if stage == 'opened' else 'NonExistent'))],
                            observedPresentation=OBSERVED_PRESENTATION)
    require(same(value.get('titleChannels'), expected), 'Wrong sealed transient/terminal channel observation')


def engine(case):
    namespace = runpy.run_path(str(Path(public.__file__)))
    g = namespace['validate'].__globals__
    old = dict(g)
    g.update(STAGES=STAGES, COUNTS=COUNTS)

    def requests(values, actual_case, epoch):
        require(actual_case == case and isinstance(values, list) and len(values) == COUNTS[case][-1], 'Wrong missing request count')
        for index, q in enumerate(values):
            known = case != 'retirement' and index == len(values) - 1
            serial = 1 if known else index + 1
            method = 'conversations.genericInfo' if case == 'failure' and index == 1 else 'channels.info'
            version = (0 if case in ('omission', 'failure') else 102) if known else 0
            require(q.get('manager') == ('conditional' if known else 'unknown') and q.get('serial') == serial
                    and q.get('method') == method and q.get('ids') == [public.CHANNEL]
                    and q.get('versions') == {public.CHANNEL: version}, 'Wrong exact manager/request/version')
            stage = 'confirmed' if known and case != 'cancel' else 'reopened' if known else 'opened'
            source(q.get('predecessor'), case, stage, index + 1, epoch, request=True, conditional=known, request_serial=serial)
            expected = dict(raw=dict(results=[], member_channels=[]))
            if not known:
                if case == 'failure': expected = dict(error='closed_failure')
                elif case == 'omission': expected = dict(raw=dict(results=[], member_channels=[public.CHANNEL]))
                elif case != 'omission': expected = dict(raw=dict(results=[public.channel('fetched-harbor', 102)], member_channels=[public.CHANNEL]))
            require(same(q.get('response'), expected), 'Unknown response/omission/failure raw evidence changed')

    def geometry(value, viewport, stage, h, actual_case):
        mapped = 'renamed-mounted' if stage == 'settled-mounted' else stage
        rich = mapped not in old['HIDDEN']
        adjusted = copy.deepcopy(value)
        if rich:
            label = (TERMINAL if case in ('omission', 'failure') else FOUND) if stage in ('confirmed', 'reopened') else UNKNOWN
            require([t.get('text') for t in value.get('tokens', [])] == [label, label]
                    and [t.get('label') for t in value.get('captured', {}).get('tokens', [])] == [label, label],
                    'Transient, mounted or fresh-opening label changed')
            legacy = 'Notes for #' + (public.NEW if mapped in ('confirmed', 'reopened', 'socket-retired') else public.OLD)
            for token in adjusted['tokens']: token['text'] = legacy
            for token in adjusted['captured']['tokens']: token['label'] = legacy
        old['geometry'](adjusted, viewport, mapped, h, actual_case)

    def exercise(value, stage, count, epoch):
        if stage == 'opened': expected = dict(immediateUnknown=True, request=1)
        elif stage in ('settled-mounted', 'failure-retained'):
            expected = dict(mountedLabelRetained=True, localTerminalNotServerAbsence=case != 'positive')
        elif stage == 'socket-retired': expected = dict(socket=dict(before=2, after=3), latePublicationRefused=True)
        elif stage == 'pre-mount-retired': expected = dict(oldOpeningRefused=True, socket=dict(before=1, after=2), latePublicationRefused=True)
        elif stage == 'account-replaced':
            extras = dict(replacementBefore=replacement_snapshot(epoch), replacementAfter=replacement_snapshot(epoch),
                workersQuiescent=True, globalIdle=False, foreignPendingPreserved=True)
            require(all(same(value.get(k), v) for k, v in extras.items()),
                    'Retired response changed exact foreign Boot snapshot or quiescence')
            return old['exercise']({k: v for k, v in value.items() if k not in extras}, stage, count, epoch)
        else:
            return old['exercise'](value, stage, count, epoch)
        require(same(value, expected), 'Missing actual transient/settlement/retirement proof')

    g.update(validate_requests=requests, validate_source=lambda v, c, s, n, e: source(v, c, s, n, e),
             validate_cache=lambda v, s: cache(v, case, s), geometry=geometry, exercise=exercise)
    return g


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    require(receipt.get('ok') is True,
            'Native missing-title fixture failed: ' + str(receipt.get('error', 'missing success result')))
    require(receipt.get('missingChannelTitle') is True, 'Missing explicit missing-title receipt mode')
    return engine(case)['validate'](fixture, receipt, captures, h, case=case, platform=platform, png=png)
