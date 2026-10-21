"""Closed mounted own-reply deletion; process/binary/log custody stays in the shared runner."""
import copy
import runpy
from pathlib import Path

O = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
N, G, R, C, READ, H, S = (O[k] for k in ('N','G','R','C','READ','H','S'))
require, exact = O['require'], O['exact']
TS, CHANNEL, SELF = O['TS'], O['CHANNEL'], O['SELF']
STAGES = {'own-delete': ('draft','confirming','cancelled','delete-held','deleted','readback','reopened'),
          'own-delete-failure': ('draft','rejected','unconfirmed','unconfirmed-checked','observed','reopened')}
READS = {'own-delete': (4,8,8,16,16,20,24), 'own-delete-failure': (4,12,16,20,24,28)}
POLICIES = {'own-delete': (0,1,1,3,3,3,3), 'own-delete-failure': (0,2,3,3,3,3)}
PHASES = {'own-delete': (None,None,None,'Pending','Confirmed','Confirmed','Confirmed'),
          'own-delete-failure': (None,'Rejected','Uncertain','Uncertain','Observed','Observed')}
REJECTED = {'ok':False,'error':'cant_delete_message'}
ACK = dict(ok=True,channel=CHANNEL,ts=TS,delete_fixture_opaque={'original':True})
BLOCKED = dict(ok=True,disabled=True,clicks=2)
NOTICES = {'rejected':'The message could not be deleted. Current permissions will be checked before another explicit Delete.',
    'unconfirmed':'Deletion is unconfirmed. Check the result; this attempt will not be sent again.',
    'unconfirmed-checked':'Deletion is still unconfirmed. Missing or changed comments are not proof; the original attempt will not be sent again.'}
PREFIX = 'slack-list-comment-delete-'


def wire():
    return dict(method='POST',path='/api/chat.delete',args=dict(channel=CHANNEL,ts=TS,_x_reason='animateAndDeleteMessageApi'))


def policy_cycle():
    generic = O['policy_cycle']()[0]
    return [dict(request=generic[0],response=generic[1]),
        dict(request=dict(method='POST',path='/cache/T1/permissions/info',args=dict(user_id=SELF,channel_id=CHANNEL,permissions=['DELETE_OWN_MESSAGE'])),
             response=dict(ok=True,allowed=['DELETE_OWN_MESSAGE'],permission_fixture_opaque={'original':True}))]


def source(initial):
    return dict(version=1,workspace='T1',user=SELF,ts=TS,
        target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=CHANNEL,root=READ['ROOT_TS']),
        http=initial['comments']['page'],message=O['one_message'](initial['comments']['pageMessages']))


def canonical(initial):
    page=copy.deepcopy(initial['comments']['page'])
    page['messages']=[m for m in page['messages'] if m.get('ts')!=TS]
    page['deleted']=[TS]
    return page


def layout(case,i):
    normal=case=='own-delete'
    modal=i in ((1,3) if normal else (1,2,3))
    held=normal and i==3
    removed=i>=4
    published=i>=(5 if normal else 4)
    reopened=i==len(STAGES[case])-1
    count=int(i>=3) if normal else min(i,2)
    return modal,held,removed,published,reopened,count


def fixture_ledger(f,case,i,record,previous):
    modal,held,removed,published,reopened,count=layout(case,i)
    seed=dict(received=O['received'](),boot=O['boot']())
    require(f.get('enabled') is True and exact(f.get('networkCalls'),0) and f.get('httpCapability') is False,
            'Deletion fixture acquired network capability')
    require(exact(f.get('seed'),seed) and exact(f.get('received'),seed['received']), 'Original received/boot source changed')
    require(exact(f.get('policy'),policy_cycle()*POLICIES[case][i]), 'Missing or rewritten scoped permission reads')
    require(exact(f.get('prepared'),[wire()]*count) and exact(f.get('deletes'),[wire()]*count)
        and exact(f.get('records'),[record]*count), 'Deletion endpoint, original owner/body, or once-only dispatch changed')
    responses=([dict(disposition='accepted',raw=ACK,retryAfter=None)] if count else []) if case=='own-delete' else (
        [dict(disposition='rejected',raw=REJECTED,retryAfter=None),dict(disposition='unconfirmed',raw=None,retryAfter=None)][:count])
    require(exact(f.get('responses'),responses) and f.get('held') is held,
            'Original ACK/rejection/unknown receipt was invented or changed')
    future=count>0 if case=='own-delete' else count==2
    require(f.get('futureDeleted')==(TS if future else None) and f.get('published') is published
        and f.get('deleted')==([TS] if published else [])
        and exact(f.get('releases'),[O['RELEASE']] if case=='own-delete' and i>=4 else []),
        'Held delivery or independent future deletion changed')
    if previous:
        for key in ('policy','prepared','deletes','records','responses','releases'):
            require(exact(f[key][:len(previous[key])],previous[key]), 'Original deletion ledger prefix changed')


