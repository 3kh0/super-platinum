"""Actual SQLite contention at capture, clear, begin and accepted-result settlement.

The shared runner supplies the exclusive-lock ACKs and separately checks process
exit, closed logs, original executable and the final read-only database snapshot.
"""
import copy
from pathlib import Path
import runpy

O = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
REC = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_recovery.py')))
RST = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_restart.py')))
N, G, R, C, READ, H, S = (O[k] for k in ('N', 'G', 'R', 'C', 'READ', 'H', 'S'))
require, exact = O['require'], O['exact']
STAGES = ('draft', 'editing', 'capture-failed', 'capture-recovered', 'return-failed', 'returned',
          'reopened-edit', 'begin-failed', 'begin-recovered', 'saving', 'receipt-failed', 'receipt-recovered', 'reopened')
READS = (4, 8, 8, 8, 8, 8, 12, 16, 16, 20, 20, 20, 24)
POLICIES = (0, 4, 4, 4, 4, 4, 8, 12, 12, 16, 16, 16, 16)
COMMANDS = (0, 0, 1, 2, 3, 4, 4, 5, 6, 6, 7, 8, 8)
ACTIVE = (1, 2, 3, 4, 6, 7, 8, 9, 10)
LOCKED = (2, 4, 7, 10)
B = ' Original edit B \u65e5\U0001f98a'
C_TEXT = ' Newer edit C \u4e2d\U0001f642'
D = ' Final edit D \u65e5\U0001f98a'
TS, CHANNEL = O['TS'], O['CHANNEL']
NOTICES = {2: REC['FAILED'], 4: 'The edit could not finish local recovery. Retry return; both drafts are retained.',
    7: 'The edit was not sent because its recovery record could not be saved. Try Save again after local storage recovers; both drafts are retained.',
    9: 'Saving the edited comment\u2026',
    10: 'The save result is retained here but could not be saved locally. Retry Cancel or Check result after storage recovers; this attempt will not be sent again.'}
NOTICES[8] = NOTICES[7]


def body_text(blocks):
    def leaf(node):
        if node.get('type') in ('text','plain_text'):
            require(isinstance(node.get('text'),str),'Missing original body text')
            return node['text']
        require(node.get('type') in ('rich_text','rich_text_section','context')
            and isinstance(node.get('elements'),list),'Unsupported final body fixture')
        return ''.join(leaf(v) for v in node['elements'])
    return ''.join(leaf(v) for v in blocks)


def controls(i):
    if i not in ACTIVE:
        return [('slack-list-comments-back', False), ('slack-list-comment-send', False)]
    result = [('slack-list-comment-edit-cancel', i in (2, 9)),
              ('slack-list-comment-edit-save', i in (2, 4, 9, 10))]
    if i == 2:
        result.append(('slack-list-comment-edit-retry', False))
    return result


