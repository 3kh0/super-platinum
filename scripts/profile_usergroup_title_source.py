"""Finite source/transport contract for the closed nested usergroup title fixture."""
from profile_channel_title import ACCOUNT, FOREIGN, TEAM, SELF, FILE, blocks, require, same

GROUP = 'SGROUP001'
OLD, NEW = 'Notes for @design', 'Notes for @renamed'
STAGES = {
    'known': ('cold-loading', 'fresh-opened', 'mounted-held', 'renamed-mounted', 'used', 'confirmed'),
    'failure': ('cold-loading', 'fresh-opened', 'mounted-held', 'failure-retained', 'fresh-reopened', 'retry-held', 'renamed-mounted', 'used', 'confirmed'),
    'cancel': ('cold-loading', 'fresh-opened', 'mounted-held', 'late-settled', 'reopened', 'used', 'confirmed'),
    'retirement': ('cold-loading', 'fresh-opened', 'mounted-held', 'pre-mount-retired', 'socket-retired', 'account-held', 'account-replaced'),
}
COUNTS = {'known': (0, 0, 2, 2, 2, 2), 'failure': (0, 0, 2, 3, 3, 5, 5, 5, 5),
          'cancel': (0, 0, 2, 2, 2, 2, 2), 'retirement': (0, 0, 2, 2, 6, 8, 8)}
HIDDEN = {'cold-loading', 'late-settled', 'pre-mount-retired', 'used', 'account-replaced'}
HELD = {'mounted-held', 'retry-held', 'account-held'}


def raw(renamed=False, poison=False):
    return dict(id=GROUP, team_id=TEAM, name='Fixture group',
                handle='retired-socket-name' if poison else 'renamed' if renamed else 'design',
                date_update=999 if poison else 102 if renamed else 101,
                date_delete=0, is_visible=True, fixture_metadata=dict(keep=True))


def effective(renamed=False):
    return dict(raw(renamed), user_count=0, _name_lc='fixture group',
                is_usergroup=True, isNonExistent=False, users=[])


def native_group(renamed=False):
    return dict(id=GROUP, team_id=TEAM, name='Fixture group',
                handle='renamed' if renamed else 'design', description='', users=[], members_loaded=False,
                prefs=dict(channels=[], groups=[]), date_delete=0, label_source=raw(renamed))


def group_rows(renamed=False):
    return [dict(source=raw(renamed), effective=effective(renamed))]


def seed():
    user = dict(id=SELF, name='fixture-self', team_id=TEAM, profile={})
    team = dict(id=TEAM, name='Group title')
    return dict(boot=dict(ok=True, team=team, self=user, users=[user], prefs={},
                         channels=[dict(id='CINIT0001', name='warm-harbor', is_channel=True, is_member=True)]),
                init=dict(ok=True, self=user, team=team, workspaces=[dict(id=TEAM)],
                          onboarding=dict(user_data=dict(onboarding_complete=True)), user_date_created=123),
                rows=[raw()], actualSourceSink=True)


def title_blocks():
    return [dict(type='rich_text', opaque=dict(keep=1), elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Notes for '),
        dict(type='usergroup', usergroup_id=GROUP, style=dict(bold=True), opaque=dict(keep=2))])])]


def raw_file():
    return dict(ok=True, file=dict(id=FILE, mode='quip', filetype='quip', created=101, updated=101,
        title='Misleading Canvas fallback', name='wrong-name-fallback', fixture_opaque=dict(preserve=[FILE, 1, 2]),
        title_blocks=title_blocks()))


