"""Inert own-delete attempts recovered by a fresh process from closed SQLite bytes."""
import copy
import json
import runpy
from pathlib import Path

D=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete.py')))
O,N,G,R,C,READ,H,S=(D[k] for k in ('O','N','G','R','C','READ','H','S'))
require,exact=D['require'],D['exact']
TS,SELF=D['TS'],D['SELF']
STAGES={f'own-delete-restart-{mode}-{phase}': (
    ('exercise-draft','exercise-confirming','exercise-unresolved') if phase=='exercise' else
    ('restored-reply','restored-delete','unconfirmed-checked','cancelled','reopened-delete','observed','reopened'))
    for mode in ('pending','unknown') for phase in ('exercise','restore')}


def inventory(value,scope,record,phase):
    require(isinstance(value,dict) and set(value)=={'scope','ts','revision','attempt','retry_at'}
        and exact(value['scope'],scope) and value['ts']==TS and exact(value['retry_at'],0)
        and R['integer'](value['revision']),'Wrong original deletion inventory')
    if phase is None:
        require(exact(value['revision'],0) and value['attempt'] is None,'Opening created deletion authority')
        return
    a=value['attempt']
    require(isinstance(a,dict) and set(a)=={'id','record','phase','raw','readback','retry_at'}
        and R['uuid'](a['id']) and exact(a['record'],record) and a['phase']==phase and a['raw'] is None
        and exact(a['retry_at'],0) and R['integer'](value['revision'],1),'Restored attempt lost original UUID/record or invented ACK')
    readback=dict(target=record['target'],raw=D['canonical']({'comments':{'page':record['http']}})) if phase=='Observed' else None
    require(exact(a['readback'],readback),'Restored deletion resolved without exact positive scoped deleted marker')


def fixture_ledger(f,restore,mode,i,record):
    count=int(not restore and i==2)
    require(f.get('enabled') is True and exact(f.get('networkCalls'),0) and f.get('httpCapability') is False
        and exact(f.get('seed'),dict(received=O['received'](),boot=O['boot']())) and exact(f.get('received'),O['received']()),'Original isolated deletion source changed')
    require(exact(f.get('policy'),[] if restore else D['policy_cycle']()*i)
        and exact(f.get('prepared'),[D['wire']()]*count) and exact(f.get('deletes'),[D['wire']()]*count)
        and exact(f.get('records'),[record]*count),'Recovered deletion replayed or issued unrequested policy work')
    response=dict(disposition='accepted' if mode=='pending' else 'unconfirmed',raw=D['ACK'] if mode=='pending' else None,retryAfter=None)
    published=restore and i>=5
    require(exact(f.get('responses'),[response] if count else []) and f.get('releases')==[]
        and f.get('held') is (count==1 and mode=='pending') and f.get('published') is published
        and f.get('deleted')==([TS] if published else []) and f.get('futureDeleted')==(TS if count or published else None),
        'Unreleased original ACK or independent publication changed')


def geometry(g,restore,mode,i,reply,active,visible):
    # Reuse the exact accepted modal/preview/native-node contract by semantic state.
    if restore:
        case,index=(('own-delete',0),('own-delete-failure',2),('own-delete-failure',3),
                    ('own-delete',2),('own-delete-failure',2),('own-delete',4),('own-delete',6))[i]
    elif i<2:case,index='own-delete',i
    else:case,index=('own-delete',3) if mode=='pending' else ('own-delete-failure',2)
    D['geometry'](g,case,index,reply,active,visible)


