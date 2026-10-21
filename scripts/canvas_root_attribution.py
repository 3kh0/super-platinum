"""Independent source, metadata, native-node and SQLite checks for attribution."""
import json
import math
import copy
from urllib.parse import urlsplit

STAGES = ('stream', 'thread', 'draft', 'hover', 'profile', 'returned',
          'real-name', 'hidden-name', 'empty-name', 'unavailable', 'hidden-root',
          'missing-editor', 'editor-changed', 'policy-held', 'stale-refused')
READS = (1, 2, 2, 2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11)
POLICY_READS = (0, 2, 2, 2, 2, 2, 3, 5, 7, 7, 7, 7, 9, 10, 12)
ROOT = '1719800000.000100'
AUTHOR = 'UATTR0004'
EDITOR = 'UATTR0001'
OTHER = 'UATTR0002'
EMPTY = 'UATTR0003'
MISSING = 'UATTR0009'
BODY = 'Original Canvas root & retained body.'
ISOLATION = {'syntheticSession': True, 'noHttp': True, 'fixture': 'canvas-root-attribution',
             'account': '147b5480-9f1a-4443-89da-836472cbb209', 'team': 'T1', 'self': 'UATTRSELF'}


def same(left, right):
    return json.dumps(left, sort_keys=True, ensure_ascii=False, allow_nan=False) == json.dumps(right, sort_keys=True, ensure_ascii=False, allow_nan=False)


def require(value, why):
    if not value:
        raise RuntimeError(why)


def fields(row):
    pairs = row.get('fields')
    require(isinstance(pairs, list) and all(isinstance(p, list) and len(p) == 2 for p in pairs), 'Malformed original request')
    result = dict(pairs)
    require(len(result) == len(pairs) and 'token' not in result, 'Duplicate or secret request field')
    return result


def geometry(value, viewport):
    require(isinstance(value, dict) and value.get('inView') is True and value.get('hit') is True, 'Target not visible/hit')
    r = value.get('rect', {})
    require(set(r) == {'x', 'y', 'width', 'height'}, 'Missing target rectangle')
    require(all(type(v) in (int, float) and math.isfinite(v) for v in r.values()), 'Nonfinite target rectangle')
    require(r['x'] >= 0 and r['y'] >= 0 and r['width'] > 0 and r['height'] > 0
            and r['x'] + r['width'] <= viewport[0] + 1
            and r['y'] + r['height'] <= viewport[1] + 1, 'Target rectangle outside viewport')


def attribution_geometry(value, stage, viewport):
    owns_header = stage not in ('draft', 'returned', 'hover', 'profile')
    label, avatar, retry = (value.get(k) for k in ('label', 'avatar', 'retry'))
    if value['state'] == 'known':
        require(isinstance(label, dict) and isinstance(avatar, dict), 'Missing attribution control geometry')
        require(avatar['rect']['width'] == 24 and avatar['rect']['height'] == 24, 'Attribution avatar crushed')
        require(label['text'] == value['name'], 'Measured attribution label changed')
        if label['text']:
            require(len(label['lineRects']) == 1 and label['fontSize'] == 14
                    and label['scrollWidth'] <= label['clientWidth'] + 1, 'Attribution name wraps or clips')
            line = label['lineRects'][0]
            require(all(type(v) in (int, float) and math.isfinite(v) for v in line.values())
                    and line['width'] > 0 and line['height'] > 0, 'Invalid attribution text rectangle')
            if owns_header: geometry(label, viewport)
        if owns_header: geometry(avatar, viewport)
        require(retry is None, 'Known attribution unexpectedly offers retry')
    else:
        require(label is None and avatar is None, 'Suppressed attribution retains interactive controls')
    if value['state'] == 'unavailable':
        require(isinstance(retry, dict) and retry['text'] == 'Retry editor' and retry['disabled'] is False
                and retry['rect']['height'] == 32 and retry['scrollWidth'] <= retry['clientWidth'] + 1,
                'Retry editor wraps, clips, or is disabled')
        if owns_header: geometry(retry, viewport)
    else:
        require(retry is None, 'Unexpected attribution retry')


