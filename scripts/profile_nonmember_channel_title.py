"""Explicit English private placeholder; raw source is never a rendered label."""
import copy
import sys

import profile_private_channel_title as private

STAGES = dict(private.STAGES, membership=(
    'cold-loading', 'opened', 'pre-mount-revoked', 'nonmember-placeholder',
    'restored-mounted', 'restored-opened', 'used', 'confirmed'))
COUNTS = dict(private.COUNTS)
RAW_OLD, RAW_NEW = 'must-not-leak', 'renamed-must-not-leak'
PLACEHOLDER = 'Notes for #private-channel'
RESTORED = 'member-only-harbor'


def engine(case):
    # Reuse the exact public/private known contract in an isolated namespace.
    # The earlier membership-refusal oracle is intentionally not this route.
    g = private.engine('known')
    old = dict(g)
    require, same = g['require'], g['same']
    membership = case == 'membership'
    initial_name = old['OLD'] if membership else RAW_OLD
    changed_name = old['NEW'] if membership else RAW_NEW

    def channel(name=initial_name, version=101):
        return dict(old['channel'](name, version), is_member=membership)

    def projected_channel(name=initial_name, version=101):
        return old['projected_channel'](name, version)

    g.update(channel=channel, projected_channel=projected_channel,
             OLD=initial_name, NEW=changed_name, STAGES=STAGES, COUNTS=COUNTS)
    if membership:
        def current_row(stage):
            if stage in ('cold-loading', 'opened'): return projected_channel()
            return dict(projected_channel(RESTORED, 102),
                        is_member=stage not in ('pre-mount-revoked', 'nonmember-placeholder'))
        g['current_row'] = current_row
        g['HELD'] = {'opened', 'restored-opened'}

    def validate_requests(requests, actual_case, epoch):
        require(actual_case == case and isinstance(requests, list)
                and len(requests) == COUNTS[case][-1], 'Wrong placeholder conditional count')
        for serial, request in enumerate(requests, 1):
            if membership:
                row = projected_channel() if serial == 1 else g['current_row']('restored-opened')
            else:
                row = projected_channel(RAW_NEW, 102) if case == 'retirement' and serial >= 3 else projected_channel()
            method = 'conversations.genericInfo' if case == 'failure' and serial == 3 else 'channels.info'
            require(request.get('serial') == serial and request.get('method') == method
                    and request.get('ids') == [g['CHANNEL']]
                    and request.get('versions') == {g['CHANNEL']: row['updated']},
                    'Wrong exact placeholder request/version/replay')
            before = request.get('predecessor', {})
            require(before.get('account') == g['ACCOUNT'] and before.get('epoch') == epoch
                    and before.get('case') == case
                    and before.get('socketGeneration') == (2 if case == 'retirement' and serial >= 4 else 1)
                    and same(before.get('channel'), row) and before.get('nativeName') == row['name']
                    and same(before.get('seed'), g['seed']()) and before.get('networkCalls') == 0,
                    'Wrong exact placeholder source predecessor')
            if membership:
                results = [dict(channel(RESTORED, 102), is_member=False)] if serial == 1 else []
                expected = dict(raw=dict(results=results, member_channels=[]))
            elif case == 'failure' and serial in (2, 3):
                expected = dict(error='closed_failure')
            else:
                changed = ((case in ('known', 'retirement') and serial == 2) or case == 'cancel'
                           or (case == 'failure' and serial == 4) or (case == 'retirement' and serial in (3, 5)))
                name, version = (('retired-socket-name', 999) if case == 'retirement' and serial == 3 else
                                 ('retired-account-name', 1000) if case == 'retirement' and serial == 5 else (RAW_NEW, 102))
                expected = dict(raw=dict(results=[channel(name, version)] if changed else [], member_channels=[]))
            require(same(request.get('response'), expected), 'Placeholder response/raw membership changed')

    def exercise(value, stage, count, epoch):
        expected = {
            'pre-mount-revoked': dict(oldOpeningRefused=True, rawDraftRetained=True),
            'nonmember-placeholder': dict(placeholderAdmitted=True, suppliedNameNotUsed=True, rawDraftRetained=True),
            'restored-mounted': dict(event=dict(type='channel_joined', channel=dict(id=g['CHANNEL']),
                event_ts='2000000002.0'), mountedPlaceholderRetained=True, rawDraftRetained=True),
            'restored-opened': dict(conditional=2, freshOpening=True),
        }
        if membership and stage in expected:
            require(same(value, expected[stage]), 'Missing placeholder/membership/mounted custody proof')
        else: old['exercise'](value, stage, count, epoch)

    def geometry(value, viewport, stage, h, actual_case):
        mapped = ('file-cancelled' if stage == 'pre-mount-revoked' else
                  'opened' if stage in ('nonmember-placeholder', 'restored-mounted', 'restored-opened') else stage)
        rich = mapped not in old['HIDDEN']
        if rich:
            expected = (('Notes for #' + old['OLD']) if stage == 'opened' else
                        ('Notes for #' + RESTORED) if stage in ('restored-opened', 'confirmed') else PLACEHOLDER) if membership else PLACEHOLDER
            require([t.get('text') for t in value.get('tokens', [])] == [expected, expected]
                    and [t.get('label') for t in value.get('captured', {}).get('tokens', [])] == [expected, expected],
                    'Placeholder/new-opening label or captured token custody changed')
        # Source/debug/sidebar are deliberately outside this assertion. Every
        # title/editor token, raw captured description and feedback is checked.
        forbidden = [RAW_OLD, RAW_NEW, 'retired-socket-name', 'retired-account-name']
        if membership and stage not in ('restored-opened', 'used', 'confirmed'): forbidden.append(RESTORED)
        import json
        rendered = json.dumps({k: value.get(k) for k in ('tokens', 'captured', 'notices', 'controls')})
        require(all(name not in rendered for name in forbidden), 'Supplied private name entered title/editor evidence')
        adjusted = copy.deepcopy(value)
        if rich:
            legacy = 'Notes for #' + (g['NEW'] if mapped in ('confirmed', 'reopened', 'socket-retired') else g['OLD'])
            for token in adjusted['tokens']: token['text'] = legacy
            for token in adjusted['captured']['tokens']: token['label'] = legacy
        old['geometry'](adjusted, viewport, mapped, h, actual_case)

    g.update(validate_requests=validate_requests, exercise=exercise, geometry=geometry)
    return g


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    g = engine(case)
    require = g['require']
    require(receipt.get('privateChannelTitle') is True and receipt.get('nonmemberChannelTitle') is True,
            'Missing explicit private placeholder receipt mode')
    sources = [row.get('proof', {}).get('source', {}) for row in receipt.get('stages', [])]
    sources += [request.get('predecessor', {}) for request in receipt.get('requests', [])]
    require(all(s.get('privateChannelTitle') is True and s.get('nonmemberChannelTitle') is True for s in sources),
            'Source lost explicit private placeholder mode')
    return g['validate'](fixture, receipt, captures, h, case=case, platform=platform, png=png)
