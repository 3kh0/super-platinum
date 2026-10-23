"""Exact rich-value no-op Save, with original native and durable reply custody."""
import copy
from pathlib import Path
import runpy

O=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
T=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_storage.py')))
N,G,R,C,READ,H,S=(O[k] for k in ('N','G','R','C','READ','H','S'))
require,exact=O['require'],O['exact']
STAGES={'own-edit-noop':('draft','unchanged-edit','unchanged-return','modified','reverted','reverted-return','readback','reopened'),
 'own-edit-noop-storage':('draft','editing','return-failed','returned')}
READS={'own-edit-noop':(4,8,8,12,12,12,16,20),'own-edit-noop-storage':(4,8,8,8)}
POLICIES={'own-edit-noop':(0,1,1,2,2,2,2,3),'own-edit-noop-storage':(0,1,1,1)}
TEMP=' Temporary native change 日🦊'
ORIGINAL=O['received']()['blocks'][:1]


def no_updates(model):
    f,e=model['fixture'],model['edit']
    require(f.get('enabled') is True and f.get('networkCalls')==0 and f.get('httpCapability') is False
        and exact(f.get('received'),O['received']()) and exact(f.get('boot'),O['boot']()),'Original own-comment fixture/source changed')
    require(all(f.get(k)==[] for k in ('prepared','updates','responses','futureRows','releases'))
        and f.get('held') is False and f.get('published') is False and f.get('publishedRow') is None
        and e.get('attempts')==[],'No-op prepared an update, attempt, ACK or publication')


def no_preflight(before,after):
    no_updates(before);no_updates(after)
    require(exact(before['comments']['ledger'],after['comments']['ledger'])
        and exact(before['fixture']['policy'],after['fixture']['policy']), 'No-op performed read or preflight')
    require(exact(before['edit']['active']['blocks'],ORIGINAL), 'Save did not capture exact original rich values')


def native(g,reply,blocks=None,failed=False):
    # Reuse the original painted geometry checks, including exact inert finishing
    # evidence. These indices describe native states, not the new stage order.
    T['geometry'](g,4 if failed else 1 if blocks is not None else 0,reply,blocks or ORIGINAL)
    require(g.get('reply',{}).get('sameNode') is True,'Original independent reply node replaced')
    require(TEMP not in g.get('row',{}).get('text',''),'Temporary edit leaked into transcript')
    if blocks is not None:
        require(g['edit'].get('sameNode') is True,'Original edit node replaced')
    if failed:
        require(g['controls'][0].get('text')=='Retry return','Failed no-op lost Retry return')
    return g['viewport']


def active(model,blocks,mount=None,failed=False):
    e=model['edit'];a=e.get('active');cap=model.get('capture')
    require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[], 'Unexpected edit opening/event/retirement')
    if blocks is None:
        require(a is None and cap is None,'Returned no-op retained edit or capture authority');return
    require(isinstance(a,dict) and a.get('current') is True and a.get('ts')==O['TS']
        and R['integer'](a.get('generation'),1) and R['integer'](a.get('revision'))
        and exact(a.get('source'),O['received']()) and exact(a.get('preserved'),O['received']()['blocks'][1:])
        and exact(a.get('blocks'),blocks) and a.get('busy') is False and a.get('composing') is False
        and a.get('attempt') is None and a.get('captureFailed') is False
        and a.get('nativeReady') is (not failed)
        and a.get('notice')==(T['NOTICES'][4] if failed else None), 'Original no-op edit binding or buffer changed')
    require(a.get('scope')==f"list-edit:{model['comments']['comments']['generation']}:{a['generation']}", 'Foreign original edit scope')
    require(isinstance(cap,dict) and set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'}
        and cap['scope']==a['scope'] and cap['generation']==a['generation']
        and cap['mount']==(None if failed else mount['mount']), 'Native capture owner or mount changed')
    expected=dict(failed=False,canSave=not failed,canCancel=True,canCheck=False,canRetry=False,busy=False,finishing=failed)
    require(all(cap[k] is v for k,v in expected.items()),'No-op action availability changed')


