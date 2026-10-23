"""Actual Save-owned Agentforce access failure and same-editor retry."""
import copy
import json
from pathlib import Path
import runpy

# Private helper namespace; accepted ordinary/standard validators are not changed.
E=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_plain_emoji.py')))
W,O,T,N,G,R,C,READ,H,S=(E[k] for k in ('W','O','T','N','G','R','C','READ','H','S'))
require,exact=E['require'],E['exact']
CASES=('own-edit-plain-member-agents',)
STAGES={CASES[0]:('draft','editing','save-refused','saving','saved','readback')}
READS={CASES[0]:(4,8,12,16,16,20)}
POLICIES={CASES[0]:(0,1,2,3,3,3)}
TEXT='Known @harbor and @not-found'
WIRE_TEXT='Known @ULISTMEM1 and @not-found'
CHANGED_OPS=[{'insert':TEXT+'\n'}]
BODY=[{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[
    {'type':'text','text':'Known '},{'type':'user','user_id':'ULISTMEM1'},
    {'type':'text','text':' and @not-found'}]}]}]
INITIAL_BYTES,INITIAL,INITIAL_OPS=(E[k] for k in ('INITIAL_BYTES','INITIAL','INITIAL_OPS'))
FUTURE=copy.deepcopy(INITIAL)
FUTURE['messages'][1].update(text=WIRE_TEXT,blocks=BODY+INITIAL['messages'][1]['blocks'][1:],edited={'user':O['SELF'],'ts':'1781525800.000001'})
FUTURE_BYTES=json.dumps(FUTURE,ensure_ascii=False,separators=(',',':'))+'\r\n'
ACK=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=FUTURE['messages'][1])
PREFS,REFUSAL=E['PREFS'],E['REFUSAL']
CHECKING='Checking permission to save this comment…'
E['delta'].__globals__.update(TEXT=TEXT,CHANGED_OPS=CHANGED_OPS,BODY=BODY,FUTURE=FUTURE,FUTURE_BYTES=FUTURE_BYTES,ACK=ACK)
delta,ticket,page,ordered,observations,seed_value,attempt,inventory,exposed,policy_cycle=(E[k] for k in ('delta','ticket','page','ordered','observations','seed_value','attempt','inventory','exposed','policy_cycle'))


_base_observations=observations
from slack_list_plain_member_agents_facts import member_stage, observations_for

def observations(value,owner,storage):
    observations_for(value,owner,storage,_base_observations)


def member_stage_checked(row,owner,index,rows):
    member_stage(row,owner,index,rows)


def attempt(value,seed,phase,original=None):
    require(isinstance(value,dict) and set(value)=={'id','record','capture','phase','raw','readback','retry_at'}
        and R['uuid'](value['id']) and R['uuid'](value['capture']) and value['phase']==phase and value['readback'] is None
        and value['retry_at']==0 and exact(value['raw'],ACK if phase=='Confirmed' else None),'Original member durable attempt changed')
    require(exact(value['record'],dict(version=2,seed=seed,blocks=FUTURE['messages'][1]['blocks'],text=WIRE_TEXT,plain_delta=dict(version=1,ops=CHANGED_OPS))),
        'Member record lost original Delta/body/context')
    if original:require(all(exact(value[k],original[k]) for k in ('id','record','capture','retry_at')),'Settlement rewrote original send attempt')

E['inventory'].__globals__['attempt']=attempt

def key(value,mount,viewport):
    require(value.get('ok') is True and value.get('action')=='original-keydown' and value.get('key')=='Enter' and value.get('keyCode')==13
        and value.get('defaultPrevented') is True and value.get('isTrusted') is False and value.get('sameNode') is True,'Original Enter Save missing')
    ticket(value['before'],CHANGED_OPS,mount);exposed(value['exposed'],viewport)

