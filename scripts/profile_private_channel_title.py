"""Private-member title evidence, with explicit refusal and restored-source custody.

The public oracle is loaded into a fresh namespace per validation. Its exact
wire, layout, Save and closure contracts stay intact; no shared module is patched.
"""
import copy
from pathlib import Path
import runpy
import sys

PUBLIC = Path(__file__).with_name('profile_channel_title.py')
_public = runpy.run_path(str(PUBLIC))
STAGES = dict(_public['STAGES'], membership=(
    'cold-loading', 'opened', 'pre-mount-revoked', 'nonmember-refused',
    'restored-source', 'restored-opened', 'used', 'confirmed'))
COUNTS = dict(_public['COUNTS'], membership=(0, 1, 1, 1, 1, 2, 2, 2))
RESTORED = 'member-only-harbor'
UNAVAILABLE = ('Document titles are unavailable or changed while opening. Your description '
               'is unchanged. Retry to load document details.')
REFUSED = {'pre-mount-revoked', 'nonmember-refused'}
UNOPENED = {'nonmember-refused', 'restored-source'}


def engine(case):
    """Source-specific expected values without altering the accepted public oracle."""
    namespace = runpy.run_path(str(PUBLIC))
    g = namespace['validate'].__globals__
    old = dict(g)
    require, same = g['require'], g['same']

    def channel(name=g['OLD'], version=101):
        return dict(old['channel'](name, version), is_private=True)

    def projected_channel(name=g['OLD'], version=101):
        row = old['projected_channel'](name, version)
        # projection::public is false for this modern private-channel source.
        del row['is_org_default']
        del row['is_org_mandatory']
        return row

    g.update(channel=channel, projected_channel=projected_channel,
             STAGES=STAGES, COUNTS=COUNTS)
    if case != 'membership':
        return g

    def current_row(stage):
        if stage in ('cold-loading', 'opened'):
            return projected_channel()
        return dict(projected_channel(RESTORED, 102), is_member=stage not in REFUSED)

    g['current_row'] = current_row
    g['HELD'] = {'opened', 'restored-opened'}

    def validate_requests(requests, actual_case, epoch):
        require(actual_case == 'membership' and isinstance(requests, list) and len(requests) == 2,
                'Membership request count changed')
        for serial, request in enumerate(requests, 1):
            row = projected_channel() if serial == 1 else current_row('restored-opened')
            require(request.get('serial') == serial and request.get('method') == 'channels.info'
                    and request.get('ids') == [g['CHANNEL']]
                    and request.get('versions') == {g['CHANNEL']: row['updated']},
                    'Membership selector identity/version/replay changed')
            before = request.get('predecessor', {})
            require(before.get('privateChannelTitle') is True and before.get('case') == 'membership'
                    and before.get('account') == g['ACCOUNT'] and before.get('epoch') == epoch
                    and before.get('socketGeneration') == 1 and same(before.get('channel'), row)
                    and before.get('nativeName') == row['name']
                    and same(before.get('seed'), g['seed']()) and before.get('networkCalls') == 0,
                    'Membership request lost exact private predecessor')
            results = [dict(channel(RESTORED, 102), is_member=False)] if serial == 1 else []
            require(same(request.get('response'), dict(raw=dict(results=results, member_channels=[]))),
                    'Membership response changed raw identity/member authority')

    def validate_cache(value, stage):
        if stage not in REFUSED:
            return old['validate_cache'](value, stage)
        require(isinstance(value, dict) and value.get('current') is False
                and value.get('titleChannels') is None,
                'Nonmember source granted title authority')
        # Reuse every exact file/Quip/opaque check, substituting only the rejected
        # dependency proof after explicitly verifying that it is absent above.
        admitted = copy.deepcopy(value)
        admitted.update(current=True, titleChannels=dict(ids=[g['CHANNEL']], rows=[current_row(stage)]))
        old['validate_cache'](admitted, stage)

    def exercise(value, stage, count, epoch):
        expected = {
            'pre-mount-revoked': dict(oldOpeningRefused=True, rawDraftRetained=True),
            'nonmember-refused': dict(titleAdmissionRefused=True, rawDraftRetained=True),
            'restored-source': dict(event=dict(type='channel_joined', channel=dict(id=g['CHANNEL']),
                event_ts='2000000002.0'), oldOpeningNotResumed=True, rawDraftRetained=True),
            'restored-opened': dict(conditional=2, explicitRetry=True),
        }
        if stage in expected:
            require(same(value, expected[stage]), 'Missing membership refusal/restoration/Retry proof')
        else:
            old['exercise'](value, stage, count, epoch)

    def geometry(value, viewport, stage, h, actual_case):
        if stage in UNOPENED:
            require(isinstance(value, dict), 'Missing unavailable editor geometry')
            controls = value.get('controls', [])
            require(sorted(c.get('text') for c in controls) == sorted([
                'Back', 'Save section', 'Retry description', 'Cancel description edit'])
                and all(c.get('disabled') is (c.get('text') == 'Save section') for c in controls),
                'Unavailable editor Retry/Cancel custody changed')
            require(UNAVAILABLE in [n.get('text') for n in value.get('notices', [])],
                    'Missing unavailable title feedback')
            require(all(RESTORED not in n.get('text', '') for n in value.get('notices', [])),
                    'Nonmember name leaked through title feedback')
            # The public hidden-editor shape supplies exact positive bounds for
            # all controls/notices before inventory normalization.
            for item in controls:
                require(item.get('visible') is True, 'Unavailable action hidden')
                h['rectangle'](item.get('rect'), viewport)
                for fragment in item.get('fragments', []): h['rectangle'](fragment, viewport)
            adjusted = copy.deepcopy(value)
            adjusted['controls'] = [c for c in adjusted['controls'] if c['text'] in ('Back', 'Save section')]
            return old['geometry'](adjusted, viewport, 'file-cancelled', h, actual_case)
        if stage == 'pre-mount-revoked':
            return old['geometry'](value, viewport, 'file-cancelled', h, actual_case)
        if stage not in ('restored-opened', 'confirmed'):
            return old['geometry'](value, viewport, stage, h, actual_case)
        # Pin every actual label before the unchanged public raw-token/style and
        # geometry checks. Labels are never written into the source blocks.
        label = 'Notes for #' + RESTORED
        require([t.get('text') for t in value.get('tokens', [])] == [label, label]
                and [t.get('label') for t in value.get('captured', {}).get('tokens', [])] == [label, label],
                'Restored opening did not use the newly admitted exact title')
        adjusted = copy.deepcopy(value)
        mapped = 'opened' if stage == 'restored-opened' else 'confirmed'
        public_label = 'Notes for #' + (g['OLD'] if mapped == 'opened' else g['NEW'])
        for token in adjusted['tokens']: token['text'] = public_label
        for token in adjusted['captured']['tokens']: token['label'] = public_label
        old['geometry'](adjusted, viewport, mapped, h, actual_case)

    g.update(validate_requests=validate_requests, validate_cache=validate_cache,
             exercise=exercise, geometry=geometry)
    return g


def validate(fixture, receipt, captures, h, *, case, platform=sys.platform, png=True):
    g = engine(case)
    require = g['require']
    require(receipt.get('privateChannelTitle') is True, 'Missing explicit private title receipt mode')
    for row in receipt.get('stages', []):
        require(row.get('proof', {}).get('source', {}).get('privateChannelTitle') is True,
                'Stage source lost private title mode')
    for request in receipt.get('requests', []):
        require(request.get('predecessor', {}).get('privateChannelTitle') is True,
                'Conditional predecessor lost private title mode')
    return g['validate'](fixture, receipt, captures, h, case=case, platform=platform, png=png)