def state_for(case, stage):
    i = STAGES[case].index(stage)
    count = COUNTS[case][i]
    resets = int(i >= 2)
    if case == 'failure' and i >= 5: resets = 2
    if case == 'retirement' and i >= 4: resets = 3 if i == 4 else 4
    renamed = stage in ('renamed-mounted', 'used', 'confirmed', 'late-settled', 'reopened',
                        'pre-mount-retired', 'socket-retired', 'account-held')
    model_prefix = ('Saved with ' if stage in ('used', 'confirmed') else
                    'Retained draft with ' if case == 'failure' and i >= 4 else 'Working with ')
    dom_prefix = 'Retained draft with ' if stage == 'failure-retained' else model_prefix
    new_label = stage in ('confirmed', 'reopened', 'socket-retired', 'account-held')
    files = 1 if i < 2 else 2
    if case == 'failure' and i >= 5: files = 3
    if case == 'retirement' and i >= 4: files = 3 if i == 4 else 5
    completed = 0 if i < 3 else 1
    if case == 'failure' and i >= 6: completed = 2
    if case == 'retirement' and i >= 4: completed = 3
    return dict(count=count, resets=resets, source_renamed=renamed, model_prefix=model_prefix,
                dom_prefix=dom_prefix, label=NEW if new_label else OLD, files=files,
                completed=completed, held=stage in HELD, retired=stage == 'account-replaced',
                editor=stage not in HIDDEN, saved=stage == 'confirmed',
                can_save=stage == 'used' or (case == 'failure' and stage in ('fresh-reopened', 'retry-held', 'renamed-mounted')))


def requests(case, count=None, held=False):
    kinds = (['discovery', 'directory', 'direct', 'discovery', 'directory'] if case == 'failure'
             else ['discovery', 'directory'] * (4 if case == 'retirement' else 1))
    if count is not None: kinds = kinds[:count]
    result = []
    for i, kind in enumerate(kinds, 1):
        if kind == 'discovery':
            method, body = 'api.getFlannelHttpUrl', dict(include_external_workspaces='true', _x_reason='client-needs-valid-flannel-url')
            response = dict(raw=dict(ok=True, url=f'https://edgeapi.slack.com/cache/{TEAM}', ttl_seconds=300))
        elif kind == 'direct':
            method, body = 'subteams.info', dict(usergroups=GROUP, include_users='false', include_count='true',
                                               include_disabled='true', _x_reason='fallback:fetchRawUserGroupsById')
            response = dict(error='offline')
        else:
            method, body = 'usergroups/info', dict(ids=[GROUP])
            response = (dict(error='offline') if case == 'failure' and i == 2 else
                        dict(raw=dict(ok=True, results=[raw(True, case == 'retirement' and i == 4)])))
        pending = held and i == len(kinds)
        row = dict(serial=i, kind=kind, method=method, body=body, account=ACCOUNT, team=TEAM, epoch=0, released=not pending)
        if not pending:
            row.update(response=response, outcome='offline' if 'error' in response else 'response',
                       receiverRetired=False, delivered=True)
        result.append(row)
    return result


def file_ledger(expected, stage):
    cold = stage == 'cold-loading'
    rows = [dict(serial=i, method='files.info', args=dict(file=FILE, page='1', count='500', truncate='true'))
            for i in range(1, expected['files'] + 1)]
    if not cold:
        rows[0]['releasedResponse'] = raw_file()
        for row in rows: row['response'] = dict(raw=raw_file())
    return dict(requests=rows, completed=0 if cold else len(rows), cancelled=0, held=cold, active=int(cold), networkCalls=0)


def reset(ordinal):
    before = dict(slowConnectionCount=ordinal, major=1)
    after = dict(slowConnectionCount=ordinal+1, major=1)
    return dict(team=TEAM, before=before, after=after, duplicate=after,
                oldGeneration=ordinal, generation=ordinal+1, actualAcceptedConnection=True)


def scheduler(case, stage, e):
    completed, held = e['completed'], e['held']
    last = None if completed == 0 else dict(ids=[GROUP], attempted=[GROUP],
        adopted=not (case == 'failure' and completed == 1),
        errors=2 if case == 'failure' and completed == 1 else 0, timedOut=False)
    return dict(scope=dict(account=ACCOUNT, team=TEAM, user=SELF, epoch=0),
                freshIds=[] if held else [GROUP], pending=[], inFlight=[GROUP] if held else None,
                worker=held, started=completed+int(held), finished=completed,
                observations=dict(slowConnectionCount=e['resets']+1, major=1), lastSettlement=last)


