"""Exact finite files.info carriers and source history; not a native producer."""
from profile_channel_title import ACCOUNT, FOREIGN, FILE, blocks, require, same

STAGES = {
    'template': ('cold-loading', 'opened-private', 'refreshed-mounted', 'used', 'confirmed'),
    'sticky': ('cold-loading', 'opened-private', 'refreshed-mounted', 'used', 'confirmed'),
    'failure': ('cold-loading', 'malformed', 'offline', 'opened-private', 'used', 'confirmed'),
    'cancel': ('cold-loading', 'cancelled', 'opened-private', 'used', 'confirmed'),
    'retirement': ('cold-loading', 'opened-private', 'pre-mount-retired', 'socket-held', 'socket-retired', 'account-held', 'account-replaced'),
}
COUNTS = {'template': (1, 1, 2, 2, 2), 'sticky': (1, 1, 2, 2, 2),
          'failure': (1, 1, 2, 3, 3, 3), 'cancel': (1, 1, 2, 2, 2),
          'retirement': (1, 1, 1, 2, 2, 4, 4)}
PRIVATE, RECOVERED = 'Private canvas', 'Recovered canvas'
TOKEN = dict(kind='private-canvas', icon='lock', disabled=False)
CONTEXT = dict(locale='en-US', namespace='canvas', source=PRIVATE, value=PRIVATE,
               origin='application-presentation', token=TOKEN)
LOADING = 'Loading document titles. Your description is unchanged.'
UNAVAILABLE = 'Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'


def private_response(case):
    if case == 'sticky':
        return dict(ok=False, error='not_visible', filetype='quip', title='SECRET ERROR TITLE')
    return dict(ok=False, error='template_not_visible')


def normal_response():
    return dict(ok=True, file=dict(id=FILE, mode='quip', filetype='quip', title=RECOVERED, updated=102))


def state_for(case, stage):
    i = STAGES[case].index(stage)
    refreshed = case in ('template', 'sticky') and i >= 2
    retired = stage == 'account-replaced'
    editor = stage in ('opened-private', 'refreshed-mounted', 'confirmed', 'account-held')
    cold = stage == 'cold-loading'
    held = stage in ('cold-loading', 'socket-held', 'account-held')
    stale = stage in ('pre-mount-retired', 'socket-held', 'socket-retired')
    no_cache = cold or stage in ('malformed', 'offline', 'cancelled', 'account-replaced')
    socket = 1 if case == 'retirement' and i >= 4 and not retired else 0
    return dict(index=i, refreshed=refreshed, retired=retired, editor=editor, cold=cold,
                held=held, stale=stale, no_cache=no_cache, socket=socket, files=COUNTS[case][i],
                private=not (case == 'template' and stage == 'confirmed'), saved=stage == 'confirmed',
                can_save=stage == 'used', model_prefix='Saved with ' if stage in ('used', 'confirmed') else 'Working with ',
                dom_prefix='Retained draft with ' if stage == 'refreshed-mounted' else 'Saved with ' if stage == 'confirmed' else 'Working with ')


def selected_response(case, stage):
    e = state_for(case, stage)
    if e['refreshed']: return normal_response()
    if case == 'failure' and e['index'] < 3: return dict(ok=False, error=7)
    return private_response(case)


def file_ledger(case, stage):
    e = state_for(case, stage)
    rows = []
    for serial in range(1, e['files'] + 1):
        row = dict(serial=serial, method='files.info', args=dict(file=FILE, page='1', count='500', truncate='true'))
        pending = e['held'] and serial == e['files']
        is_held_read = serial == 1 or (case == 'retirement' and serial in (2, 4))
        cancelled = case == 'cancel' and serial == 1 and not pending
        raw = private_response(case)
        if case in ('template', 'sticky') and serial == 2: raw = normal_response()
        if case == 'failure' and serial == 1: raw = dict(ok=False, error=7)
        if not pending:
            if is_held_read: row['releasedResponse'] = raw
            if not cancelled:
                row['response'] = dict(error='offline') if case == 'failure' and serial == 2 else dict(raw=raw)
        rows.append(row)
    return dict(requests=rows, completed=e['files']-int(e['held']),
                cancelled=int(case == 'cancel' and not e['cold']), held=e['held'], active=int(e['held']), networkCalls=0)