def inventory(value,case,i,scope,record,previous):
    if i==0:
        require(value is None,'Unopened deletion unexpectedly loaded durable state')
        return
    require(isinstance(value,dict) and set(value)=={'scope','ts','revision','attempt','retry_at'}
        and exact(value['scope'],scope) and value['ts']==TS and exact(value['retry_at'],0)
        and R['integer'](value['revision']), 'Wrong durable deletion scope/deadline')
    phase=PHASES[case][i]
    a=value['attempt']
    if phase is None:
        require(a is None and exact(value['revision'],0),'Opening/cancelling created a durable deletion')
    else:
        require(isinstance(a,dict) and set(a)=={'id','record','phase','raw','readback','retry_at'} and R['uuid'](a['id'])
            and exact(a['record'],record) and a['phase']==phase and exact(a['retry_at'],0)
            and R['integer'](value['revision'],1), 'Wrong inert deletion attempt/record/phase')
        raw=ACK if phase=='Confirmed' else REJECTED if phase=='Rejected' else None
        require(exact(a['raw'],raw), 'Readback fabricated an ACK or changed original receipt')
        expected_read=dict(target=record['target'],raw=canonical({'comments':{'page':record['http']}})) if phase=='Observed' else None
        require(exact(a['readback'],expected_read), 'Deletion confirmed from absence, foreign scope, or a fabricated receipt')
    if previous is not None:
        require(value['revision']>=previous['revision'],'Deletion journal revision regressed')
        old=previous['attempt']
        if a and old:
            replaced=case=='own-delete-failure' and i==2
            require((a['id']!=old['id']) is replaced,'Explicit retry reused an old deletion or created replay authority')
        unchanged=(case=='own-delete' and i in (2,5,6)) or (case=='own-delete-failure' and i in (3,5))
        if unchanged:
            require(exact(value,previous),'Unchanged/closed deletion inventory was rewritten')


def geometry(g,case,i,reply,active,visible):
    modal,held,removed,published,reopened,count=layout(case,i)
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and isinstance(viewport,list)
        and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),'Invalid deletion native geometry')
    names=([PREFIX+'check'] if case=='own-delete-failure' and i in (2,3) else [])+[PREFIX+'cancel',PREFIX+'confirm'] if modal else ['slack-list-comments-back','slack-list-comment-send']
    controls=g.get('controls')
    require(isinstance(controls,list) and [c.get('id') for c in controls]==names,'Wrong deletion controls')
    for c in controls:
        disabled=held or (c['id']==PREFIX+'confirm' and case=='own-delete-failure' and i in (2,3))
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
    wanted='Working\u2026' if held else NOTICES.get(STAGES[case][i])
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


def validate(fixture,receipt,directory,*,expected_case='own-delete',dimensions=None,png=True):
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
    record=source(initial);previous_fixture=None;previous_reads=[];previous_draft=None;previous_inventory=None
    root_typed=[m for m in initial['comments']['pageMessages'] if m.get('ts')!=TS]
    require(len(root_typed)==1 and root_typed[0].get('ts')==READ['ROOT_TS'],'Missing canonical original root')
    for i,row in enumerate(rows):
        modal,held,removed,published,reopened,count=layout(case,i)
        p,model=row['proof'],row['model'];m,f,d=model['comments'],model['fixture'],model['delete'];c=m['comments']
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply) and p.get('runtimeForgotten',False) is reopened,'Original source/reply/reopen proof changed')
        blocked=i>=(3 if case=='own-delete' else 2)
        require(exact(p.get('blockedConfirm'),BLOCKED if blocked else None)
            and exact(p.get('release'),O['RELEASE'] if case=='own-delete' and i>=4 else None),'Held/unknown duplicate confirmation proof changed')
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
        page=canonical(initial) if (case=='own-delete' and i>=5) or reopened else initial['comments']['page']
        require(exact(c.get('page'),page) and exact(c.get('hasMore'),initial['comments']['hasMore']), 'Raw HTTP Page changed without canonical read')
        expected_typed=root_typed if removed else initial['comments']['pageMessages']
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
            phase=None if case=='own-delete' else PHASES[case][i]
            require(isinstance(active,dict) and R['integer'](active.get('id'),1) and active.get('generation')==c['generation'] and active.get('ts')==TS
                and active.get('busy') is held and active.get('phase')==phase and active.get('localPending') is False
                and active.get('sourceCurrent') is True and exact(active.get('record'),record)
                and active.get('canConfirm') is (not held and phase!='Uncertain') and active.get('canCheck') is (phase=='Uncertain')
                and active.get('canRetryLocal') is False,'Delete session/policy/once-only actions changed')
        else:
            require(active is None,'Dismissed deletion retained an active modal')
        inventory(row.get('deleteInventory'),case,i,scope,record,previous_inventory);previous_inventory=row.get('deleteInventory')
        geometry(row['geometry'],case,i,reply,active,visible)
        require(str(draft['generation'])==row['geometry']['reply']['generation'],'Native reply generation differs from original binding')
        path=Path(directory)/f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture',''))==path,'Foreign deletion screenshot')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt.get('final'),rows[-1]['model']),'Final deletion state changed')
    return receipt


def validator(case):
    require(case in STAGES,'Unknown own-delete case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
