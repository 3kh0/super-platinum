"""Strict receipt validation for editing B while List comment A is unconfirmed."""
import json
import math
from pathlib import Path
import re
import runpy

# Loaded only by the runner's lazy case dispatch; importing it does not run a case.
C = runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
READ, H = C['READ'], C['H']
require, rectangle = READ['require'], READ['rectangle']
STAGES = ('a-unconfirmed', 'b-edited', 'b-reopened', 'a-confirmed-b-retained', 'b-sent')
A_TEXT = 'Final review \u4e2d\U0001f642\nOriginal attempt A stays unchanged.'
B_TEXT = '\nNewer draft B \u65e5\U0001f98a\nKeep this newer text after confirming A.'
UUID = re.compile(r'[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}')


def nodes(value):
    if isinstance(value, list):
        for child in value:
            yield from nodes(child)
    elif isinstance(value, dict):
        if 'type' in value:
            yield value
        for child in value.values():
            yield from nodes(child)


def text_units(blocks):
    return [(char, node.get('style', {})) for node in nodes(blocks)
            if node.get('type') == 'text' for char in node.get('text', '')]


def opaque(blocks):
    return [node for node in nodes(blocks) if node.get('type') in ('user', 'link')]


def paragraphs(blocks):
    """Bounded fixture paragraphs, retaining boundaries independently of text leaves."""
    result = []
    for block in blocks:
        require(block.get('type') == 'rich_text' and isinstance(block.get('elements'), list), 'Wrong rich block structure')
        for section in block['elements']:
            require(section.get('type') == 'rich_text_section' and isinstance(section.get('elements'), list), 'Wrong paragraph structure')
            units, visible = [], ''
            for node in section['elements']:
                kind = node.get('type')
                if kind == 'text':
                    require(isinstance(node.get('text'), str), 'Invalid paragraph text')
                    units.extend(('text', char, node.get('style', {})) for char in node['text'])
                    visible += node['text']
                else:
                    require(kind in ('user', 'link'), 'Unexpected fixture inline content')
                    units.append(('opaque', node))
                    visible += '@' + node['user_id'] if kind == 'user' else node['text']
            result.append((units, visible))
    return result


def visible_text(blocks):
    return '\n'.join(text for _, text in paragraphs(blocks))


def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def uuid(value):
    return isinstance(value, str) and UUID.fullmatch(value) is not None


