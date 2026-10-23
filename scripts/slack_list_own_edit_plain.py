"""Closed ordinary plain Delta editing; independent rich reply and original bytes."""
import copy
import json
from pathlib import Path
import runpy

W=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_ordered.py')))
O,T,N,G,R,C,READ,H,S=(W[k] for k in ('O','T','N','G','R','C','READ','H','S'))
require,exact=W['require'],W['exact']
STAGES={'own-edit-plain':('draft','editing','cancelled','saving','saved','readback'),
 'own-edit-plain-storage':('draft','editing','return-failed','returned')}
READS={'own-edit-plain':(4,8,8,16,16,20),'own-edit-plain-storage':(4,8,8,8)}
POLICIES={'own-edit-plain':(0,1,1,3,3,3),'own-edit-plain-storage':(0,1,1,1)}
INITIAL_BYTES=W['INITIAL_BYTES'];INITIAL=W['decoded'](INITIAL_BYTES)
INITIAL_OPS=[{'insert':'_*Original own comment.*_Second editable line.\n'}]
CHANGED_OPS=[{'insert':'Changed plain comment\n'}]
BODY=[{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[{'type':'text','text':'Changed plain comment'}]}]}]
FUTURE=copy.deepcopy(INITIAL)
FUTURE['messages'][1].update(text='Changed plain comment',blocks=BODY+INITIAL['messages'][1]['blocks'][1:],edited={'user':O['SELF'],'ts':'1781525800.000001'})
FUTURE_BYTES=json.dumps(FUTURE,ensure_ascii=False,separators=(',',':'))+'\n'
ACK=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=FUTURE['messages'][1])
PREFS={'ok':True,'prefs':{'plain_text_mode':True,'emoji_mode':'as_text','locale':'en-US','all_notifications_prefs':{'channels':{},'global':{}}}}


def policy_cycle():
    rows=copy.deepcopy(O['policy_cycle']());rows[1]=(rows[1][0],copy.deepcopy(PREFS));return rows


def delta(value,ops):
    require(exact(value,{'version':1,'ops':ops}),'Original neutral Delta changed or was coerced into blocks')


def ticket(value,ops,mount=None):
    require(isinstance(value,dict) and set(value)=={'ops','mode','revision','generation','composing','pending','attachmentEffects'}
        and exact(value['ops'],ops) and value['mode']=='PlainText' and R['integer'](value['revision'])
        and isinstance(value['generation'],str) and value['generation'].isdigit()
        and value['composing'] is False and value['pending'] is False and value.get('attachmentEffects')=='none-v1','Original native Delta ticket unavailable/changed')
    if mount:require(value['generation']==mount['generation'],'Native Delta generation borrowed')


def page(value,text,typed=None):
    raw=W['decoded'](text);message=raw['messages'][1]
    require(exact(value.get('raw'),raw) and exact(value.get('witness'),{'messageIndex':1,'raw':message,
        'styleKeys':['italic','bold'] if text==INITIAL_BYTES else None}),'Original Page index/raw/style order changed')
    W['typed'](value.get('messages'),typed or raw)


def ordered(v,count,owner,ledger,readback=False,previous=None):
    require(v.get('initialBytes')==INITIAL_BYTES and v.get('futureBytes')==FUTURE_BYTES and v.get('plainModeEnabled') is True,
        'Wrong original bytes or plain-mode observation')
    require(len(v['requests'])==count and len(v['pages'])==count and v.get('completed')==count
        and v.get('cancelled')==0 and v.get('held') is False and v.get('heldOwnerCurrent') is None and v.get('releases')==[],
        'Original ordered requests or completion changed')
    for i,(q,p) in enumerate(zip(v['requests'],v['pages'])):
        future=readback and i==count-1;text=FUTURE_BYTES if future else INITIAL_BYTES
        require(q.get('serial')==4*(i+1) and q.get('method')=='conversations.replies'
            and exact(q.get('args'),ledger['requests'][4*(i+1)-1]['args']) and q.get('bytes')==text
            and exact(q.get('raw'),W['decoded'](text)) and q.get('variant')==('future' if future else 'initial')
            and q.get('owner')==dict(serial=owner['comments']['generation'],account=owner['account'],epoch=owner['epoch'])
            and R['integer'](q.get('readSerial'),1),'Original factory/owner/bytes changed')
        page(p,text)
        require(p.get('current') is True and p.get('requestSerial')==q['serial'] and p.get('readSerial')==q['readSerial'],
            'Parsed Page borrowed private preflight identity')
    if previous:
        for key in ('requests','pages'):require(exact(v[key][:len(previous[key])],previous[key]),'Original ordered history rewritten')