def geometry(g,reply,editing,busy,notice,accepted):
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('captureFault') is None and g.get('editing') is editing
        and isinstance(viewport,list) and len(viewport)==2 and all(R['integer'](x,1) for x in viewport),'Original native geometry unavailable')
    expected=[('slack-list-comment-edit-cancel',busy),('slack-list-comment-edit-save',busy)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)]
    require([(c.get('id'),c.get('disabled')) for c in g['controls']]==expected,'Busy controls enabled or original controls lost')
    for c in g['controls']:
        require(c.get('visible') is True and c.get('hit') is True,'Original control covered');N['rectangle'](c['rect'],viewport)
    if busy:O['inert_text'](g['text'],g['inertTextPaint'],viewport)
    else:require(g.get('inertTextPaint') is None,'Unexpected inert exception');N['text_proof'](g['text'],viewport)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False
        and r.get('visible') is (not editing) and r.get('hidden') is editing and r.get('inert') is editing and g.get('replyNotice') is None,
        'Independent original rich reply changed/exposed')
    require(g.get('releasedPaint') is None,'Member Save borrowed released editor paint')
    if editing:
        e=g['edit'];ticket(e.get('delta'),CHANGED_OPS)
        require(e.get('sameNode') is True and e.get('inputSameNode') is True and e.get('editable') is (not busy)
            and e.get('text','').rstrip('\n')==TEXT,'Original live Quill input changed')
        N['rectangle'](e['rect'],viewport)
    else:require(g.get('edit') is None,'Returned editor survived');N['rectangle'](r['rect'],viewport)
    if notice:
        n=g.get('notice') or {};require(n.get('text')==notice and n.get('visible') is True and n.get('hit') is True,'Original Save guidance missing');N['rectangle'](n['rect'],viewport)
    else:require(g.get('notice') is None,'Unexpected Save guidance')
    text=g.get('row',{}).get('text','')
    expected=('Known ','@harbor',' and @not-found') if accepted else INITIAL['messages'][1]['text'].split('\n')
    require(all(v in text for v in expected) and 'Preserved original context' in text and ('Known ' in text) is accepted,
        'Transcript lost accepted body/context or leaked draft')
    return viewport


def active(m,mount,busy,notice,sent):
    e=m['edit'];a=e.get('active');cap=m.get('capture')
    require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[],'Unexpected edit lifecycle')
    if mount is None:require(a is None and cap is None,'Closed editor retained authority');return
    require(a.get('current') is True and a.get('scope')==mount['scope'] and str(a.get('generation'))==mount['generation']
        and a.get('ts')==O['TS'] and R['integer'](a.get('revision'),1) and a.get('blocks')==[]
        and exact(a.get('source'),INITIAL['messages'][1]) and exact(a.get('preserved'),INITIAL['messages'][1]['blocks'][1:])
        and a.get('busy') is busy and a.get('nativeReady') is True and a.get('captureFailed') is False
        and a.get('composing') is False and a.get('nativePending') is False and a.get('notice')==notice
        and a.get('attempt')==(e['attempts'][0]['id'] if sent else None),'Original edit owner/Delta/attempt changed')
    delta(a.get('plainDelta'),CHANGED_OPS)
    require(set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'}
        and cap['scope']==a['scope'] and cap['generation']==a['generation'] and cap['mount']==mount['mount']
        and all(cap[k] is v for k,v in dict(failed=False,canSave=not busy,canCancel=not busy,canCheck=False,canRetry=False,busy=busy,finishing=False).items()),
        'Original native lease/disabled action proof changed')