def source(proof, count, editor, visible):
    require(same(proof['isolation'], ISOLATION), 'Wrong synthetic owner/no-HTTP capability')
    requests, responses = proof['requests'], proof['responses']
    require(len(requests) == len(responses) == count, 'Comment read count changed')
    for index, (request, response) in enumerate(zip(requests, responses)):
        f = fields(request)
        method = 'conversations.history' if index == 0 else 'conversations.replies'
        url = urlsplit(request['url'])
        require(url.scheme == 'https' and url.netloc == 'fixture.slack.com' and url.path == '/api/' + method
                and not url.fragment and request['method'] == 'POST', 'Unexpected comment method')
        expected = {'channel': 'Cfixture', 'inclusive': 'true', 'limit': '28' if index else '20', 'cached_latest_updates': '{}'}
        if index: expected['ts'] = ROOT
        else: expected.update(document_stream='latest_reply', ignore_replies='true', include_pin_count='true', no_user_profile='true', include_stories='true', include_free_team_extra_messages='true', include_date_joined='false')
        require(f == expected, 'Comment binding/write changed')
        require(response.get('ok') is True and response.get('has_more') is False, 'Original comment read failed')
    raw = responses[-1]['messages'][0]
    require(raw.get('user') == AUTHOR and raw.get('ts') == ROOT and raw.get('text') == BODY, 'Original author/body changed')
    comment = raw['document_comment']
    require(comment.get('most_recent_editor') == editor and comment.get('is_visible') is visible
            and comment.get('thread_id') == 'fixture-attribution-root', 'Original editor/control changed')
    if editor is None:
        require('most_recent_editor' not in comment, 'Missing editor became explicit value')
    retained = proof['retained']
    require(retained['ownerCurrent'] is True and retained['loading'] is False, 'Retained owner/read not current')
    require(type(retained['serial']) is int and retained['serial'] > 0, 'No original read serial')
    expected = responses[-1]['messages']
    require(len(retained['rows']) == len(expected), 'Retained row count changed')
    for actual, original in zip(retained['rows'], expected):
        for key in ('ts', 'user', 'text', 'subtype', 'document_comment', 'thread_ts'):
            require(same(actual.get(key), original.get(key)), 'Retained original field changed: ' + key)
    if count > 1:
        require(same(retained['root']['document_comment'], comment)
                and retained['root']['user'] == AUTHOR, 'Retained root custody changed')
    boot = proof['boot']
    require(boot['accepted'] is True and boot['response']['self']['id'] == ISOLATION['self'], 'Canonical boot owner missing')
    require(boot['canonicalIntake']['operation'] == 'ordinary-toLowerCase', 'Canonical intake absent')
    members = {row['id']: row for row in boot['response']['users']}
    require(EDITOR in members and OTHER in members and EMPTY in members and MISSING not in members, 'Canonical membership controls changed')
    for request, response in zip(proof['policyRequests'], proof['policyResponses']):
        f = fields(request)
        method = request['url'].split('?')[0].rsplit('/', 1)[-1]
        url = urlsplit(request['url'])
        require(method in ('users.prefs.get', 'team.prefs.get') and url.scheme == 'https'
                and url.netloc == 'canvas-attribution-fixture.slack.com' and not url.fragment
                and request['method'] == 'POST', 'Unexpected policy request')
        require(f == ({} if method == 'users.prefs.get' else {'target_team': 'T1'}), 'Wrong policy workspace')
        require(response.get('ok') is True and isinstance(response.get('prefs'), dict), 'Policy receipt malformed')
    require(len(proof['policyRequests']) == len(proof['policyResponses']), 'Policy original response lost')


def draft(row):
    value = row['reply']
    require(value['runtime']['saving'] is False and value['runtime']['loading'] is False
            and value['runtime']['sending'] is False and value['runtime']['error'] is None, 'Reply not settled')
    journal = value['journal']
    require(journal['attempt'] is None and journal['history'] == [] and len(journal['drafts']) == 1, 'Unexpected write or journal versions')
    blocks = journal['drafts'][0]['blocks']
    require(same(blocks, value['runtime']['blocks']) and same(blocks, row['layout']['native']['blocks']), 'Native/runtime/SQLite blocks differ')
    require(journal['drafts'][0]['capture'] == value['runtime']['capture'], 'Original capture ID differs')
    return blocks


def rich_content(blocks):
    leaves = []
    def walk(value):
        if isinstance(value, dict):
            if value.get('type') in ('text', 'link', 'user'): leaves.append(value)
            for child in value.values(): walk(child)
        elif isinstance(value, list):
            for child in value: walk(child)
    walk(blocks)
    require(any(v.get('text') == 'Review the ' and v.get('style', {}).get('bold') is True for v in leaves), 'Original bold reply lost')
    require(any(v.get('type') == 'link' and v.get('url') == 'https://example.invalid/plan'
                and v.get('text') == 'launch plan' and v.get('style', {}).get('italic') is True for v in leaves), 'Original italic link lost')
    require(any(v.get('type') == 'user' and v.get('user_id') == 'U1' for v in leaves), 'Original mention lost')
    require('Keep the exact formatting and \U0001f600.' in ''.join(v.get('text', '') for v in leaves), 'Original Unicode reply lost')


