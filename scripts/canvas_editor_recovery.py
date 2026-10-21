"""Independent original-request, canonical adoption, cooldown and draft oracle."""
import copy
import math
from urllib.parse import urlsplit
import canvas_root_attribution as prior
from canvas_root_attribution import same, require, geometry

STAGES = ('stream', 'draft', 'profile', 'closed', 'recovering', 'recovered',
          'reopened', 'returned', 'offline', 'cooldown', 'expired', 'retirement-held',
          'retired', 'passive-unavailable', 'final-recovered')
READS = (1,) + (2,) * 11 + (3, 4, 4)
POLICIES = (0, 2, 2, 2, 2, 4, 4, 4, 4, 4, 4, 4, 6, 6, 8)
MEMBERS = (0, 0, 0, 0, 1, 1, 1, 1, 2, 3, 3, 4, 4, 4, 5)
KINDS = ('hidden', 'known', 'unavailable', 'unavailable', 'pending', 'known',
         'unavailable', 'unavailable', 'unavailable', 'unavailable', 'unavailable',
         'pending', 'known', 'unavailable', 'known')
ISOLATION = dict(prior.ISOLATION, fixture='canvas-editor-recovery')
LABEL = 'Editor Recovered'


def requested(row, path, body):
    require(row['method'] == 'POST', 'Recovery issued non-POST')
    u = urlsplit(row['url'])
    host = 'canvas-attribution-fixture.slack.com' if path.startswith('/api/') else 'edgeapi.slack.com'
    require(u.scheme == 'https' and u.netloc == host and u.path == path and not u.fragment,
            'Recovery used foreign endpoint')
    require(same(row['body'], body), 'Recovery original request body changed')


def recovery_reads(proof, count):
    requests = proof['requests']
    require(len(requests) == count * 2, 'Discovery/member count changed')
    modes = ('held', 'offline', 'rate-limited', 'held', 'success')
    for i in range(count):
        discovery, member = requests[2*i:2*i+2]
        require(discovery['kind'] == 'discovery' and member['kind'] == 'member'
                and member['serial'] == i+1 and member['mode'] == modes[i], 'Original request order/identity changed')
        requested(discovery['request'], '/api/api.getFlannelHttpUrl',
                  [['include_external_workspaces', 'true'], ['_x_reason', 'client-needs-valid-flannel-url']])
        require(discovery['response'] == {'ok': True, 'url': 'https://edgeapi.slack.com/cache/T1', 'ttl_seconds': 300},
                'Original discovery response changed')
        requested(member['request'], '/cache/T1/users/info',
                  {'ids': [prior.EDITOR], 'check_interaction': True, 'include_profile_only_users': True})
        if i in (1, 2):
            require(member['response'] is None, 'Typed failure invented positive response')
        else:
            raw = member['response']
            require(raw.get('ok') is True and len(raw['results']) == 1
                    and raw['can_interact'] == {prior.EDITOR: True}, 'Positive member response changed')
            user = raw['results'][0]
            require(user['id'] == prior.EDITOR and user['team_id'] == 'T1' and user['updated'] == 102
                    and user['name'] == 'fixture_' + prior.EDITOR and user['real_name'] == 'Recovered Real'
                    and user['profile']['display_name'] == LABEL and user['profile']['real_name'] == 'Recovered Real'
                    and user['profile']['title'] == 'Synthetic loaded profile'
                    and user['is_bot'] is False and user['is_app_user'] is False and user['deleted'] is False,
                    'Fresh exact canonical editor changed')