def observations(value,owner,storage):
    require(value.get('enabled') is True and value.get('storage') is storage and value.get('noLiveNetwork') is True
        and exact(value.get('preferences'),PREFS),'Positive plain preferences missing/changed')
    require(exact(value.get('canonical'),dict(locale='en-US',membership=[],rows=[],current=True)),
        'Actual canonical locale/group membership unavailable or invented')
    c=value['completion'];seed=c['seed']
    require(c.get('enabled') is True and c.get('named') is True and c.get('protectedFixture') is True and c.get('identityCurrent') is True
        and c.get('account')==owner['account'] and c.get('epoch')==owner['epoch'] and c.get('team')=='T1' and c.get('user')==O['SELF']
        and c.get('networkCalls')==0 and c.get('httpCapability') is False and c.get('held')==0
        and c.get('completed')==0 and c.get('cancelled')==0 and all(c.get(k)==[] for k in ('requests','responses','releases')),
        'Plain source borrowed or performed unsupported semantic acquisition')
    user=dict(id=O['SELF'],name='fixture-self',team_id='T1',is_admin=True,profile={})
    boot=dict(ok=True,team=dict(id='T1',name='List completion'),self=user,users=[user],channels=[dict(id='CLISTLOCAL',name='project-local',team_id='T1',is_channel=True,is_member=True,updated=100,previous_names=[])],prefs=PREFS['prefs'],subteams={'self':[]})
    init=dict(ok=True,self=user,team=dict(id='T1',name='List completion'),workspaces=[{'id':'T1'},{'id':'TOTHER001'}],onboarding={'user_data':{'onboarding_complete':True}},user_date_created=123)
    require(exact(seed,dict(boot=boot,init=init,accepted=True)),'Original accepted Boot/Init source changed')


def seed_value(initial):
    v=W['disk_seed'](initial);v['editable']=[]
    v['plain']=dict(version=1,ordered_response=INITIAL_BYTES.rstrip('\n'),initial=dict(version=1,ops=INITIAL_OPS))
    return v


def attempt(value,seed,phase,original=None):
    require(isinstance(value,dict) and set(value)=={'id','record','capture','phase','raw','readback','retry_at'}
        and R['uuid'](value['id']) and R['uuid'](value['capture']) and value['phase']==phase and value['readback'] is None
        and value['retry_at']==0 and exact(value['raw'],ACK if phase=='Confirmed' else None),'Plain durable attempt/ACK changed')
    require(exact(value['record'],dict(version=2,seed=seed,blocks=FUTURE['messages'][1]['blocks'],text='Changed plain comment',plain_delta=dict(version=1,ops=CHANGED_OPS))),
        'Plain record lost source, Delta, prepared body or original context')
    if original:require(all(exact(value[k],original[k]) for k in ('id','record','capture','retry_at')),'Settlement rewrote original send attempt')


