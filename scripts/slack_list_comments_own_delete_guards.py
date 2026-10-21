"""Fresh deletion permission/source refusal and issued-429 explicit retry custody."""
import copy
import runpy
from pathlib import Path
D=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete.py')))
REC=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_recovery.py')))
E=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_edits.py')))
O,N,G,R,C,READ,H,S=(D[k] for k in ('O','N','G','R','C','READ','H','S'))
require,exact=D['require'],D['exact']
TS,CHANNEL,SELF,PREFIX=D['TS'],D['CHANNEL'],D['SELF'],D['PREFIX']
STAGES={'own-delete-guards':('draft','confirming','policy-refused','cancelled','source-confirming','source-changed','source-cancelled','reopened'),
 'own-delete-rate-limit':('draft','confirming','rate-limited','cooldown-cancelled','cooldown-reopened','cooldown-expired','deleted','readback','reopened')}
READS={'own-delete-guards':(4,8,12,12,16,16,16,20),'own-delete-rate-limit':(4,8,12,12,12,12,16,20,24)}
POLICIES={'own-delete-guards':(0,1,2,2,3,3,3,3),'own-delete-rate-limit':(0,1,2,2,2,2,3,3,3)}
POLICY='The message or deletion permissions changed. Nothing was sent; Cancel returns to your saved reply.'
STALE='This comment changed. Cancel returns to your saved reply; no new deletion is allowed.'
RATE='Slack asked you to wait. Your drafts are retained; no request is automatically retried.'


def layout(case,i):
    guard=case=='own-delete-guards'
    return (i in ((1,2,4,5) if guard else (1,2,4,5)),False,not guard and i>=6,
        not guard and i>=7,i==len(STAGES[case])-1,0 if guard else int(i>=2)+int(i>=6))


def notice(case,i):
    return ({2:POLICY,5:STALE} if case=='own-delete-guards' else {2:RATE,4:RATE,5:RATE}).get(i)


def can_confirm(case,i):
    return i in ((1,2,4) if case=='own-delete-guards' else (1,5))


def permission(before,after):
    return dict(allowedBefore=before,allowedAfter=after,response=dict(ok=True,allowed=['DELETE_OWN_MESSAGE'] if after else [],
        permission_fixture_opaque={'original':True}),workspace='T1',user=SELF,channel=CHANNEL,fixtureOnly=True)


def fixture_ledger(f,case,i,record,previous):
    count=layout(case,i)[-1];guard=case=='own-delete-guards'
    require(f.get('enabled') is True and exact(f.get('networkCalls'),0) and f.get('httpCapability') is False
        and exact(f.get('seed'),dict(received=O['received'](),boot=O['boot']())) and exact(f.get('received'),O['received']()),'Original disconnected deletion fixture changed')
    policy=[copy.deepcopy(q) for _ in range(POLICIES[case][i]) for q in D['policy_cycle']()]
    if guard and i>=2:policy[3]['response']['allowed']=[]
    require(exact(f.get('policy'),policy) and exact(f.get('prepared'),[D['wire']()]*count)
        and exact(f.get('deletes'),[D['wire']()]*count) and exact(f.get('records'),[record]*count),'Permission/source refusal dispatched or explicit retry lost original target')
    responses=[dict(disposition='unconfirmed-rate-limited',raw=None,retryAfter=15),dict(disposition='accepted',raw=D['ACK'],retryAfter=None)][:count]
    require(exact(f.get('responses'),responses) and f.get('held') is False and f.get('releases')==[]
        and f.get('futureDeleted')==(TS if count==2 else None) and f.get('published') is (not guard and i>=7)
        and f.get('deleted')==([TS] if not guard and i>=7 else []),'Typed issued429/originalACK/publication was rewritten')
    if previous:
        for key in ('policy','prepared','deletes','records','responses','releases'):
            require(exact(f[key][:len(previous[key])],previous[key]),'Original ledger prefix changed')