def policy_transcript(proof):
    sequence = [('u', 0, ''), ('t', None, None), ('u', 1, ''), ('u', 0, EDITOR), ('t', None, None),
                ('u', 0, ''), ('t', None, None), ('u', 0, ''), ('t', None, None),
                ('u', 0, ''), ('u', 0, ''), ('t', None, None)]
    require(len(proof['policyRequests']) == len(sequence), 'Final policy read set differs')
    for (kind, real, hidden), request, response in zip(sequence, proof['policyRequests'], proof['policyResponses']):
        expected = {'ok': True, 'prefs': {'display_real_names_override': real, 'hidden_users': hidden}
                    if kind == 'u' else {'display_real_names': False}}
        require(same(response, expected), 'Original numeric naming preference receipt changed')
        require(request['url'].split('?')[0].endswith('/' + ('users.prefs.get' if kind == 'u' else 'team.prefs.get')), 'Original preference sequence changed')


def canonical_member(raw):
    """Exact ASCII-only fixture intake, including production derived fields.

    member_hydration/profile.rs effective supplies first/last defaults; derive
    retains original fields and appends the original lowercasing observations.
    This does not permit arbitrary enrichment or normalize original identities.
    """
    value = copy.deepcopy(raw)
    profile = value['profile']
    for key in ('first_name', 'last_name'):
        if profile.get(key) is None: profile[key] = ''
    fields = {'_name_lc': value['name'], '_first_name_lc': profile['first_name'],
              '_last_name_lc': profile['last_name'], '_real_name_lc': profile['real_name'],
              '_display_name_lc': profile['display_name'], '_real_name_normalized_lc': profile['real_name'],
              '_display_name_normalized_lc': profile['display_name'], '_email_normalized_lc': profile.get('email', '')}
    require(all(isinstance(v, str) and v.isascii() for v in fields.values()), 'Fixture source exceeded exact ASCII intake')
    value.update({k: v.lower() for k, v in fields.items()})
    for key in ('real_name', 'display_name'):
        if profile.get(key + '_normalized') is None: profile[key + '_normalized'] = profile[key]
    return value


