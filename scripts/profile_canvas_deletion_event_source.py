"""Finite event/source expectations; no native producer or transport execution."""
from copy import deepcopy
from profile_channel_title import ACCOUNT, FOREIGN, FILE, blocks, require, same
from profile_canvas_deleted_source import TOKEN, CONTEXT, LOADING, UNAVAILABLE

NORMAL, DELETED = 'Known canvas', 'Deleted canvas'
SECOND, ABSENT = 'FSECOND01', 'FABSENT01'
STAGES = {
    'known': ('cold-loading', 'known-opened', 'event-closed', 'team-away', 'team-returned', 'reopened-deleted', 'reconnected-deleted', 'used', 'confirmed'),
    'mounted': ('cold-loading', 'known-opened', 'event-mounted', 'used', 'confirmed'),
    'held': ('cold-loading', 'known-opened', 'refresh-held', 'late-read-refused', 'used', 'confirmed'),
    'retirement': ('cold-loading', 'known-opened', 'old-socket-refused', 'pre-mount-retired', 'current-deleted', 'account-replaced'),
    'plural': ('cold-loading', 'known-opened', 'plural-mounted', 'plural-deleted', 'used', 'confirmed'),
}


def response(id=FILE):
    row = dict(id=id, mode='quip', filetype='quip', title=NORMAL if id == FILE else 'Second known canvas', updated=101)
    if id == FILE: row['opaque'] = dict(retain=True)
    return dict(ok=True, file=row)


def deletion():
    return dict(type='file_deleted', file_id=FILE, fixture_event='exact owned frame')


def plural_events():
    return [dict(type='file_deleted', file_ids=[], file_id=FILE),
            dict(type='file_deleted', file=dict(id=FILE)),
            dict(type='file_deleted', file_ids=[FILE, None, False, '', FILE, SECOND, ABSENT], file_id='FIGNORED01')]


def state_for(case, stage):
    i = STAGES[case].index(stage)
    retired, closed = stage == 'account-replaced', stage in ('event-closed', 'team-away', 'team-returned')
    deleted = stage in ('reopened-deleted', 'reconnected-deleted', 'late-read-refused', 'current-deleted', 'plural-deleted', 'confirmed')
    editor = stage in ('known-opened', 'event-mounted', 'reopened-deleted', 'reconnected-deleted',
                       'late-read-refused', 'old-socket-refused', 'current-deleted', 'plural-mounted', 'plural-deleted', 'confirmed')
    event_count = 0 if i < 2 else 4 if case == 'retirement' and retired else 3 if case == 'retirement' and i >= 3 else 1
    if case == 'held' and stage == 'refresh-held': event_count = 0
    if case == 'plural' and i >= 2: event_count = 3
    socket = int((case == 'known' and i >= 6) or (case == 'retirement' and i >= 2 and not retired))
    return dict(index=i, retired=retired, closed=closed, editor=editor, deleted=deleted,
                socket=socket, event_count=event_count, files=2 if case == 'held' and i >= 2 else 1,
                held=stage in ('cold-loading', 'refresh-held'), saved=stage == 'confirmed',
                can_save=stage == 'used', model_prefix='Saved with ' if stage in ('used', 'confirmed') else 'Working with ',
                dom_prefix='Retained draft with ' if stage == 'event-mounted' else 'Saved with ' if stage == 'confirmed' else 'Working with ')


def file_ledger(case, stage):
    e = state_for(case, stage)
    rows = []
    for serial in range(1, e['files'] + 1):
        row = dict(serial=serial, method='files.info', args=dict(file=FILE, page='1', count='500', truncate='true'))
        if not (e['held'] and serial == e['files']):
            row.update(releasedResponse=response(), response=dict(raw=response()))
        rows.append(row)
    return dict(requests=rows, completed=e['files'] - int(e['held']), cancelled=0,
                held=e['held'], active=int(e['held']), networkCalls=0)


