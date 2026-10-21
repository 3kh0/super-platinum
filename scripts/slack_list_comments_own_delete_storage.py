"""Real SQLite contention before deletion dispatch and after the retained ACK."""
import copy
import runpy
from pathlib import Path
D=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete.py')))
O,N,G,R,C,READ,H,S=(D[k] for k in ('O','N','G','R','C','READ','H','S'))
require,exact=D['require'],D['exact']
TS,CHANNEL,SELF,PREFIX=D['TS'],D['CHANNEL'],D['SELF'],D['PREFIX']
STAGES=('draft','confirming','begin-failed','begin-recovered','delete-held','receipt-failed','receipt-unlocked','receipt-recovered','readback','reopened')
READS=(4,8,12,12,16,16,16,16,20,24)
POLICIES=(0,1,2,2,3,3,3,3,3,3)
COMMANDS=(0,0,1,2,2,3,4,4,4,4)
ACTIVE=(1,2,3,4,5,6)
LOCKED=(2,5)
BEGIN='The deletion was not sent because its recovery record could not be saved. Your drafts are retained.'
PENDING='The original result could not be saved locally. Retry recovery; no deletion will be resent.'
NOTICES={2:BEGIN,3:BEGIN,4:'Working\u2026',5:PENDING,6:PENDING}


def inventory(value,scope,record,phase):
    require(isinstance(value,dict) and set(value)=={'scope','ts','revision','attempt','retry_at'}
        and exact(value['scope'],scope) and value['ts']==TS and exact(value['retry_at'],0)
        and R['integer'](value['revision']),'Wrong original deletion inventory')
    if phase is None:
        require(exact(value['revision'],0) and value['attempt'] is None,'Failed begin created durable deletion authority')
    else:
        a=value['attempt']
        require(R['integer'](value['revision'],1) and isinstance(a,dict)
            and set(a)=={'id','record','phase','raw','readback','retry_at'} and R['uuid'](a['id'])
            and exact(a['record'],record) and a['phase']==phase and a['readback'] is None and exact(a['retry_at'],0)
            and exact(a['raw'],D['ACK'] if phase=='Confirmed' else None),'Durable deletion lost original UUID/record/receipt')


def lock_ack(value,ready,sequence,account,scope,reply,pending):
    extras={'editInventories','editHistory','deleteInventories','deleteHistory'}
    require(isinstance(value,dict) and extras<=set(value),'Missing independent deletion table inspection')
    basic={k:v for k,v in value.items() if k not in extras}
    require(exact(S['ack'](basic,ready,sequence,account,scope),reply),'Lock changed independent reply')
    expected=[pending] if sequence>=3 else []
    require(value['editInventories']==[] and value['editHistory']==[]
        and exact(value['deleteInventories'],expected)
        and exact(value['deleteHistory'],[pending['attempt']] if sequence>=3 else []),
        'Lock covered wrong deletion/edit rows or settlement occurred while locked')


def fixture_ledger(f,i,record,previous):
    count=int(i>=4)
    require(f.get('enabled') is True and exact(f.get('networkCalls'),0) and f.get('httpCapability') is False
        and exact(f.get('seed'),dict(received=O['received'](),boot=O['boot']())) and exact(f.get('received'),O['received']()),
        'Deletion fixture source/isolation changed')
    require(exact(f.get('policy'),D['policy_cycle']()*POLICIES[i]) and exact(f.get('prepared'),[D['wire']()]*count)
        and exact(f.get('deletes'),[D['wire']()]*count) and exact(f.get('records'),[record]*count),
        'Failed begin/retry duplicated dispatch or skipped fresh scoped policy')
    require(exact(f.get('responses'),[dict(disposition='accepted',raw=D['ACK'],retryAfter=None)] if count else [])
        and f.get('held') is (i==4) and f.get('futureDeleted')==(TS if count else None)
        and f.get('published') is (i>=8) and f.get('deleted')==([TS] if i>=8 else [])
        and exact(f.get('releases'),[O['RELEASE']] if i>=5 else []),'Original held ACK or canonical readback was rewritten')
    if previous:
        for key in ('policy','prepared','deletes','records','responses','releases'):
            require(exact(f[key][:len(previous[key])],previous[key]),'Original immutable deletion ledger changed')