def inventory(value, scope, seed, blocks, *, phase=None, original=None):
    require(isinstance(value, dict) and set(value) == {'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}
        and exact(value['scope'], scope) and value['ts'] == TS and R['integer'](value['revision'])
        and exact(value['retry_at'], 0) and exact(value['seed'], seed) and exact(value['blocks'], blocks), 'Wrong independent edit inventory')
    require((value['revision'] == 0 and value['capture'] == '') or (value['revision'] > 0 and R['uuid'](value['capture'])), 'Missing edit capture identity')
    if phase is None:
        require(value['attempt'] is None, 'Unissued or cleared edit has an attempt')
        return
    attempt(value['attempt'], seed, blocks, phase, original=original)


def attempt(value, seed, blocks, phase, *, original=None):
    ack = dict(ok=True, channel=CHANNEL, ts=TS, message=O['update_row'](blocks))
    require(isinstance(value, dict) and set(value) == {'id','record','capture','phase','raw','readback','retry_at'}
        and R['uuid'](value['id']) and R['uuid'](value['capture']) and value['phase'] == phase
        and value['readback'] is None and exact(value['retry_at'], 0)
        and exact(value['raw'], ack if phase == 'Confirmed' else None), 'Original durable update disposition/ACK changed')
    require(exact(value['record'], dict(version=1, seed=seed,
        blocks=blocks + O['received']()['blocks'][1:], text=G['display'](blocks))), 'Durable update lost exact original owner/body/context')
    if original is not None:
        require(all(exact(value[k], original[k]) for k in ('id','record','capture','retry_at')), 'Settlement replaced original durable attempt')


def lock_ack(value, ready, sequence, account, scope, reply, disk):
    require(isinstance(value, dict) and 'editInventories' in value and 'editHistory' in value, 'Missing bounded edit lock evidence')
    # Keep the preceding protocol oracle exact: the extension is admitted only here.
    basic = {k:v for k,v in value.items() if k not in ('editInventories','editHistory')}
    observed = S['ack'](basic, ready, sequence, account, scope)
    require(exact(observed, reply) and exact(value['editInventories'], [disk]), 'Lock covered another reply/edit or forged saved buffer')
    expected_history = [disk['attempt']] if disk['attempt'] is not None else []
    require(exact(value['editHistory'], expected_history), 'Lock history differs from original durable Pending record')


def finishing_text(raw, value, viewport):
    require(isinstance(value, dict) and set(value) == {'locked','finishing','parentClass','parentId','lines','visibleLines','ok'}
        and value['finishing'] is True and value['parentClass'] == 'slack-list-comment-compose'
        and value['parentId'] == 'slack-list-comment-edit', 'Missing exact finishing-region paint evidence')
    # Geometry algorithm is identical; only the independently pinned inert parent differs.
    normalized = {k:v for k,v in value.items() if k not in ('finishing','parentId')}
    normalized['parentClass'] = 'slack-list-comment-compose-editor'
    O['inert_text'](raw, normalized, viewport)


def geometry(g, i, reply, blocks):
    viewport = g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport, list)
        and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport)
        and g.get('editing') is (i in ACTIVE) and g.get('captureFault') is None, 'Storage native capture/mount changed')
    buttons = g.get('controls')
    require(isinstance(buttons, list) and [(c.get('id'),c.get('disabled')) for c in buttons] == controls(i), 'Storage failure allowed resend/departure or wrong controls')
    for c in buttons:
        require(c.get('visible') is True and c.get('hit') is True, 'Storage controls clipped/covered')
        N['rectangle'](c.get('rect', {}), viewport)
    if i == 4:
        finishing_text(g.get('text'), g.get('inertTextPaint'), viewport)
    elif i == 9:
        O['inert_text'](g.get('text'), g.get('inertTextPaint'), viewport)
    else:
        require(g.get('inertTextPaint') is None, 'Inert exception escaped its exact locked stage')
        N['text_proof'](g.get('text'), viewport)
    native = g.get('reply', {})
    require(exact(native.get('blocks'), reply) and native.get('sameNode') is (i != 12)
        and native.get('hidden') is (i in ACTIVE) and native.get('inert') is (i in ACTIVE)
        and native.get('visible') is (i not in ACTIVE) and native.get('composing') is False
        and g.get('replyNotice') is None, 'Independent reply changed or was exposed while editing')
    if i in ACTIVE:
        edit = g.get('edit', {})
        require(exact(edit.get('blocks'), blocks) and edit.get('sameNode') is True
            and edit.get('editable') is (i not in (4,9)), 'Original edit node/buffer or inert state changed')
        N['rectangle'](edit.get('rect', {}), viewport)
    else:
        require(g.get('edit') is None, 'Returned native edit survived')
        N['rectangle'](native.get('rect', {}), viewport)
    if i in NOTICES:
        notice = g.get('notice')
        require(isinstance(notice, dict) and notice.get('text') == NOTICES[i]
            and notice.get('visible') is True and notice.get('hit') is True, 'Storage recovery guidance missing/covered')
        N['rectangle'](notice.get('rect', {}), viewport)
    else:
        require(g.get('notice') is None, 'Unexpected storage notice')
    text = g.get('row', {}).get('text')
    expected=O['update_row'](blocks) if i>=11 else O['received']()
    require(isinstance(text, str) and all(v in text for v in expected['text'].split('\n'))
        and 'Preserved original context' in text and all(v not in text for v in (B,C_TEXT))
        and (D in text) is (i>=11), 'Recovered accepted body missing from DOM or unsettled edit leaked into transcript')
    return viewport


