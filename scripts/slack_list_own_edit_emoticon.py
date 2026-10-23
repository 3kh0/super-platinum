"""Original key events, semantic Delta, ordered preflight and closed rich reply custody."""
import copy
import json
from pathlib import Path
import runpy

P=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_plain_emoji.py')))
W,O,T,N,G,R,C,READ,H,S=(P[k] for k in ('W','O','T','N','G','R','C','READ','H','S'))
require,exact=P['require'],P['exact']
INITIAL_BYTES,INITIAL,INITIAL_OPS=(P[k] for k in ('INITIAL_BYTES','INITIAL','INITIAL_OPS'))
CASES=('own-edit-plain-emoticon','own-edit-plain-emoticon-off')
STAGES={CASES[0]:('draft','editing','space','undo','redo','guards','cancelled','saving','saved','readback'),CASES[1]:('draft','editing','literal','cancelled')}
READS={CASES[0]:(4,8,8,8,8,8,8,16,16,20),CASES[1]:(4,8,8,8)}
POLICIES={CASES[0]:(0,1,1,1,1,1,1,3,3,3),CASES[1]:(0,1,1,1)}
TEXT='Enter :slightly_smiling_face:'
EMOJI={'insert':{'slackemoji':{'text':':slightly_smiling_face:'}}}
literal=lambda text:[{'insert':text+'\n'}]
HAPPY=literal('Happy :)');SPACE=[{'insert':'Happy '},EMOJI,{'insert':' \n'}]
ENTER=[{'insert':'Enter '},EMOJI,{'insert':'\n'}]
BODY=[{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[{'type':'text','text':'Enter '},{'type':'emoji','name':'slightly_smiling_face','unicode':'1f642'}]}]}]
FUTURE=copy.deepcopy(INITIAL)
FUTURE['messages'][1].update(text=TEXT,blocks=BODY+INITIAL['messages'][1]['blocks'][1:],edited={'user':O['SELF'],'ts':'1781525800.000001'})
FUTURE_BYTES=json.dumps(FUTURE,ensure_ascii=False,separators=(',',':'))+'\n'
ACK=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=FUTURE['messages'][1])
PREFS={'ok':True,'prefs':{'plain_text_mode':True,'emoji_mode':'as_text','locale':'en-US','all_notifications_prefs':{'channels':{},'global':{}},'convert_emoticons':True}}
P['ticket'].__globals__.update(TEXT=TEXT,CHANGED_OPS=ENTER,BODY=BODY,FUTURE=FUTURE,FUTURE_BYTES=FUTURE_BYTES,ACK=ACK)
ticket,delta,seed_value,inventory,attempt,page,ordered,exposed=(P[k] for k in ('ticket','delta','seed_value','inventory','attempt','page','ordered','exposed'))

def prefs(enabled):
    value=copy.deepcopy(PREFS);value['prefs']['convert_emoticons']=enabled;return value

def policy_cycle(enabled=True):
    v=copy.deepcopy(O['policy_cycle']());v[1]=(v[1][0],prefs(enabled));return v

def observations(value,owner,enabled):
    # Isolated dependency namespace; accepted baseline modules are not modified.
    P['observations'].__globals__['PREFS']=prefs(enabled)
    P['observations'](value,owner,False)
    require('customCatalog' not in value,'Keyboard conversion acquired unrelated custom catalog')

def ops_for(name):
    return {'editing':HAPPY,'space':SPACE,'undo':HAPPY,'redo':SPACE,'guards':HAPPY,'literal':literal('Happy :) '),'saving':ENTER}.get(name)

def selection(v,collapsed=None,text=None):
    require(isinstance(v,dict) and set(v)=={'collapsed','contained','anchorOffset','focusOffset','text'} and v['contained'] is True
        and type(v['collapsed']) is bool and R['integer'](v['anchorOffset']) and R['integer'](v['focusOffset']) and isinstance(v['text'],str),'Original DOM selection absent/foreign')
    if collapsed is not None:require(v['collapsed'] is collapsed,'Original selection shape changed')
    if text is not None:require(v['text']==text,'Original selection text changed')