def validate_source(s, case, stage, model):
    e = state_for(case, stage)
    require(isinstance(s, dict) and s.get('groupTitle') is True and s.get('case') == case
            and same(s.get('seed'), seed()) and s.get('networkCalls') == 0, 'Wrong group source/seed/mode')
    require(s.get('account') == (FOREIGN if e['retired'] else ACCOUNT)
            and s.get('epoch') == int(e['retired'])
            and s.get('socketGeneration') == (0 if e['retired'] else e['resets']+1), 'Wrong group account/epoch/socket')
    wanted = requests(case, e['count'], e['held'])
    require(same(s.get('requests'), wanted) and same(s.get('pending'), [wanted[-1]] if e['held'] else []),
            'Shared group request/body/response/hold/retry ledger changed')
    require(same(s.get('files'), file_ledger(e, stage)), 'File receipt lifecycle changed')
    require(same(s.get('group'), None if e['retired'] else native_group(e['source_renamed']))
            and s.get('label') == (None if e['retired'] else '@renamed' if e['source_renamed'] else '@design'),
            'Canonical native group/source/name/membership changed')
    shared = s.get('shared', {})
    require(shared.get('seed') is None and shared.get('networkCalls') == 0
            and same(shared.get('resets'), [reset(i) for i in range(1, e['resets']+1)]), 'Missing genuine lifecycle reset history')
    if e['retired']:
        require(all(shared.get(k) is None for k in ('source', 'scheduler', 'opening')), 'Old source authority escaped retirement')
    else:
        require(same(shared.get('source'), dict(rows=group_rows(e['source_renamed']), membership=None, current=True)),
                'Canonical raw/effective group projection changed')
        require(same(shared.get('scheduler'), scheduler(case, stage, e)), 'Freshness scheduler attempted/read state changed')
        opening = shared.get('opening', {})
        require(opening.get('visit') == model.get('visit') and opening.get('serial') == model.get('serial')
                and type(opening.get('key')) is int and opening['key'] >= 0
                and opening.get('mounted') is e['editor'] and opening.get('reading') is (stage == 'cold-loading')
                and opening.get('lease') is (stage == 'cold-loading'), 'Wrong editor opening or unexpected recovery lease')


def validate_cache(value, case, stage):
    e = state_for(case, stage)
    if stage in ('cold-loading', 'account-replaced'):
        require(value is None, 'Cold/retired file cache granted authority')
        return
    require(isinstance(value, dict), 'Missing exact file observation')
    source = raw_file()['file']; projection = {k: v for k, v in source.items() if k != 'fixture_opaque'}
    projection.update(is_deleted=False, is_tombstoned=False, isUnknown=False, isNonExistent=False)
    require(same(value.get('source'), source) and same(value.get('effective'), projection), 'File raw title/style/opaque changed')
    current = stage != 'socket-retired'
    require(value.get('current') is current and value.get('titleUsers') is None and value.get('titleChannels') is None,
            'Group dependency borrowed user/channel or old socket authority')
    require(value.get('quipTitle') == dict(current=True, value='nullish', fileId=FILE), 'Group-only title bypassed Quip observation')
    expected = dict(ids=[GROUP], rows=group_rows(e['source_renamed']), labels={GROUP:'@renamed' if e['source_renamed'] else '@design'},
                    provenance='shared-group-history-snapshot') if current else None
    require(same(value.get('titleGroups'), expected), 'Missing exact shared group title dependency')
    from profile_emoji_title import DESCRIPTOR, FALLBACK
    require(value.get('canvasFallback') == dict(current=current, descriptor=FALLBACK)
            and value.get('emojiText') == dict(current=current, descriptor=DESCRIPTOR), 'Wrong independent presentation contexts')