def inventory(v,scope,seed=None,ops=None,phase=None):
    keys={'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}
    if ops is not None:keys.add('plain_delta')
    require(isinstance(v,dict) and set(v)==keys and exact(v['scope'],scope) and v['ts']==O['TS']
        and R['integer'](v['revision']) and v['retry_at']==0 and exact(v['seed'],seed) and v['blocks']==[],
        'Original plain inventory/seed/scope changed')
    require((v['revision']==0 and v['capture']=='') or (v['revision']>0 and R['uuid'](v['capture'])),'Missing original durable capture')
    if ops is not None:delta(v['plain_delta'],ops)
    if phase:attempt(v['attempt'],seed,phase)
    else:require(v['attempt'] is None,'Unissued/cleared plain edit has send work')


def geometry(g,reply,editing=False,saving=False,failed=False,changed=False):
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('captureFault') is None
        and g.get('editing') is editing and isinstance(viewport,list) and len(viewport)==2 and all(R['integer'](v,1) for v in viewport),
        'Plain native geometry or mount unavailable')
    expected=[('slack-list-comment-edit-cancel',saving),('slack-list-comment-edit-save',saving or failed)] if editing else [('slack-list-comments-back',False),('slack-list-comment-send',False)]
    require([(c.get('id'),c.get('disabled')) for c in g['controls']]==expected,'Plain controls changed or pending actions enabled')
    for c in g['controls']:
        require(c.get('visible') is True and c.get('hit') is True,'Original control clipped/covered');N['rectangle'](c['rect'],viewport)
    if failed:T['finishing_text'](g['text'],g['inertTextPaint'],viewport)
    elif saving:O['inert_text'](g['text'],g['inertTextPaint'],viewport)
    else:require(g['inertTextPaint'] is None,'Unexpected inert exception');N['text_proof'](g['text'],viewport)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False
        and r.get('visible') is (not editing) and r.get('hidden') is editing and r.get('inert') is editing and g.get('replyNotice') is None,
        'Independent rich reply exposed/replaced/changed')
    if editing:
        e=g['edit']
        if failed:
            paint=g.get('releasedPaint')
            require(e.get('delta') is None and isinstance(paint,dict) and set(paint)=={'originalNode','originalInput','scope','generation','liveDeltaAbsent','readOnly'}
                and all(paint[k] is True for k in ('originalNode','originalInput','liveDeltaAbsent','readOnly'))
                and isinstance(paint.get('scope'),str) and isinstance(paint.get('generation'),str),'Released native editor borrowed a live Delta ticket or replacement DOM')
        else:
            require(g.get('releasedPaint') is None,'Released paint exception escaped finishing');ticket(e.get('delta'),CHANGED_OPS)
        require(e.get('sameNode') is True and e.get('inputSameNode') is True and e.get('editable') is (not saving and not failed)
            and e.get('text','').rstrip('\n')=='Changed plain comment','Original Quill node/input/text changed')
        N['rectangle'](e['rect'],viewport)
    else:require(g.get('edit') is None,'Returned plain edit survived');N['rectangle'](r['rect'],viewport)
    notice=T['NOTICES'][4] if failed else T['NOTICES'][9] if saving else None
    if notice:
        n=g.get('notice');require(n.get('text')==notice and n.get('visible') is True and n.get('hit') is True,'Actual recovery/saving guidance missing');N['rectangle'](n['rect'],viewport)
    else:require(g.get('notice') is None,'Unexpected native notice')
    text=g.get('row',{}).get('text','');source=FUTURE if changed else INITIAL
    require(all(t in text for t in source['messages'][1]['text'].split('\n')) and 'Preserved original context' in text
        and ('Changed plain comment' in text) is changed,'Original transcript leaked draft or lost accepted body')
    return viewport


def active(m,mount=None,saving=False,failed=False):
    e=m['edit'];a=e.get('active');cap=m.get('capture')
    require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[],'Unexpected plain opening/retirement')
    if mount is None:require(a is None and cap is None,'Returned editor retains original authority');return
    require(a.get('current') is True and a.get('scope')==mount['scope'] and str(a.get('generation'))==mount['generation']
        and a.get('ts')==O['TS'] and R['integer'](a.get('revision'),1) and a.get('blocks')==[]
        and exact(a.get('source'),INITIAL['messages'][1]) and exact(a.get('preserved'),INITIAL['messages'][1]['blocks'][1:])
        and a.get('busy') is saving and a.get('nativeReady') is (not failed) and a.get('captureFailed') is False
        and a.get('composing') is False and a.get('nativePending') is False,'Plain original edit source/representation/state changed')
    delta(a.get('plainDelta'),CHANGED_OPS)
    require(a.get('attempt')==(e['attempts'][0]['id'] if saving else None)
        and a.get('notice')==(T['NOTICES'][4] if failed else T['NOTICES'][9] if saving else None),'Plain attempt or notice changed')
    require(set(cap)=={'scope','generation','mount','failed','canSave','canCancel','canCheck','canRetry','busy','finishing'}
        and cap['scope']==a['scope'] and cap['generation']==a['generation'] and cap['mount']==(None if failed else mount['mount'])
        and all(cap[k] is v for k,v in dict(failed=False,canSave=not saving and not failed,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=failed).items()),
        'Plain capture mount or guarded action availability changed')


def exposed(v,viewport):
    require(v.get('visible') is True and v.get('hit') is True,'Original input was covered/clipped');N['rectangle'](v['rect'],viewport)