def capture_state(model, p, i, blocks):
    edit, cap = model['edit'], model.get('capture')
    a = edit.get('active')
    require(edit.get('opening') is False and exact(edit.get('events'), 0) and edit.get('retired') == [], 'Unexpected opening/event/retirement')
    if i not in ACTIVE:
        require(a is None and cap is None, 'Inactive edit retained capture/session')
        return
    witness = p['editRemembered']
    require(isinstance(cap, dict) and set(cap) == {'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'}
        and cap['scope'] == witness['scope'] and R['integer'](cap['generation'],1)
        and str(cap['generation']) == witness['generation']
        and cap['mount'] == (None if i == 4 else witness['mount']), 'Storage Retry replaced native scope/mount')
    expected = dict(failed=i==2, canSave=i in (1,3,6,7,8), canCancel=i not in (2,9),
                    canCheck=False,canRetry=i==2,busy=i==9,finishing=i==4)
    require(all(cap[k] is v for k,v in expected.items()), 'Storage action availability changed')
    require(isinstance(a, dict) and a.get('scope') == cap['scope'] and a.get('generation') == cap['generation']
        and a.get('ts') == TS and a.get('current') is True and R['integer'](a.get('revision'),1)
        and a.get('busy') is (i==9) and a.get('composing') is False
        and a.get('captureFailed') is (i==2) and a.get('nativeReady') is (i not in (2,4))
        and exact(a.get('source'), O['received']()) and exact(a.get('preserved'), O['received']()['blocks'][1:])
        and exact(a.get('blocks'), blocks) and a.get('notice') == NOTICES.get(i)
        and a.get('attempt') == (edit['attempts'][-1]['id'] if i >= 7 else None), 'Original edit source/buffer/state changed')