def _message(message, post, scope):
    args = post['args']
    require(message.get('channel') == args['channel'] and message.get('thread_ts') == args['thread_ts']
            and message.get('client_msg_id') == args['client_msg_id']
            and message.get('blocks') == json.loads(args['blocks']) and message.get('user') == scope['user']
            and isinstance(message.get('ts'), str) and re.fullmatch(r'\d+\.\d{6}', message['ts'])
            and message['ts'] != args['thread_ts'], 'Positive message lost original send identity')


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    directory = Path(directory)
    require(fixture in C['FIXTURES'] and receipt.get('ok') is True
            and receipt.get('case') == 'newer-draft' and receipt.get('fixture') == fixture
            and receipt.get('noLiveNetwork') is True, 'Wrong newer-draft receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, READ['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [row.get('stage') for row in rows] == list(STAGES), 'Missing newer-draft stages')
    initial = rows[0]['model']
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']}, 'Missing canonical source inventory')
    require(initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == READ['ROOT_TS'], 'Wrong original List/root')
    seed = rows[0]['proof']['seed']
    scope = seed.get('scope', {})
    require(seed.get('reopened') is True and integer(seed.get('revision'), 1), 'Seed was not durably restored')
    require(set(scope) == {'workspace', 'user', 'list', 'record', 'created', 'creator', 'root'}
            and scope['list'] == READ['LIST'] and scope['record'] == READ['KNOWN'] and scope['root'] == READ['ROOT_TS']
            and all(isinstance(scope[k], str) and scope[k] for k in ('workspace', 'user'))
            and scope['created'] == touched[READ['KNOWN']].get('date_created')
            and scope['creator'] == touched[READ['KNOWN']].get('created_by'), 'Wrong persisted incarnation/owner')
    a, b = rows[0]['proof']['originalA'], rows[1]['proof']['newerB']
    require(isinstance(a, list) and isinstance(b, list) and a and b and a != b, 'B is not a new rich draft')
    au, bu = text_units(a), text_units(b)
    ap, bp = paragraphs(a), paragraphs(b)
    require(bu[:len(au)] == au and len(bu) > len(au), 'New native text/style prefix changed')
    require(len(ap) == 2 and len(bp) == 4 and bp[:len(ap)] == ap
            and ap[0][1].endswith(A_TEXT.split('\n')[0] + '\n')
            and ap[1][1] == A_TEXT.split('\n')[1]
            and [text for _, text in bp[2:]] == B_TEXT[1:].split('\n')
            and visible_text(b) == visible_text(a) + B_TEXT, 'New native paragraph layout/content changed')
    require(any(style.get('bold') is True for _, style in au), 'Original bold text missing')
    require(len(opaque(seed.get('blocks'))) == 2 and opaque(a) == opaque(seed['blocks']) == opaque(b),
            'Opaque source user/link metadata changed')
    previous_posts, previous_reads, previous_policies, previous_preflights = [], [], [], []
    viewport = None
    first_attempt = None
    prior_journal = None
    drafts = []
    for index, row in enumerate(rows):
        name, model, geometry, proof = row['stage'], row['model'], row['geometry'], row['proof']
        require(proof.get('originalA') == a and proof.get('newerB') == (None if index == 0 else b)
                and proof.get('seed') == seed and proof.get('runtimeForgotten') is (index == 2), 'Original native proof rewritten')
        require(model.get('account') == initial.get('account') and uuid(model.get('account'))
                and model.get('epoch') == initial.get('epoch') and integer(model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated, 'Canonical source changed')
        comments = model.get('comments', {})
        require(comments.get('active') is True and comments.get('loading') is False
                and comments.get('record') == READ['KNOWN'] and comments.get('root') == READ['ROOT_TS']
                and model.get('detail', {}).get('record') == READ['KNOWN'], 'Wrong active record/root')
        reads = model.get('ledger', {}).get('requests', [])
        require(reads[:len(previous_reads)] == previous_reads and len(reads) >= 4, 'Read evidence replaced')
        C['validate_reads'](reads)
        require(all(r['args'].get('id', READ['KNOWN']) == READ['KNOWN'] and
                    (r['method'] != 'conversations.replies' or r['args']['ts'] == READ['ROOT_TS']) for r in reads), 'Foreign record read')
        previous_reads = reads
        ledger = model.get('composeLedger', {})
        count = 2 if index == 4 else 1
        posts, prepared = ledger.get('posts'), ledger.get('prepared')
        require(isinstance(posts, list) and len(posts) == count and prepared == posts
                and posts[:len(previous_posts)] == previous_posts, 'Extra, replayed or replaced POST')
        previous_posts = posts
        require(len({post['args']['ts'] for post in posts}) == count, 'Client timestamp reused')
        require([C['outgoing'](post) for post in posts] == [a, b][:count]
                and len({post['args']['client_msg_id'] for post in posts}) == count, 'Wrong A/B payload or reused UUID')
        policies, preflights = ledger.get('policies', []), ledger.get('preflights', [])
        require(len(policies) == len(preflights) == count and policies[:len(previous_policies)] == previous_policies
                and preflights[:len(previous_preflights)] == previous_preflights, 'Fresh posting observations missing/rewritten')
        previous_policies, previous_preflights = policies, preflights
        for request in preflights:
            require(request == {'method':'conversations.genericInfo','args':{'updated_channels':'{"CCOMMENT01":0}'}}, 'Wrong channel preflight')
        for policy in policies:
            require(policy.get('request') == {'method':'client.userBoot','args':{'include_min_version_bump_check':'1'}}
                    and policy.get('boot', {}).get('ok') is True and policy['boot'].get('team', {}).get('id') == scope['workspace']
                    and policy['boot'].get('self', {}).get('id') == scope['user']
                    and policy.get('channel', {}).get('id') == 'CCOMMENT01' and policy.get('threadReadOnly') is False,
                    'Original observed posting owner unavailable')
        messages = ledger.get('messages', [])
        require(len(messages) == count and ledger.get('published') is (index >= 3)
                and ledger.get('held') is False and ledger.get('heldPost') is False, 'Wrong completed/publication state')
        for message, post in zip(messages, posts):
            _message(message, post, scope)
        store = model.get('draftStore', {})
        entries = store.get('drafts')
        require(isinstance(entries, list) and len(entries) == 1 and isinstance(store.get('active'), str) and store['active'], 'Original active draft missing')
        draft, journal = entries[0], proof.get('journal', {})
        require(store['active'] == f"{draft['id']}:{comments['generation']}:{draft['generation']}", 'Active original binding changed')
        drafts.append(draft)
        expected_blocks = a if index == 0 else b if index < 4 else []
        require(draft.get('scope') == journal.get('scope') == scope
                and draft.get('blocks') == journal.get('blocks') == expected_blocks
                and draft.get('capture') == journal.get('capture') and uuid(draft.get('capture'))
                and draft.get('savedRevision') == journal.get('revision') and integer(journal.get('revision'), 1)
                and draft.get('attempt') == journal.get('attempt'), 'Native model and independent disk inventory disagree')
        require(all(draft.get(k) is False for k in ('busy','saving','loading','captureFailed','terminalPending'))
                and draft.get('error') is None, 'Draft has unfinished capture/storage work')
        attempt = journal.get('attempt')
        if index < 3:
            require(isinstance(attempt, dict) and uuid(attempt.get('capture')), 'Original pending attempt missing')
            record = attempt.get('record', {})
            args = posts[0]['args']
            require(record.get('client_msg_id') == args['client_msg_id'] and record.get('client_ts') == args['ts']
                    and record.get('blocks') == a and record.get('workspace') == scope['workspace'] and record.get('user') == scope['user'], 'Attempt A was rebound to draft B')
            target = record.get('target', {})
            require(target.get('channel') == args['channel'] and target.get('root') == scope['root']
                    and target.get('document') == initial['initialDocument'] and target.get('record') == touched[READ['KNOWN']], 'Attempt target lost original raw source')
            if index == 0:
                first_attempt = attempt
                require(attempt['capture'] == journal['capture'], 'A capture did not bind attempt')
            else:
                require(attempt == first_attempt and attempt['capture'] != journal['capture'], 'B replaced the locked original capture')
        else:
            require(attempt is None, 'Confirmed attempt stayed locked')
        if prior_journal is not None:
            require(journal['revision'] >= prior_journal['revision'], 'Persisted revision moved backward')
        if index == 1:
            require(journal['revision'] > prior_journal['revision'] and draft['id'] == drafts[0]['id'], 'B did not persist in original draft')
        if index == 2:
            require(journal == prior_journal and draft['id'] != drafts[1]['id'], 'Reopen did not restore exact disk state into fresh runtime')
        if index == 3:
            require(draft['id'] == drafts[2]['id'] and draft['generation'] == drafts[2]['generation']
                    and journal['capture'] == prior_journal['capture'] and journal['revision'] > prior_journal['revision'], 'A confirmation disturbed B mount/capture')
        prior_journal = journal
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True and geometry.get('dialog') is True
                and geometry.get('comments') is True and geometry.get('compose') is True, 'Compose surface missing/overflowing')
        current_viewport = geometry.get('viewport')
        require(isinstance(current_viewport, list) and len(current_viewport) == 2
                and all(type(v) in (int,float) and math.isfinite(v) and v > 0 for v in current_viewport), 'Bad viewport')
        if viewport is None: viewport = current_viewport
        require(current_viewport == viewport, 'Viewport changed')
        controls = geometry.get('controls', [])
        require(len({c['id'] for c in controls}) == len(controls) and {'slack-list-comment-send','slack-list-comments-back','slack-list-comments-refresh'} <= {c['id'] for c in controls}, 'Missing/duplicate control')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Control obscured')
            rectangle(control.get('rect'), viewport)
        send = [c for c in controls if c.get('id') == 'slack-list-comment-send']
        require(len(send) == 1 and send[0].get('disabled') is (index != 3), 'Send admission wrong')
        editor = geometry.get('editor')
        if index < 4:
            require(isinstance(editor, dict) and editor.get('visible') is True and editor.get('editable') is True
                    and editor.get('blocks') == expected_blocks and editor.get('bold') is True, 'Newer native draft lost text/style/editability')
            rectangle(editor.get('rect'), viewport)
            require(editor.get('text') == visible_text(expected_blocks), 'Visible native paragraph text differs from source')
            require(editor.get('generation') == str(draft['generation']), 'Native generation differs from owner')
        elif editor is not None:
            require(editor.get('visible') is True and editor.get('blocks') == [] and not editor.get('text', '').strip(), 'B acceptance did not clear exact draft')
            rectangle(editor.get('rect'), viewport)
        notices = ' '.join(geometry.get('notices', [])).lower()
        if index < 3: require('unconfirmed' in notices, 'Uncertainty guidance missing')
        if index == 3:
            require(any(r.get('ts') == messages[0]['ts'] for r in geometry.get('rows', [])) and messages[0]['ts'] in comments.get('visibleTs', []), 'Confirmed A absent from visible thread')
            actual = [m for m in (comments.get('page') or {}).get('messages', []) if m.get('client_msg_id') == messages[0]['client_msg_id']]
            require(len(actual) == 1, 'Confirmed A absent/duplicated in actual readback page')
            _message(actual[0], posts[0], scope)
        capture = directory / f'{fixture}-{name}.png'
        require(Path(row.get('capture','')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    require(rows[0]['proof'].get('release') == {'delivered':True,'cancelledBeforeReply':False}, 'Original A receipt not delivered')
    require(receipt.get('final') == rows[-1]['model'], 'Final model changed')
    return receipt