def inventory(value,case,i,scope,record,previous,deadline):
    if i==0:require(value is None,'Unopened deletion has inventory');return
    require(isinstance(value,dict) and set(value)=={'scope','ts','revision','attempt','retry_at'}
        and exact(value['scope'],scope) and value['ts']==TS and R['integer'](value['revision']),'Wrong deletion inventory owner')
    attempted=case=='own-delete-rate-limit' and i>=2
    require(exact(value['retry_at'],deadline if attempted else 0),'Original monotonic cooldown changed')
    if not attempted:
        require(exact(value['revision'],0) and value['attempt'] is None,'Refused/opened source created deletion attempt');return
    a=value['attempt'];phase='RateLimited' if i<6 else 'Confirmed'
    require(isinstance(a,dict) and set(a)=={'id','record','phase','raw','readback','retry_at'}
        and R['uuid'](a['id']) and exact(a['record'],record) and a['phase']==phase and a['readback'] is None
        and exact(a['retry_at'],deadline) and exact(a['raw'],None if i<6 else D['ACK']) and R['integer'](value['revision'],1),'Issued429 became unknown/ACK or original immutable record changed')
    if previous and previous.get('attempt'):
        if i==6:
            require(a['id']!=previous['attempt']['id'] and value['revision']>previous['revision'],'Explicit fresh confirmation reused old UUID')
        else:require(exact(value,previous),'Cooldown wait/reopen/readback rewrote durable original attempt')


def scenario_proof(p,case,i,rows):
    at,until=p.get('observedAt'),p.get('observedUntil')
    require(R['integer'](at,1) and R['integer'](until,at),'Missing original capture clock bounds')
    if i:require(at>=rows[i-1]['proof']['observedUntil'],'Capture clock regressed')
    guard=case=='own-delete-guards'
    require(exact(p.get('permissionDenied'),permission(True,False) if guard and i>=2 else None)
        and exact(p.get('permissionRestored'),permission(False,True) if guard and i>=4 else None),'Scoped permission setter/source observation changed')
    require(exact(p.get('blockedConfirm'),D['BLOCKED'] if i>=(5 if guard else 2) else None)
        and exact(p.get('reopenedBlockedConfirm'),D['BLOCKED'] if not guard and i>=4 else None),'Disabled original Confirm proof changed')
    expected=rows[5]['proof'].get('sourceChanged') if guard and i>=5 else None
    require(exact(p.get('sourceChanged'),expected),'Original parsed source-change witness was rewritten')
    if guard:
        require(p.get('rateIssuedAt') is None and p.get('cooldownDeadline') is None,'Guard refusal invented cooldown')
        return
    if i<2:
        require(p.get('rateIssuedAt') is None and p.get('cooldownDeadline') is None,'Cooldown exists before original request');return
    issued,deadline=p.get('rateIssuedAt'),p.get('cooldownDeadline')
    require(R['integer'](issued,1) and R['integer'](deadline,issued+15)
        and issued<=rows[2]['proof']['observedAt'] and deadline<=rows[2]['proof']['observedAt']+15
        and issued==rows[2]['proof']['rateIssuedAt'] and deadline==rows[2]['proof']['cooldownDeadline'],
        'Issued15second response/deadline changed or lacks original clock provenance')
    if i<=4:require(until<deadline,'Disabled cooldown capture crossed actual expiry')
    else:require(at>=deadline,'Retry enabled or sent before original cooldown expired')


def source_event(rows):
    v=rows[5]['proof'].get('sourceChanged');changed=REC['changed_row']()
    require(isinstance(v,dict) and set(v)=={'team','generation','frame','parsed','witness','before','after'}
        and v['team']=='T1' and R['integer'](v['generation']) and v['parsed'] is True
        and exact(v['frame'],dict(type='message',subtype='message_changed',channel=CHANNEL,message=changed))
        and exact(v['witness'],dict(kind='Changed',outerChannel=CHANNEL,typedTs=TS,typedChannel=CHANNEL,raw=changed)),
        'Source retirement lacks original parsed/scoped event')
    for side,index in (('before',4),('after',5)):
        require(exact(v[side],{k:rows[index]['model'][k] for k in ('delete','comments')}),'Source event differs from native before/after state')
    before,after=v['before'],v['after']
    require(exact(before['comments']['draftStore'],after['comments']['draftStore'])
        and exact(before['comments']['comments']['page'],after['comments']['comments']['page'])
        and exact(before['delete']['active']['record'],after['delete']['active']['record'])
        and before['delete']['active']['sourceCurrent'] is True and after['delete']['active']['sourceCurrent'] is False,
        'Socket event overwrote original HTTP/delete/reply custody')


