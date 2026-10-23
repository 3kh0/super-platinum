"""Unchanged/exact native Delta reversion with real Save reads and no update work."""
import copy
from pathlib import Path
import runpy

P=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_plain.py')))
W,O,T,N,G,R,C,READ,H,S=(P[k] for k in ('W','O','T','N','G','R','C','READ','H','S'))
require,exact=P['require'],P['exact']
INITIAL_BYTES,INITIAL,INITIAL_OPS,FUTURE_BYTES=(P[k] for k in ('INITIAL_BYTES','INITIAL','INITIAL_OPS','FUTURE_BYTES'))
ticket,delta,inventory,seed_value,page,ordered,observations,policy_cycle,exposed=(P[k] for k in ('ticket','delta','inventory','seed_value','page','ordered','observations','policy_cycle','exposed'))
CASES=('own-edit-plain-noop','own-edit-plain-noop-storage')
STAGES={CASES[0]:('draft','unchanged-edit','unchanged-return','modified','reverted','reverted-return','readback','reopened'),
        CASES[1]:('draft','editing','return-failed','returned')}
READS={CASES[0]:(4,8,12,16,16,20,24,28),CASES[1]:(4,8,12,12)}
POLICIES={CASES[0]:(0,1,2,3,3,4,4,5),CASES[1]:(0,1,2,2)}
ORIGINAL_TEXT='_*Original own comment.*_Second editable line.'
TEMP_TEXT='Temporary plain edit'
TEMP_OPS=[{'insert':TEMP_TEXT+'\n'}]


def geometry(g,reply,ops,failed=False):
    editing=ops is not None;viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('captureFault') is None and g.get('editing') is editing
        and isinstance(viewport,list) and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),'Original native geometry unavailable')
    expected=[('slack-list-comment-edit-cancel',False),('slack-list-comment-edit-save',failed)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)]
    require([(c.get('id'),c.get('disabled')) for c in g['controls']]==expected,'Original native controls changed')
    for c in g['controls']:
        require(c.get('visible') is True and c.get('hit') is True,'Original control covered/clipped');N['rectangle'](c['rect'],viewport)
    if failed:T['finishing_text'](g['text'],g['inertTextPaint'],viewport)
    else:require(g.get('inertTextPaint') is None,'Unexpected inert exception');N['text_proof'](g['text'],viewport)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False
        and r.get('visible') is (not editing) and r.get('hidden') is editing and r.get('inert') is editing and g.get('replyNotice') is None,'Independent original rich reply changed')
    if editing:
        e=g['edit'];require(e.get('sameNode') is True and e.get('inputSameNode') is True and e.get('editable') is (not failed)
            and e.get('text','').rstrip('\n')==ops[0]['insert'].rstrip('\n'),'Original native input changed/remounted')
        if failed:
            paint=g.get('releasedPaint');require(e.get('delta') is None and isinstance(paint,dict) and set(paint)=={'originalNode','originalInput','scope','generation','liveDeltaAbsent','readOnly'}
                and all(paint[k] is True for k in ('originalNode','originalInput','liveDeltaAbsent','readOnly')) and isinstance(paint.get('scope'),str) and isinstance(paint.get('generation'),str),
                'Local cleanup failure invented a live native ticket')
        else:require(g.get('releasedPaint') is None,'Released paint exception escaped finishing');ticket(e.get('delta'),ops)
        N['rectangle'](e['rect'],viewport)
    else:require(g.get('edit') is None and g.get('releasedPaint') is None,'Closed edit authority survived');N['rectangle'](r['rect'],viewport)
    if failed:
        notice=g.get('notice') or {};require(notice.get('text')==T['NOTICES'][4] and notice.get('visible') is True and notice.get('hit') is True,'Retry return guidance missing');N['rectangle'](notice['rect'],viewport)
    else:require(g.get('notice') is None,'Unexpected no-op guidance')
    text=g.get('row',{}).get('text','');require(all(t in text for t in INITIAL['messages'][1]['text'].split('\n')) and 'Preserved original context' in text and TEMP_TEXT not in text,'Original transcript changed/leaked draft')
    return viewport