def validate(fixture,receipt,directory,*,expected_case='own-edit-plain',dimensions=None,png=True):
    case=expected_case;storage=case=='own-edit-plain-storage'
    require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True
        and receipt.get('fixture')==fixture and fixture in C['FIXTURES'],'Wrong plain terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r['stage'] for r in rows]==list(STAGES[case]),'Missing/reordered plain stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);initial=rows[0]['model']['comments'];first=initial['comments']
    p0=rows[0]['proof'];reply=p0['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope'];seed=p0['replySeed'];ds=seed_value(initial)
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated)
        and R['uuid'](initial['account']) and initial['initialDocument']['id']==READ['LIST'] and exact(initial['document'],initial['initialDocument']), 'Original isolated List/records changed')
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(seed['scope'],scope) and exact(seed['blocks'],G['seed_blocks']()) and seed['reopened'] is True and R['integer'](seed['revision'],1),'Original independent reply seed/scope changed')
    O['reply_content'](reply,seed['blocks']);S['inventory'](journal,scope)
    original=p0['originalDisk']
    require(original.get('account')==initial['account'] and exact(original.get('scope'),scope)
        and isinstance(original.get('path'),str) and original['path'].endswith('.sqlite'),'Original private database address/scope changed')
    W['disk'](original,original,journal,rows[0]['editInventory'])
    previous=None;old_proof={};old_requests=[]
    for i,row in enumerate(rows):
        m=row['model'];c=m['comments'];p=row['proof'];locked=storage and i==2;saving=not storage and i==3;accepted=not storage and i>=4;editing=i==1 or saving or locked
        require(exact(p['source'],dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p['replySeed'],seed) and exact(p['replyBlocks'],reply), 'Original source/reply proof changed')
        for k,v in old_proof.items():
            if isinstance(v,list):require(exact(p[k][:len(v)],v),'Original native proof history rewritten')
            elif k!='storage':require(exact(p[k],v),'Original proof replaced')
        old_proof=copy.deepcopy(p)
        require(c['account']==initial['account'] and c['epoch']==initial['epoch'] and c['noSession'] is False and c['noTransport'] is False
            and c['writes']==0 and c['networkCalls']==0,'Original account/session changed')
        for k in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore','composeLedger','reactionLedger'):
            require(exact(c[k],initial[k]),f'Independent state changed: {k}')
        for k in ('active','record','root','error','visibleTs','deleted','generation','hasMore'):
            require(exact(c['comments'][k],first[k]),f'Comments owner changed: {k}')
        require(c['comments'].get('loading') is saving,'Original Save read lease loading changed')
        ledger=c['ledger'];count=READS[case][i];C['validate_reads'](ledger['requests'])
        require(len(ledger['requests'])==count and ledger['completed']==count and ledger['cancelled']==0 and ledger['held'] is False and ledger['replies']==count//4
            and [q['method'] for q in ledger['requests']]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(count//4)
            and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in ledger['requests']),
            'Original private read count/disposition/target changed')
        require(exact(ledger['requests'][:len(old_requests)],old_requests),'Read history rewritten');old_requests=ledger['requests']
        ordered(row['ordered'],count//4,initial,ledger,not storage and i==5,previous);previous=row['ordered']
        page(row['ordered']['entryPage'],FUTURE_BYTES if not storage and i==5 else INITIAL_BYTES,FUTURE if accepted else INITIAL)
        require(exact(c['comments']['page'],row['ordered']['entryPage']['raw']) and exact(c['comments']['pageMessages'],row['ordered']['entryPage']['messages']),'Installed original Page differs')
        observations(row['plain'],initial,storage)
        f=m['fixture'];require(f['enabled'] is True and f['networkCalls']==0 and f['httpCapability'] is False and exact(f['received'],INITIAL['messages'][1]) and exact(f['boot'],O['boot']()),'Fixture authority changed')
        require(exact(f['policy'],[dict(request=q,response=r) for _ in range(POLICIES[case][i]) for q,r in policy_cycle()]),'Fresh current plain preference/policy reads changed')
        sent=saving or accepted;n=int(sent)
        require(len(f['prepared'])==n and exact(f['updates'],f['prepared']) and exact(f['responses'],[dict(disposition='accepted',raw=ACK)] if sent else [])
            and exact(f['futureRows'],[FUTURE['messages'][1]] if sent else []) and f['held'] is saving and exact(f['releases'],[O['RELEASE']] if accepted else [])
            and f['published'] is False and f['publishedRow'] is None,'Actual update/ACK/disposition changed')
        for q in f['prepared']:O['update_wire'](q,BODY)
        attempts=m['edit']['attempts'];require(len(attempts)==n,'Missing/duplicate plain attempt')
        if sent:
            a=attempts[0];require(R['integer'](a['id'],1) and a['ts']==O['TS'] and exact(a['blocks'],FUTURE['messages'][1]['blocks'])
                and a['phase']==('sent' if saving else 'confirmed') and exact(a['receipt'],None if saving else ACK) and a['retryAt']==0,'Actual plain update phase changed')
            require(exact(p['doubleSave'],dict(ok=True,disabled=True)),'Pending Save protection missing')
        viewport=geometry(row['geometry'],reply,editing,saving,locked,accepted)
        opens=p['opens'];require(len(opens)==(0 if i==0 else 1 if storage or i<3 else 2),'Original plain open count changed')
        for op in opens:
            mount=op['mount'];a=op['active'];ticket(mount['delta'],INITIAL_OPS)
            require(mount['ok'] is True and isinstance(mount['mount'],str) and mount['mount'] and mount['scope']==a['scope']
                and mount['generation']==str(a['generation']) and a['blocks']==[] and exact(a['source'],INITIAL['messages'][1]),'Original plain initialization owner changed')
            delta(a['plainDelta'],INITIAL_OPS);inventory(op['inventory'],scope,ds,INITIAL_OPS)
            O['native_button'](op['action'],op['action']['proof'].get('id'),viewport)
        require(len({v['mount']['mount'] for v in opens})==len(opens),'Fresh plain Edit reused detached mount')
        require(len(p['inputs'])==len(opens),'Native input missing or repeated')
        for entry,op in zip(p['inputs'],opens):
            v=entry['native'];require(v['ok'] is True and v['action']=='native-select-all-insertText' and v['text']=='Changed plain comment' and v['sameNode'] is True,'Actual native input missing')
            ticket(v['before'],INITIAL_OPS,op['mount']);ticket(v['after'],CHANGED_OPS,op['mount']);exposed(v['exposed'],viewport);inventory(entry['inventory'],scope,ds,CHANGED_OPS)
        active(m,opens[-1]['mount'] if editing else None,saving,locked)
        if locked:
            require(row['geometry']['releasedPaint']['scope']==m['edit']['active']['scope']
                and row['geometry']['releasedPaint']['generation']==str(m['edit']['active']['generation']),
                'Released original DOM borrowed another controller scope/generation')
        if locked:require(row['journal'] is None and row['editInventory'] is None,'SQLite read occurred while locked')
        else:
            require(exact(row['journal'],journal),'Independent reply journal changed');S['paired'](c['draftStore']['drafts'][0],journal)
            inventory(row['editInventory'],scope,ds if editing else None,CHANGED_OPS if editing else None,'Pending' if saving else None)
        if not storage:
            for key,when,which in [('escape',2,'Escape'),('enter',3,'Enter')]:
                if i>=when:
                    v=p[key];require(v['ok'] is True and v['action']=='original-keydown' and v['key']==which and v['keyCode']==(27 if which=='Escape' else 13)
                        and v['defaultPrevented'] is True and v['isTrusted'] is False and v['sameNode'] is True,'Original native key action missing/refused')
                    ticket(v['before'],CHANGED_OPS);exposed(v['exposed'],viewport)
            ids=['slack-list-comments-refresh'] if i==5 else []
            require(p['storageControls']==[],'Unexpected storage commands')
        else:
            ids=['slack-list-comment-edit-cancel']*(0 if i<2 else i-1)
            require(len(p['storageControls'])==len(ids),'Missing actual lock protocol')
            if i>=2:
                require(exact(p['cancelBefore'],rows[1]['model']) and exact(p['cancelAfter'],rows[2]['model']),'Cancel proof detached from original stage')
                require(exact(p['cancelBefore']['comments']['ledger'],p['cancelAfter']['comments']['ledger']) and exact(p['cancelBefore']['fixture'],p['cancelAfter']['fixture']),'Storage Cancel created read/send work')
                for seq,v in enumerate(p['storageControls'],1):T['lock_ack'](v,ready,seq,initial['account'],scope,journal,rows[1]['editInventory'])
        require(len(p['actions'])==len(ids),'Native button order changed')
        for v,id in zip(p['actions'],ids):O['native_button'](v,id,viewport)
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row['capture'])==path,'Foreign capture path')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and all(exact(receipt['proof'].get(k),v) for k,v in rows[-1]['proof'].items()),'Terminal rewrote original stages')
    W['disk'](receipt['proof']['closedDisk'],p0['originalDisk'],journal,rows[-1]['editInventory'])
    return receipt


def validate_closed(v,receipt):
    rows=receipt['stages'];last=rows[-1];storage=receipt['case']=='own-edit-plain-storage';p=receipt['proof']
    require(set(v)=={'account','inventories','editInventories','editHistory','contentSha256'} and v['account']==p['originalDisk']['account']
        and exact(v['inventories'],[rows[0]['journal']]) and exact(v['editInventories'],[last['editInventory']]) and len(v['editHistory'])==int(not storage),
        'Closed SQLite lost original reply/edit/unique send history')
    if not storage:attempt(v['editHistory'][0],seed_value(rows[0]['model']['comments']),'Confirmed',rows[3]['editInventory']['attempt'])
    require(isinstance(v['contentSha256'],str) and len(v['contentSha256'])==64 and all(c in '0123456789abcdef' for c in v['contentSha256']),'Closed SQLite hash missing')


def validator(case):
    require(case in STAGES,'Unknown plain case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
