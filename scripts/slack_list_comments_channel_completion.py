"""Closed List channel completion: native token custody and exact source authority."""
import copy
import json
import math
import runpy
import sys
from pathlib import Path

R = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_newer_draft.py')))
X = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_actions.py')))
C, READ, H = R['C'], R['READ'], R['H']
require, rectangle = R['require'], R['rectangle']
STAGES = {
    'channel-completion': ('draft', 'local-held', 'refreshed', 'keyboard-inserted',
                           'mouse-inserted', 'reopened', 'sending', 'sent', 'readback'),
    'channel-completion-custody': ('draft', 'held', 'query-retired', 'stale-refused',
                                   'malformed', 'failure', 'empty', 'cancelled-reopened'),
}
LOCAL, PRIVATE, REMOTE = 'CLISTLOCAL', 'GLISTPRIVATE', 'CLISTREMOTE'
LABELS = {LOCAL: '#project-local', PRIVATE: '#project-private', REMOTE: '#project-cross-team'}
METHODS = {'channels/search', 'users.prefs.get', 'experiments.getByUser', 'client.gantryBoot',
           'api.getFlannelHttpUrl', 'drafts.list', 'drafts.listActive',
           'search.autocomplete.model', 'search.autocomplete.offlineFeatures'}
FAILURE = 'Channel suggestions could not load. Edit your query to try again.'
EMPTY = 'No matching channels'


def exact(left, right):
    return type(left) is type(right) and (all(k in right and exact(v, right[k]) for k, v in left.items())
        and len(left) == len(right) if isinstance(left, dict) else
        len(left) == len(right) and all(exact(a, b) for a, b in zip(left, right)) if isinstance(left, list) else left == right)


def validator(case):
    require(case in STAGES, 'Unknown channel completion case')
    return lambda fixture, receipt, directory, **kw: validate(fixture, receipt, directory, expected_case=case, **kw)


def rich(blocks):
    """Compare exact styled characters and opaque tokens, permitting only text-run splitting."""
    require(isinstance(blocks, list), 'Missing rich blocks')
    result = []
    for block in blocks:
        require(block.get('type') == 'rich_text' and isinstance(block.get('elements'), list), 'Wrong rich block')
        sections = []
        for section in block['elements']:
            require(section.get('type') == 'rich_text_section' and isinstance(section.get('elements'), list), 'Wrong rich section')
            values = []
            for value in section['elements']:
                kind = value.get('type')
                if kind == 'text':
                    require(isinstance(value.get('text'), str), 'Invalid text leaf')
                    metadata = {k: v for k, v in value.items() if k != 'text'}
                    values.extend(('text', char, metadata) for char in value['text'])
                else:
                    require(kind in ('channel', 'user', 'link'), 'Unexpected rich inline element')
                    values.append(('opaque', value))
            sections.append(({k: v for k, v in section.items() if k != 'elements'}, values))
        result.append(({k: v for k, v in block.items() if k != 'elements'}, sections))
    return result


def channels(blocks):
    return [v for v in R['nodes'](blocks) if v.get('type') == 'channel']


def display(blocks):
    paragraphs = []
    for block in blocks:
        for section in block['elements']:
            text = ''
            for v in section['elements']:
                kind = v['type']
                if kind == 'channel':
                    require(v.get('channel_id') in LABELS, 'Unknown channel token')
                    text += LABELS[v['channel_id']]
                elif kind == 'user': text += '@' + v['user_id']
                else: text += v.get('text', '')
            paragraphs.append(text)
    return '\n'.join(paragraphs)