def geometry(g,case,i,reply,active,visible):
    modal,held,removed,published,reopened,count=layout(case,i)
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport,list)
        and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),'Invalid deletion native geometry')
    names=[PREFIX+'cancel',PREFIX+'confirm'] if modal else ['slack-list-comments-back','slack-list-comment-send']
    controls=g.get('controls')
    require(isinstance(controls,list) and [c.get('id') for c in controls]==names,'Wrong deletion controls')
    for c in controls:
        disabled=(c['id']==PREFIX+'confirm' and not can_confirm(case,i)) or (case=='own-delete-rate-limit' and i==3 and c['id']=='slack-list-comment-send')
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
    wanted=globals()['notice'](case,i)
    if wanted is None:
        require(notice is None,'Unexpected deletion status')
    else:
        require(isinstance(notice,dict) and notice.get('text')==wanted and notice.get('role')=='status'
            and notice.get('visible') is True,'Missing actual rejection/unknown guidance')
        N['rectangle'](notice.get('rect',{}),viewport)
    require(isinstance(g.get('rows'),list) and [r.get('ts') for r in g['rows']]==visible,'DOM retained a deleted row or removed another row')
    if not removed:
        text=g['rows'][0].get('text','')
        expected=REC['changed_row']() if case=='own-delete-guards' and i in (5,6) else O['received']()
        require(all(s in text for s in expected['text'].split('\n')) and 'Preserved original context' in text,
                'Unconfirmed/rejected deletion removed or changed the actual body')