def native_event(v,action,before,after,mount,viewport,composing=False,after_composing=False):
    require(v.get('ok') is True and v.get('action')==action and v.get('sameNode') is True and v.get('scope')==mount['scope'] and v.get('generation')==mount['generation'],'Original event borrowed editor/owner')
    for name,ops,phase in [('before',before,composing),('after',after,after_composing)]:
        observed=v[name]
        require(observed.get('composing') is phase,'Composition phase missing/changed')
        normalized=dict(observed,composing=False);ticket(normalized,ops,mount)
    require(v['after']['revision']>=v['before']['revision'],'Native revision moved backwards')
    exposed(v['exposed'],viewport)

def key(v,name,before,after,mount,viewport,consumed,default=False,composing=False,collapsed=True):
    native_event(v,'key',before,after,mount,viewport,composing,composing)
    require(v.get('key')==name and v.get('keyCode')==({'Space':32,'Enter':13,'Undo':90,'Redo':90}[name]) and v.get('isTrusted') is False
        and v.get('defaultPrevented') is consumed and v.get('defaultApplied') is default and v.get('ctrlKey') is (name in ('Undo','Redo'))
        and v.get('shiftKey') is (name=='Redo'),'Original key/default/history event changed')
    selection(v['selectionBefore'],collapsed);selection(v['selectionAfter'])

def guard_proof(guards,mount,viewport):
    require([g['kind'] for g in guards]==['code','url','selection','composition'],'Missing/reordered original native guards')
    for g,text in zip(guards[:2],('`Happy :)','https://example.test/:)')):
        key(g['key'],'Space',literal(text),literal(text+' '),mount,viewport,False,True)
    g=guards[2];v=g['selection'];native_event(v,'select',HAPPY,HAPPY,mount,viewport)
    require(v.get('start')==6 and v.get('end')==8,'Selection no longer targets original candidate');selection(v['selection'],False,':)')
    key(g['key'],'Space',HAPPY,literal('Happy  '),mount,viewport,False,True,collapsed=False)
    g=guards[3]
    for name,active in [('start',True),('end',False)]:
        v=g[name];native_event(v,'composition',HAPPY,HAPPY,mount,viewport,not active,active)
        require(v.get('active') is active and v.get('isTrusted') is False,'Original composition event changed');selection(v['selection'])
    key(g['key'],'Space',HAPPY,HAPPY,mount,viewport,False,False,True)

def geometry(g,reply,ops,saving,accepted):
    editing=ops is not None;viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('captureFault') is None and g.get('releasedPaint') is None
        and g.get('editing') is editing and isinstance(viewport,list) and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),'Original geometry unavailable')
    ids=[('slack-list-comment-edit-cancel',saving),('slack-list-comment-edit-save',saving)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)]
    require([(c['id'],c['disabled']) for c in g['controls']]==ids,'Original controls or busy guards changed')
    for c in g['controls']:exposed(c,viewport)
    if saving:O['inert_text'](g['text'],g['inertTextPaint'],viewport)
    else:require(g['inertTextPaint'] is None,'Unexpected inert exception');N['text_proof'](g['text'],viewport)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False and r.get('visible') is (not editing)
        and r.get('hidden') is editing and r.get('inert') is editing and g.get('replyNotice') is None,'Independent original rich reply changed')
    if editing:
        e=g['edit'];ticket(e.get('delta'),ops)
        require(e.get('sameNode') is True and e.get('inputSameNode') is True and e.get('editable') is (not saving),'Original editor remounted or became writable while saving')
        # The emoji embed is rendered by the installed private codec; exact
        # semantic identity is the neutral ticket, not a guessed DOM glyph.
        if all(isinstance(o['insert'],str) for o in ops):require(e.get('text','').rstrip('\n')==''.join(o['insert'] for o in ops).rstrip('\n'),'Original literal DOM text changed')
        N['rectangle'](e['rect'],viewport)
    else:require(g.get('edit') is None,'Returned edit remained mounted');N['rectangle'](r['rect'],viewport)
    if saving:
        n=g.get('notice') or {};require(n.get('text')==T['NOTICES'][9],'Original saving notice missing');exposed(n,viewport)
    else:require(g.get('notice') is None,'Unexpected native notice')
    text=g.get('row',{}).get('text','');require('Preserved original context' in text,'Transcript lost original context')
    require(('Enter ' in text) is accepted and (accepted or all(t in text for t in INITIAL['messages'][1]['text'].split('\n'))),'Original transcript leaked draft or failed accepted update')
    return viewport

