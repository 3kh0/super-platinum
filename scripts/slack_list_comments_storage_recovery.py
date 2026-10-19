"""Receipt oracle for real isolated SQLite load/save/receipt recovery."""
import json
import math
from pathlib import Path
import re
import runpy

N = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_newer_draft.py')))
C, READ, H = N['C'], N['READ'], N['H']
require, rectangle = READ['require'], READ['rectangle']
STAGES = ('load-failed', 'load-recovered', 'save-failed', 'back-refused',
          'save-recovered', 'save-reopened', 'receipt-failed', 'receipt-recovered')
A_TEXT = 'Final review \u4e2d\U0001f642\nOriginal saved draft A.'
B_TEXT = '\nUnsaved draft B \u65e5\U0001f98a\nRetain these changes through local recovery.'
FAILED = {0, 2, 3, 6}
ERRORS = {0: 'The saved comment draft could not be loaded. Retry local recovery before editing.',
          2: 'Your draft is retained here but could not be saved locally. Retry local save before leaving.',
          6: 'The delivery receipt could not be saved. This attempt remains locked; retry local recovery.'}


def ack(value, ready, sequence, account, scope):
    action = 'hold' if sequence % 2 else 'release'
    fields = {'version', 'nonce', 'pid', 'fixture', 'sequence', 'action', 'ok',
              'account', 'inventories', 'contentSha256'} | ({'unchanged'} if action == 'release' else set())
    require(isinstance(value, dict) and set(value) == fields
            and all(value.get(k) == v for k, v in ready.items())
            and all(type(value.get(k)) is int for k in ('version', 'pid', 'sequence'))
            and value['sequence'] == sequence and value['action'] == action and value['ok'] is True
            and value['account'] == account and isinstance(value['contentSha256'], str)
            and re.fullmatch('[0-9a-f]{64}', value['contentSha256']), 'Storage lock acknowledgment identity changed')
    require(action != 'release' or value['unchanged'] is True, 'Storage lock changed journal data')
    inventories = value['inventories']
    require(isinstance(inventories, list) and len(inventories) == 1
            and inventories[0].get('scope') == scope, 'Missing/foreign isolated journal inventory')
    inventory(inventories[0], scope)
    return inventories[0]


def inventory(value, scope):
    require(isinstance(value, dict) and set(value) == {'scope', 'revision', 'capture', 'blocks', 'attempt'}
            and value['scope'] == scope and N['integer'](value['revision'], 1)
            and N['uuid'](value['capture']) and isinstance(value['blocks'], list), 'Malformed independent inventory')


