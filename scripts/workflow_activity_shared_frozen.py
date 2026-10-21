"""Independent expectations for canonical shared/frozen Activity labels."""
from workflow_activity_channel_spec import ChannelSpec, SOURCE_KEYS, FLAGS
from datetime import datetime, timedelta

NAMES = {
    'CSHPRIV01': 'shared-private', 'CSHARCH01': 'shared-archived',
    'CFRZACT01': 'frozen-active', 'CFRZPRV01': 'frozen-private',
    'CGRPRIV01': 'grace-private', 'CGRARCH01': 'grace-archived',
    'CGRPRAR01': 'grace-private-archived', 'CGRNOGT01': 'grace-public-no-gate',
}


class SharedFrozenSpec(ChannelSpec):
    def __init__(self, grace=False):
        object.__setattr__(self, 'special', True)
        object.__setattr__(self, 'grace', grace)

    @property
    def case(self):
        return 'channel-frozen-grace' if self.grace else 'channel-shared-frozen'

    @property
    def pairs(self):
        return (('private-grace', 'CGRPRIV01', 1),
                ('archived-grace', 'CGRARCH01', 3)) if self.grace else (
                ('frozen-active', 'CFRZACT01', 5),
                ('shared-archived', 'CSHARCH01', 3))

    @property
    def returns(self):
        return ('returned', *(stage+'-return' for stage, _, _ in self.pairs), 'final-return')

    @property
    def destinations(self):
        return ('known', *(stage for stage, _, _ in self.pairs), 'retried')

    @property
    def archived_destinations(self):
        return ('archived-grace',) if self.grace else ('shared-archived',)

    @property
    def local_clicks(self):
        return {stage: f'workflow-activity-channel-step-1-{part}'
                for stage, _, part in self.pairs}

    @property
    def ids(self):
        return (('CGRPRIV01', 'CGRARCH01', 'CGRPRAR01', 'CGRNOGT01') if self.grace
                else ('CSHPRIV01', 'CSHARCH01', 'CFRZACT01', 'CFRZPRV01'))

    @property
    def archived_ids(self):
        return {'CGRARCH01', 'CGRPRAR01'} if self.grace else {'CSHARCH01'}

    @property
    def private_ids(self):
        return {'CGRPRIV01', 'CGRPRAR01'} if self.grace else {'CSHPRIV01', 'CFRZPRV01'}

    def selected(self, stage, original):
        return {'known': 'CACTKNOWN01', 'retried': 'CACTMISS01',
                **{stage: id for stage, id, _ in self.pairs}}.get(stage, original)

    def source_ids(self, original, fetched):
        return {original, 'CACTKNOWN01', *self.ids} | ({'CACTMISS01'} if fetched else set())

    def sidebar_ids(self, original, stage, fetched):
        selected = self.selected(stage, original)
        return (self.source_ids(original, fetched) - self.archived_ids) | {selected}

    def vectors(self, fetched=False):
        base = ChannelSpec().vectors(fetched)
        kinds = ('LockFilled', 'Tag', 'LockFilled', 'Archive') if self.grace else (
            'LockFilled', 'Archive', 'Archive', 'ArchiveFilled')
        return base + tuple((f'workflow-activity-channel-step-1-{part}', id,
                             NAMES[id] + (' (archived)' if id == 'CSHARCH01' else ''), kind)
                            for part, id, kind in zip((1, 3, 5, 7), self.ids, kinds))

    def inert(self):
        containers = [('workflow-activity-trigger-message', 0),
                      ('workflow-activity-trigger-message', 1),
                      ('workflow-activity-step-message-0', 0)]
        containers += [('workflow-activity-step-message-1', i) for i in range(4)]
        return [(*containers[i], id, channel, name)
                for i, (id, channel, name, _) in enumerate(self.vectors())]

    def validate_flags(self, rows, require):
        for row in rows:
            require(set(row) == SOURCE_KEYS | FLAGS, 'Unexpected shared/frozen source fields')
            require(row['isPrivate'] is (row['id'] in self.private_ids)
                    and row['isGroup'] is False
                    and row['isArchived'] is (row['id'] in self.archived_ids),
                    'Shared/frozen flags differ from original source')


SHARED = SharedFrozenSpec()
GRACE = SharedFrozenSpec(grace=True)
SPECS = {spec.case: spec for spec in (SHARED, GRACE)}