def validate(fixture, receipt, directory, *, dimensions=None, png=True):
    require(receipt.get('case') == 'own-edit-storage' and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture') == fixture and fixture in C['FIXTURES'], 'Wrong storage terminal')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES), 'Missing storage stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Wrong original inventory')
    first = rows[0]
    initial, reply, seed = first['model']['comments'], first['proof']['replyBlocks'], first['proof']['replySeed']
    scope = dict(workspace='T1', user=O['SELF'], list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']()) and seed.get('reopened') is True
        and R['integer'](seed.get('revision'),1), 'Wrong original reply seed')
    O['reply_content'](reply,seed['blocks'])
    b, newer, final = (rows[j]['proof'][k] for j,k in ((1,'originalEditBlocks'),(2,'newerEditBlocks'),(6,'finalEditBlocks')))
    for blocks,suffix in ((b,B),(newer,B+C_TEXT),(final,D)):
        O['edited_content'](blocks,suffix)
    first_mount, second_mount = rows[1]['proof']['editRemembered'], rows[6]['proof']['editRemembered']
    REC['witness'](first_mount,b)
    REC['witness'](second_mount,final)
    require(all(first_mount[k] != second_mount[k] for k in ('scope','mount','generation')), 'Cancelled edit mount reused')
    disk_seed = RST['seed_value'](first['model'])
    journal = first['journal']
    pending = rows[9]['editInventory']['attempt']
    disk_by_hold = [rows[j]['editInventory'] for j in (1,3,6,9)]
    all_controls = rows[-1]['proof'].get('storageControls')
    require(isinstance(all_controls,list) and len(all_controls)==8, 'Missing four actual lock/release pairs')
    for j,ack in enumerate(all_controls):
        lock_ack(ack,ready,j+1,initial['account'],scope,journal,disk_by_hold[j//2])
        if j%2:
            require(exact({k:v for k,v in ack.items() if k not in ('action','sequence','unchanged')},
                          {k:v for k,v in all_controls[j-1].items() if k not in ('action','sequence')}), 'Held database changed before release')
    previous = None
    for i,row in enumerate(rows):
        p,model = row['proof'],row['model']
        m,f,e = (model[k] for k in ('comments','fixture','edit'))
        c,ledger = m['comments'],m['ledger']
        blocks = None if i==0 else b if i==1 else newer if i<6 else final
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply) and exact(p.get('editBlocks'),blocks)
            and p.get('runtimeForgotten',False) is (i==12), 'Original source/reply/edit proof changed')
        for key,value,start in (('originalEditBlocks',b,1),('newerEditBlocks',newer,2),('finalEditBlocks',final,6),
                ('originalEditRemembered',first_mount,1),('blockedCapture',dict(ok=True,saveDisabled=True,cancelDisabled=True),2),
                ('blockedReturnSave',dict(ok=True,disabled=True),4),('blockedSaving',dict(ok=True,disabled=True),9),
                ('blockedReceiptSave',dict(ok=True,disabled=True),10),('updateRelease',O['RELEASE'],10)):
            require(exact(p.get(key),value if i>=start else None), 'Missing/rewritten control proof: '+key)
        require(exact(p.get('editRemembered'),None if i==0 else first_mount if i<6 else second_mount)
            and exact(p.get('storageControls'),all_controls[:COMMANDS[i]]), 'Original mount/lock command history changed')
        n=COMMANDS[i]
        expected_storage = None if n==0 else dict(hold=all_controls[(n-1)//2*2],release=None if n%2 else all_controls[n-1])
        require(exact(p.get('storage'),expected_storage), 'Wrong lock stage or release pairing')
        require(m.get('account')==initial['account'] and R['uuid'](m['account']) and exact(m.get('epoch'),initial['epoch'])
            and m.get('noSession') is False and m.get('noTransport') is False and exact(m.get('writes'),0) and exact(m.get('networkCalls'),0), 'Closed original owner changed')
        require(exact(m.get('document'),initial['initialDocument']) and initial['initialDocument'].get('id')==READ['LIST']
            and exact(m.get('initialDocument'),initial['initialDocument']) and exact(m.get('records'),touched)
            and exact(m.get('initialRecords'),touched) and exact(m.get('unrelated'),unrelated) and exact(m.get('initialUnrelated'),unrelated), 'Canonical List changed')
        require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None and c.get('visibleTs')==[TS] and c.get('deleted')==[]
            and m.get('detail',{}).get('record')==READ['KNOWN'] and (c.get('generation')==initial['comments']['generation']) is (i!=12), 'Original comments visit changed')
        expected_row=O['update_row'](final) if i>=11 else O['received']()
        expected_page=copy.deepcopy(initial['comments']['page'])
        if i==12:
            expected_page['messages']=[copy.deepcopy(expected_row) if v.get('ts')==TS else v for v in expected_page['messages']]
        require(exact(c.get('page'),expected_page) and exact(c.get('hasMore'),initial['comments'].get('hasMore'))
            and exact(c.get('reactions'),initial['comments']['reactions']), 'Original HTTP/canonical readback provenance or reactions changed')
        expected_messages=copy.deepcopy(initial['comments']['pageMessages'])
        if i>=11:
            projected=O['one_message'](expected_messages)
            for key in ('text','blocks','edited'):
                projected[key]=copy.deepcopy(expected_row[key])
        require(exact(c.get('pageMessages'),expected_messages), 'Recovered accepted body missing from typed projection or unrelated metadata changed')
        known=O['one_message'](c['pageMessages'])
        require(all(exact(known.get(k),v) for k,v in expected_row.items() if k not in ('type','edit_fixture_opaque')), 'Own message body/author/context changed')
        require(len(ledger.get('requests',[]))==READS[i] and exact(ledger.get('completed'),READS[i])
            and exact(ledger.get('replies'),READS[i]//4) and exact(ledger.get('cancelled'),0) and ledger.get('held') is False, 'Storage retry replayed read/update')
        C['validate_reads'](ledger['requests'])
        require([q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(READS[i]//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in ledger['requests']), 'Foreign item/root acquisition')
        require(exact(m.get('composeLedger'),dict(prepared=[],posts=[],preflights=[],policies=[],messages=[],published=False,held=False,heldPost=False))
            and exact(m.get('reactionLedger'),dict(prepared=[],issued=[],outcomes=[],completed=0,held=False,networkCalls=0,fullReads=[])), 'Unrelated message/reaction write')
        require(f.get('enabled') is True and exact(f.get('received'),O['received']()) and exact(f.get('boot'),O['boot']())
            and exact(f.get('networkCalls'),0) and f.get('httpCapability') is False
            and exact(f.get('policy'),[dict(request=q,response=r) for _ in range(POLICIES[i]//4) for q,r in O['policy_cycle']()]), 'Original policy acquisition changed')
        count=int(i>=9)
        require(isinstance(f.get('prepared'),list) and len(f['prepared'])==count and exact(f.get('updates'),f['prepared']), 'Duplicate/missing actual chat.update')
        for q in f['prepared']:
            O['update_wire'](q,final)
        ack=dict(ok=True,channel=CHANNEL,ts=TS,message=O['update_row'](final))
        require(exact(f.get('responses'),[dict(disposition='accepted',raw=ack)] if count else [])
            and exact(f.get('futureRows'),[ack['message']] if count else []) and f.get('held') is (i==9)
            and exact(f.get('releases'),[O['RELEASE']] if i>=10 else []) and f.get('published') is (i==12)
            and exact(f.get('publishedRow'),ack['message'] if i==12 else None), 'Original ACK/delivery changed or canonical readback publication missing')
        require(exact(p.get('serverPublication'),dict(originalReceipt=ack,publishedRow=ack['message'],updates=1) if i==12 else None),
            'Independent canonical publication lost original ACK or issued another update')
        expected_text=body_text(final+O['received']()['blocks'][1:])
        require(exact(row.get('renderedBody'),dict(ok=True,ts=TS,text=expected_text,expectedText=expected_text,bodyCount=1) if i>=11 else None),
            'Exact recovered rendered body missing, duplicated, or self-certified against wrong text')
        require(isinstance(e.get('attempts'),list) and len(e['attempts'])==(0 if i<7 else 1 if i<9 else 2), 'Wrong once-only runtime attempts')
        for j,a in enumerate(e['attempts']):
            phase='not-sent' if j==0 else 'sent' if i==9 else 'confirmed'
            require(R['integer'](a.get('id'),1) and a.get('ts')==TS and exact(a.get('blocks'),final+O['received']()['blocks'][1:])
                and a.get('phase')==phase and exact(a.get('receipt'),ack if j==1 and i>=10 else None) and exact(a.get('retryAt'),0), 'Unsent/ACK outcome conflated or attempt replayed')
            original=rows[7 if j==0 else 9]['model']['edit']['attempts'][j]
            require(a['id']==original['id'], 'Original runtime attempt replaced')
        require(len({a['id'] for a in e['attempts']})==len(e['attempts']), 'Runtime attempt identities collided')
        store=m['draftStore']
        require(len(store.get('drafts',[]))==1,'Independent reply missing')
        draft=store['drafts'][0]
        require(exact(draft.get('scope'),scope) and exact(draft.get('blocks'),reply) and draft.get('attempt') is None
            and draft.get('native') is True and draft.get('error') is None
            and all(draft.get(k) is False for k in ('busy','saving','loading','terminalPending','captureFailed'))
            and store.get('active')==f"{draft['id']}:{c['generation']}:{draft['generation']}", 'Independent reply consumed/changed')
        S['inventory'](journal,scope)
        S['paired'](draft,journal)
        if i in LOCKED:
            require(row.get('journal') is None and row.get('editInventory') is None, 'Pretended a fresh Journal read while SQLite was exclusively locked')
        else:
            require(exact(row.get('journal'),journal), 'Independent persisted reply changed')
            cleared=i in (0,5,11,12)
            inventory(row.get('editInventory'),scope,None if cleared else disk_seed,[] if cleared else blocks,
                phase='Pending' if i==9 else None)
        capture_state(model,p,i,blocks)
        viewport=geometry(row['geometry'],i,reply,blocks)
        require(viewport==first['geometry']['viewport'], 'CSS viewport changed during capture')
        path=Path(row.get('capture',''))
        require(path==Path(directory)/f'{fixture}-{STAGES[i]}.png','Capture escaped original stage directory')
        if png:
            H['png_signature'](path,dimensions)
        if previous:
            old=previous['model']['comments']
            require(exact(ledger['requests'][:len(old['ledger']['requests'])],old['ledger']['requests']), 'Original read history rewritten')
            olddraft=old['draftStore']['drafts'][0]
            require((draft['id']==olddraft['id']) is (i!=12) and (draft['generation']==olddraft['generation']) is (i!=12), 'Reply remounted before actual forget/reopen')
        previous=row
    # Capture recovery updates the disk, clear creates a fresh tombstone, and failed
    # begin cannot create a durable attempt/history before the explicit next Save.
    require(rows[3]['editInventory']['revision']>rows[1]['editInventory']['revision']
        and rows[3]['editInventory']['capture']!=rows[1]['editInventory']['capture']
        and rows[5]['editInventory']['revision']>rows[3]['editInventory']['revision']
        and rows[5]['editInventory']['capture']!=rows[3]['editInventory']['capture']
        and exact(rows[6]['editInventory'],rows[8]['editInventory'])
        and rows[9]['editInventory']['revision']>rows[8]['editInventory']['revision']
        and rows[11]['editInventory']['revision']>rows[9]['editInventory']['revision']
        and rows[11]['editInventory']['capture']!=rows[9]['editInventory']['capture']
        and exact(rows[11]['editInventory'],rows[12]['editInventory']), 'Recovery/clear/predispatch tombstone ordering changed')
    attempt(pending,disk_seed,final,'Pending')
    require(pending['capture']==rows[9]['editInventory']['capture'],'Pending attempt borrowed another edit capture')
    require(exact(receipt.get('final'),rows[-1]['model']),'Terminal storage model changed')
    return receipt


def validate_closed(value, native):
    """Final read-only snapshot after all eight commands and process exit."""
    rows=native['stages']
    last=rows[-1]
    require(isinstance(value,dict) and set(value)=={'account','inventories','editInventories','editHistory','contentSha256'}
        and value['account']==last['model']['comments']['account'] and exact(value['inventories'],[last['journal']])
        and exact(value['editInventories'],[last['editInventory']])
        and isinstance(value['editHistory'],list) and len(value['editHistory'])==1,
        'Closed database lost reply, clear tombstone, or retained accepted history')
    attempt(value['editHistory'][0],RST['seed_value'](rows[0]['model']),last['proof']['finalEditBlocks'],
        'Confirmed',original=rows[9]['editInventory']['attempt'])
    require(isinstance(value['contentSha256'],str) and len(value['contentSha256'])==64
        and all(c in '0123456789abcdef' for c in value['contentSha256']), 'Missing closed SQL content fingerprint')
    return value