def empty_row(id, workspace_deleted=False):
    return dict(id=id, source=None, effective=None, receipt=None, event=None, preimageSource=None,
                preimageReceipt=None, current=False, workspaceDeleted=workspace_deleted)


def normal_row(id, current=True):
    raw = response(id)['file']
    # The source/receipt retain opaque metadata; merge_title_fields projects only FIELDS.
    projected = {k: raw[k] for k in ('id', 'mode', 'filetype', 'title', 'updated')}
    return dict(id=id, source=raw, effective=dict(projected, is_deleted=False, is_tombstoned=False,
                isUnknown=False, isNonExistent=False), receipt=response(id), event=None,
                preimageSource=None, preimageReceipt=None, current=current, workspaceDeleted=False)


def initial_shared(loaded=True, second=False, current=True):
    return dict(rows=[normal_row(FILE, current) if loaded else empty_row(FILE),
                      normal_row(SECOND, current) if second else empty_row(SECOND), empty_row(ABSENT)],
                revision=0, queryCount=0)


def event_result(before, raw, generation=0, accepted=True):
    after = deepcopy(before)
    if accepted:
        ids = raw.get('file_ids')
        if ids is None: ids = [raw.get('file_id')]
        ids = [x for x in ids if type(x) is str and x]
        changed = False
        for row in after['rows']:
            if row['id'] not in ids: continue
            row['workspaceDeleted'] = True
            if row['source'] is None: continue
            old = deepcopy(row)
            tombstone = dict(id=row['id'], mode='tombstone', is_deleted=True)
            row.update(source=tombstone, effective=tombstone, receipt=deepcopy(raw), event=deepcopy(raw),
                       preimageSource=old['source'], preimageReceipt=old['receipt'], current=True)
            changed = True
        after['revision'] += int(changed)
    return dict(raw=raw, acceptedOwner=accepted, generation=generation, before=before, after=after)


def events(case, stage):
    e = state_for(case, stage)
    if not e['event_count']: return []
    if case == 'plural':
        out, before = [], initial_shared(second=True)
        for raw in plural_events():
            v = event_result(before, raw);out.append(v);before = v['after']
        return out
    if case == 'retirement':
        first = event_result(initial_shared(current=False), deletion(), accepted=False)
        if e['event_count'] == 1: return [first]
        second = event_result(first['after'], deletion(), generation=1)
        third = event_result(second['after'], deletion(), generation=1)
        out = [first, second, third]
        if e['retired']:
            out.append(dict(raw=deletion(), acceptedOwner=False, generation=1, before=None, after=None))
        return out
    return [event_result(initial_shared(), deletion())]


def source(case, stage):
    e = state_for(case, stage)
    ledger = events(case, stage)
    shared = None if e['retired'] or stage == 'team-away' else ledger[-1]['after'] if ledger else initial_shared(stage != 'cold-loading')
    return dict(case=case, account=FOREIGN if e['retired'] else ACCOUNT, epoch=int(e['retired']),
                socketGeneration=e['socket'], files=file_ledger(case, stage), events=ledger,
                shared=shared, networkCalls=0)


def expected_cache(case, stage):
    e = state_for(case, stage)
    if stage in ('cold-loading', 'team-away') or e['retired']: return None
    row = source(case, stage)['shared']['rows'][0]
    value = dict(source=row['source'], effective=row['effective'], current=row['current'])
    if row['event'] is not None:
        value['reference'] = dict(status='deleted-canvas', receipt=row['event'], presentation=CONTEXT,
                                  dependencies=dict(users=[], channels=[], groups=[]), quip=None)
    else:
        value['canvasFallback'] = dict(current=row['current'], descriptor=dict(locale='en-US', namespace='canvas',
            source='Untitled', value='Untitled', origin='application-presentation'))
        value['emojiText'] = dict(current=row['current'], descriptor=dict(locale='en-US', format='canonical-colon-name',
            catalog='retained-v16-v17-canonical-intersection', origin='application-presentation'))
    return value