def active(m,mount,ops,saving):
    e=m['edit'];a=e.get('active');cap=m.get('capture')
    require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[],'Unexpected edit lifecycle')
    if mount is None:require(a is None and cap is None,'Returned editor retains authority');return
    require(a.get('current') is True and a.get('scope')==mount['scope'] and str(a.get('generation'))==mount['generation'] and a.get('ts')==O['TS']
        and R['integer'](a.get('revision'),1) and a.get('blocks')==[] and exact(a.get('source'),INITIAL['messages'][1]) and exact(a.get('preserved'),INITIAL['messages'][1]['blocks'][1:])
        and a.get('busy') is saving and a.get('nativeReady') is True and a.get('captureFailed') is False and a.get('composing') is False and a.get('nativePending') is False,'Original active edit source/state changed')
    delta(a.get('plainDelta'),ops)
    require(a.get('attempt')==(e['attempts'][0]['id'] if saving else None) and a.get('notice')==(T['NOTICES'][9] if saving else None),'Original active attempt changed')
    require(set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'} and cap['scope']==a['scope']
        and cap['generation']==a['generation'] and cap['mount']==mount['mount'] and all(cap[k] is v for k,v in dict(failed=False,canSave=not saving,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=False).items()),'Original capture/lease guards changed')

def validate(fixture,receipt,directory,*,expected_case=CASES[0],dimensions=None,png=True):
    case=expected_case;enabled=case==CASES[0]
    require(case in CASES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong emoticon terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r['stage'] for r in rows]==list(STAGES[case]),'Missing/reordered emoticon stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);initial=rows[0]['model']['comments'];first=initial['comments'];p0=rows[0]['proof']
    reply=p0['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope'];seed=p0['replySeed'];ds=seed_value(initial)
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated) and R['uuid'](initial['account'])
        and initial['initialDocument']['id']==READ['LIST'] and exact(initial['document'],initial['initialDocument']),'Original isolated List changed')
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(seed['scope'],scope) and exact(seed['blocks'],G['seed_blocks']()) and seed['reopened'] is True and R['integer'](seed['revision'],1),'Original independent reply seed changed')
    O['reply_content'](reply,seed['blocks']);S['inventory'](journal,scope)
    original=p0['originalDisk'];require(original['account']==initial['account'] and exact(original['scope'],scope) and original['path'].endswith('.sqlite'),'Original private DB owner changed')
    W['disk'](original,original,journal,rows[0]['editInventory'])
    old_proof={};previous=None;old_requests=[]
    for i,row in enumerate(rows):
        name=row['stage'];m=row['model'];c=m['comments'];p=row['proof'];ops=ops_for(name);saving=name=='saving';accepted=name in ('saved','readback');sent=saving or accepted
        require(exact(p['source'],dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p['replySeed'],seed) and exact(p['replyBlocks'],reply) and exact(p['originalDisk'],original)
            and p.get('emoticonsEnabled') is enabled,'Original source/reply/policy identity changed')
        for k,v in old_proof.items():require(exact(p[k][:len(v)] if isinstance(v,list) else p[k],v),'Original native proof history rewritten')
        old_proof=copy.deepcopy(p)
        require(c['account']==initial['account'] and c['epoch']==initial['epoch'] and c['noSession'] is False and c['noTransport'] is False and c['writes']==0 and c['networkCalls']==0,'Original account changed')
        for k in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore','composeLedger','reactionLedger'):require(exact(c[k],initial[k]),f'Independent state changed: {k}')
        for k in ('active','record','root','error','visibleTs','deleted','generation','hasMore'):require(exact(c['comments'][k],first[k]),f'Original comments owner changed: {k}')
        require(c['comments']['loading'] is saving,'Original Save read lease changed')
        ledger=c['ledger'];n=READS[case][i];C['validate_reads'](ledger['requests'])
        require(len(ledger['requests'])==n and ledger['completed']==n and ledger['cancelled']==0 and ledger['held'] is False and ledger['replies']==n//4
            and [q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(n//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in ledger['requests']),'Original read factory/count/target changed')
        require(exact(ledger['requests'][:len(old_requests)],old_requests),'Original request history rewritten');old_requests=ledger['requests']
        ordered(row['ordered'],n//4,initial,ledger,name=='readback',previous);previous=row['ordered']
        page(row['ordered']['entryPage'],FUTURE_BYTES if name=='readback' else INITIAL_BYTES,FUTURE if accepted else INITIAL)
        require(exact(c['comments']['page'],row['ordered']['entryPage']['raw']) and exact(c['comments']['pageMessages'],row['ordered']['entryPage']['messages']),'Installed Page differs from original ordered witness')
        observations(row['plain'],initial,enabled)
        f=m['fixture'];require(f['enabled'] is True and f['networkCalls']==0 and f['httpCapability'] is False and exact(f['received'],INITIAL['messages'][1]) and exact(f['boot'],O['boot']()),'Fixture source changed')
        require(exact(f['policy'],[dict(request=q,response=r) for _ in range(POLICIES[case][i]) for q,r in policy_cycle(enabled)]),'Original selected policy/prefs reads changed')
        require(len(f['prepared'])==int(sent) and exact(f['updates'],f['prepared']) and exact(f['responses'],[dict(disposition='accepted',raw=ACK)] if sent else [])
            and exact(f['futureRows'],[FUTURE['messages'][1]] if sent else []) and f['held'] is saving and exact(f['releases'],[O['RELEASE']] if accepted else []) and f['published'] is False and f['publishedRow'] is None,'Actual update/ACK/disposition changed')
        for q in f['prepared']:O['update_wire'](q,BODY)
        require(len(m['edit']['attempts'])==int(sent),'Missing/duplicate update attempt')
        if sent:
            a=m['edit']['attempts'][0];require(R['integer'](a['id'],1) and a['ts']==O['TS'] and exact(a['blocks'],FUTURE['messages'][1]['blocks']) and a['phase']==('sent' if saving else 'confirmed') and exact(a['receipt'],None if saving else ACK) and a['retryAt']==0,'Actual update phase changed')
            require(exact(p['doubleSave'],dict(ok=True,disabled=True)),'Pending Save duplicate protection missing')
        viewport=geometry(row['geometry'],reply,ops,saving,accepted)
        opens=p['opens'];require(len(opens)==(0 if i==0 else 2 if enabled and i>=7 else 1),'Actual fresh Edit count changed')
        for op in opens:
            mount=op['mount'];a=op['active'];ticket(mount['delta'],INITIAL_OPS)
            require(mount['ok'] is True and isinstance(mount['mount'],str) and bool(mount['mount']) and mount['scope']==a['scope'] and mount['generation']==str(a['generation']) and a['blocks']==[] and exact(a['source'],INITIAL['messages'][1]),'Fresh Edit initialization/owner changed')
            delta(a['plainDelta'],INITIAL_OPS);inventory(op['inventory'],scope,ds,INITIAL_OPS);O['native_button'](op['action'],op['action']['proof'].get('id'),viewport)
        require(len({o['mount']['mount'] for o in opens})==len(opens),'Fresh Edit reused old mount')
        active(m,opens[-1]['mount'] if ops is not None else None,ops,saving)
        if ops is not None:ticket(row['geometry']['edit']['delta'],ops,opens[-1]['mount'])
        require(exact(row['journal'],journal),'Independent reply journal changed');S['paired'](c['draftStore']['drafts'][0],journal)
        inventory(row['editInventory'],scope,ds if ops is not None else None,ops,'Pending' if saving else None)
        expected=[] if i==0 else [(0,'Happy :)',INITIAL_OPS,HAPPY)]
        if enabled and i>=5:expected += [(0,'`Happy :)',SPACE,literal('`Happy :)')),(0,'https://example.test/:)',literal('`Happy :) '),literal('https://example.test/:)')),(0,'Happy :)',literal('https://example.test/:) '),HAPPY),(0,'Happy :)',literal('Happy  '),HAPPY)]
        if enabled and i>=7:expected.append((1,'Enter :)',INITIAL_OPS,literal('Enter :)')))
        require(len(p['inputs'])==len(expected),'Missing/extra original text inputs')
        for entry,(idx,text,before,after) in zip(p['inputs'],expected):
            v=entry['native'];mount=opens[idx]['mount'];require(v['ok'] is True and v['action']=='native-select-all-insertText' and v['text']==text and v['sameNode'] is True,'Original input action changed')
            ticket(v['before'],before,mount);ticket(v['after'],after,mount);exposed(v['exposed'],viewport);inventory(entry['inventory'],scope,ds,after)
        if i>=2:
            require(R['integer'](p['historyWaitMillis'],1000),'Normal history grouping boundary not measured')
            key(p['space'],'Space',HAPPY,SPACE if enabled else literal('Happy :) '),opens[0]['mount'],viewport,enabled,not enabled)
        if enabled and i>=3:key(p['undo'],'Undo',SPACE,HAPPY,opens[0]['mount'],viewport,True)
        if enabled and i>=4:key(p['redo'],'Redo',HAPPY,SPACE,opens[0]['mount'],viewport,True)
        if enabled and i>=5:guard_proof(p['guards'],opens[0]['mount'],viewport)
        if i>=(6 if enabled else 3):O['native_button'](p['cancel'],'slack-list-comment-edit-cancel',viewport)
        if sent:key(p['enter'],'Enter',literal('Enter :)'),ENTER,opens[1]['mount'],viewport,True)
        require(p['storageControls']==[] and len(p['actions'])==int(name=='readback'),'Unexpected storage or button actions')
        if name=='readback':O['native_button'](p['actions'][0],'slack-list-comments-refresh',viewport)
        path=Path(directory)/f'{fixture}-{name}.png';require(Path(row['capture'])==path,'Foreign capture path')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and all(exact(receipt['proof'].get(k),v) for k,v in rows[-1]['proof'].items()),'Terminal rewrote captured stages')
    W['disk'](receipt['proof']['closedDisk'],original,journal,rows[-1]['editInventory'])
    return receipt

def validate_closed(v,receipt):
    rows=receipt['stages'];p=receipt['proof'];enabled=receipt['case']==CASES[0]
    require(set(v)=={'account','inventories','editInventories','editHistory','contentSha256'} and v['account']==p['originalDisk']['account'] and exact(v['inventories'],[rows[0]['journal']])
        and exact(v['editInventories'],[rows[-1]['editInventory']]) and len(v['editHistory'])==int(enabled),'Closed database lost reply, cleanup or unique update history')
    if enabled:attempt(v['editHistory'][0],seed_value(rows[0]['model']['comments']),'Confirmed',rows[7]['editInventory']['attempt'])
    require(isinstance(v['contentSha256'],str) and len(v['contentSha256'])==64 and all(c in '0123456789abcdef' for c in v['contentSha256']),'Closed database hash missing')

def validator(case):
    require(case in CASES,'Unknown emoticon case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