def geometry_checks(layout, index):
    geometry(layout['target'], layout['viewport'])
    a = layout['attribution']
    require(a['state'] == KINDS[index], 'Wrong recovery presentation')
    label = ('Editor Display' if index == 1 else LABEL if index in (5, 14)
             else 'Other Display' if index == 12 else None)
    editor = prior.OTHER if index == 12 else prior.EDITOR if label is not None else None
    require((a['id'], a['name'], a['interactive']) == (editor, label, label is not None),
            'Wrong recovered name/identity/action')
    if label is not None:
        prior.attribution_geometry(a, STAGES[index], layout['viewport'])
    else:
        require(a['label'] is None and a['avatar'] is None and a['avatars'] == [],
                'Unavailable/pending attribution leaked original control')
        has_retry = index in (2, 3, 6, 7, 8, 10, 13)
        require((a['retry'] is not None) == has_retry, 'Wrong recovery/cooldown retry eligibility')
        if has_retry:
            retry = a['retry']
            require(retry['text'] == 'Retry editor' and retry['disabled'] is False
                    and retry['rect']['height'] == 32 and retry['scrollWidth'] <= retry['clientWidth'] + 1,
                    'Retry editor clipped/wrapped')
            if index not in (2, 6, 7): geometry(retry, layout['viewport'])
    if index in (2, 6):
        p = layout['profile']
        expected = 'Editor Display' if index == 2 else LABEL
        title = 'Synthetic loaded profile' if index == 2 else 'Synthetic reloaded profile'
        require(p['visible'] is True and p['name'] == expected
                and p['identity']['name']['text'] == expected and p['identity']['title']['text'] == title,
                'Admitted hydrated profile not visibly retained')
        for v in (p['close'], p['identity']['name'], p['identity']['title']): geometry(v, layout['viewport'])
    else: require(layout['profile'] is None, 'Unexpected profile navigation')


def observation(row, index):
    p = row['proof']
    o = p['retained']['attribution']
    present = index in (1, 2, 3, 5, 6, 7, 12, 14)
    require((o['observed'] is not None) == present and o['loading'] is (index in (4, 11))
            and o['current'] is (index in (1, 5, 12, 14)), 'Original attribution observation custody changed')
    if not present: return
    original = o['observed']
    editor = prior.OTHER if index == 12 else prior.EDITOR
    if index in (1, 2, 3, 12):
        raw = next(v for v in p['boot']['response']['users'] if v['id'] == editor)
    else:
        raw = copy.deepcopy(p['recovery']['requests'][1]['response']['results'][0])
        raw['is_stranger'] = False  # actual project_interaction(can_interact=true)
    require(original['editor'] == editor and original['team'] == 'T1'
            and original['serial'] == p['retained']['serial']
            and same(original['root'], p['retained']['root'])
            and same(original['member'], prior.canonical_member(raw)),
            'Fresh attribution did not bind original canonical intake/root')
    require(original['userPreferences'] == {'ok': True, 'prefs': {'display_real_names_override': 0, 'hidden_users': ''}}
            and original['enterprisePreferences'] is None
            and original['teamPreferences'] == {'T1': {'ok': True, 'prefs': {'display_real_names': False}}},
            'Fresh attribution naming policy changed')