def validate(fixture,receipt,directory,*,case,dimensions=None,png=True):
    require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True
        and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong deletion restart terminal')
    ready=receipt.get('readiness',{});H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages');require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case]),'Missing exact deletion restart stages')
    restore=case.endswith('-restore');mode='pending' if '-pending-' in case else 'unknown';phase='Pending' if mode=='pending' else 'Uncertain'
    reads=(4,4,8,8,8,12,16) if restore else (4,8,12)
    first=rows[0];initial=first['model']['comments'];p0=first['proof'];reply=p0['replyBlocks']
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Wrong original List inventory')
    scope=dict(workspace='T1',user=SELF,list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    O['reply_content'](reply,G['seed_blocks']())
    if restore:
        require(p0.get('replySeed') is None and exact(p0.get('initialReplyInventory'),first['journal']),'Restore seeded/replaced independent reply')
    else:
        seed=p0.get('replySeed',{})
        require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']())
            and seed.get('reopened') is True and R['integer'](seed.get('revision'),1),'Missing original rich reply seed')
        require(exact(p0.get('initialReplyInventory'),dict(scope=scope,revision=0,capture='',blocks=[],attempt=None)),'Exercise reused reply database')
    require(exact(O['one_message'](initial['comments']['page']['messages']),O['received']()),'Original received Page changed')
    record=D['source'](initial);initial_inv=p0.get('initialDeleteInventory')
    inventory(initial_inv,scope,record,phase if restore else None)
    require(exact(first['deleteInventory'],initial_inv),'Opening reply mutated inert deletion inventory')
    root=[m for m in initial['comments']['pageMessages'] if m.get('ts')!=TS]
    require(len(root)==1 and root[0].get('ts')==READ['ROOT_TS'],'Missing exact original root')
    previous=None
    for i,row in enumerate(rows):
        p,model=row['proof'],row['model'];m,f,d=model['comments'],model['fixture'],model['delete'];c=m['comments'];inv=row['deleteInventory']
        active_expected=i in (1,2,4) if restore else i>0
        held=not restore and mode=='pending' and i==2;removed=restore and i>=5;reopened=restore and i==6
        require(p.get('processId')==ready['pid'] and p.get('scenario')==mode and p.get('phase')==('restore' if restore else 'exercise')
            and exact(p.get('source'),dict(received=O['received'](),boot=O['boot']()))
            and exact(p.get('replySeed'),p0.get('replySeed')) and exact(p.get('replyBlocks'),reply)
            and exact(p.get('initialDeleteInventory'),initial_inv) and exact(p.get('initialReplyInventory'),p0['initialReplyInventory'])
            and p.get('runtimeForgotten',False) is reopened,'Original process/source/draft proof changed')
        require(exact(p.get('blockedConfirm'),D['BLOCKED'] if (restore and i>=1) or (not restore and i==2) else None)
            and exact(p.get('reopenedBlockedConfirm'),D['BLOCKED'] if restore and i>=4 else None),'Restored locked attempt did not refuse duplicate Confirm')
        publication=None
        if removed:
            original=initial_inv['attempt']
            publication=dict(attemptId=original['id'],record=original['record'],originalReceipt=None,deletes=0,
                fromPersistedRecord=True,deleted=TS)
        require(exact(p.get('publication'),publication),'Readback was not independently derived from original inert record')
        require(m.get('account')==initial.get('account') and R['uuid'](m.get('account')) and exact(m.get('epoch'),initial.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False and exact(m.get('writes'),0) and exact(m.get('networkCalls'),0),'Original disconnected identity changed')
        require(exact(m.get('document'),initial['initialDocument']) and exact(m.get('initialDocument'),initial['initialDocument'])
            and exact(m.get('records'),touched) and exact(m.get('initialRecords'),touched)
            and exact(m.get('unrelated'),unrelated) and exact(m.get('initialUnrelated'),unrelated),'Deletion changed unrelated List state')
        visible=[READ['ROOT_TS']] if removed else [TS]
        require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
            and c.get('loading') is False and c.get('error') is None and exact(c.get('retryAt'),0)
            and c.get('visibleTs')==visible and c.get('deleted')==([TS] if removed and not reopened else [])
            and m.get('detail',{}).get('record')==READ['KNOWN'],'Wrong original visit/deletion projection')
        require((c.get('generation')==initial['comments']['generation']) is (not reopened),'Visit changed without reopen')
        require(exact(c.get('page'),D['canonical'](initial) if reopened else initial['comments']['page'])
            and exact(c.get('pageMessages'),root if removed else initial['comments']['pageMessages'])
            and exact(c.get('hasMore'),initial['comments']['hasMore']),'Raw HTTP provenance or typed current row changed')
        require(c.get('accepted')==[] and c.get('live')==[] and c.get('acceptedTs')==[] and c.get('liveTs')==[],'Deletion inserted new comment authority')
        require(exact(c.get('reactions'),[dict(ts=READ['ROOT_TS'],reactions=[])] if removed else [dict(ts=TS,reactions=[dict(name='wave',count=1)])]),'Original reaction/root changed')
        ledger=m.get('ledger',{});requests=ledger.get('requests')
        require(isinstance(requests,list) and len(requests)==reads[i] and exact(ledger.get('completed'),reads[i])
            and exact(ledger.get('replies'),reads[i]//4) and exact(ledger.get('cancelled'),0) and ledger.get('held') is False,'Unexpected restart read or hidden policy work')
        C['validate_reads'](requests)
        require([q['method'] for q in requests]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(reads[i]//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in requests),'Read targeted different item/root')
        compose=m.get('composeLedger',{})
        require(all(compose.get(k)==[] for k in ('prepared','posts','messages','preflights','policies'))
            and all(compose.get(k) is False for k in ('published','held','heldPost')),'Restart sent independent reply')
        require(exact(m.get('reactionLedger'),dict(prepared=[],issued=[],outcomes=[],completed=0,held=False,networkCalls=0,fullReads=[])),'Restart issued reaction')
        require(exact(model.get('editFixture'),dict(enabled=True,received=O['received'](),boot=O['boot'](),policy=[],prepared=[],updates=[],responses=[],futureRows=[],published=False,publishedRow=None,held=False,releases=[],networkCalls=0,httpCapability=False)),'Restart mutated own-edit custody')
        fixture_ledger(f,restore,mode,i,record)
        store=m.get('draftStore',{});drafts=store.get('drafts')
        require(isinstance(drafts,list) and len(drafts)==1,'Independent reply draft missing/duplicated');draft=drafts[0]
        require(exact(draft.get('scope'),scope) and exact(draft.get('blocks'),reply) and draft.get('attempt') is None and draft.get('error') is None
            and all(draft.get(k) is False for k in ('busy','saving','loading','terminalPending','captureFailed')) and draft.get('native') is True
            and store.get('active')==f"{draft['id']}:{c['generation']}:{draft['generation']}",'Original rich reply buffer changed')
        require(exact(row['journal'],first['journal']),'Reply journal was rewritten');S['inventory'](row['journal'],scope);S['paired'](draft,row['journal'])
        invphase='Observed' if removed else phase if restore or i==2 else None
        inventory(inv,scope,record,invphase)
        if restore:
            if i<=4:require(exact(inv,initial_inv),'Inert restoration/cancel/negative Check rewrote original inventory')
            else:
                a=inv['attempt'];old=initial_inv['attempt']
                require(all(exact(a[k],old[k]) for k in ('id','record','raw','retry_at')) and inv['revision']>initial_inv['revision']
                    and exact(inv,rows[5]['deleteInventory']),'Positive Check created new attempt or lost original receipt')
        require(exact(d.get('retainedCount'),int(i>0)),'Wrong recovered retained deletion count')
        active=d.get('active')
        if active_expected:
            wanted_phase=phase if restore or (i==2 and mode=='unknown') else None
            require(isinstance(active,dict) and R['integer'](active.get('id'),1) and active.get('generation')==c['generation']
                and active.get('ts')==TS and active.get('busy') is held and active.get('phase')==wanted_phase
                and active.get('localPending') is False and active.get('canRetryLocal') is False and active.get('sourceCurrent') is True
                and exact(active.get('record'),record) and active.get('canConfirm') is (not restore and i==1)
                and active.get('canCheck') is (restore or (i==2 and mode=='unknown')),'Recovered attempt can replay or lost original scope')
        else:require(active is None,'Inactive restored deletion modal survived')
        geometry(row['geometry'],restore,mode,i,reply,active,visible)
        require(str(draft['generation'])==row['geometry']['reply']['generation'],'Reply native generation mismatch')
        if previous:
            old=previous['model']['comments'];old_draft=old['draftStore']['drafts'][0]
            require(exact(requests[:len(old['ledger']['requests'])],old['ledger']['requests']),'Read history changed')
            require((draft['id']==old_draft['id']) is (not reopened) and (draft['generation']==old_draft['generation']) is (not reopened),'Reply node rotated without reopen')
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign restart PNG')
        if png:H['png_signature'](path,dimensions)
        previous=row
    require(exact(receipt.get('final'),rows[-1]['model']) and exact(receipt.get('deleteInventory'),rows[-1]['deleteInventory']),'Final durable/native state differs')
    return receipt


def validator(case):
    require(case in STAGES,'Unknown exact own-delete restart case')
    return lambda fixture,receipt,directory,**kwargs:validate(fixture,receipt,directory,case=case,**kwargs)


def sql_custody(snapshot,row):
    require(snapshot.get('account')==row['model']['comments']['account'],'SQL account differs from original owner')
    values=snapshot.get('inventories',{});inv=row['deleteInventory'];attempt=inv['attempt']
    expected=dict(drafts=[row['journal']],history=[],edit_drafts=[],edit_history=[],delete_state=[inv],delete_history=[attempt])
    require(exact(values,expected),'Independent SQL lost original reply/deletion or contains unrelated authority')
    tables=snapshot.get('tables',{});scope=row['journal']['scope']
    for table in expected:
        require(isinstance(tables.get(table),list) and len(tables[table])==len(expected[table]),'SQL decoded/table row count mismatch')
        for stored,value in zip(tables[table],expected[table]):
            owner=json.loads(stored['owner'])
            require(exact(owner,scope if table=='drafts' else [scope,TS]) and exact(json.loads(stored['body']),value),'Original persisted owner/body differs')
            if table=='delete_history':require(stored['id']==attempt['id'],'Persisted deletion history key changed')


def validate_pair(mode,exercise,restore,original_db,copied_db,final_db):
    require(mode in ('pending','unknown') and exercise['case']==f'own-delete-restart-{mode}-exercise'
        and restore['case']==f'own-delete-restart-{mode}-restore' and exercise['fixture']==restore['fixture'],'Wrong deletion restart pair')
    require(exercise['readiness']['pid']!=restore['readiness']['pid'] and exercise['readiness']['nonce']!=restore['readiness']['nonce'],'Reused original process owner')
    last,first,final=exercise['stages'][-1],restore['stages'][0],restore['stages'][-1]
    require(exact(last['deleteInventory'],first['deleteInventory']) and exact(last['journal'],first['journal'])
        and exact(last['proof']['replyBlocks'],first['proof']['replyBlocks']) and first['proof']['replySeed'] is None,'Fresh process did not recover exact original disk records')
    require(original_db['sha256']==copied_db['sha256'] and exact(original_db['tables'],copied_db['tables'])
        and original_db['path']!=copied_db['path'],'Database transfer changed original bytes')
    sql_custody(original_db,last);sql_custody(copied_db,first);sql_custody(final_db,final)
    original=last['deleteInventory']['attempt'];observed=final['deleteInventory']['attempt']
    require(original['phase']==('Pending' if mode=='pending' else 'Uncertain') and observed['phase']=='Observed'
        and all(exact(original[k],observed[k]) for k in ('id','record','raw','retry_at')) and observed['raw'] is None,'Readback fabricated ACK/replayed original attempt')
    require(all(all(row['model']['fixture'][key]==[] for key in ('policy','prepared','deletes','records','responses','releases')) for row in restore['stages']),'Restore dispatched deletion/policy work')
    return True