def validate_observation(channel, spec, require):
    """Check real source and time evidence separately from icon DOM assertions."""
    seed = channel.get('seed', {})
    require(set(seed) == {'boot', 'init', 'extras', 'startSeconds'},
            'Missing original canonical fixture inputs')
    start = seed['startSeconds']
    require(type(start) is int and start > 0, 'Missing actual synthetic plan start')
    expected_plan = {'is_plan_frozen': True, 'reconnect_grace_period_start_ts': start}
    require(seed['extras'].get('plan_info') == expected_plan,
            'Plan must originate in the actual Extras input')
    boot = seed['boot']
    require(boot.get('ok') is True and boot.get('team', {}).get('id') == 'TWORKFLOW'
            and seed['init'].get('ok') is True
            and seed['init'].get('team', {}).get('id') == 'TWORKFLOW',
            'Missing original same-team Boot/Init inputs')
    expected_rows = []
    for id in ('CACTKNOWN01', *spec.ids):
        raw = dict(id=id, name='activity-known' if id == 'CACTKNOWN01' else NAMES[id],
                   is_channel=True, is_member=True, is_archived=id in spec.archived_ids,
                   is_private=id in spec.private_ids)
        if spec.grace:
            raw.update(is_frozen=True, unlinked=start)
            if id in ('CACTKNOWN01', 'CGRARCH01'):
                raw['grace_period_end'] = 1
        elif id in ('CACTKNOWN01', 'CSHPRIV01', 'CSHARCH01'):
            raw.update(is_ext_shared=True, is_shared=True, is_org_shared=True,
                       connected_teams=[{'id': 'TWORKFLOW'}, {'id': 'TPARTNER'}])
        else:
            raw['is_frozen'] = True
            if id == 'CFRZACT01':
                raw['unlinked'] = start
        expected_rows.append(raw)
    require(isinstance(boot.get('channels'), list) and len(boot['channels']) == 6
            and boot['channels'][:5] == expected_rows,
            'Original shared/frozen raw inputs changed')
    users = boot.get('users', [])
    require([user.get('id') for user in users] == ['UWORKFLOW', 'UHOME0001']
            and all(isinstance(user.get('profile'), dict) for user in users)
            and boot['channels'][5].get('user') == 'UHOME0001',
            'Initial canonical member seed differs from the supported synthetic identities')
    observation = channel.get('labels')
    if observation is None:
        return  # No workflow document exists in the initial About stage.
    require(isinstance(observation, dict), 'Malformed canonical label observation')
    if not observation.get('current'):
        return  # Before detail/after navigation is not a current detail observation.
    require(observation.get('plan') == expected_plan, 'Current PlanInfo differs from accepted input')
    rows = observation.get('rows')
    require(isinstance(rows, list) and len({r.get('id') for r in rows}) == len(rows),
            'Missing or duplicate canonical label observations')
    by_id = {r['id']: r for r in rows}
    require({'CACTKNOWN01', *spec.ids} <= set(by_id), 'Missing canonical shared/frozen rows')
    for id in ('CACTKNOWN01', *spec.ids):
        row = by_id[id]
        raw, policy = row.get('raw', {}), row.get('policy', {})
        require(raw.get('id') == id and raw.get('fromAnotherTeam') is False,
                'Ordinary-client canonical row missing')
        expected_raw = next(r for r in expected_rows if r['id'] == id)
        # The existing canonical channel reducer drops wire-only is_shared;
        # ext/org sharing and connected Team objects remain source facts.
        require('is_shared' not in raw, 'Wire-only is_shared leaked into canonical row')
        for key in ('name', 'is_private', 'is_archived', 'is_frozen', 'unlinked',
                    'grace_period_end', 'is_ext_shared', 'is_org_shared',
                    'connected_teams'):
            require(raw.get(key) == expected_raw.get(key),
                    'Canonical label inputs differ from original source')
        require(isinstance(policy, dict) and type(policy.get('capturedNowMs')) is int,
                'Missing actual background clock observation')
        now = policy['capturedNowMs']
        require(start * 1000 < now < (start + 7200) * 1000, 'Clock is not contemporaneous with input')
        grace = spec.grace and id != 'CGRNOGT01'
        require(policy.get('inGrace') is grace, 'Wrong original grace result')
        if grace:
            end = policy.get('graceEndMs')
            expected_end = int((datetime.fromtimestamp(start) + timedelta(days=5)).timestamp() * 1000)
            require(type(end) is int and end == expected_end and now < end,
                    'Deadline differs from five actual local calendar days')
        else:
            require(policy.get('graceEndMs') is None, 'Structural-false case invented a deadline')
        kind = next(kind for _, channel_id, _, kind in spec.vectors() if channel_id == id)
        require(policy.get('private') is (id in spec.private_ids)
                and policy.get('archiveIcon') is (kind in ('Archive', 'ArchiveFilled'))
                and policy.get('archivedSuffix') is (id == 'CSHARCH01'),
                'Independent icon/suffix source policy differs')