def geometry(g,i,reply,active,visible):
    modal,held,removed,reopened = i in ACTIVE, i==4, i>=7, i==9
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport,list)
        and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),'Invalid deletion native geometry')
    names=([PREFIX+'retry'] if i in (5,6) else [])+[PREFIX+'cancel',PREFIX+'confirm'] if modal else ['slack-list-comments-back','slack-list-comment-send']
    controls=g.get('controls')
    require(isinstance(controls,list) and [c.get('id') for c in controls]==names,'Wrong deletion controls')
    for c in controls:
        disabled=held or (c['id']==PREFIX+'confirm' and i in (5,6))
        require(c.get('disabled') is disabled and c.get('visible') is True and c.get('hit') is True and c.get('type')=='button',
                'Deletion control disabled/visibility semantics changed')
        N['rectangle'](c.get('rect',{}),viewport)
    r=g.get('reply')
    require(isinstance(r,dict) and exact(r.get('blocks'),reply) and r.get('sameNode') is (not reopened)
        and r.get('hidden') is modal and r.get('inert') is modal and r.get('visible') is (not modal)
        and r.get('composing') is False,'Original reply native node/content/seal changed')
    require(isinstance(r.get('generation'),str) and r['generation'].isascii() and r['generation'].isdecimal(), 'Missing native reply generation')
    if modal:
        p=g.get('panel')
        require(isinstance(p,dict) and p.get('id')=='slack-list-comment-delete' and p.get('role')=='alertdialog'
            and p.get('modal')=='true' and p.get('busy')==str(held).lower() and p.get('visible') is True
            and p.get('generation')==str(active['generation']) and p.get('session')==str(active['id'])
            and p.get('title')=='Delete message' and p.get('description')=='Are you sure you want to delete this message? This cannot be undone.',
            'Confirmation panel is missing or belongs to another source')
        N['rectangle'](p.get('rect',{}),viewport)
        preview=p.get('preview',{})
        require(preview.get('visible') is True and preview.get('inert') is True and exact(preview.get('actions'),0) and exact(preview.get('tools'),0)
            and all(t in preview.get('text','') for t in O['received']()['text'].split('\n'))
            and 'Preserved original context' in preview.get('body','')
            and preview.get('bold')==['Original own comment \u4e2d\U0001f642.']
            and preview.get('italic')==['Second editable line.'],'Original rich confirmation preview changed or gained actions')
        N['rectangle'](preview.get('rect',{}),viewport)
        require(g.get('text') is None,'Hidden reply claimed visible text evidence')
    else:
        require(g.get('panel') is None,'Closed confirmation survived')
        N['rectangle'](r.get('rect',{}),viewport)
        N['text_proof'](g.get('text'),viewport)
    notice=g.get('notice')
    wanted=NOTICES.get(i)
    if wanted is None:
        require(notice is None,'Unexpected deletion status')
    else:
        require(isinstance(notice,dict) and notice.get('text')==wanted and notice.get('role')=='status'
            and notice.get('visible') is True,'Missing actual rejection/unknown guidance')
        N['rectangle'](notice.get('rect',{}),viewport)
    require(isinstance(g.get('rows'),list) and [r.get('ts') for r in g['rows']]==visible,'DOM retained a deleted row or removed another row')
    if not removed:
        text=g['rows'][0].get('text','')
        require(all(s in text for s in O['received']()['text'].split('\n')) and 'Preserved original context' in text,
                'Unconfirmed/rejected deletion removed or changed the actual body')