def active(m,mount,ops,failed=False):
    e=m['edit'];a=e.get('active');cap=m.get('capture')
    require(e.get('attempts')==[] and e.get('opening') is False and e.get('events')==0 and e.get('retired')==[],'No-op created attempt or unexpected lifecycle')
    if mount is None:require(a is None and cap is None,'Closed edit retained original authority');return
    require(a.get('current') is True and a.get('scope')==mount['scope'] and str(a.get('generation'))==mount['generation'] and a.get('ts')==O['TS']
        and R['integer'](a.get('revision'),0) and a.get('blocks')==[] and exact(a.get('source'),INITIAL['messages'][1]) and exact(a.get('preserved'),INITIAL['messages'][1]['blocks'][1:])
        and a.get('busy') is False and a.get('nativeReady') is (not failed) and a.get('captureFailed') is False and a.get('composing') is False and a.get('nativePending') is False
        and a.get('attempt') is None and a.get('notice')==(T['NOTICES'][4] if failed else None),'Original no-op edit source/state changed')
    delta(a.get('plainDelta'),ops)
    require(set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'} and cap['scope']==a['scope'] and cap['generation']==a['generation']
        and cap['mount']==(None if failed else mount['mount']) and all(cap[k] is v for k,v in dict(failed=False,canSave=not failed,canCancel=True,canCheck=False,canRetry=False,busy=False,finishing=failed).items()),
        'No-op original capture lease/recovery guard changed')


def no_updates(m):
    f=m['fixture'];require(all(f[k]==[] for k in ('prepared','updates','responses','futureRows','releases')) and f['held'] is False
        and f['published'] is False and f['publishedRow'] is None and m['edit']['attempts']==[],'Unchanged Save created send work or invented ACK')