def validate_source(value, case, stage):
    require(same(value, source(case, stage)), 'Wrong exact event/source/preimage/owner/request ledger')


def validate_cache(value, case, stage):
    require(same(value, expected_cache(case, stage)), 'Wrong exact current event reference or canonical predecessor')


def account_proof():
    return dict(oldOpeningRefused=True, oldEditorHidden=True, previousAccount=ACCOUNT, account=FOREIGN,
                previousEpoch=0, epoch=1, previousAttempts=[], previousBlocks=blocks())


def validate_exercise(value, case, stage, token_check, model, previous_model):
    ledger = events(case, stage)
    if stage in ('team-away', 'team-returned'):
        require(isinstance(value, dict) and set(value) == {'navigation'}, 'Missing exact workspace navigation proof')
        n = value['navigation'];target = 'TEVENTOTHER' if stage == 'team-away' else 'TINIT0001'
        require(isinstance(n, dict) and type(n.get('profileGenerationBefore')) is int
                and type(n.get('profileGenerationAfter')) is int
                and n['profileGenerationAfter'] == n['profileGenerationBefore'] + 1,
                'Actual workspace navigation did not retire Profile generation')
        require(same(n, dict(target=target, activeTeam=target, before=ledger[-1]['after'], after=ledger[-1]['after'],
                sharedUnchanged=True, credentialsCurrent=True, profileGenerationBefore=n['profileGenerationBefore'],
                profileGenerationAfter=n['profileGenerationAfter'])), 'Workspace switch changed original source or credentials')
        return
    expected = {
        'cold-loading': dict(held=True), 'refresh-held': dict(held=True),
        'event-closed': dict(event=ledger[-1]) if ledger else None,
        'pre-mount-retired': dict(openingRefused=True, draftUnchanged=True, event=ledger[1], successor=ledger[2]) if len(ledger) >= 3 else None,
        'account-replaced': dict(account=account_proof(), event=ledger[-1]) if ledger else None,
    }
    if stage in expected:
        require(same(value, expected[stage]), 'Wrong exact event retirement proof')
        return
    require(isinstance(value, dict), 'Missing lifecycle exercise')
    if stage == 'used':
        wanted = dict(rawTokensPreserved=True, tamper=None)
        if case != 'mounted':
            wanted['tamper'] = dict(ok=True, iconMutationRefused=True, disabledMutationRefused=True,
                                    classRemovalRefused=True, restored=True)
        require(same(value, wanted), 'Wrong raw Use or actual tamper evidence')
        return
    token_check(value.get('token'))
    other = {k: v for k, v in value.items() if k != 'token'}
    wanted = {
        'known-opened': dict(release=dict(delivered=True, cancelledBeforeReply=False)),
        'event-mounted': dict(event=ledger[-1], mountedNormalRetained=True) if ledger else None,
        'reconnected-deleted': dict(socket=dict(before=0, after=1)),
        'late-read-refused': dict(event=ledger[-1], release=dict(delivered=True, cancelledBeforeReply=False), accepted=False) if ledger else None,
        'old-socket-refused': dict(socket=dict(before=0, after=1), event=ledger[0]) if ledger else None,
        'current-deleted': {}, 'plural-deleted': {}, 'confirmed': dict(savedAndReadBack=True),
        'plural-mounted': dict(proof=dict(secondReceipt=response(SECOND), empty=ledger[0], nested=ledger[1], event=ledger[2])) if len(ledger) >= 3 else None,
    }
    if stage == 'reopened-deleted':
        require(set(other) == {'reopened'}, 'Missing exact Profile visit transition')
        require(same(other['reopened'], dict(newVisit=model['visit'], newSerial=model['serial'], sharedUnchanged=True)), 'Reopen observation does not own actual current model')
    else:
        require(stage in wanted and same(other, wanted[stage]), 'Wrong mounted/reconnect/late event proof')