def validate(receipt):
    require(receipt.get('ok') is True and receipt.get('requestCount') == 11, 'Attribution terminal failed')
    require(same(receipt.get('isolation'), ISOLATION), 'Terminal context changed')
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES), 'Attribution stages changed')
    expected = [('hidden', None, None)] + [('known', EDITOR, 'Editor Display')] * 5 + [
        ('known', EDITOR, 'Editor Real'), ('known', EDITOR, 'Name hidden'), ('known', EMPTY, ''),
        ('unavailable', None, None), ('hidden', None, None), ('hidden', None, None),
        ('known', OTHER, 'Other Display'), ('pending', None, None), ('known', OTHER, 'Other Display')]
    expected[4] = ('unavailable', None, None)  # normal profile hydration retires future attribution actions
    sources = [EDITOR] * 8 + [EMPTY, MISSING, EDITOR, None, OTHER, EDITOR, OTHER]
    initial_draft = None
    for index, row in enumerate(rows):
        require(row.get('ok') is True and row.get('ordinal') == index and row.get('noSession') is False
                and row.get('noTransport') is False and same(row['isolation'], ISOLATION), 'Stage original context changed')
        source(row['proof'], READS[index], sources[index], index != 10)
        require(len(row['proof']['policyRequests']) == POLICY_READS[index], 'Policy acquisition count differs')
        if index:
            for key in ('requests', 'responses', 'policyRequests', 'policyResponses'):
                before = rows[index-1]['proof'][key]
                require(same(before, row['proof'][key][:len(before)]), 'Original request/response prefix changed')
        layout = row['layout']
        require(layout['ok'] is True, 'Native target assertion failed')
        geometry(layout['target'], layout['viewport'])
        a = layout['attribution']
        require((a['state'], a['id'], a['name']) == expected[index], 'Wrong attribution label/editor/state')
        require(a['interactive'] is (a['state'] == 'known'), 'Wrong interaction eligibility')
        attribution_geometry(a, row['stage'], layout['viewport'])
        observation = row['proof']['retained']['attribution']
        if a['state'] == 'known' or index == 4:
            observed = observation['observed']
            expected_editor = sources[index]
            require(observation['current'] is (index != 4) and observation['loading'] is False
                    and observed['editor'] == expected_editor and observed['team'] == 'T1'
                    and observed['serial'] == row['proof']['retained']['serial'], 'Original acquired attribution lease changed')
            require(same(observed['root'], row['proof']['retained']['root']), 'Acquired attribution root differs')
            member = next(v for v in row['proof']['boot']['response']['users'] if v['id'] == expected_editor)
            require(same(observed['member'], canonical_member(member)), 'Acquired canonical member substituted')
            user_prefs = {'ok': True, 'prefs': {'display_real_names_override': 1 if index == 6 else 0,
                                             'hidden_users': EDITOR if index == 7 else ''}}
            require(same(observed['userPreferences'], user_prefs) and observed['enterprisePreferences'] is None
                    and same(observed['teamPreferences'], {} if index == 6 else {'T1': {'ok': True, 'prefs': {'display_real_names': False}}}), 'Acquired naming policy differs')
        else:
            require(observation['observed'] is None and observation['current'] is False
                    and observation['loading'] is (index == 13), 'Suppressed/pending state borrowed old observation')
        if a['state'] in ('hidden', 'unavailable', 'pending'):
            require(a['avatars'] == [], 'Suppressed attribution leaked avatar')
        else:
            require(all(v['ready'] is True and 'example.test' not in v['src'] for v in a['avatars']), 'Avatar bypassed ready/opaque media path')
        require(layout['rows'][0]['body'] == ('This content was deleted.' if index == 10 else BODY), 'Root body changed')
        if index:
            require(layout['rows'][1]['body'] == 'Ordinary reply sender remains unchanged.'
                    and 'Reply Author' in layout['rows'][1]['header'], 'Ordinary reply sender changed')
            blocks = draft(row)
            rich_content(blocks)
            if index == 1:
                require('Attribution draft' not in json.dumps(blocks), 'Draft typed before actual input')
            elif index == 2:
                initial_draft = blocks
                require('Attribution draft \u4e2d\U0001f98a' in json.dumps(blocks, ensure_ascii=False), 'Actual Unicode input missing')
            else:
                require(same(initial_draft, blocks), 'Rich reply changed through attribution action')
        else:
            require(row['reply'] is None and layout['native'] is None, 'Stream unexpectedly mounted reply')
        if 2 <= index <= 5:
            n = layout['native']
            original = rows[2]['layout']['native']
            require(n['sameOriginal'] is True and all(n[k] == original[k] for k in ('mount', 'scope', 'generation')), 'Original reply node replaced')
        if index == 3:
            require(layout['hover']['name'].startswith('Editor Display') and layout['hover']['actions'] == 0, 'Guarded hover mismatch')
        if index == 4:
            require(layout['profile']['name'] == 'Editor Display' and layout['profile']['visible'] is True, 'Profile not visibly scoped')
            geometry(layout['profile']['close'], layout['viewport'])
            identity = layout['profile']['identity']
            require(identity['name']['text'] == 'Editor Display'
                    and identity['title']['text'] == 'Synthetic loaded profile', 'Visible profile identity changed')
            geometry(identity['name'], layout['viewport'])
            geometry(identity['title'], layout['viewport'])
            h = row['proof']['hydration']
            require(len(h) == 1 and h[0]['restore'] is False and h[0]['before']['id'] == EDITOR
                    and h[0]['after']['id'] == EDITOR and h[0]['after']['profile']['title'] == 'Synthetic loaded profile'
                    and h[0]['after']['im_mpim_ids'] == ['DATTR0001'], 'Same-user hydration proof missing')
        if index == 5:
            require(layout['profile'] is None, 'Profile failed to return to original Canvas')
            h = row['proof']['hydration']
            require(len(h) == 2 and h[1]['restore'] is True and same(h[0]['after'], h[1]['before'])
                    and same(h[0]['before'], h[1]['after']), 'Synthetic hydration restoration changed source')
    require(rows[-2]['proof']['held'] is True and rows[-1]['proof']['held'] is False
            and rows[-1]['proof']['releases'] == [{'delivered': False}], 'Old policy response acquired successor')
    require(same(receipt['proof'], rows[-1]['proof']) and same(receipt['layout'], rows[-1]['layout']), 'Terminal original source changed')
    policy_transcript(receipt['proof'])
