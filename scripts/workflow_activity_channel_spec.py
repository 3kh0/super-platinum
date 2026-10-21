"""Immutable occurrence expectations for the two closed Activity channel cases."""
from dataclasses import dataclass


@dataclass(frozen=True)
class ChannelSpec:
    special: bool = False

    @property
    def case(self):
        return 'channel-special' if self.special else 'channel'

    @property
    def returns(self):
        return (('returned', 'archived-return', 'private-archived-return', 'final-return')
                if self.special else ('returned', 'final-return'))

    @property
    def destinations(self):
        return (('known', 'archived', 'private-archived', 'retried')
                if self.special else ('known', 'retried'))

    @property
    def archived_destinations(self):
        return ('archived', 'private-archived') if self.special else ()

    @property
    def local_clicks(self):
        return ({'archived': 'workflow-activity-channel-step-1-1',
                 'private-archived': 'workflow-activity-channel-step-1-3'}
                if self.special else {})

    def selected(self, stage, original):
        return dict(known='CACTKNOWN01', archived='CACTARCH01',
                    **{'private-archived': 'CACTPRARCH01', 'retried': 'CACTMISS01'}).get(stage, original)

    def source_ids(self, original, fetched):
        return ({original, 'CACTKNOWN01'}
                | ({'CACTARCH01', 'CACTPRARCH01'} if self.special else set())
                | ({'CACTMISS01'} if fetched else set()))

    def sidebar_ids(self, original, stage, fetched):
        return ({original, 'CACTKNOWN01'} | ({'CACTMISS01'} if fetched else set())
                | ({self.selected(stage, original)}
                   if self.special and stage in ('archived', 'private-archived') else set()))

    def vectors(self, fetched=False):
        # (original element ID, original channel, exact label, expected Material icon)
        return (
            ('workflow-activity-channel-trigger-1', 'CACTKNOWN01', 'activity-known',
             'LockFilled' if self.special else 'Tag'),
            ('workflow-activity-channel-trigger-3', 'CACTKNOWN01', 'activity-known',
             'LockFilled' if self.special else 'Tag'),
            ('workflow-activity-channel-step-0-1', 'CACTMISS01',
             'activity-fetched' if fetched else 'Channel unavailable', 'Tag' if fetched else None),
        ) + ((
            ('workflow-activity-channel-step-1-1', 'CACTARCH01', 'activity-archived (archived)', 'Archive'),
            ('workflow-activity-channel-step-1-3', 'CACTPRARCH01', 'activity-private-archived (archived)', 'ArchiveFilled'),
        ) if self.special else ())

    def occurrences(self):
        return [dict(id=id, channel=channel, text=name, tag='BUTTON')
                for id, channel, name, _ in self.vectors()]

    def inert(self):
        containers = (('workflow-activity-trigger-message', 0),
                      ('workflow-activity-trigger-message', 1),
                      ('workflow-activity-step-message-0', 0),
                      ('workflow-activity-step-message-1', 0),
                      ('workflow-activity-step-message-1', 1))
        return [(*containers[i], id, channel, name)
                for i, (id, channel, name, _) in enumerate(self.vectors())]


ORDINARY = ChannelSpec()
SPECIAL = ChannelSpec(special=True)
NAMES = {'CACTKNOWN01': 'activity-known', 'CACTMISS01': 'activity-fetched',
         'CACTARCH01': 'activity-archived', 'CACTPRARCH01': 'activity-private-archived'}
ARCHIVED_REASON = 'This channel is archived. Unarchive it in channel details to send messages.'
SOURCE_KEYS = frozenset(('id', 'name', 'isMember', 'isOpen', 'unreadCount',
                         'unreadDisplay', 'mentionCount', 'hasUnreads'))
FLAGS = frozenset(('isPrivate', 'isGroup', 'isArchived'))


def validate_source_flags(rows, spec, require):
    if hasattr(spec, 'validate_flags'):
        return spec.validate_flags(rows, require)
    for row in rows:
        require(set(row) == SOURCE_KEYS | (FLAGS if spec.special else frozenset()),
                'Unexpected channel source fields')
        if spec.special:
            require(row['isPrivate'] is (row['id'] in ('CACTKNOWN01', 'CACTPRARCH01'))
                    and row['isGroup'] is False
                    and row['isArchived'] is (row['id'] in ('CACTARCH01', 'CACTPRARCH01')),
                    'Special channel flags differ from original fixture source')