def validate(fixture,receipt,directory,*,expected_case=CASES[0],dimensions=None,png=True):
    case=expected_case;storage=case==CASES[1]
    require(case in CASES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong no-op terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r['stage'] for r in rows]==list(STAGES[case]),'Missing/reordered original no-op stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);initial=rows[0]['model']['comments'];first=initial['comments'];p0=rows[0]['proof']
    reply=p0['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope'];seed=p0['replySeed'];ds=seed_value(initial)
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated) and R['uuid'](initial['account'])
        and initial['initialDocument']['id']==READ['LIST'] and exact(initial['document'],initial['initialDocument']),'Original isolated List/records changed')
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(seed['scope'],scope) and exact(seed['blocks'],G['seed_blocks']()) and seed['reopened'] is True and R['integer'](seed['revision'],1),'Original reply seed changed')
    O['reply_content'](reply,seed['blocks']);S['inventory'](journal,scope)
    original=p0['originalDisk'];require(original.get('account')==initial['account'] and exact(original.get('scope'),scope) and isinstance(original.get('path'),str) and original['path'].endswith('.sqlite'),'Original private database address changed')
    W['disk'](original,original,journal,rows[0]['editInventory'])
    previous=None;old_proof={};old_requests=[]
    for i,row in enumerate(rows):
        m=row['model'];c=m['comments'];p=row['proof'];failed=storage and i==2
        editing=i in ((1,2) if storage else (1,3,4,7));ops=(TEMP_OPS if row['stage']=='modified' else INITIAL_OPS) if editing else None
        require(exact(p['source'],dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p['replySeed'],seed) and exact(p['replyBlocks'],reply) and exact(p['originalDisk'],original),'Original source/reply proof changed')
        for k,v in old_proof.items():
            if k=='storage':require(exact(p[k]['hold'],v['hold']),'Original held SQLite proof replaced')
            else:require(exact(p[k][:len(v)] if isinstance(v,list) else p[k],v),'Original native proof history rewritten')
        old_proof=copy.deepcopy(p)
        if i>0:require(exact(p['originalOps'],INITIAL_OPS),'Original native initial ops changed')
        require(c['account']==initial['account'] and c['epoch']==initial['epoch'] and c['noSession'] is False and c['noTransport'] is False and c['writes']==0 and c['networkCalls']==0,'Original account/session changed')
        for k in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore','composeLedger','reactionLedger'):
            require(exact(c[k],initial[k]),f'Independent state changed: {k}')
        for k in ('active','record','root','error','visibleTs','deleted','generation','hasMore'):
            require(exact(c['comments'][k],first[k]),f'Original comments owner changed: {k}')
        require(c['comments'].get('loading') is False,'Completed no-op stage retained pending read')
        ledger=c['ledger'];n=READS[case][i];C['validate_reads'](ledger['requests'])
        require(len(ledger['requests'])==n and ledger['completed']==n and ledger['cancelled']==0 and ledger['held'] is False and ledger['replies']==n//4
            and [q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(n//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in ledger['requests']),'Fresh original Save preflight missing/extra/retargeted')
        require(exact(ledger['requests'][:len(old_requests)],old_requests),'Read history rewritten');old_requests=ledger['requests']
        ordered(row['ordered'],n//4,initial,ledger,False,previous);previous=row['ordered'];page(row['ordered']['entryPage'],INITIAL_BYTES)
        require(exact(c['comments']['page'],row['ordered']['entryPage']['raw']) and exact(c['comments']['pageMessages'],row['ordered']['entryPage']['messages'])
            and exact(c['comments']['pageMessages'],first['pageMessages']),'No-op changed original typed/raw transcript')
        observations(row['plain'],initial,storage);require('customCatalog' not in row['plain'],'No-op acquired unrelated catalog')
        f=m['fixture'];require(f['enabled'] is True and f['networkCalls']==0 and f['httpCapability'] is False and exact(f['received'],INITIAL['messages'][1]) and exact(f['boot'],O['boot']()),'Original fixture authority changed')
        require(exact(f['policy'],[dict(request=q,response=r) for _ in range(POLICIES[case][i]) for q,r in policy_cycle()]),'Original current Save policy/prefs reads changed');no_updates(m)
        viewport=geometry(row['geometry'],reply,ops,failed)
        open_count=(int(i>0) if storage else (0 if i==0 else 1 if i<3 else 2 if i<7 else 3))
        require(len(p['opens'])==open_count,'Original Edit/reopen count changed')
        for op in p['opens']:
            mount=op['mount'];a=op['active'];ticket(mount['delta'],INITIAL_OPS)
            require(mount['ok'] is True and isinstance(mount['mount'],str) and bool(mount['mount']) and mount['scope']==a['scope'] and mount['generation']==str(a['generation'])
                and a['blocks']==[] and exact(a['source'],INITIAL['messages'][1]),'Fresh plain Edit changed original initializer')
            delta(a['plainDelta'],INITIAL_OPS);inventory(op['inventory'],scope,ds,INITIAL_OPS);O['native_button'](op['action'],op['action']['proof'].get('id'),viewport)
        require(len({op['mount']['mount'] for op in p['opens']})==open_count,'Fresh Edit reused detached mount')
        mount=p['opens'][-1]['mount'] if editing else None;active(m,mount,ops,failed)
        if failed:
            paint=row['geometry']['releasedPaint'];require(paint['scope']==mount['scope'] and paint['generation']==mount['generation'],'Released paint borrowed owner')
            require(row['journal'] is None and row['editInventory'] is None,'SQLite read occurred under exclusive lock')
        else:
            require(exact(row['journal'],journal),'Independent reply journal changed');S['paired'](c['draftStore']['drafts'][0],journal)
            inventory(row['editInventory'],scope,ds if editing else None,ops)
        inputs=0 if storage or i<3 else 1 if i==3 else 2
        require(len(p['inputs'])==inputs,'Native actual change/reversion proof missing or repeated')
        for j,entry in enumerate(p['inputs']):
            v=entry['native'];before,after,text=(INITIAL_OPS,TEMP_OPS,TEMP_TEXT) if j==0 else (TEMP_OPS,INITIAL_OPS,ORIGINAL_TEXT)
            require(v['ok'] is True and v['action']=='native-select-all-insertText' and v['text']==text and v['sameNode'] is True,'Actual same-node input change/revert missing')
            ticket(v['before'],before,p['opens'][1]['mount']);ticket(v['after'],after,p['opens'][1]['mount']);exposed(v['exposed'],viewport);inventory(entry['inventory'],scope,ds,after)
        if i>=2:
            require(exact(p['saveBefore'],rows[1]['model']) and exact(p['saveAfter'],rows[2]['model']),'Original unchanged Save proof detached from captured stages')
            for k in ('document','records','unrelated','draftStore','composeLedger','reactionLedger'):
                require(exact(p['saveBefore']['comments'][k],p['saveAfter']['comments'][k]),'Unchanged Save changed independent state')
        ids=[]
        if storage:
            if i>=2:ids.append('slack-list-comment-edit-save')
            if i==3:ids.append('slack-list-comment-edit-cancel')
            require(len(p['storageControls'])==(0 if i<2 else i-1),'Original SQLite lock protocol changed')
            if i>=2:
                for seq,v in enumerate(p['storageControls'],1):T['lock_ack'](v,ready,seq,initial['account'],scope,journal,rows[1]['editInventory'])
                require(exact(p['storage'],dict(hold=p['storageControls'][0],release=p['storageControls'][1] if i==3 else None)),'Storage acknowledgments differ')
        else:
            require(p['storageControls']==[],'Unexpected SQLite lock commands')
            if i>=2:ids.append('slack-list-comment-edit-save')
            if i>=6:ids.append('slack-list-comments-refresh')
            if i>=5:
                require(exact(p['revertSaveBefore'],rows[4]['model']) and exact(p['revertSaveAfter'],rows[5]['model']),'Reverted Save proof detached from actual stages')
                v=p['enter'];require(v['ok'] is True and v['action']=='original-keydown' and v['key']=='Enter' and v['keyCode']==13 and v['defaultPrevented'] is True and v['isTrusted'] is False and v['sameNode'] is True,'Original Enter Save missing')
                ticket(v['before'],INITIAL_OPS,p['opens'][1]['mount']);exposed(v['exposed'],viewport)
        require(len(p['actions'])==len(ids),'Original action order changed')
        for v,id in zip(p['actions'],ids):O['native_button'](v,id,viewport)
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row['capture'])==path,'Foreign original capture path')
        if png:H['png_signature'](path,dimensions)
    terminal=receipt['proof'];require(all(exact(terminal.get(k),v) for k,v in rows[-1]['proof'].items()),'Terminal rewrote original proof')
    if storage:
        require(exact(receipt['final'],rows[-1]['model']),'Storage return changed terminal');cleared=rows[-1]['editInventory']
    else:
        final=terminal['finalReturn'];require(exact(receipt['final'],final['model']) and exact(final['model']['comments'],rows[-1]['model']['comments']) and exact(final['model']['fixture'],rows[-1]['model']['fixture']),
            'Final Cancel created reads/mutations or changed original independent state')
        active(final['model'],None,None);no_updates(final['model']);geometry(final['native'],reply,None)
        O['native_button'](terminal['finalCancel'],'slack-list-comment-edit-cancel',rows[-1]['geometry']['viewport'])
        cleared=final['disk']['edit'];inventory(cleared,scope);W['disk'](final['disk'],original,journal,cleared)
    W['disk'](terminal['closedDisk'],original,journal,cleared)
    return receipt


def validate_closed(v,receipt):
    p=receipt['proof'];require(set(v)=={'account','inventories','editInventories','editHistory','contentSha256'} and v['account']==p['originalDisk']['account']
        and exact(v['inventories'],[receipt['stages'][0]['journal']]) and exact(v['editInventories'],[p['closedDisk']['edit']]) and v['editHistory']==[],
        'Closed no-op database lost reply/clear or acquired send history')
    inventory(v['editInventories'][0],p['originalDisk']['scope'])
    require(isinstance(v['contentSha256'],str) and len(v['contentSha256'])==64 and all(c in '0123456789abcdef' for c in v['contentSha256']),'Closed SQLite hash missing')


def validator(case):
    require(case in CASES,'Unknown plain no-op case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