def source(case, stage):
    e = state_for(case, stage)
    return dict(case=case, selectedResponse=selected_response(case, stage), files=file_ledger(case, stage),
                account=FOREIGN if e['retired'] else ACCOUNT, epoch=int(e['retired']),
                socketGeneration=e['socket'], networkCalls=0, presentation=TOKEN)


def mapped_private(case):
    if case == 'sticky':
        return dict(id=FILE, mode='quip', title='SECRET ERROR TITLE', file_access='not_visible')
    return dict(id=FILE, filetype='quip', mode='template_not_visible')


def expected_cache(case, stage):
    e = state_for(case, stage)
    if e['no_cache']: return None
    current = not e['stale']
    receipt = normal_response() if e['refreshed'] else private_response(case)
    raw = receipt['file'] if e['refreshed'] else mapped_private(case)
    effective = dict(raw)
    if e['refreshed']:
        effective.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
        if case == 'sticky': effective['file_access'] = 'not_visible'
    value = dict(source=raw, effective=effective, current=current)
    if not e['refreshed'] or case == 'sticky':
        value['reference'] = dict(status='private-canvas', receipt=receipt, presentation=CONTEXT,
                                  dependencies=dict(users=[], channels=[], groups=[]), quip=None)
    else:
        value['canvasFallback'] = dict(current=True, descriptor=dict(locale='en-US', namespace='canvas',
            source='Untitled', value='Untitled', origin='application-presentation'))
        value['emojiText'] = dict(current=True, descriptor=dict(locale='en-US', format='canonical-colon-name',
            catalog='retained-v16-v17-canonical-intersection', origin='application-presentation'))
    return value


def validate_source(value, case, stage):
    require(same(value, source(case, stage)), 'Wrong exact files.info carrier/owner/request history')


def validate_cache(value, case, stage):
    require(same(value, expected_cache(case, stage)), 'Wrong exact source/status receipt/effective history or presentation')


def released(cancelled=False):
    return dict(delivered=not cancelled, cancelledBeforeReply=cancelled)


def account_proof():
    return dict(oldOpeningRefused=True, oldEditorHidden=True, previousAccount=ACCOUNT, account=FOREIGN,
                previousEpoch=0, epoch=1, previousAttempts=[], previousBlocks=blocks())


def validate_exercise(value, case, stage, token_check):
    expected = {'cold-loading': dict(held=True), 'cancelled': dict(release=released(True)),
        'malformed': dict(statusRefused=True), 'offline': dict(statusRefused=True),
        'used': dict(rawTokensPreserved=True),
        'pre-mount-retired': dict(openingRefused=True, draftUnchanged=True, noWrite=True),
        'socket-held': dict(held=True),
        'socket-retired': dict(socket=dict(before=0, after=1), release=released(), latePublicationRefused=True)}
    if stage in expected:
        require(same(value, expected[stage]), 'Wrong explicit lifecycle proof')
    elif stage in ('opened-private', 'refreshed-mounted', 'confirmed', 'account-held'):
        require(isinstance(value, dict), 'Missing token/presentation proof')
        token_check(value.get('token'))
        other = {k: v for k, v in value.items() if k != 'token'}
        wanted = {'opened-private': dict(tamper=dict(ok=True, iconMutationRefused=True, restored=True)),
                  'refreshed-mounted': dict(mountedPrivateRetained=True),
                  'confirmed': dict(savedAndReadBack=True, label=RECOVERED if case == 'template' else PRIVATE),
                  'account-held': dict(held=True)}[stage]
        require(same(other, wanted), 'Wrong mounted/icon mutation/Save proof')
    elif stage == 'account-replaced':
        after = source(case, stage)
        before = dict(after, files=file_ledger(case, 'account-held'))
        require(same(value, dict(account=account_proof(), release=released(), accepted=False,
                                replacementBefore=before, replacementAfter=after)),
                'Late response changed replacement owner or retained stale source')
    else: raise RuntimeError('Unmodeled status lifecycle')