def closed_source(source, model):
    require(isinstance(source, dict) and all(source.get(k) is True for k in
        ('enabled', 'named', 'protectedFixture', 'identityCurrent')), 'Missing named protected identity')
    require(source.get('httpCapability') is False and exact(source.get('networkCalls'), 0), 'Completion has HTTP capability')
    require(source.get('account') == model.get('account') and R['uuid'](source.get('account'))
        and exact(source.get('epoch'), model.get('epoch')) and R['integer'](source.get('epoch'))
        and source.get('team') == 'T1' and source.get('user') == 'ULIST0001', 'Completion identity changed')
    requests = source.get('requests')
    require(isinstance(requests, list), 'Missing completion requests')
    for serial, request in enumerate(requests, 1):
        require(set(request) == {'serial', 'method', 'args'} and exact(request['serial'], serial)
            and request['method'] in METHODS and isinstance(request['args'], dict), 'Unowned completion request')
        args = request['args']
        require(not {'token', 'enterprise_token', 'cookie', 'cookies'}.intersection(args), 'Secret in request evidence')
        if request['method'] == 'channels/search':
            require({'users.prefs.get', 'experiments.getByUser', 'client.gantryBoot', 'api.getFlannelHttpUrl'}
                <= {prior['method'] for prior in requests[:serial-1]}, 'Search skipped actual preference/canonical routing observations')
            query = args.get('query')
            require(isinstance(query, str) and not query.startswith(('#', '\uff03')) and exact(args, dict(
                query=query, count=25, fuzz=1, filter='xws', include_record_channels=True,
                check_membership=True, top_channels=[], default_workspace='T1')), 'Channel query wire changed')
        else:
            platform = {'win32': 'windows', 'darwin': 'darwin'}.get(sys.platform, sys.platform)
            expected = {
                'users.prefs.get': {},
                'experiments.getByUser': {'ssb_platform': platform, 'ssb_release_channel': 'prod'},
                'client.gantryBoot': {'include_channels': 'false', '_x_reason': 'fetch-client-data'},
                'api.getFlannelHttpUrl': {'include_external_workspaces': 'true', '_x_reason': 'client-needs-valid-flannel-url'},
                'drafts.list': {'is_active': 'true', 'limit': '25', '_x_reason': 'home-workflow-draft-presence'},
                'drafts.listActive': {'limit': '1000', '_x_reason': 'home-workflow-draft-presence'},
                'search.autocomplete.model': {'_x_reason': 'AutocompleteModelCache'},
                'search.autocomplete.offlineFeatures': {'_x_reason': 'search-offline-features'},
            }[request['method']]
            require(exact(args, expected), 'Required observation request changed')
    searches = [r for r in requests if r['method'] == 'channels/search']
    require(all(R['integer'](source.get(k)) for k in ('completed', 'cancelled', 'held'))
        and source['cancelled'] <= source['completed'] <= len(searches)
        and source['held'] <= len(searches), 'Completion lifecycle counters invalid')
    return requests, searches

def source_rows():
    return [dict(id=PRIVATE, name='project-private', team_id='T1', is_channel=True, is_private=True,
                 is_member=True, updated=101, previous_names=[], fixture_opaque={'keep': 'private'}),
            dict(id=REMOTE, name='project-cross-team', team_id='TOTHER001', is_channel=True,
                 is_member=False, updated=101, previous_names=['project-previous'], fixture_opaque={'keep': 'cross-team'})]


def source_receipts(source):
    requests, responses = source['requests'], source.get('responses')
    require(isinstance(responses, list) and len(responses) == len(requests), 'Missing original completion responses')
    for req, reply in zip(requests, responses):
        require(set(reply) == {'serial', 'disposition', 'raw'} and exact(reply['serial'], req['serial']), 'Reply lost original request')
        raw, disposition, method = reply['raw'], reply['disposition'], req['method']
        if method == 'channels/search':
            require(disposition in ('response', 'held', 'offline'), 'Wrong search disposition')
            require(raw is None if disposition == 'offline' else
                    any(exact(raw, candidate) for candidate in (
                        {'ok': True, 'results': source_rows()}, {'ok': True, 'results': []}, {'ok': True, 'results': False})),
                    'Original private/cross-team search response changed')
        else:
            expected = {
                'users.prefs.get': {'ok': True, 'prefs': {'frecency': '{}'}},
                'experiments.getByUser': {'ok': True, 'assignments': {'friendly_channel_names': {'group': 'off'}}},
                'client.gantryBoot': {'ok': True, 'self': {'id': 'ULIST0001', 'team_id': 'T1', 'profile': {}}, 'team': {'id': 'T1'},
                    'workspaces': [{'id': 'T1', 'prefs': {'display_real_names': False}}, {'id': 'TOTHER001', 'prefs': {'display_real_names': False}}], 'default_workspace': 'T1'},
                'api.getFlannelHttpUrl': {'ok': True, 'url': 'https://edgeapi.slack.com/cache/T1', 'ttl_seconds': 300},
                'drafts.list': {'ok': True, 'drafts': [], 'has_more': False},
                'drafts.listActive': {'ok': True, 'active_draft_ids': [], 'has_more': False},
                'search.autocomplete.model': {'ok': True, 'model': None},
                'search.autocomplete.offlineFeatures': {'ok': True},
            }[method]
            require(disposition == 'response' and exact(raw, expected), 'Original required source observation changed')
    user = dict(id='ULIST0001', name='fixture-self', team_id='T1', is_admin=True, profile={})
    seed = dict(accepted=True,
        boot=dict(ok=True, team=dict(id='T1', name='List completion'), self=user, users=[user],
            channels=[dict(id=LOCAL, name='project-local', team_id='T1', is_channel=True, is_member=True, updated=100, previous_names=[])], prefs={}),
        init=dict(ok=True, self=user, team=dict(id='T1', name='List completion'), workspaces=[dict(id='T1'), dict(id='TOTHER001')],
            onboarding=dict(user_data=dict(onboarding_complete=True)), user_date_created=123))
    require(exact(source.get('seed'), seed), 'Original canonical boot/init source changed')
    releases = source.get('releases')
    require(isinstance(releases, list) and len({r.get('serial') for r in releases}) == len(releases), 'Repeated held release')
    held = {r['serial'] for r in responses if r['disposition'] == 'held'}
    for release in releases:
        require(set(release) == {'serial', 'delivered', 'cancelledBeforeReply'} and release['serial'] in held
            and type(release['delivered']) is bool and release['cancelledBeforeReply'] is (not release['delivered']), 'Wrong held response custody')
    require(exact(source['held'], len(held) - len(releases)), 'Held inventory changed')