def paired(draft, journal):
    require(draft.get('blocks') == journal['blocks'] and draft.get('capture') == journal['capture']
            and draft.get('attempt') == journal['attempt'] and draft.get('savedRevision') == journal['revision'],
            'Model and independent persisted journal disagree')


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    directory = Path(directory)
    require(fixture in C['FIXTURES'] and receipt.get('ok') is True and receipt.get('case') == 'storage-recovery'
            and receipt.get('fixture') == fixture and receipt.get('noLiveNetwork') is True, 'Wrong storage recovery receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, READ['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES), 'Missing storage recovery stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']} and touched[READ['KNOWN']].get('thread_ts') == READ['ROOT_TS'],
            'Original canonical inventory missing')
    initial = rows[0]['model']
    seed = rows[0]['proof']['seed']
    scope = seed.get('scope', {})
    require(seed.get('reopened') is True and N['integer'](seed.get('revision'), 1)
            and set(scope) == {'workspace', 'user', 'list', 'record', 'created', 'creator', 'root'}
            and scope['list'] == READ['LIST'] and scope['record'] == READ['KNOWN'] and scope['root'] == READ['ROOT_TS']
            and scope['created'] == touched[READ['KNOWN']].get('date_created')
            and scope['creator'] == touched[READ['KNOWN']].get('created_by')
            and all(isinstance(scope[k], str) and scope[k] for k in ('workspace', 'user')), 'Original persisted scope missing')
    a, b = rows[0]['proof']['originalA'], rows[2]['proof']['newerB']
    require(isinstance(a, list) and isinstance(b, list) and a and b and a != b, 'Missing distinct native drafts A/B')
    ap, bp = N['paragraphs'](a), N['paragraphs'](b)
    au, bu = N['text_units'](a), N['text_units'](b)
    require(len(ap) == 2 and len(bp) == 4 and bp[:2] == ap and bu[:len(au)] == au
            and ap[0][1].endswith(A_TEXT.split('\n')[0] + '\n') and ap[1][1] == A_TEXT.split('\n')[1]
            and [text for _, text in bp[2:]] == B_TEXT[1:].split('\n')
            and N['visible_text'](b) == N['visible_text'](a) + B_TEXT
            and any(style.get('bold') is True for _, style in au), 'Rich paragraph/style prefix changed')
    require(len(N['opaque'](seed.get('blocks'))) == 2 and N['opaque'](a) == N['opaque'](seed['blocks']) == N['opaque'](b),
            'Opaque source token custody changed')
    holds, releases, drafts, journals = {}, {}, [], []
    previous_reads, previous_posts = [], []
    viewport = None
    for i, row in enumerate(rows):
        proof, model, geometry = row['proof'], row['model'], row['geometry']
        require(proof.get('originalA') == a and proof.get('newerB') == (None if i < 2 else b)
                and proof.get('seed') == seed and proof.get('runtimeForgotten') is (i in (0, 1, 5)), 'Original recovery proof rewritten')
        require(model.get('account') == initial.get('account') and N['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and N['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and initial['initialDocument'].get('id') == READ['LIST']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated, 'Canonical List data changed')
        comments = model.get('comments', {})
        require(comments.get('active') is True and comments.get('loading') is False
                and comments.get('record') == READ['KNOWN'] and comments.get('root') == READ['ROOT_TS']
                and model.get('detail', {}).get('record') == READ['KNOWN'], 'Original comments owner lost')
        reads = model.get('ledger', {}).get('requests', [])
        require(reads[:len(previous_reads)] == previous_reads and len(reads) >= 4, 'Read evidence replaced')
        C['validate_reads'](reads)
        require(all(r['args'].get('id', READ['KNOWN']) == READ['KNOWN'] and
                    (r['method'] != 'conversations.replies' or r['args']['ts'] == READ['ROOT_TS']) for r in reads), 'Foreign record acquisition')
        previous_reads = reads
        storage = proof.get('storage', {})
        sequence = 1 if i < 2 else 3 if i < 6 else 5
        hold = storage.get('hold')
        held_inventory = ack(hold, ready, sequence, model['account'], scope)
        if sequence in holds:require(hold == holds[sequence], 'Original lock receipt rewritten')
        holds[sequence] = hold
        locked = i in FAILED
        release = storage.get('release')
        if locked:
            require(release is None and proof.get('journal') is None, 'Invented inventory read under exclusive lock')
        else:
            ack(release, ready, sequence + 1, model['account'], scope)
            require(release['inventories'] == hold['inventories'] and release['contentSha256'] == hold['contentSha256'],
                    'Journal changed during exclusive lock')
            if sequence in releases:require(release == releases[sequence], 'Original release receipt rewritten')
            releases[sequence] = release
            inventory(proof.get('journal'), scope)
        journal = proof.get('journal')
        journals.append(journal)
        store = model.get('draftStore', {})
        require(isinstance(store.get('drafts'), list) and len(store['drafts']) == 1, 'Original draft missing')
        draft = store['drafts'][0]
        drafts.append(draft)
        require(draft.get('scope') == scope and N['uuid'](draft.get('capture'))
                and store.get('active') == f"{draft['id']}:{comments['generation']}:{draft['generation']}"
                and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'captureFailed')),
                'Draft owner/native work not settled')
        require(draft.get('terminalPending') is (i == 6), 'Original terminal receipt was lost or replayed')
        require(isinstance(draft.get('error'), str) and bool(draft['error']) if locked else draft.get('error') is None,
                'Local recovery error state changed')
        if i == 0:
            require(draft.get('savedRevision') is None and draft.get('blocks') == [] and draft.get('attempt') is None
                    and draft.get('native') is False and held_inventory['blocks'] == a and held_inventory['attempt'] is None,
                    'Failed inventory load admitted an editor or erased disk draft')
        elif i in (2, 3):
            require(draft.get('blocks') == b and held_inventory == journals[1] and held_inventory['blocks'] == a
                    and draft.get('savedRevision') == held_inventory['revision'] and draft.get('capture') != held_inventory['capture']
                    and draft.get('attempt') is None, 'Autosave failure lost B or changed saved A')
        elif i == 6:
            paired(draft, held_inventory)
            require(held_inventory['blocks'] == b and isinstance(held_inventory['attempt'], dict), 'Receipt failure lost durable attempt')
        else:
            paired(draft, journal)
            require(journal['blocks'] == (a if i == 1 else b if i < 7 else []) and journal['attempt'] is None,
                    'Recovery did not restore/clear exact captured draft')
        if i == 1:require(journal == held_inventory and draft['id'] == drafts[0]['id'], 'Load Retry changed original inventory/entry')
        if i == 3:
            require((draft['id'], draft['generation'], draft['capture']) == (drafts[2]['id'], drafts[2]['generation'], drafts[2]['capture'])
                    and comments['generation'] == rows[2]['model']['comments']['generation'], 'Failed Back retired the original editor')
        if i == 4:
            require(journal['revision'] > held_inventory['revision'] and journal['capture'] == drafts[2]['capture']
                    and draft['id'] == drafts[2]['id'], 'Explicit save Retry lost original B capture')
        if i == 5:
            require(journal == journals[4] and draft['id'] != drafts[4]['id']
                    and comments['generation'] != rows[4]['model']['comments']['generation'], 'B did not restore through a fresh runtime')
        if i == 7:require(journal['revision'] > held_inventory['revision'], 'Receipt recovery did not persist settlement')
        ledger = model.get('composeLedger', {})
        posts, prepared = ledger.get('posts'), ledger.get('prepared')
        count = 1 if i >= 6 else 0
        require(isinstance(posts, list) and len(posts) == count and prepared == posts
                and posts[:len(previous_posts)] == previous_posts, 'Local Retry sent an extra/replaced POST')
        previous_posts = posts
        require(len(ledger.get('policies', [])) == len(ledger.get('preflights', [])) == count
                and ledger.get('held') is False and ledger.get('heldPost') is False, 'Posting work/count changed')
        require(ledger.get('published') is False and len(ledger.get('messages', [])) == count, 'Unexpected readback/response state')
        if count:
            require(C['outgoing'](posts[0]) == b, 'POST is not exact native B')
            require(ledger['preflights'] == [{'method':'conversations.genericInfo','args':{'updated_channels':'{"CCOMMENT01":0}'}}], 'Wrong posting channel observation')
            policy = ledger['policies'][0]
            require(policy.get('request') == {'method':'client.userBoot','args':{'include_min_version_bump_check':'1'}}
                    and policy.get('boot', {}).get('ok') is True and policy['boot'].get('team', {}).get('id') == scope['workspace']
                    and policy['boot'].get('self', {}).get('id') == scope['user'] and policy.get('channel', {}).get('id') == 'CCOMMENT01'
                    and policy.get('threadReadOnly') is False, 'Posting authority lost original owner')
            N['_message'](ledger['messages'][0], posts[0], scope)
            require(proof.get('postRelease') == {'delivered':True,'cancelledBeforeReply':False}, 'Held success receipt not delivered')
            attempt = held_inventory['attempt']
            record, args = attempt['record'], posts[0]['args']
            require(attempt['capture'] == held_inventory['capture'] and record.get('blocks') == b
                    and record.get('client_msg_id') == args['client_msg_id'] and record.get('client_ts') == args['ts']
                    and record.get('workspace') == scope['workspace'] and record.get('user') == scope['user']
                    and record.get('target', {}).get('document') == initial['initialDocument']
                    and record['target'].get('record') == touched[READ['KNOWN']]
                    and record['target'].get('channel') == args['channel'] and record['target'].get('root') == scope['root'],
                    'Durable pending attempt lost original POST/source')
            if i == 7:require(ledger == rows[6]['model']['composeLedger'], 'Receipt Retry changed transport ledger')
        else:require(proof.get('postRelease') is None, 'Unexpected POST receipt')
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True
                and all(geometry.get(k) is True for k in ('dialog','comments','compose')), 'Recovery surface missing/overflowing')
        current = geometry.get('viewport')
        require(isinstance(current, list) and len(current) == 2 and all(type(x) in (int,float) and math.isfinite(x) and x > 0 for x in current), 'Invalid viewport')
        if viewport is None:viewport = current
        require(current == viewport, 'Viewport changed')
        controls = geometry.get('controls', [])
        ids = [c['id'] for c in controls]
        require(len(set(ids)) == len(ids) and {'slack-list-comments-back','slack-list-comments-refresh','slack-list-comment-send'} <= set(ids), 'Missing/duplicate recovery controls')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Recovery control obscured')
            rectangle(control.get('rect'), viewport)
        send = next(c for c in controls if c['id'] == 'slack-list-comment-send')
        require(send.get('disabled') is (locked or i == 7), 'Send admission changed during storage recovery')
        if locked:
            retry = [c for c in controls if c['id'] == 'slack-list-comment-retry']
            require(len(retry) == 1 and retry[0].get('disabled') is False, 'Local Retry unavailable')
            require(draft['error'] in geometry.get('notices', []), 'Local recovery guidance not visible')
            if i in ERRORS:require(draft['error'] == ERRORS[i], 'Wrong local recovery guidance')
        editor = geometry.get('editor')
        if i == 0:require(editor is None, 'Failed load mounted an untracked editor')
        elif i == 7:
            require(editor is None or (editor.get('blocks') == [] and not editor.get('text', '').strip()), 'Receipt recovery failed to clear captured B')
        else:
            expected_blocks = a if i == 1 else b
            require(isinstance(editor, dict) and editor.get('editable') is True and editor.get('visible') is True
                    and editor.get('blocks') == expected_blocks and editor.get('text') == N['visible_text'](expected_blocks)
                    and editor.get('bold') is True and editor.get('generation') == str(draft['generation']),
                    'Recovery changed original native text/style/mount')
        if editor is not None:rectangle(editor.get('rect'), viewport)
        path = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture','')) == path, 'Foreign recovery capture')
        if png:H['png_signature'](path, dimensions)
    require(set(holds) == set(releases) == {1,3,5} and receipt.get('final') == rows[-1]['model'], 'Incomplete lock protocol/final state')
    return receipt
