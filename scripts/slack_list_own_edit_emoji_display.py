"""Original display-mode Delta, complete catalog and independent owned media/Save."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
from urllib.parse import urlsplit, parse_qs

_base=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_plain.py')))
P=_base['delta'].__globals__
W,O,T,N,G,R,C,READ,H,S=(P[k] for k in ('W','O','T','N','G','R','C','READ','H','S'))
require,exact=P['require'],P['exact']
CASES=('own-edit-plain-emoji-display','own-edit-plain-emoji-display-recovery')
STAGES={CASES[0]:('draft','display-pending','display-ready','typed','tone','saving','saved','readback'),CASES[1]:('draft','unavailable','editing','cancelled')}
READS={CASES[0]:(4,8,8,8,8,12,12,16),CASES[1]:(4,8,12,12)}
POLICIES={CASES[0]:(0,1,1,1,1,2,2,2),CASES[1]:(0,1,2,2)}
ROOT=Path(__file__).resolve().parents[1]
DATA=ROOT/'src/desktop/slack_lists/comments/fixture/plain/emoji_display'
HASHES={'initial.json':'62688b1877814e707aba7b110fca23f314cdc5bc2026fb8fe5544d6624adcf73','future.json':'4b17429dcfb63e1c92059967223199f1a531b89bad5c443ff655c87177410e1a','catalog.json':'4850d6b114d0762d2ba6d2348cf4b5e7d2862bba7795471ca3ea2de766fdf020'}
def authored(name):
    raw=(DATA/name).read_bytes()
    require(hashlib.sha256(raw).hexdigest()==HASHES[name],'Independent authored original bytes changed')
    return raw.decode('utf-8')
INITIAL_BYTES,FUTURE_BYTES,RAW_BYTES=(authored(n) for n in ('initial.json','future.json','catalog.json'))
INITIAL,FUTURE,RAW=(json.loads(v) for v in (INITIAL_BYTES,FUTURE_BYTES,RAW_BYTES))
BODY=FUTURE['messages'][1]['blocks'][:1]
TEXT='Initial :thumbsup: Custom :parrot_alias: Typed :smile: :smile:'
ACK=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=FUTURE['messages'][1])
PREFS=copy.deepcopy(P['PREFS']);PREFS['prefs']['emoji_mode']='native'
OPEN_ERROR='This comment could not be opened for editing. Saved edits and your reply draft are retained. Retry Edit when local storage and permissions are available.'

def ops(tone=4,appended=False):
    result=[{'insert':'Initial '},{'insert':{'slackemoji':{'text':f':thumbsup::skin-tone-{tone}:'}}},{'insert':' Custom '},{'insert':{'slackemoji':{'text':':parrot_alias:'}}}]
    if appended:result += [{'insert':' Typed '},{'insert':{'slackemoji':{'text':':smile:'}}},{'insert':' '},{'insert':{'slackemoji':{'text':':smile:'}}}]
    return result+[{'insert':'\n'}]
INITIAL_OPS,CHANGED_OPS=ops(),ops(5,True)
P.update(INITIAL_BYTES=INITIAL_BYTES,INITIAL=INITIAL,INITIAL_OPS=INITIAL_OPS,CHANGED_OPS=CHANGED_OPS,FUTURE=FUTURE,FUTURE_BYTES=FUTURE_BYTES,ACK=ACK,BODY=BODY,PREFS=PREFS)
ticket,delta,inventory,observations,ordered,policy_cycle=(P[k] for k in ('ticket','delta','inventory','observations','ordered','policy_cycle'))

def seed_value(initial):
    return dict(workspace='T1',user=O['SELF'],ts=O['TS'],target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=O['CHANNEL'],root=READ['ROOT_TS']),http=INITIAL,editable=[],preserved=INITIAL['messages'][1]['blocks'][1:],plain=dict(version=1,ordered_response=INITIAL_BYTES.rstrip('\n'),initial=dict(version=1,ops=INITIAL_OPS)))

def page(v,text,typed=None):
    raw=W['decoded'](text)
    require(exact(v.get('raw'),raw) and exact(v.get('witness'),dict(messageIndex=1,raw=raw['messages'][1],styleKeys=None)),'Original Page byte/order witness changed')
    W['typed'](v.get('messages'),typed or raw)
P['page']=page

def attempt(v,seed,phase,original=None):
    require(set(v)=={'id','record','capture','phase','raw','readback','retry_at'} and R['uuid'](v['id']) and R['uuid'](v['capture']) and v['phase']==phase and v['readback'] is None and v['retry_at']==0 and exact(v['raw'],ACK if phase=='Confirmed' else None),'Original durable attempt/ACK changed')
    require(exact(v['record'],dict(version=2,seed=seed,blocks=FUTURE['messages'][1]['blocks'],text=TEXT,plain_delta=dict(version=1,ops=CHANGED_OPS))),'Semantic edit wire or durable seed changed')
    if original:require(all(exact(v[k],original[k]) for k in ('id','record','capture','retry_at')),'Original attempt replaced')
P['attempt']=attempt

def catalog(v,owner,count,recovery):
    require(v.get('networkCalls')==0 and v.get('httpCapability') is False and v.get('contextsCurrent')==[True]*count and len(v.get('requests',[]))==count,'Complete catalog source lifecycle changed')
    for i,q in enumerate(v['requests'],1):
        require(exact(q,dict(serial=i,request=dict(method='POST',path='/api/emoji.list',args={}),owner=dict(account=owner['account'],epoch=owner['epoch'],team='T1',user=O['SELF']),response=dict(error='offline') if recovery and i==1 else dict(raw=RAW,bytes=RAW_BYTES),settled=True,contextCurrentAtReturn=True)),'Original complete catalog factory/raw/owner changed')

def native(v,expected,mount,action='state'):
    require(v.get('ok') is True and v.get('action')==action and v.get('sameNode') is True and v.get('scope')==mount['scope'] and v.get('generation')==mount['generation'],'Native emoji proof borrowed node/owner')
    ticket(v['delta'],expected,mount)
    labels=[x['insert']['slackemoji']['text'] for x in expected if isinstance(x['insert'],dict)]
    require([e.get('label') for e in v['embeds']]==labels,'Actual emoji occurrences differ from original Delta')
    for e in v['embeds']:
        if e['label']==':parrot_alias:':
            im=e.get('image');require(isinstance(im,dict) and im.get('alt')==':parrot_alias:' and isinstance(im.get('src'),str) and 'fixture.invalid' not in im['src'] and 'media' in im['src'],'Custom display borrowed raw external URL or lost original alt')
        else:require(e.get('image') is None,'Standard glyph changed into unobserved custom image')

def settled(v,expected,mount,scope,seed):
    native(v['native'],expected,mount);inventory(v['inventory'],scope,seed,expected)

def media(p,mount):
    before,after,d=p['mediaBefore'],p['mediaAfter'],p['mediaDelivery']
    native(before,INITIAL_OPS,mount,'media-hold');native(after,INITIAL_OPS,mount,'media-ready')
    require(all(after.get(k) is True for k in ('sameSelection','sameDelta','sameImage')) and exact(before['delta'],after['delta']) and exact(before['selection'],after['selection']),'Media delivery altered semantic revision, selection or original nodes')
    a,b=before['embeds'][1]['image'],after['embeds'][1]['image']
    require(a['width']!=16 and b['width']==16 and b['height']==16 and b['complete'] is True and after['oldSrc']==a['src'] and after['newSrc']==b['src'] and a['src'].split('?')[0]==b['src'].split('?')[0] and a['src']!=b['src'],'Original pending image did not refresh through same opaque ID')
    urls=[urlsplit(x) for x in (a['src'],d.get('uriBefore',''),b['src'])]
    queries=[parse_qs(u.query,strict_parsing=True) for u in urls]
    require(all(set(q)=={'v'} and len(q['v'])==1 and q['v'][0].isdigit() for q in queries)
        and len({(u.scheme,u.netloc,u.path,u.fragment) for u in urls})==1,'Media delivery changed original opaque URI beyond epoch')
    epochs=[int(q['v'][0]) for q in queries]
    require(epochs[0]<=epochs[1]<epochs[2] and epochs[1]==d.get('epochBefore'),'Original observed image/delivery/ready epochs changed')
    require(d.get('readyBefore') is False and d.get('readyAfter') is True and d.get('url')==RAW['emoji']['party_parrot'] and d.get('mime')=='image/png' and d.get('bytes')==(ROOT/'assets/icons/png/icon_16.png').stat().st_size and R['integer'](d.get('epochBefore')),'Actual original registry delivery changed')
    require(d.get('slot')==f"list-comment/{p['originalDisk']['account']}/T1/parrot_alias" and d.get('id','').startswith('super-platinum-media://emoji/'),'Media slot borrowed another account/name')

def geometry(g,reply,expected,saving,accepted):
    editing=expected is not None;vp=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('captureFault') is None and g.get('editing') is editing and isinstance(vp,list) and len(vp)==2 and all(R['integer'](x,1) for x in vp),'Original geometry missing')
    wanted=[('slack-list-comment-edit-cancel',saving),('slack-list-comment-edit-save',saving)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)]
    require([(c.get('id'),c.get('disabled')) for c in g['controls']]==wanted,'Original controls not settled')
    for c in g['controls']:require(c.get('visible') is True and c.get('hit') is True,'Control covered');N['rectangle'](c['rect'],vp)
    if saving:O['inert_text'](g['text'],g['inertTextPaint'],vp)
    else:require(g['inertTextPaint'] is None,'Unpermitted inert paint');N['text_proof'](g['text'],vp)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False and r.get('visible') is (not editing) and r.get('hidden') is editing and r.get('inert') is editing and g.get('replyNotice') is None,'Independent reply changed or exposed')
    require(g.get('releasedPaint') is None,'No released-editor exception permitted')
    if editing:
        e=g['edit'];ticket(e.get('delta'),expected);require(e.get('sameNode') is True and e.get('inputSameNode') is True and e.get('editable') is (not saving),'Original editor replaced');N['rectangle'](e['rect'],vp)
    else:require(g.get('edit') is None,'Original editor survived closure');N['rectangle'](r['rect'],vp)
    if saving:require(g.get('notice',{}).get('text')==T['NOTICES'][9],'Original saving notice missing')
    else:require(g.get('notice') is None,'Unexpected native edit notice')
    require('Preserved original context' in g.get('row',{}).get('text',''),'Original context disappeared')
    return vp

def active(m,mount,expected,stage,rows):
    """Untouched mount is revision zero; actual edits retain positive revisions."""
    e=m['edit'];a=e.get('active');cap=m.get('capture');saving=stage=='saving'
    require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[],'Unexpected display opening/retirement')
    if mount is None:
        require(a is None and cap is None,'Returned editor retains authority');return
    initial=stage in ('display-pending','display-ready','editing')
    require(R['integer'](a.get('revision')) and (a['revision']==0 if initial else a['revision']>0),'Untouched/edited original semantic revision changed')
    require(a.get('current') is True and a.get('scope')==mount['scope'] and str(a.get('generation'))==mount['generation'] and a.get('ts')==O['TS'] and a.get('blocks')==[]
        and exact(a.get('source'),INITIAL['messages'][1]) and exact(a.get('preserved'),INITIAL['messages'][1]['blocks'][1:])
        and a.get('busy') is saving and a.get('nativeReady') is True and a.get('captureFailed') is False and a.get('composing') is False and a.get('nativePending') is False,'Original display edit owner/source changed')
    delta(a.get('plainDelta'),expected)
    require(a.get('attempt')==(e['attempts'][0]['id'] if saving else None) and a.get('notice')==(T['NOTICES'][9] if saving else None),'Original display attempt/notice changed')
    require(set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'} and cap['scope']==a['scope'] and cap['generation']==a['generation'] and cap['mount']==mount['mount']
        and all(cap[k] is v for k,v in dict(failed=False,canSave=not saving,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=False).items()),'Original display capture authority changed')
    if stage=='tone':require(a['revision']>next(r for r in rows if r['stage']=='typed')['model']['edit']['active']['revision'],'Tone/history did not change original semantic revision')
    if saving:require(a['revision']==next(r for r in rows if r['stage']=='tone')['model']['edit']['active']['revision'],'Save changed semantic revision')

def transcript(value,accepted,geometry):
    """Shared message body, separate from the private Delta editor appearance."""
    thumb='\U0001f44d'+('\U0001f3fe' if accepted else '\U0001f3fd')
    glyphs=[thumb,':parrot_alias:']+(['\U0001f604','\U0001f604'] if accepted else [])
    titles=[':thumbsup:',':parrot_alias:']+([':smile:',':smile:'] if accepted else [])
    before,after=value['before'],value['after']
    for v in (before,after):
        require(v.get('ok') is True and v.get('action')=='transcript' and v.get('ts')==O['TS'] and v.get('images')==0 and v.get('viewport')==geometry['viewport'],'Original shared transcript target changed')
        require(all(R['integer'](v.get(k),1) for k in ('pane','row','body')) and len({v[k] for k in ('pane','row','body')})==3,'Original shared row/body identity absent')
        require(isinstance(v.get('bodyText'),str) and f'Initial {thumb} Custom :parrot_alias:' in v['bodyText'] and v['bodyText'] in geometry['row']['text'],'Actual shared transcript body differs from captured original row')
        if accepted:require(' Typed \U0001f604 \U0001f604' in v['bodyText'],'Saved semantic emoji occurrences absent from shared body')
        require(len(v['emojis'])==len(glyphs) and len({e['node'] for e in v['emojis']})==len(glyphs),'Shared emoji occurrences missing/duplicated')
        for e,text,title in zip(v['emojis'],glyphs,titles):
            require(R['integer'](e.get('node'),1) and e.get('text')==e.get('rendered')==text and e.get('title')==title and e.get('codepoints')==[ord(c) for c in text] and e.get('connected') is True and e.get('painted') is True,'Shared rich-block tone/Unicode/title changed')
    for k in ('pane','row','body','bodyText','renderedText','images'):require(exact(before[k],after[k]),'Shared original transcript changed across capture')
    for a,b in zip(before['emojis'],after['emojis']):
        for k in ('node','text','rendered','title','codepoints','connected','painted'):require(exact(a[k],b[k]),'Shared original emoji node changed across capture')

def validate(fixture,receipt,directory,*,expected_case=CASES[0],dimensions=None,png=True):
    case=expected_case;recovery=case==CASES[1]
    require(case in CASES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong display terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r['stage'] for r in rows]==list(STAGES[case]),'Missing/reordered display stages')
    initial=rows[0]['model']['comments'];first=initial['comments'];p0=rows[0]['proof'];reply=p0['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope'];ds=seed_value(initial)
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated) and R['uuid'](initial['account']),'Original records/account changed')
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(p0['replySeed']['scope'],scope) and exact(p0['replySeed']['blocks'],G['seed_blocks']()) and p0['replySeed']['reopened'] is True,'Original reply seed scope changed')
    O['reply_content'](reply,p0['replySeed']['blocks']);S['inventory'](journal,scope);W['disk'](p0['originalDisk'],p0['originalDisk'],journal,rows[0]['editInventory'])
    previous=None;oldreads=[];oldproof={}
    for i,row in enumerate(rows):
        stage=row['stage'];m=row['model'];c=m['comments'];p=row['proof'];saving=stage=='saving';accepted=stage in ('saved','readback');editing=stage in ('display-pending','display-ready','typed','tone','saving','editing')
        expected=(ops(5,True) if stage in ('tone','saving') else ops(4,True) if stage=='typed' else INITIAL_OPS) if editing else None
        require(exact(p['source'],dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p['replyBlocks'],reply) and exact(p['replySeed'],p0['replySeed']),'Original source/reply custody changed')
        for k,v in oldproof.items():
            if k=='displaySource':continue
            require(exact(p[k][:len(v)],v) if isinstance(v,list) else exact(p[k],v),'Original input proof rewritten')
        oldproof=copy.deepcopy(p)
        require(c['account']==initial['account'] and c['epoch']==initial['epoch'] and c['noSession'] is False and c['noTransport'] is False and c['writes']==0 and c['networkCalls']==0,'Original owner changed')
        for k in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore','composeLedger','reactionLedger'):require(exact(c[k],initial[k]),f'Independent state changed: {k}')
        for k in ('active','record','root','visibleTs','deleted','generation','hasMore'):require(exact(c['comments'][k],first[k]),f'Original comments owner changed: {k}')
        require(c['comments']['error']==(OPEN_ERROR if stage=='unavailable' else first['error']) and c['comments']['loading'] is saving,'Opening error/read lease state changed')
        ledger=c['ledger'];count=READS[case][i];C['validate_reads'](ledger['requests'])
        require(len(ledger['requests'])==count and ledger['completed']==count and ledger['cancelled']==0 and ledger['held'] is False and ledger['replies']==count//4 and [q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(count//4),'Original preflight read count or disposition changed')
        require(exact(ledger['requests'][:len(oldreads)],oldreads),'Original read history rewritten');oldreads=ledger['requests']
        ordered(row['ordered'],count//4,initial,ledger,stage=='readback',previous);previous=row['ordered'];page(row['ordered']['entryPage'],FUTURE_BYTES if stage=='readback' else INITIAL_BYTES,FUTURE if accepted else INITIAL)
        require(exact(c['comments']['page'],row['ordered']['entryPage']['raw']) and exact(c['comments']['pageMessages'],row['ordered']['entryPage']['messages']),'Installed Page differs')
        observations(row['plain'],initial,False)
        catalog(p['displaySource'],initial,0 if not i else (1 if recovery and i==1 else 2 if recovery or saving or accepted else 1),recovery)
        f=m['fixture'];require(f['enabled'] is True and f['networkCalls']==0 and f['httpCapability'] is False and exact(f['received'],INITIAL['messages'][1]) and exact(f['boot'],O['boot']()),'Original fixture authority changed')
        require(exact(f['policy'],[dict(request=q,response=r) for _ in range(POLICIES[case][i]) for q,r in policy_cycle()]),'Actual complete prefs/assignment read changed')
        sent=saving or accepted;n=int(sent)
        require(len(f['prepared'])==n and exact(f['updates'],f['prepared']) and exact(f['responses'],[dict(disposition='accepted',raw=ACK)] if sent else []) and exact(f['futureRows'],[FUTURE['messages'][1]] if sent else []) and f['held'] is saving and exact(f['releases'],[O['RELEASE']] if accepted else []) and f['published'] is False and f['publishedRow'] is None,'Original update/ACK changed')
        for q in f['prepared']:O['update_wire'](q,BODY)
        require(len(m['edit']['attempts'])==n,'Unexpected update attempts')
        if sent:
            a=m['edit']['attempts'][0];require(R['integer'](a['id'],1) and a['ts']==O['TS'] and exact(a['blocks'],FUTURE['messages'][1]['blocks']) and a['phase']==('sent' if saving else 'confirmed') and exact(a['receipt'],None if saving else ACK) and a['retryAt']==0 and exact(p['doubleSave'],dict(ok=True,disabled=True)),'Original attempt guard/settlement changed')
        vp=geometry(row['geometry'],reply,expected,saving,accepted);transcript(row['transcript'],accepted,row['geometry']);opens=p['opens'];require(len(opens)==int(i>0 and not (recovery and i==1)) and p['inputs']==[] and p['storageControls']==[],'Unexpected original mount/input/storage work')
        for op in opens:
            mount=op['mount'];a=op['active'];ticket(mount['delta'],INITIAL_OPS);delta(a['plainDelta'],INITIAL_OPS);inventory(op['inventory'],scope,ds,INITIAL_OPS)
            require(mount['ok'] is True and mount['scope']==a['scope'] and mount['generation']==str(a['generation']) and a['blocks']==[] and exact(a['source'],INITIAL['messages'][1]),'Original initial mount borrowed source');O['native_button'](op['action'],op['action']['proof'].get('id'),vp)
        active(m,opens[-1]['mount'] if editing else None,expected,stage,rows)
        if editing:
            proof_ticket=p['typed']['native']['delta'] if stage=='typed' else p['redoSettled']['native']['delta'] if stage in ('tone','saving') else p['initial']['native']['delta']
            require(exact(row['geometry']['edit']['delta'],proof_ticket),'Captured editor borrowed a different actual semantic ticket')
        require(exact(row['journal'],journal),'Independent durable reply changed');S['paired'](c['draftStore']['drafts'][0],journal);inventory(row['editInventory'],scope,ds if editing else None,expected,'Pending' if saving else None)
        if opens:
            mount=opens[0]['mount'];settled(p['initial'],INITIAL_OPS,mount,scope,ds)
        if not recovery and i>=1:native(p['mediaBefore'],INITIAL_OPS,mount,'media-hold')
        if not recovery and i>=2:media(p,mount)
        if not recovery and i>=3:
            e=p['append'];native(e,INITIAL_OPS,mount,'append');require(e.get('inputCommand')=='insertText' and e.get('text')==' Typed :smile: \U0001f604','Original typed input changed');settled(p['typed'],ops(4,True),mount,scope,ds)
        if not recovery and i>=4:
            require(R['integer'](p['historyPauseMillis'],1100),'History grouping pause not observed');native(p['tone'],ops(4,True),mount,'tone');require(p['tone']['label']==':thumbsup::skin-tone-4:' and p['tone']['hit'] is True and exact(p['tone']['event'],dict(type='click',isTrusted=False)),'Actual tone click missing');N['rectangle'](p['tone']['rect'],vp)
            for k,eop,aop in [('toneSettled',ops(5,True),None),('undoSettled',ops(4,True),None),('redoSettled',ops(5,True),None)]:settled(p[k],eop,mount,scope,ds)
            for k,before in [('undo',ops(5,True)),('redo',ops(4,True))]:native(p[k],before,mount,k);require(exact(p[k]['event'],dict(type='keydown',isTrusted=False,defaultPrevented=True,ctrlKey=True,shiftKey=k=='redo')),'Original normal history key changed')
        key='escape' if recovery and stage=='cancelled' else 'enter' if sent else None
        if key:
            v=p[key];require(v['ok'] is True and v['action']=='original-keydown' and v['key']==('Escape' if key=='escape' else 'Enter') and v['isTrusted'] is False and v['defaultPrevented'] is True and v['sameNode'] is True,'Original Save/Cancel key missing');ticket(v['before'],INITIAL_OPS if recovery else CHANGED_OPS);P['exposed'](v['exposed'],vp)
        require(len(p['actions'])==int(stage=='readback'),'Unexpected button actions')
        if p['actions']:O['native_button'](p['actions'][0],'slack-list-comments-refresh',vp)
        path=Path(directory)/f'{fixture}-{stage}.png';require(Path(row['capture'])==path,'Foreign capture path')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and all(exact(receipt['proof'].get(k),v) for k,v in rows[-1]['proof'].items()),'Terminal rewrote final evidence')
    W['disk'](receipt['proof']['closedDisk'],p0['originalDisk'],journal,rows[-1]['editInventory'])
    return receipt

def validate_closed(v,receipt):
    rows=receipt['stages'];recovery=receipt['case']==CASES[1];p=receipt['proof']
    require(set(v)=={'account','inventories','editInventories','editHistory','contentSha256'} and v['account']==p['originalDisk']['account'] and exact(v['inventories'],[rows[0]['journal']]) and exact(v['editInventories'],[rows[-1]['editInventory']]) and len(v['editHistory'])==int(not recovery),'Closed SQLite lost original drafts/history')
    if not recovery:attempt(v['editHistory'][0],seed_value(rows[0]['model']['comments']),'Confirmed',rows[5]['editInventory']['attempt'])
    require(isinstance(v['contentSha256'],str) and len(v['contentSha256'])==64 and all(c in '0123456789abcdef' for c in v['contentSha256']),'Closed SQLite hash missing')

def validator(case):
    require(case in CASES,'Unknown emoji display case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