def validate(fixture,receipt,directory,*,expected_case=CASES[0],dimensions=None,png=True):
    case=expected_case;retired=False
    require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True
        and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong member terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r['stage'] for r in rows]==list(STAGES[case]),'Missing/reordered member stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);initial=rows[0]['model']['comments'];first=initial['comments']
    p0=rows[0]['proof'];reply=p0['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope'];seed=p0['replySeed'];ds=seed_value(initial)
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated)
        and R['uuid'](initial['account']) and initial['initialDocument']['id']==READ['LIST'] and exact(initial['document'],initial['initialDocument']),'Original List/records changed')
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(seed['scope'],scope) and exact(seed['blocks'],G['seed_blocks']()) and seed['reopened'] is True and R['integer'](seed['revision'],1),'Independent reply seed changed')
    O['reply_content'](reply,seed['blocks']);S['inventory'](journal,scope)
    original=p0['originalDisk'];require(original.get('account')==initial['account'] and exact(original.get('scope'),scope)
        and isinstance(original.get('path'),str) and original['path'].endswith('.sqlite'),'Original isolated SQLite address changed')
    W['disk'](original,original,journal,rows[0]['editInventory'])
    previous=None;old_proof={};old_requests=[]
    for i,row in enumerate(rows):
        replaced=row['stage']=='replaced';p=row['proof'];m=row['model'];c=m if replaced else m['comments']
        failed=row['stage'] in ('save-refused','source-ready');held=False;saving=row['stage']=='saving';accepted=row['stage'] in ('saved','readback')
        editing=row['stage'] in ('editing','source-stale','save-refused','source-ready','saving');busy=held or saving
        notice=REFUSAL if failed else CHECKING if held else T['NOTICES'][9] if saving else None
        member_stage_checked(row,initial,i,rows)
        require(exact(p['source'],dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p['replySeed'],seed) and exact(p['replyBlocks'],reply)
            and exact(p['originalDisk'],original) and p['storageControls']==[],'Original source/reply/capture proof changed')
        for k,v in old_proof.items():
            require(exact(p[k][:len(v)] if isinstance(v,list) else p[k],v),'Original proof history rewritten')
        old_proof=copy.deepcopy(p)
        require(c['writes']==0 and c['networkCalls']==0 and exact(c['composeLedger'],initial['composeLedger']) and exact(c['reactionLedger'],initial['reactionLedger']),
            'Unrelated mutation issued')
        ledger=c['ledger'];count=READS[case][i];C['validate_reads'](ledger['requests'])
        require(len(ledger['requests'])==count and ledger['completed']==count and ledger['cancelled']==0 and ledger['held'] is False and ledger['replies']==count//4
            and [q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(count//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in ledger['requests']),
            'Original private reads changed')
        require(exact(ledger['requests'][:len(old_requests)],old_requests),'Original read history rewritten');old_requests=ledger['requests']
        ordered(row['ordered'],count//4,initial,ledger,row['stage']=='readback',previous);previous=row['ordered']
        require(exact(row['journal'],journal),'Independent durable reply changed')
        require(c['account']==initial['account'] and c['epoch']==initial['epoch'] and c['noSession'] is False and c['noTransport'] is False,'Original session changed')
        for k in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore'):
            require(exact(c[k],initial[k]),f'Independent state changed: {k}')
        for k in ('active','record','root','error','visibleTs','deleted','generation','hasMore'):
            require(exact(c['comments'][k],first[k]),f'Comments owner changed: {k}')
        require(c['comments'].get('loading') is busy,'Original Save read lease changed')
        page(row['ordered']['entryPage'],FUTURE_BYTES if row['stage']=='readback' else INITIAL_BYTES,FUTURE if accepted else INITIAL)
        require(exact(c['comments']['page'],row['ordered']['entryPage']['raw']) and exact(c['comments']['pageMessages'],row['ordered']['entryPage']['messages']),'Installed Page differs')
        observations(row['plain'],initial,False)
        f=m['fixture'];require(f['enabled'] is True and f['networkCalls']==0 and f['httpCapability'] is False and exact(f['received'],INITIAL['messages'][1]) and exact(f['boot'],O['boot']()),'Original fixture source changed')
        require(exact(f['policy'],[dict(request=q,response=r) for _ in range(POLICIES[case][i]) for q,r in policy_cycle()]),'Fresh current preference/policy reads changed')
        sent=saving or accepted;n=int(sent)
        require(len(f['prepared'])==n and exact(f['updates'],f['prepared']) and exact(f['responses'],[dict(disposition='accepted',raw=ACK)] if sent else [])
            and exact(f['futureRows'],[FUTURE['messages'][1]] if sent else []) and f['held'] is saving and exact(f['releases'],[O['RELEASE']] if accepted else [])
            and f['published'] is False and f['publishedRow'] is None,'Update/ACK count or disposition changed')
        for q in f['prepared']:O['update_wire'](q,BODY)
        ats=m['edit']['attempts'];require(len(ats)==n,'Missing/duplicate or premature update attempt')
        if sent:
            a=ats[0];require(R['integer'](a['id'],1) and a['ts']==O['TS'] and exact(a['blocks'],FUTURE['messages'][1]['blocks'])
                and a['phase']==('sent' if saving else 'confirmed') and exact(a['receipt'],None if saving else ACK) and a['retryAt']==0,'Original attempt/semantic alias payload changed')
            require(exact(p['doubleSave'],dict(ok=True,disabled=True)),'Pending duplicate Save protection missing')
        viewport=geometry(row['geometry'],reply,editing,busy,notice,accepted)
        active(m,p['opens'][0]['mount'] if editing else None,busy,notice,saving)
        S['paired'](c['draftStore']['drafts'][0],journal)
        inventory(row['editInventory'],scope,ds if editing else None,CHANGED_OPS if editing else None,'Pending' if saving else None)
        require(len(p['opens'])==int(i>0) and len(p['inputs'])==int(i>0),'Original input/open repeated or missing')
        if i>0:
            op=p['opens'][0];mount=op['mount'];a=op['active'];ticket(mount['delta'],INITIAL_OPS)
            require(mount['ok'] is True and isinstance(mount['mount'],str) and bool(mount['mount']) and mount['scope']==a['scope'] and mount['generation']==str(a['generation'])
                and a['blocks']==[] and exact(a['source'],INITIAL['messages'][1]),'Original initializer/mount changed')
            delta(a['plainDelta'],INITIAL_OPS);inventory(op['inventory'],scope,ds,INITIAL_OPS);O['native_button'](op['action'],op['action']['proof'].get('id'),viewport)
            entry=p['inputs'][0];v=entry['native'];require(v['ok'] is True and v['action']=='native-select-all-insertText' and v['text']==TEXT and v['sameNode'] is True,'Actual native input changed')
            ticket(v['before'],INITIAL_OPS,mount);ticket(v['after'],CHANGED_OPS,mount);exposed(v['exposed'],viewport);inventory(entry['inventory'],scope,ds,CHANGED_OPS)
        if sent:
            v=p['enter'];require(v['ok'] is True and v['action']=='original-keydown' and v['key']=='Enter' and v['keyCode']==13 and v['defaultPrevented'] is True
                and v['isTrusted'] is False and v['sameNode'] is True,'Original Enter Save missing')
            ticket(v['before'],CHANGED_OPS,p['opens'][0]['mount']);exposed(v['exposed'],viewport)
        ids=['slack-list-comments-refresh'] if row['stage']=='readback' else []
        require(len(p['actions'])==len(ids),'Unexpected native action')
        for v,id in zip(p['actions'],ids):O['native_button'](v,id,viewport)
        if accepted:require(exact(p['release'],O['RELEASE']),'Original update release changed')
        if i>=2:
            before=p['failedSaveBefore'];after=p['failedSaveAfter'];require(exact(before,rows[1]['model']) and exact(after,rows[2]['model']),'Failed Save proof detached from actual stages')
            for k in ('scope','generation','source','preserved','plainDelta','revision'):
                require(exact(before['edit']['active'][k],after['edit']['active'][k]),'Failed member source changed edit custody')
            require(exact(rows[2]['editInventory'],p['inputs'][0]['inventory']),'Failed member source rewrote durable original Delta')
            key(p['failedSave'],p['opens'][0]['mount'],viewport)
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row['capture'])==path,'Foreign capture path')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and all(exact(receipt['proof'].get(k),v) for k,v in rows[-1]['proof'].items()),'Terminal rewrote original stages')
    W['disk'](receipt['proof']['closedDisk'],original,journal,rows[-1]['editInventory'])
    return receipt


def validate_closed(v,receipt):
    rows=receipt['stages'];retired=False
    require(set(v)=={'account','inventories','editInventories','editHistory','contentSha256'} and v['account']==receipt['proof']['originalDisk']['account']
        and exact(v['inventories'],[rows[0]['journal']]) and exact(v['editInventories'],[rows[-1]['editInventory']]) and len(v['editHistory'])==int(not retired),
        'Closed original SQLite lost edit/reply or unique send history')
    if not retired:attempt(v['editHistory'][0],seed_value(rows[0]['model']['comments']),'Confirmed',rows[3]['editInventory']['attempt'])
    require(isinstance(v['contentSha256'],str) and len(v['contentSha256'])==64 and all(c in '0123456789abcdef' for c in v['contentSha256']),'Closed SQLite hash missing')


def validator(case):
    require(case in CASES,'Unknown member case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