def validate(fixture,receipt,directory,*,expected_case='own-delete-guards',dimensions=None,png=True):
    case=expected_case
    require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong deletion terminal')
    ready=receipt.get('readiness',{})
    H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case]),'Missing deletion stages')
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Original inventory changed')
    first=rows[0];initial=first['model']['comments'];seed=first['proof']['replySeed'];reply=first['proof']['replyBlocks']
    scope=dict(workspace='T1',user=SELF,list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'),1),'Missing original rich reply seed')
    O['reply_content'](reply,seed['blocks'])
    require(exact(O['one_message'](initial['comments']['page']['messages']),O['received']()),'Original HTTP comment changed')
    record=D['source'](initial);previous_fixture=None;previous_reads=[];previous_draft=None;previous_inventory=None
    if case=='own-delete-guards':source_event(rows)
    root_typed=[m for m in initial['comments']['pageMessages'] if m.get('ts')!=TS]
    require(len(root_typed)==1 and root_typed[0].get('ts')==READ['ROOT_TS'],'Missing canonical original root')
    for i,row in enumerate(rows):
        modal,held,removed,published,reopened,count=layout(case,i)
        p,model=row['proof'],row['model'];m,f,d=model['comments'],model['fixture'],model['delete'];c=m['comments']
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply) and p.get('runtimeForgotten',False) is reopened,'Original source/reply/reopen proof changed')
        scenario_proof(p,case,i,rows)
        expected_deadline=p.get('cooldownDeadline',0) if case=='own-delete-rate-limit' and i in (2,3,4,6) else 0
        require(exact(c.get('retryAt'),expected_deadline) and R['integer'](c.get('retryAfter'))
            and max(0,expected_deadline-p['observedUntil'])<=c['retryAfter']<=max(0,expected_deadline-p['observedAt']),
            'Actual comment cooldown timer/deadline disagrees with capture clock')
        require(m.get('account')==initial.get('account') and R['uuid'](m.get('account')) and exact(m.get('epoch'),initial.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False and exact(m.get('writes'),0) and exact(m.get('networkCalls'),0),
            'Original disconnected identity changed')
        require(exact(m.get('document'),initial['initialDocument']) and exact(m.get('initialDocument'),initial['initialDocument'])
            and exact(m.get('records'),touched) and exact(m.get('initialRecords'),touched)
            and exact(m.get('unrelated'),unrelated) and exact(m.get('initialUnrelated'),unrelated),'Deletion mutated a List item/document')
        visible=[READ['ROOT_TS']] if removed else [TS]
        require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None
            and c.get('visibleTs')==visible and c.get('deleted')==([TS] if removed and not reopened else [])
            and m.get('detail',{}).get('record')==READ['KNOWN'],'Original visit/visible deletion changed')
        require((c.get('generation')==initial['comments']['generation']) is (not reopened),'Wrong original/reopened comment visit')
        page=D['canonical'](initial) if case=='own-delete-rate-limit' and i>=7 else initial['comments']['page']
        require(exact(c.get('page'),page) and exact(c.get('hasMore'),initial['comments']['hasMore']), 'Raw HTTP Page changed without canonical read')
        expected_typed=copy.deepcopy(root_typed if removed else initial['comments']['pageMessages'])
        if case=='own-delete-guards' and i in (5,6):
            changed=REC['changed_row']()
            expected_typed=[E['typed_edit'](m,changed['text'],changed['blocks'],changed['edited'],None,changed['reactions']) if m.get('ts')==TS else m for m in expected_typed]
        require(exact(c.get('pageMessages'),expected_typed),'Typed selected/root body was changed or deletion not applied')
        require(c.get('accepted')==[] and c.get('live')==[] and c.get('acceptedTs')==[] and c.get('liveTs')==[], 'Deletion invented another confirmed/live comment')
        expected_reactions=[dict(ts=READ['ROOT_TS'],reactions=[])] if removed else [dict(ts=TS,reactions=[dict(name='wave',count=1)])]
        require(exact(c.get('reactions'),expected_reactions),'Unrelated reaction/root projection changed')
        ledger=m.get('ledger',{});reads=ledger.get('requests')
        require(isinstance(reads,list) and len(reads)==READS[case][i] and exact(reads[:len(previous_reads)],previous_reads)
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
        fixture_ledger(f,case,i,record,previous_fixture);previous_fixture=f
        store=m.get('draftStore',{});drafts=store.get('drafts')
        require(isinstance(drafts,list) and len(drafts)==1,'Original reply draft missing/duplicated')
        draft=drafts[0]
        require(exact(draft.get('scope'),scope) and exact(draft.get('blocks'),reply) and draft.get('attempt') is None and draft.get('error') is None
            and all(draft.get(k) is False for k in ('busy','saving','loading','terminalPending','captureFailed')) and draft.get('native') is True
            and store.get('active')==f"{draft['id']}:{c['generation']}:{draft['generation']}",'Deletion consumed or replaced reply draft')
        S['inventory'](row['journal'],scope);S['paired'](draft,row['journal'])
        require(exact(row['journal'],first['journal']),'Unchanged reply journal was rewritten')
        if previous_draft:
            require((draft['id']==previous_draft['id']) is (not reopened) and (draft['generation']==previous_draft['generation']) is (not reopened),'Reply binding changed unexpectedly')
        previous_draft=draft
        require(exact(d.get('retainedCount'),int(i>0)),'Missing/duplicate retained deletion custody')
        active=d.get('active')
        if modal:
            phase='RateLimited' if case=='own-delete-rate-limit' and i>=2 else None
            require(isinstance(active,dict) and R['integer'](active.get('id'),1) and active.get('generation')==c['generation'] and active.get('ts')==TS
                and active.get('busy') is held and active.get('phase')==phase and active.get('localPending') is False
                and active.get('sourceCurrent') is (not(case=='own-delete-guards' and i==5)) and exact(active.get('record'),record)
                and active.get('canConfirm') is can_confirm(case,i) and active.get('canCheck') is False
                and active.get('canRetryLocal') is False,'Delete session/policy/once-only actions changed')
        else:
            require(active is None,'Dismissed deletion retained an active modal')
        inventory(row.get('deleteInventory'),case,i,scope,record,previous_inventory,p.get('cooldownDeadline',0));previous_inventory=row.get('deleteInventory')
        geometry(row['geometry'],case,i,reply,active,visible)
        require(str(draft['generation'])==row['geometry']['reply']['generation'],'Native reply generation differs from original binding')
        path=Path(directory)/f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture',''))==path,'Foreign deletion screenshot')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt.get('final'),rows[-1]['model']),'Final deletion state changed')
    return receipt


def validator(case):
    require(case in STAGES,'Unknown deletion guards case')
    return lambda fixture,receipt,directory,**kwargs:validate(fixture,receipt,directory,expected_case=case,**kwargs)