def validate(fixture,receipt,directory,*,dimensions=None,png=True):
    case='own-delete-storage'
    require(receipt.get('case')==case and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong deletion terminal')
    ready=receipt.get('readiness',{})
    H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES),'Missing deletion stages')
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Original inventory changed')
    first=rows[0];initial=first['model']['comments'];seed=first['proof']['replySeed'];reply=first['proof']['replyBlocks']
    scope=dict(workspace='T1',user=SELF,list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'),1),'Missing original rich reply seed')
    O['reply_content'](reply,seed['blocks'])
    require(exact(O['one_message'](initial['comments']['page']['messages']),O['received']()),'Original HTTP comment changed')
    record=D['source'](initial);previous_fixture=None;previous_reads=[];previous_draft=None
    pending=rows[4].get('deleteInventory')
    inventory(pending,scope,record,'Pending')
    acknowledgments=rows[-1]['proof'].get('storageControls')
    require(isinstance(acknowledgments,list) and len(acknowledgments)==4,'Missing four exact storage commands')
    for sequence,value in enumerate(acknowledgments,1):
        lock_ack(value,ready,sequence,initial['account'],scope,first['journal'],pending)
    for hold,release in zip(acknowledgments[::2],acknowledgments[1::2]):
        require(exact(release,{**hold,'sequence':hold['sequence']+1,'action':'release','unchanged':True}),'Locked rows changed or release lost original ACK')
    root_typed=[m for m in initial['comments']['pageMessages'] if m.get('ts')!=TS]
    require(len(root_typed)==1 and root_typed[0].get('ts')==READ['ROOT_TS'],'Missing canonical original root')
    for i,row in enumerate(rows):
        modal,held,removed,published,reopened,count=i in ACTIVE,i==4,i>=7,i>=8,i==9,int(i>=4)
        p,model=row['proof'],row['model'];m,f,d=model['comments'],model['fixture'],model['delete'];c=m['comments']
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply) and p.get('runtimeForgotten',False) is reopened,'Original source/reply/reopen proof changed')
        blocked=i>=4
        require(exact(p.get('blockedConfirm'),D['BLOCKED'] if blocked else None)
            and exact(p.get('release'),O['RELEASE'] if i>=5 else None),'Held/unknown duplicate confirmation proof changed')
        require(m.get('account')==initial.get('account') and R['uuid'](m.get('account')) and exact(m.get('epoch'),initial.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False and exact(m.get('writes'),0) and exact(m.get('networkCalls'),0),
            'Original disconnected identity changed')
        require(exact(m.get('document'),initial['initialDocument']) and exact(m.get('initialDocument'),initial['initialDocument'])
            and exact(m.get('records'),touched) and exact(m.get('initialRecords'),touched)
            and exact(m.get('unrelated'),unrelated) and exact(m.get('initialUnrelated'),unrelated),'Deletion mutated a List item/document')
        visible=[READ['ROOT_TS']] if removed else [TS]
        require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None and exact(c.get('retryAt'),0)
            and c.get('visibleTs')==visible and c.get('deleted')==([TS] if removed and not reopened else [])
            and m.get('detail',{}).get('record')==READ['KNOWN'],'Original visit/visible deletion changed')
        require((c.get('generation')==initial['comments']['generation']) is (not reopened),'Wrong original/reopened comment visit')
        page=D['canonical'](initial) if i>=8 else initial['comments']['page']
        require(exact(c.get('page'),page) and exact(c.get('hasMore'),initial['comments']['hasMore']), 'Raw HTTP Page changed without canonical read')
        expected_typed=root_typed if removed else initial['comments']['pageMessages']
        require(exact(c.get('pageMessages'),expected_typed),'Typed selected/root body was changed or deletion not applied')
        require(c.get('accepted')==[] and c.get('live')==[] and c.get('acceptedTs')==[] and c.get('liveTs')==[], 'Deletion invented another confirmed/live comment')
        expected_reactions=[dict(ts=READ['ROOT_TS'],reactions=[])] if removed else [dict(ts=TS,reactions=[dict(name='wave',count=1)])]
        require(exact(c.get('reactions'),expected_reactions),'Unrelated reaction/root projection changed')
        ledger=m.get('ledger',{});reads=ledger.get('requests')
        require(isinstance(reads,list) and len(reads)==READS[i] and exact(reads[:len(previous_reads)],previous_reads)
            and exact(ledger.get('completed'),len(reads)) and exact(ledger.get('cancelled'),0) and ledger.get('held') is False
            and exact(ledger.get('replies'),len(reads)//4),'Unexpected duplicate/missing/replaced reads')
        C['validate_reads'](reads)
        require([r['method'] for r in reads]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(len(reads)//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in reads),
            'Read acquisition changed item/root')
        previous_reads=reads
        compose=m.get('composeLedger',{})
        require(all(compose.get(k)==[] for k in ('prepared','posts','messages','preflights','policies'))
            and all(compose.get(k) is False for k in ('published','held','heldPost')),'Deletion sent a reply')
        require(exact(m.get('reactionLedger'),dict(prepared=[],issued=[],outcomes=[],completed=0,held=False,networkCalls=0,fullReads=[])), 'Deletion issued a reaction')
        ef=model.get('editFixture',{})
        require(exact(ef,dict(enabled=True,received=O['received'](),boot=O['boot'](),policy=[],prepared=[],updates=[],responses=[],futureRows=[],published=False,publishedRow=None,held=False,releases=[],networkCalls=0,httpCapability=False)),
            'Deletion issued or mutated an edit')
        fixture_ledger(f,i,record,previous_fixture);previous_fixture=f
        store=m.get('draftStore',{});drafts=store.get('drafts')
        require(isinstance(drafts,list) and len(drafts)==1,'Original reply draft missing/duplicated')
        draft=drafts[0]
        require(exact(draft.get('scope'),scope) and exact(draft.get('blocks'),reply) and draft.get('attempt') is None and draft.get('error') is None
            and all(draft.get(k) is False for k in ('busy','saving','loading','terminalPending','captureFailed')) and draft.get('native') is True
            and store.get('active')==f"{draft['id']}:{c['generation']}:{draft['generation']}",'Deletion consumed or replaced reply draft')
        controls=p.get('storageControls')
        require(exact(controls,acknowledgments[:COMMANDS[i]]),'Storage protocol prefix changed')
        if COMMANDS[i]:
            last_hold=1 if COMMANDS[i]<=2 else 3
            expected_storage=dict(hold=acknowledgments[last_hold-1],release=acknowledgments[last_hold] if COMMANDS[i]%2==0 else None)
        else:expected_storage=None
        require(exact(p.get('storage'),expected_storage),'Current held/released transaction changed')
        if i in LOCKED:
            require(row.get('journal') is None and row.get('deleteInventory') is None,'Locked journal was read or fabricated by native process')
            reply_disk=expected_storage['hold']['inventories'][0]
        else:
            reply_disk=row.get('journal')
            require(exact(reply_disk,first['journal']),'Unchanged reply journal was rewritten')
        S['inventory'](reply_disk,scope);S['paired'](draft,reply_disk)
        if previous_draft:
            require((draft['id']==previous_draft['id']) is (not reopened) and (draft['generation']==previous_draft['generation']) is (not reopened),'Reply binding changed unexpectedly')
        previous_draft=draft
        require(exact(d.get('retainedCount'),int(i>0)),'Missing/duplicate retained deletion custody')
        active=d.get('active')
        if modal:
            phase='Pending' if i in (5,6) else None
            require(isinstance(active,dict) and R['integer'](active.get('id'),1) and active.get('generation')==c['generation'] and active.get('ts')==TS
                and active.get('busy') is held and active.get('phase')==phase and active.get('localPending') is (i in (5,6))
                and active.get('sourceCurrent') is True and exact(active.get('record'),record)
                and active.get('canConfirm') is (i in (1,2,3)) and active.get('canCheck') is False
                and active.get('canRetryLocal') is (i in (5,6)),'Delete session/policy/once-only actions changed')
        else:
            require(active is None,'Dismissed deletion retained an active modal')
        if i==0:require(row.get('deleteInventory') is None,'Unopened deletion has a recovery record')
        elif i not in LOCKED:
            inventory(row.get('deleteInventory'),scope,record,None if i<4 else 'Pending' if i<7 else 'Confirmed')
        if i in (4,6):require(exact(row['deleteInventory'],pending),'Local receipt failure rewrote durable Pending')
        if i>=7:
            terminal=row['deleteInventory'];attempt=terminal['attempt']
            require(attempt['id']==pending['attempt']['id'] and exact(attempt['record'],pending['attempt']['record'])
                and terminal['revision']>pending['revision'],'Local recovery created a new attempt or failed to persist ACK')
            require(exact(terminal,rows[7]['deleteInventory']),'Confirmed deletion was rewritten after recovery')
        geometry(row['geometry'],i,reply,active,visible)
        require(str(draft['generation'])==row['geometry']['reply']['generation'],'Native reply generation differs from original binding')
        path=Path(directory)/f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture',''))==path,'Foreign deletion screenshot')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt.get('final'),rows[-1]['model']),'Final deletion state changed')
    return receipt


def validate_closed(value,native):
    last=native['stages'][-1]
    require(isinstance(value,dict) and set(value)=={'account','inventories','editInventories','editHistory','deleteInventories','deleteHistory','contentSha256'}
        and value['account']==last['model']['comments']['account'] and exact(value['inventories'],[last['journal']])
        and value['editInventories']==[] and value['editHistory']==[]
        and exact(value['deleteInventories'],[last['deleteInventory']])
        and exact(value['deleteHistory'],[last['deleteInventory']['attempt']]), 'Closed database lost exact reply or original confirmed deletion history')
    inventory(last['deleteInventory'],last['journal']['scope'],D['source'](native['stages'][0]['model']['comments']),'Confirmed')
    digest=value['contentSha256']
    require(isinstance(digest,str) and len(digest)==64 and all(c in '0123456789abcdef' for c in digest),'Missing closed SQL fingerprint')
    return value