def validate(receipt):
    require(receipt.get('ok') is True and receipt.get('requestCount') == 4
            and receipt.get('memberRequestCount') == 5 and same(receipt.get('isolation'), ISOLATION),
            'Recovery terminal failed')
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r['stage'] for r in rows] == list(STAGES), 'Recovery stages changed')
    draft = None
    for i, row in enumerate(rows):
        require(row['ok'] is True and row['ordinal'] == i and row['noSession'] is False
                and row['noTransport'] is False and same(row['isolation'], ISOLATION), 'Stage owner/capability changed')
        p = row['proof']
        require(same(p['isolation'], ISOLATION), 'Proof owner changed')
        # Reuse the accepted exact comment request/retained-row validator only;
        # its synthetic fixture label is adapted on a copy, not source evidence.
        original = copy.deepcopy(p)
        original['isolation'] = prior.ISOLATION
        prior.source(original, READS[i], prior.OTHER if i == 12 else prior.EDITOR, True)
        require(len(p['policyRequests']) == POLICIES[i], 'Passive/explicit policy reads changed')
        for n, response in enumerate(p['policyResponses']):
            expected = {'ok': True, 'prefs': {'display_real_names_override': 0, 'hidden_users': ''} if n % 2 == 0 else {'display_real_names': False}}
            require(same(response, expected), 'Original policy response changed')
        r = p['recovery']
        require(r['setup']['path'] == 'canonical::fixture_seed' and r['setup']['memberFlagsCurrent'] is True
                and r['setup']['assignments'] == {'ok': True, 'assignments': {}}
                and same(r['setup']['boot'], p['boot']['response']), 'Observed transformation owner missing')
        recovery_reads(r, MEMBERS[i])
        outcomes = [{'serial': 1, 'outcome': 'response'}] if i >= 5 else []
        if i >= 8: outcomes.append({'serial': 2, 'outcome': 'offline'})
        if i >= 9: outcomes.append({'serial': 3, 'outcome': 'rate-limited'})
        if i >= 12: outcomes.append({'serial': 4, 'outcome': 'cancelled'})
        if i >= 14: outcomes.append({'serial': 5, 'outcome': 'response'})
        require(same(r['outcomes'], outcomes), 'Original issued outcomes changed/replayed')
        releases = ([{'serial': 1, 'delivered': True}] if i >= 5 else [])
        if i >= 12: releases.append({'serial': 4, 'delivered': False})
        require(same(r['releases'], releases) and r['held'] == (1 if i == 4 else 4 if i == 11 else None),
                'Held original delivery/cancellation changed')
        require(len(r['hydration']) == (0 if i < 2 else 1 if i < 6 else 2), 'Hydration was replayed/restored')
        for n, h in enumerate(r['hydration']):
            require(h['restore'] is False and h['before']['id'] == h['after']['id'] == prior.EDITOR
                    and h['after']['profile']['title'] == ('Synthetic loaded profile' if n == 0 else 'Synthetic reloaded profile')
                    and h['after']['im_mpim_ids'] == ['DATTR0001'], 'Original profile hydration identity changed')
            expected = copy.deepcopy(h['before'])
            expected['profile']['title'] = h['after']['profile']['title']
            expected.update(im_mpim_ids=['DATTR0001'], has_more_mpims=False)
            require(same(expected, h['after']), 'Synthetic hydration changed unrelated native fields')
        if i in (2, 3, 4): require(same(r['native'], r['hydration'][0]['after']), 'Original hydration silently restored')
        if 6 <= i <= 13: require(same(r['native'], r['hydration'][1]['after']), 'Second hydration silently restored')
        require(len(r['stale']) == (1 if i >= 5 else 0), 'Saved original hover witness missing')
        if i >= 5:
            stale = r['stale'][0]
            require(stale['current'] is False and stale['opened'] is False and same(stale['before'], stale['after'])
                    and stale['before']['profile'] is None, 'Old hover callback acquired successor')
        if i >= 10:
            expiry = r['expiry']
            require(type(expiry['elapsedMillis']) is int and 15000 <= expiry['elapsedMillis'] <= 30000
                    and expiry['memberRequests'] == 3 and expiry['automaticRequests'] == 0, 'Cooldown did not wait or sent automatically')
        else: require(r['expiry'] is None, 'Cooldown proof appeared before actual expiry')
        if i:
            before = rows[i-1]['proof']
            for key in ('requests', 'responses', 'policyRequests', 'policyResponses'):
                require(same(before[key], p[key][:len(before[key])]), 'Original request/response prefix changed')
            for key in ('requests', 'outcomes', 'releases', 'hydration', 'stale'):
                require(same(before['recovery'][key], r[key][:len(before['recovery'][key])]), 'Recovery original receipt prefix changed')
        layout = row['layout']
        require(layout['ok'] is True, 'Native layout assertion failed')
        geometry_checks(layout, i)
        observation(row, i)
        require(layout['rows'][0]['body'] == prior.BODY, 'Original comment body changed')
        if i:
            require(layout['rows'][1]['body'] == 'Ordinary reply sender remains unchanged.'
                    and 'Reply Author' in layout['rows'][1]['header'], 'Ordinary reply sender changed')
            blocks = prior.draft(row)
            prior.rich_content(blocks)
            if draft is None:
                draft = blocks
                require('Attribution draft \u4e2d\U0001f98a' in str(blocks), 'Actual rich draft input missing')
            require(same(draft, blocks), 'Rich reply changed through repair/failure')
            if i <= 11:
                a, b = layout['native'], rows[1]['layout']['native']
                require(a['sameOriginal'] is True and all(a[k] == b[k] for k in ('mount', 'scope', 'generation')),
                        'Recovery replaced original mounted reply')
        else: require(row['reply'] is None and layout['native'] is None, 'Stream unexpectedly has reply')
    require(same(receipt['proof'], rows[-1]['proof']) and same(receipt['layout'], rows[-1]['layout']),
            'Terminal original source changed')