def common(model,initial,scope,reply,journal,reads,policies):
    no_updates(model)
    m=model['comments'];c=m['comments'];first=initial['comments']
    require(R['uuid'](m.get('account')) and m.get('account')==initial['account']
        and exact(m.get('epoch'),initial['epoch']) and m.get('noSession') is False and m.get('noTransport') is False
        and m.get('writes')==0 and m.get('networkCalls')==0,'Original isolated owner changed')
    for key in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated'):
        require(exact(m.get(key),initial.get(key)),'Canonical List records changed')
    for key in ('active','record','root','loading','error','visibleTs','deleted','generation','page','pageMessages','hasMore','reactions'):
        require(exact(c.get(key),first.get(key)),f'Original comment/transcript changed: {key}')
    require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
        and c.get('loading') is False and c.get('error') is None and c.get('visibleTs')==[O['TS']]
        and m.get('detail',{}).get('record')==READ['KNOWN'],'Original comment owner unavailable')
    require(exact(O['one_message'](c['page']['messages']),O['received']()),'Original HTTP comment changed')
    typed=O['one_message'](c['pageMessages'])
    require(all(exact(typed.get(k),O['received']().get(k)) for k in ('ts','thread_ts','user','blocks','text','reactions','edited','edit_fixture_opaque')),
        'Original typed rich body/context/reactions changed')
    ledger=m['ledger'];req=ledger['requests']
    require(len(req)==reads and ledger.get('completed')==reads and ledger.get('cancelled')==0
        and ledger.get('held') is False and ledger.get('replies')==reads//4,'Unexpected original read count/disposition')
    C['validate_reads'](req)
    require([q['method'] for q in req]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(reads//4)
        and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and
        (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in req),'Wrong read owner or order')
    require(exact(model['fixture'].get('policy'),[dict(request=q,response=r) for _ in range(policies) for q,r in O['policy_cycle']()]),'Original edit preflight factories changed')
    require(all(m['composeLedger'].get(k)==[] for k in ('prepared','posts','messages','preflights','policies'))
        and all(m['composeLedger'].get(k) is False for k in ('published','held','heldPost')),'No-op dispatched independent reply')
    require(exact(m.get('reactionLedger'),dict(prepared=[],issued=[],outcomes=[],completed=0,held=False,networkCalls=0,fullReads=[])), 'No-op dispatched reaction')
    store=m['draftStore'];require(len(store['drafts'])==1,'Independent reply missing')
    d=store['drafts'][0]
    require(exact(d.get('scope'),scope) and exact(d.get('blocks'),reply) and d.get('attempt') is None and d.get('error') is None
        and d.get('native') is True and all(d.get(k) is False for k in ('busy','saving','loading','terminalPending','captureFailed'))
        and store.get('active')==f"{d['id']}:{c['generation']}:{d['generation']}", 'Independent reply consumed/locked/replaced')
    base=initial['draftStore']['drafts'][0]
    require(d['id']==base['id'] and d['generation']==base['generation'],'No-op replaced independent reply binding')
    S['inventory'](journal,scope);S['paired'](d,journal)


def proof_actions(actions,ids,viewport):
    require(isinstance(actions,list) and len(actions)==len(ids),'Native no-op actions omitted/repeated')
    for action,id in zip(actions,ids):
        if id=='open':
            require(action.get('ok') is True and isinstance(action.get('proof'),dict),'Original Edit action missing')
            p=action['proof'];require(p.get('disabled') is False and p.get('visible') is True and p.get('hit') is True,'Edit action disabled/covered')
            N['rectangle'](p.get('rect',{}),viewport)
        else:
            require(action.get('id')==id,'Wrong native action order')
            O['native_button'](action.get('native'),id,viewport)


def validate(fixture,receipt,directory,*,expected_case='own-edit-noop',dimensions=None,png=True):
    case=expected_case;storage=case=='own-edit-noop-storage'
    require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True
        and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong no-op terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r.get('stage') for r in rows]==list(STAGES[case]),'Missing/reordered no-op stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords'])
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Original canonical inventory changed')
    initial=rows[0]['model']['comments'];p0=rows[0]['proof'];seed=p0['replySeed'];reply=p0['replyBlocks'];journal=rows[0]['journal']
    scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']()) and seed.get('reopened') is True
        and R['integer'](seed.get('revision'),1) and initial['initialDocument'].get('id')==READ['LIST']
        and exact(initial['document'],initial['initialDocument']) and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated),'Original persisted reply/records changed')
    O['reply_content'](reply,seed['blocks'])
    modified=copy.deepcopy(ORIGINAL);modified[0]['elements'][-1]['elements'][-1]['text']+=TEMP
    indexes=(1,2) if storage else (1,3,4,7)
    previous_requests=[];previous_opens=[];previous_actions=[]
    opens_by_stage=(0,1,1,1) if storage else (0,1,1,2,2,2,2,3)
    expected_actions=([],['open'],['open','slack-list-comment-edit-save'],['open','slack-list-comment-edit-save','slack-list-comment-edit-cancel']) if storage else (
        [],['open'],['open','slack-list-comment-edit-save'],['open','slack-list-comment-edit-save','open'],
        ['open','slack-list-comment-edit-save','open'],['open','slack-list-comment-edit-save','open','slack-list-comment-edit-save'],
        ['open','slack-list-comment-edit-save','open','slack-list-comment-edit-save','slack-list-comments-refresh'],
        ['open','slack-list-comment-edit-save','open','slack-list-comment-edit-save','slack-list-comments-refresh','open'])
    for i,row in enumerate(rows):
        p,m=row['proof'],row['model'];locked=storage and i==2
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply),'Original source/reply proof changed')
        require(exact(p.get('originalBlocks'),ORIGINAL if i else None),'Original editable rich values changed')
        common(m,initial,scope,reply,journal,READS[case][i],POLICIES[case][i])
        req=m['comments']['ledger']['requests'];require(exact(req[:len(previous_requests)],previous_requests),'Original read history rewritten');previous_requests=req
        blocks=modified if not storage and i==3 else ORIGINAL if i in indexes else None
        viewport=native(row['geometry'],reply,blocks,locked)
        rn=row['rowNative'];require(rn.get('ts')==O['TS'] and rn.get('editedMarkers')==0 and isinstance(rn.get('text'),str)
            and all(t in rn['text'] for t in O['received']()['text'].split('\n')) and 'Preserved original context' in rn['text'] and TEMP not in rn['text'],'Original row acquired edited marker/content')
        opens=p.get('opens');require(isinstance(opens,list) and len(opens)==opens_by_stage[i] and exact(opens[:len(previous_opens)],previous_opens),'Original Edit history changed')
        for op in opens:
            mount=op['mount'];a=op['active'];disk=op['inventory']
            require(mount.get('ok') is True and isinstance(mount.get('mount'),str) and bool(mount['mount']) and exact(mount.get('blocks'),ORIGINAL)
                and mount.get('scope')==a.get('scope') and str(a.get('generation'))==mount.get('generation')
                and exact(a.get('source'),O['received']()) and exact(a.get('blocks'),ORIGINAL),'Original native Edit seed/mount changed')
            T['inventory'](disk,scope,T['RST']['seed_value'](rows[0]['model']),ORIGINAL)
        require(len({op['mount']['mount'] for op in opens})==len(opens),'Reopened Edit borrowed previous native mount')
        previous_opens=opens
        active(m,blocks,opens[-1]['mount'] if opens else None,locked)
        if locked:
            require(row['journal'] is None and row['editInventory'] is None,'Read private SQLite while locked')
        else:
            require(exact(row['journal'],journal),'No-op rewrote/consumed reply journal')
            disk=row['editInventory'];T['inventory'](disk,scope,opens[-1]['inventory']['seed'] if blocks is not None else None,blocks or [])
        proof_actions(p['actions'],expected_actions[i],viewport)
        require(exact(p['actions'][:len(previous_actions)],previous_actions),'Original actions rewritten');previous_actions=p['actions']
        if i>=2:
            no_preflight(p['saveBefore'],p['saveAfter'])
            require(exact(p['saveBefore'],rows[1]['model']) and exact(p['saveAfter'],rows[2]['model']), 'Save proof detached from original stages')
        if not storage and i>=3:
            require(p.get('append',{}).get('ok') is True and exact(p['append'].get('blocks'),modified),'Actual native edit lost exact original marks/content')
        if not storage and i>=4:
            u=p['revert'];require(u.get('ok') is True and u.get('action')=='undo' and u.get('undone') is True and u.get('sameNode') is True
                and exact(u.get('before',{}).get('blocks'),modified) and exact(u.get('after',{}).get('blocks'),ORIGINAL),'No-op did not use original native Undo')
        if not storage and i>=5:
            no_preflight(p['revertSaveBefore'],p['revertSaveAfter'])
            require(exact(p['revertSaveBefore'],rows[4]['model']) and exact(p['revertSaveAfter'],rows[5]['model']),'Reverted Save proof detached from original stages')
        commands=p.get('storageControls');count=0 if not storage or i<2 else i-1
        require(isinstance(commands,list) and len(commands)==count,'Unexpected storage commands')
        if storage and i>=2:
            st=p['storage'];require(exact(commands,[st['hold']] if i==2 else [st['hold'],st['release']]),'Original lock acknowledgements changed')
            for seq,ack in enumerate(commands,1):T['lock_ack'](ack,ready,seq,initial['account'],scope,journal,rows[1]['editInventory'])
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign no-op capture')
        if png:H['png_signature'](path,dimensions)
    final=receipt['final'];p=receipt['proof']
    common(final,initial,scope,reply,journal,READS[case][-1],POLICIES[case][-1]);active(final,None)
    if storage:
        require(exact(final,rows[-1]['model']) and exact(p,rows[-1]['proof']),'Storage terminal differs from closed return')
    else:
        require(all(exact(p.get(k),v) for k,v in rows[-1]['proof'].items() if k!='actions'),'Final proof rewrote stage evidence')
        proof_actions(p['actions'],list(expected_actions[-1])+['slack-list-comment-edit-cancel'],rows[-1]['geometry']['viewport'])
        out=p['finalReturn'];native(out['native'],reply)
        require(exact(out['journal'],journal),'Final Cancel consumed independent reply')
        T['inventory'](out['inventory'],scope,None,[])
    return receipt


def validate_closed(snapshot,receipt):
    rows=receipt['stages'];p=receipt['proof'];last=rows[-1]
    disk=last['editInventory'] if receipt['case']=='own-edit-noop-storage' else p['finalReturn']['inventory']
    require(isinstance(snapshot,dict) and snapshot.get('account')==rows[0]['model']['comments']['account']
        and exact(snapshot.get('inventories'),[rows[0]['journal']]) and exact(snapshot.get('editInventories'),[disk])
        and snapshot.get('editHistory')==[] and isinstance(snapshot.get('contentSha256'),str)
        and len(snapshot['contentSha256'])==64 and all(c in '0123456789abcdef' for c in snapshot['contentSha256']),
        'Closed private SQLite lost reply, retained edit, or created durable send attempt')


def validator(case):
    require(case in STAGES,'Unknown no-op case')
    return lambda fixture,receipt,directory,**kwargs:validate(fixture,receipt,directory,expected_case=case,**kwargs)