def seed_blocks():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Existing context '),
        dict(type='user', user_id='U_FIXTURE_OPAQUE', opaque={'retain': [1, 2]}),
        dict(type='text', text=' '),
        dict(type='link', url='https://example.test/reference', text='Reference', opaque={'source': 'fixture'}),
        dict(type='channel', channel_id=LOCAL, style={'bold': True}, opaque={'occurrence': 1}),
        dict(type='channel', channel_id=LOCAL, style={'italic': True}, opaque={'occurrence': 2}),
        dict(type='text', text='\n')])])]


def original_blocks():
    value = seed_blocks()
    first, second = C['REACTION_TEXT'].split('\n')
    # Native insertion consumes the seed's empty terminal LF scaffold, then
    # creates the second section; it does not retain an extra blank line.
    value[0]['elements'][0]['elements'][-1] = dict(type='text', text=first, style={'bold': True})
    value[0]['elements'].append(dict(type='rich_text_section', elements=[dict(type='text', text=second)]))
    return value


def owner_and_records(receipt, rows):
    initial = rows[0]['model']
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
        and set(touched) == {READ['KNOWN'], READ['ALIAS']} and initial['initialDocument'].get('id') == READ['LIST']
        and touched[READ['KNOWN']].get('thread_ts') == READ['ROOT_TS'], 'Wrong original record inventory')
    scope = dict(workspace='T1', user='ULIST0001', list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    seed = rows[0]['proof']['seed']
    require(exact(seed.get('blocks'), seed_blocks()) and seed.get('scope') == scope
        and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong original persisted seed')
    a = rows[0]['proof']['originalA']
    require(exact(rich(a), rich(original_blocks())), 'Original styled/opaque rich content changed')
    for row in rows:
        m, p = row['model'], row['proof']
        require(m.get('account') == initial.get('account') and R['uuid'](m.get('account'))
            and exact(m.get('epoch'), initial.get('epoch')) and R['integer'](m.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False
            and exact(m.get('writes'), 0) and exact(m.get('networkCalls'), 0), 'Wrong closed Context or original owner')
        require(m.get('document') == m.get('initialDocument') == initial['initialDocument']
            and m.get('records') == m.get('initialRecords') == touched
            and m.get('unrelated') == m.get('initialUnrelated') == unrelated, 'Canonical List data changed')
        require(p.get('seed') == seed and p.get('originalA') == a, 'Seed or original draft proof rewritten')
        c = m['comments']
        require(c.get('active') is True and c.get('record') == READ['KNOWN'] and c.get('root') == READ['ROOT_TS']
            and c.get('hasMore') is True and c.get('loading') is False and c.get('error') is None
            and m.get('detail', {}).get('record') == READ['KNOWN'], 'Original comments target changed')
    return initial, scope, a

def source_stages(rows, case):
    success = case == 'channel-completion'
    counts = [0,1,1,1,2,2,2,2,2] if success else [0,1,2,2,3,4,5,6]
    completed = [0,0,1,1,2,2,2,2,2] if success else [0,0,2,2,3,4,5,6]
    cancelled = [0]*9 if success else [0,0,1,1,1,1,1,2]
    held = [0,1,0,0,0,0,0,0,0] if success else [0,1,1,0,0,0,0,0]
    queries = ['project', 'project-previous'] if success else ['project', 'project-new', 'malformed', 'offline', 'empty', 'cancelled']
    dispositions = ['held', 'response'] if success else ['held', 'response', 'response', 'offline', 'response', 'held']
    previous = None
    for i, row in enumerate(rows):
        s = row['model']['completionSource']
        requests, searches = closed_source(s, row['model']); source_receipts(s)
        require([r['args']['query'] for r in searches] == queries[:counts[i]] and exact(s['completed'], completed[i])
            and exact(s['cancelled'], cancelled[i]) and exact(s['held'], held[i]), 'Search lifecycle or query changed')
        responses = {r['serial']: r for r in s['responses']}
        for j, search in enumerate(searches):
            reply = responses[search['serial']]
            raw = None if dispositions[j] == 'offline' else {'ok': True, 'results':
                False if not success and j == 2 else [] if not success and j == 4 else source_rows()}
            require(reply['disposition'] == dispositions[j] and exact(reply['raw'], raw), 'Search outcome rewritten')
        release_count = int(i >= 2) if success else int(i >= 3) + int(i >= 7)
        require(len(s['releases']) == release_count, 'Wrong release count')
        for j, release in enumerate(s['releases']):
            require(release == dict(serial=searches[0 if j == 0 else 5]['serial'], delivered=success,
                                   cancelledBeforeReply=not success), 'Retired request delivered or live request cancelled')
        if previous is not None:
            for key in ('requests', 'responses', 'releases'):
                require(s[key][:len(previous[key])] == previous[key], 'Original source ledger replaced')
            require(s['seed'] == previous['seed'], 'Canonical source seed replaced')
        previous = s

def with_tail(original, values):
    value = copy.deepcopy(original)
    value[0]['elements'][0]['elements'].extend(values)
    return value


def query_blocks(original, query):
    return with_tail(original, [dict(type='text', text=' ' + query)])


def inserted_blocks(original, ids):
    values = []
    for channel in ids:
        values.extend([dict(type='text', text=' '), dict(type='channel', channel_id=channel), dict(type='text', text=' ')])
    return with_tail(original, values)


def expected_blocks(case, index, original):
    if case == 'channel-completion':
        if index == 0: return original
        if index in (1, 2): return query_blocks(original, '#project')
        if index == 3: return inserted_blocks(original, [PRIVATE])
        if index < 7: return inserted_blocks(original, [PRIVATE, REMOTE])
        return []
    queries = [None, '#project', '#project-new', '#project-new', '#malformed', '#offline', '#empty', '#cancelled']
    return original if index == 0 else query_blocks(original, queries[index])

def held_text(geometry):
    """Read-only held Send: retain original hit evidence, independently prove clipping."""
    require(geometry.get('completion', {}).get('captureMode') == 'sending'
        and geometry['completion'].get('open') is False and geometry.get('editor', {}).get('editable') is False
        and geometry['editor'].get('visible') is True, 'Held capture is not the read-only sending editor')
    send = [c for c in geometry.get('controls', []) if c.get('id') == 'slack-list-comment-send']
    require(len(send) == 1 and send[0].get('disabled') is True, 'Held Send control is enabled')
    text = geometry.get('action', {}).get('editorText', {})
    def valid_rect(r):
        return (isinstance(r, dict) and set(r) == {'left','top','right','bottom','width','height'}
            and all(type(r[k]) in (int, float) and math.isfinite(r[k]) for k in r)
            and r['width'] > 0 and r['height'] > 0
            and abs(r['right']-r['left']-r['width']) <= .01 and abs(r['bottom']-r['top']-r['height']) <= .01)
    clip = text.get('clip', {}); require(valid_rect(clip), 'Missing held text clip')
    viewport, editor = geometry['viewport'], geometry['editor']['rect']
    require(clip['left'] >= -1 and clip['top'] >= -1 and clip['right'] <= viewport[0]+1 and clip['bottom'] <= viewport[1]+1
        and clip['left'] >= editor['left']-1 and clip['top'] >= editor['top']-1
        and clip['right'] <= editor['right']+1 and clip['bottom'] <= editor['bottom']+1, 'Held text clip escaped editor/viewport')
    lines = text.get('lines'); require(isinstance(lines, list) and 0 < len(lines) <= 128, 'Missing held text ranges')
    full_count = hit_count = 0
    for line in lines:
        rect, fragments = line.get('rect', {}), line.get('fragments')
        require(valid_rect(rect) and isinstance(fragments, list) and 0 < len(fragments) <= 256
            and all(valid_rect(f.get('rect')) and type(f.get('hit')) is bool for f in fragments), 'Invalid held text fragments')
        for key, extent in (('left',min),('top',min),('right',max),('bottom',max)):
            require(abs(rect[key]-extent(f['rect'][key] for f in fragments)) <= .01, 'Held line differs from actual fragment union')
        full = rect['left'] >= clip['left']-1 and rect['right'] <= clip['right']+1 and rect['top'] >= clip['top']-1 and rect['bottom'] <= clip['bottom']+1
        hit = all(f['hit'] for f in fragments)
        require(line.get('fullVisible') is full and line.get('hit') is hit, 'Held range visibility/hit evidence changed')
        full_count += int(full); hit_count += int(full and hit)
    require(exact(text.get('visibleLines'), hit_count) and text.get('ok') is (hit_count > 0), 'Original hit-based text evidence overwritten')
    require(exact(geometry['action'].get('readOnlyText'), {'fullVisibleLines': full_count, 'ok': full_count > 0})
        and full_count >= 1, 'No complete read-only draft line is visible')


def drafts_and_geometry(rows, case, scope, a):
    success = case == 'channel-completion'
    reopen_at = 5 if success else 7
    drafts = []
    viewport = rows[0]['geometry']['viewport']
    require(isinstance(viewport, list) and len(viewport) == 2 and all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in viewport), 'Invalid viewport')
    for i, row in enumerate(rows):
        m, p, g = row['model'], row['proof'], row['geometry']
        sending, sent = success and i == 6, success and i >= 7
        expected = expected_blocks(case, i, a)
        require(exact(rich(p.get('expectedBlocks')), rich(expected)), 'Native mutation changed exact rich content')
        store = m['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1, 'Draft disappeared or duplicated')
        d = entries[0]; j = p.get('journal', {})
        require(d.get('scope') == j.get('scope') == scope and d.get('blocks') == j.get('blocks') == p['expectedBlocks']
            and d.get('capture') == j.get('capture') and R['uuid'](d.get('capture'))
            and d.get('savedRevision') == j.get('revision') and R['integer'](j.get('revision'), 1)
            and d.get('attempt') == j.get('attempt') and R['integer'](d.get('generation'), 1)
            and store.get('active') == f"{d['id']}:{m['comments']['generation']}:{d['generation']}"
            and d.get('native') is True and d.get('busy') is sending
            and all(d.get(k) is False for k in ('saving', 'loading', 'captureFailed', 'terminalPending'))
            and d.get('error') is None, 'Native draft and fresh journal custody disagree')
        require(p.get('runtimeForgotten') is (i >= reopen_at), 'Disk reopen was not demonstrated')
        if i:
            old = drafts[-1]
            require(j['revision'] >= rows[i-1]['proof']['journal']['revision'], 'Saved revision went backwards')
            if i == reopen_at:
                require(d['id'] != old['id'] and d['generation'] != old['generation'], 'Reopen reused original runtime')
                if success: require(j == rows[i-1]['proof']['journal'], 'Reopen did not recover exact saved inventory')
            elif not sent or i == 8:
                require(d['id'] == old['id'] and d['generation'] == old['generation'], 'Unchanged mount replaced')
        drafts.append(d)
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport') == viewport
            and all(g.get(k) is True for k in ('dialog', 'comments', 'compose')), 'Native surface changed')
        e = g.get('editor'); native = g.get('native') or {}; snapshot = native.get('snapshot', {})
        require(isinstance(e, dict) and e.get('visible') is True and e.get('editable') is (not sending)
            and e.get('blocks') == snapshot.get('blocks') == p['expectedBlocks']
            and e.get('generation') == snapshot.get('generation') == str(d['generation'])
            and snapshot.get('composing') is False and R['integer'](snapshot.get('revision'))
            and e.get('sameNode') is (i < reopen_at) and e.get('bold') is (not sent), 'Original native node/style/capture changed')
        require(e.get('text') in ('', '\n') if sent else e.get('text') == display(expected), 'Visible rich text or paragraph boundaries changed')
        rectangle(e['rect'], viewport)
        tokens = native.get('tokens', [])
        channel_tokens = [t for t in tokens if t.get('value', {}).get('type') == 'channel']
        require(channel_tokens == [dict(value=v, label=LABELS[v['channel_id']]) for v in channels(p['expectedBlocks'])], 'Channel identity, opaque occurrence or canonical label changed')
        open_popup = g.get('completion', {}).get('open') is True
        require(g.get('completion', {}).get('captureMode') == ('sending' if sending else 'interactive'), 'Capture mode differs from actual stage')
        if sending: held_text(g)
        else:
            require(g.get('action', {}).get('readOnlyText') is None, 'Read-only bypass used outside held Send')
            if not open_popup and not sent: X['editor_text'](g)
        controls = g.get('controls', [])
        require(len(controls) == 4 and {c.get('id') for c in controls} == {
            'slack-list-comments-back', 'slack-list-comments-next', 'slack-list-comments-refresh', 'slack-list-comment-send'}, 'Comment controls changed')
        for control in controls:
            require(control.get('visible') is True and type(control.get('hit')) is bool
                and control.get('disabled') is ((sending or sent) if control['id'] == 'slack-list-comment-send' else False), 'Read/send admission changed')
            if not open_popup: require(control['hit'] is True, 'Dismissed popup obscured controls')
            rectangle(control['rect'], viewport)
    return drafts


def writes_and_reads(rows, case, scope, a):
    success = case == 'channel-completion'
    prior_reads, prior_posts = [], []
    initial_page = rows[0]['model']['comments']['page']
    require(exact(initial_page, X['original_page']()), 'Original acquired Page source changed')
    final_blocks = inserted_blocks(a, [PRIVATE, REMOTE])
    for i, row in enumerate(rows):
        m, p, g = row['model'], row['proof'], row['geometry']
        count = int(success and i >= 6)
        read_count = ([4,4,4,4,4,8,12,12,16] if success else [4,4,4,4,4,4,4,8])[i]
        ledger, compose = m['ledger'], m['composeLedger']
        reads = ledger.get('requests', [])
        C['validate_reads'](reads)
        require(len(reads) == read_count and reads[:len(prior_reads)] == prior_reads
            and exact(ledger.get('completed'), read_count) and exact(ledger.get('cancelled'), 0)
            and ledger.get('held') is False, 'Extra/retired List read')
        prior_reads = reads
        posts = compose.get('posts', [])
        require(len(posts) == count and posts == compose.get('prepared') and posts[:len(prior_posts)] == prior_posts
            and len(compose.get('messages', [])) == count and len(compose.get('policies', [])) == count
            and len(compose.get('preflights', [])) == count and compose.get('held') is False
            and compose.get('heldPost') is (success and i == 6) and compose.get('published') is (success and i == 8), 'Write count/disposition changed')
        prior_posts = posts
        if count:
            require(exact(rich(C['outgoing'](posts[0])), rich(final_blocks)), 'Posted rich body changed')
            require(compose['preflights'] == [dict(method='conversations.genericInfo', args={'updated_channels': '{"CCOMMENT01":0}'})], 'Wrong original preflight')
            policy = compose['policies'][0]
            require(policy.get('request') == dict(method='client.userBoot', args={'include_min_version_bump_check': '1'})
                and policy.get('boot', {}).get('ok') is True and policy['boot'].get('team', {}).get('id') == 'T1'
                and policy['boot'].get('self', {}).get('id') == 'ULIST0001' and policy.get('channel', {}).get('id') == 'CCOMMENT01'
                and policy.get('threadReadOnly') is False, 'Original posting policy changed')
            message = compose['messages'][0]; R['_message'](message, posts[0], scope)
            require(compose['messages'] == rows[6]['model']['composeLedger']['messages'], 'Original ACK was rewritten')
        pending = p['journal'].get('attempt')
        if success and i == 6:
            require(isinstance(pending, dict) and pending.get('capture') == p['journal']['capture'] and pending.get('raw') is None, 'Held attempt missing')
            record = pending.get('record', {}); args = posts[0]['args']
            require(record.get('client_msg_id') == args['client_msg_id'] and record.get('client_ts') == args['ts']
                and record.get('blocks') == json.loads(args['blocks']) and record.get('workspace') == 'T1' and record.get('user') == 'ULIST0001'
                and record.get('target') == dict(document=m['initialDocument'], record=m['records'][READ['KNOWN']], channel='CCOMMENT01', root=READ['ROOT_TS']), 'Original held attempt rebound')
        else: require(pending is None, 'Unexpected pending attempt')
        comments = m['comments']; visible = comments.get('visibleTs', [])
        expected_ts = ['1781524801.000001'] + ([compose['messages'][0]['ts']] if success and i >= 7 else [])
        require(visible == expected_ts and [v.get('ts') for v in g.get('rows', [])] == expected_ts, 'Accepted reply absent/duplicated or premature')
        page = comments['page']
        if success and i == 8:
            expected_page = copy.deepcopy(initial_page); expected_page['messages'].append(compose['messages'][0])
            require(page == expected_page, 'Fresh readback lost original page or exact accepted reply')
        else: require(page == initial_page, 'Canonical page changed before readback')
        if success and i >= 7:
            require(p.get('postRelease') == {'delivered': True, 'cancelledBeforeReply': False}
                and exact(rich(p.get('sentBlocks')), rich(final_blocks)), 'Original held ACK/posted blocks missing')
            require(comments['generation'] == rows[6]['model']['comments']['generation'], 'ACK/readback changed thread visit')

def completion_and_inputs(rows, case, a):
    success = case == 'channel-completion'
    popup_indices = {1,2} if success else {1,2,3,4,5,6}
    counts = [0,1,1,2,4,4,4,4,4] if success else [0,1,2,2,3,4,5,6]
    input_spec = [('local-held','query','#project'), ('keyboard-inserted','choose',PRIVATE),
                  ('mouse-inserted','query','#project-previous'), ('mouse-inserted','choose',REMOTE)] if success else [
                  ('held','query','#project'), ('query-retired','query','#project-new'), ('malformed','query','#malformed'),
                  ('failure','query','#offline'), ('empty','query','#empty'), ('cancelled-reopened','query','#cancelled')]
    previous_inputs, previous_releases = [], []
    for i, row in enumerate(rows):
        g, m, p = row['geometry'], row['model'], row['proof']; cg = g.get('completion', {}); current = m.get('completion')
        open_popup = i in popup_indices
        require(cg.get('open') is open_popup and R['integer'](cg.get('sequence')) and R['integer'](cg.get('revision')), 'Popup lifetime changed')
        labels = [LABELS[LOCAL]] if i == 1 else [LABELS[LOCAL],LABELS[PRIVATE],LABELS[REMOTE]] if success and i == 2 else []
        actual_labels = [r.get('label') for r in cg.get('rows', [])]
        require(len(actual_labels) == len(labels) and set(actual_labels) == set(labels), 'Canonical suggestion rows changed')
        status = None if not open_popup or labels else FAILURE if i in (4,5) else EMPTY
        notice = cg.get('status')
        require((notice or {}).get('text') == status and (notice is None) == (status is None), 'Failure/empty feedback changed')
        if open_popup:
            require(isinstance(current, dict) and current.get('current') is True and current.get('started') is True
                and current.get('consumed') is False and current.get('friendly') is False
                and current.get('labels') == actual_labels and current.get('sequence') == cg['sequence']
                and R['integer'](cg['sequence'], 1), 'Popup lost current source/caret ownership')
            # Error clears the batch without publishing a replacement revision.
            if i not in (4,5) or success: require(current.get('revision') == cg['revision'], 'Popup revision detached from source')
            popup = cg.get('popup', {})
            require(popup.get('visible') is True and type(popup.get('hit')) is bool, 'Popup not readable')
            rectangle(popup.get('rect'), g['viewport'])
            for index, option in enumerate(cg['rows']):
                require(option.get('id') == f"slack-list-channel-choice-{g['editor']['generation']}-{cg['sequence']}-{index}"
                    and option.get('type') == 'button' and option.get('role') == 'option'
                    and option.get('selected') == str(index == 0).lower() and option.get('visible') is True and option.get('hit') is True, 'Suggestion semantics or hit target changed')
                rectangle(option.get('rect'), g['viewport'])
            if notice:
                require(notice.get('visible') is True and notice.get('hit') is True, 'Failure guidance not readable')
                rectangle(notice.get('rect'), g['viewport'])
        else:
            require(cg.get('popup') is None, 'Dismissed popup retained')
            if current is not None:
                require(current.get('labels') == [] and current.get('current') is False and current.get('consumed') is True, 'Dismissed query retained insertion authority')
        inputs = p.get('inputs', [])
        require(len(inputs) == counts[i] and inputs[:len(previous_inputs)] == previous_inputs, 'Native inputs missing/replaced')
        for j, item in enumerate(inputs):
            stage, kind, value = input_spec[j]; receipt = item.get('value', {})
            require(item.get('stage') == stage and item.get('kind') == kind and receipt.get('ok') is True, 'Wrong actual input path')
            if kind == 'query':
                original = inserted_blocks(a, [PRIVATE]) if success and j == 2 else a
                expected = query_blocks(original, value)
                snap = receipt.get('native', {})
                require(receipt.get('query') == value, 'Typed query text changed')
            else:
                expected = query_blocks(a if j == 1 else inserted_blocks(a, [PRIVATE]), '#project' if j == 1 else '#project-previous')
                snap = receipt.get('before', {})
                require(item.get('channel') == value and receipt.get('label') == LABELS[value]
                    and receipt.get('keyboard') is (j == 1) and R['integer'](receipt.get('index'))
                    and R['integer'](receipt.get('sequence'), 1) and R['integer'](receipt.get('revision'), 1), 'Selection path or final channel changed')
                if j == 1:
                    ref = rows[2]['geometry']['completion']
                    require(receipt['sequence'] == ref['sequence'] and receipt['revision'] == ref['revision']
                        and ref['rows'][receipt['index']]['label'] == LABELS[value], 'Keyboard selection reused stale rows')
            require(exact(rich(snap.get('blocks')), rich(expected)) and snap.get('composing') is False
                and R['integer'](snap.get('revision')) and isinstance(snap.get('generation'), str) and snap['generation'].isdigit(), 'Input native snapshot differs from exact query/caret content')
        previous_inputs = inputs
        releases = p.get('releases', [])
        expected_releases = [dict(stage='refreshed', value=r) for r in m['completionSource']['releases']] if success else [
            dict(stage=s, value=r) for s,r in zip(('stale-refused','cancelled-reopened'), m['completionSource']['releases'])]
        require(releases == expected_releases and releases[:len(previous_releases)] == previous_releases, 'Original query release proof changed')
        previous_releases = releases
        if not success and i >= 3:
            stale = p.get('staleChoice', {})
            require(stale.get('ok') is True and stale.get('accepted') is False and stale.get('before') == stale.get('after')
                and exact(rich(stale.get('before', {}).get('blocks')), rich(query_blocks(a, '#project-new'))), 'Stale insertion mutated native draft')
    require(rows[1]['geometry']['completion']['sequence'] == rows[2]['geometry']['completion']['sequence'] if success else
            rows[1]['geometry']['completion']['sequence'] < rows[2]['geometry']['completion']['sequence'], 'Refresh replaced query or replacement reused sequence')
    if not success: require(rows[2]['geometry']['completion'] == rows[3]['geometry']['completion'], 'Late reply or stale choice changed popup')


def validate(fixture, receipt, directory, *, expected_case=None, dimensions=None, png=True):
    case = expected_case or receipt.get('case')
    require(case in STAGES and receipt.get('case') == case and fixture in C['FIXTURES']
        and receipt.get('fixture') == fixture and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True, 'Wrong completion receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(STAGES[case]), 'Missing completion stages')
    initial, scope, a = owner_and_records(receipt, rows)
    source_stages(rows, case)
    drafts_and_geometry(rows, case, scope, a)
    writes_and_reads(rows, case, scope, a)
    completion_and_inputs(rows, case, a)
    for row in rows:
        capture = Path(directory) / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == capture, 'Foreign native screenshot')
        if png: H['png_signature'](capture, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Final state differs from captured state')
    return receipt
