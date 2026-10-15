"""Synthetic complete protocol records. These are never native evidence."""
import copy
from pathlib import Path
import runpy
P=Path(__file__).resolve().parent
C=runpy.run_path(str(P/'protected_adoption_receipts.py'))
T=runpy.run_path(str(C['SCRIPTS']/'protected_enrollment_test_data.py'))
ST=runpy.run_path(str(P/'test_protected_adoption_storage.py'))
E=runpy.run_path(str(P/'protected_adoption_events.py'))
clone=copy.deepcopy
uid=ST['uid']
local=C['local'];scope=C['scope'];INITIAL=C['INITIAL'];NEWER=C['NEWER'];FINAL=C['FINAL']
NONCE='a'*32
STORAGE=P/('protected-adoption-'+NONCE)
TEXT={'open':'Review draft','submit':'Enable recovery','cancel':'Cancel waiting','check':'Check status','reload':'Reload saved drafts','adopt':'Continue editing','retry':'Retry saving','resume':'Resume editing'}
def review(v):return {k:clone(v[k]) for k in ('row','local','blocked','alternates','origins','uncommitted')}
def shown(v,status='Ready'):return dict(status=status,entries={t:dict(kind='managed' if x['row'] else 'local',inventory=review(x) if x['row'] else clone(x),uncommitted=None) if status=='Ready' else None for t,x in v.items()},pendingEnrollment=False)
def click(k,generation=None):
    v=T['click'](k,TEXT[k],generation)
    if k!='submit':v['proof']['id']='protected-adoption-'+k
    return v

def good(scenario='adopt-channel',fixture='protected-adoption'):
    target='thread' if scenario=='adopt-thread' else 'channel';failure=scenario=='failed-managed-write-channel';nativecase=scenario=='native-refusals-channel'
    names=C['stages'](scenario);schedule=E['schedule'](scenario)
    final=ST['receipt'](scenario)['finalInventory']
    initial={t:dict(scope=scope(t),clock=4,row=None,local=local(INITIAL,t=='thread'),blocked=None,alternates=[],origins=[],uncommitted=None) for t in ('channel','thread')}
    enrolled=clone(initial);enrolled[target]=clone(final[target]);enrolled[target].update(clock=5,local=local(INITIAL,target=='thread'),alternates=[],origins=[])
    row=enrolled[target]['row'];row['revision']=1;row['state'].update(local_revision=1,desired=C['encoded'](INITIAL,target,target=='thread'))
    generation='protected-20';adoption='protected-30';editor='protected-40';lanes={'channel':'protected-1','thread':'protected-2'}
    installed=dict(id='protected-50',adoptionId=adoption,installAttempt=None,expected=review(enrolled[target]),installedEditorSerial=0,reboundFrom=None,scope=scope(target),laneId=lanes[target],laneSerial=1,editor=editor,writerId=uid(400),enrollment=uid(100),serial=0)
    changed=C['BASE']['changed']
    saved=clone(enrolled);saved[target]=changed(enrolled[target],NEWER,target=='thread')
    rebound={**installed,'id':'protected-51','installAttempt':'protected-91','expected':review(saved[target]),'installedEditorSerial':1,'reboundFrom':'protected-50','serial':1}
    enrollment_result=dict(kind='enrolled',current=True,account=C['ACCOUNT'],expected=initial[target],review=review(enrolled[target]))
    d=dict(open=click('open'),consent=click('submit',generation),reload=click('reload'),enrollmentResult=enrollment_result)
    records=[];actions=[];trace=[];attempt=None;attempt_no=0;install_attempt=None;result=None;input_no=0;token=0;last_capture=None;active=None;registration=False;committed=False;ack=False;busy=False
    def trace_capture(n,composition=0):
        nonlocal last_capture
        last_capture=dict(id=target+'-composer',scope=n['scope'],visit=n['visit'],input=0,composition=composition,composing=False,wire=n['text'],start=n['start'],end=n['end'],direction=n['direction'],display_start=n['start'],display_end=n['end'],display_len=n['utf16Length'])
        trace.append(dict(kind='capture',evidence=dict(before=clone(last_capture))))
    def lock():
        nonlocal active,token
        token+=1;active=str(token);trace.append(dict(kind='lock',evidence=dict(token=active,before=clone(last_capture))))
    def release():
        nonlocal active
        if active:trace.append(dict(kind='release-requested',evidence=dict(token=active)));active=None
    for stage in names:
        pos=names.index(stage)
        after=lambda s:s in names and pos>=names.index(s)
        dirty=scenario=='late-input-channel' and after('interleaved') or nativecase and after('aba-input')
        done=after('installed') or scenario=='cancel-boundaries-channel' and after('after-installed')
        writes=2 if failure and stage=='newer' else 1 if stage=='newer' or failure and after('retried') else 0
        failed=failure and stage in ('failed','away','returned')
        text=FINAL if failure and stage=='newer' else NEWER if scenario=='late-input-channel' and dirty or failure and after('failed') or writes else INITIAL
        away=stage=='away';replaced=scenario=='owner-replacement-channel' and after('replaced') or failure and after('away')
        epoch=11 if away else 12 if failure and replaced else 10;view=4 if away or scenario=='owner-replacement-channel' and replaced else 5 if failure and replaced else 3
        p=dict(account=C['OTHER'] if away else C['ACCOUNT'],epoch=epoch,viewId=view,team='T1',channel='C2',thread=C['THREAD'] if target=='thread' else None,channelGeneration=1,threadGeneration=1,fieldScopes={},sessionPresent=False,transportPresent=False,cachePresent=False,isolated=not away,book={t:local(text if t==target else INITIAL,t=='thread') for t in ('channel','thread')},routed=2 if nativecase and dirty or failure and stage=='newer' else 1 if dirty or failed or writes else 0,refused=0)
        p['fieldScopes']={t:T['OLD']['field_scope'](p,t) for t in ('channel','thread')};p['channelComposer']=clone(p['book']['channel']['composer']);p['threadComposer']=clone(p['book']['thread']['composer'] if target=='thread' else local('')['composer'])
        inv=clone(initial if stage in ('initial','dialog') else enrolled)
        if dirty:inv[target]=clone(final[target])
        elif writes:inv=clone(saved if writes==1 else final)
        g=T['OLD']['measured'](fixture,target,text,p,stage,p['routed']);n=g['native'];n['visit']='visit-'+str(view)
        if INITIAL==text:n['start']=0
        if stage in ('selection-changed','selection-refused'):n['direction']='forward'
        if failure and stage in ('away','returned','retried','resumed'):n.update(start=n['utf16Length'],direction='forward')
        n['composing']=stage=='composition-active'
        g['proof']=None;g['readOnly']=stage=='expiry-locked';g['admission']='2' if g['readOnly'] else None
        observed=T['observed'](g);observed.update(readOnly=g['readOnly'],admission=g['admission'])
        # Event objects contain actual handler payloads, including independent IDs.
        for event in schedule[stage][len(actions):]:
            kind,_,phase=event.partition(':');a={'kind':kind}
            if kind=='current-load':a.update(epoch=epoch,viewId=view)
            elif kind=='dialog':a.update(id=generation,displayed=shown(initial),pending=[]);trace_capture(n)
            elif kind=='consent-click':a['id']=generation
            elif kind in ('prepared','enqueue'):
                a['expected']=clone(initial[target])
                if kind=='enqueue':lock();release();result=clone(enrollment_result)
            elif kind=='adopt-click':attempt_no+=1;attempt='protected-'+str(60+attempt_no);a['attempt']=attempt;result=None
            elif kind=='install-captured':
                a.update(attempt=attempt,expected=review(enrolled[target]),registry='protected-10');trace_capture(n,2 if nativecase and attempt_no>=3 else 0)
            elif kind=='registration-enqueued':a.update(attempt=attempt,adoptionId=adoption,dispatchHeldByOwnerGate=scenario=='late-input-channel')
            elif kind=='registration-reserved':a.update(adoptionId=adoption,retentionId=editor)
            elif kind=='registered':a.update(attempt=attempt,adoptionId=adoption,current=True,editor=editor,laneId=lanes[target],expected=review(enrolled[target]));registration=True
            elif kind=='install-prepared':install_attempt='protected-'+str(70+attempt_no);a.update(attempt=attempt,installAttempt=install_attempt,adoptionId=adoption)
            elif kind=='install-locked':a.update(attempt=attempt,installAttempt=install_attempt,adoptionId=adoption);lock()
            elif kind=='installed':
                installed['installAttempt']=install_attempt;a.update(attempt=attempt,receipt=clone(installed));committed=True;release();result=dict(kind='installed',attempt=attempt,alreadyInstalled=False,receipt=clone(installed))
            elif kind=='installation-acknowledged':a['receiptId']=installed['id'];ack=True
            elif kind=='installation-recovered':a['receipt']={**clone(installed),'serial':1 if failure else 0};result=dict(kind='installed',receipt=clone(a['receipt']),statusOnly=True)
            elif kind.startswith('gate'):
                a['phase']=phase
                if kind!='gate':release()
            elif kind=='adoption-cancel-click':a['installed']=clone(installed) if committed else None;result=dict(kind='canceled',installed=clone(a['installed']))
            elif kind=='input':
                input_no+=1;serial=(input_no+1) if dirty else input_no
                input_text=(INITIAL+' temporary' if input_no==1 else INITIAL) if nativecase else FINAL if input_no==2 else NEWER
                a.update(key=scope(target)['key'],serial=serial,local=local(input_text,target=='thread'))
            elif kind=='shell-replaced':
                old=records[-1]['projection'];a.update(mode='away' if away else 'return' if failure else 'same-epoch',before=dict(account=old['account'],epoch=old['epoch'],viewId=old['viewId'],channelDraft=old['book']['channel'],threadDraft=old['book']['thread'],channel=old['channelComposer'],thread=old['threadComposer']),afterEpoch=epoch,afterViewId=view,afterAccount=p['account'])
            elif kind=='adoption-check-click' and not committed:result=dict(kind='registered',adoptionId=adoption,current=scenario!='owner-replacement-channel',editor=editor,laneId=lanes[target],expected=review(enrolled[target]),statusOnly=True)
            elif kind=='input-retried':a.update(retentionId=editor,receiptId=installed['id'],serialBefore=1,serialAfter=1,review=review(saved[target]),pending=None,failedInputs=[]);result=clone(a)
            elif kind=='rebind-prepared':a.update(installAttempt=rebound['installAttempt'],priorReceipt=installed['id'],editor=editor);trace_capture(n);lock()
            elif kind=='rebound':a.update(priorReceipt=installed['id'],receipt=clone(rebound));release();result=clone(a)
            actions.append(a)
        if stage.endswith('refused') or stage=='refused':result=dict(kind='install-refused',attempt=attempt,error='ReviewRequired' if stage in ('selection-refused','aba-refused') else 'Stale')
        busy=stage in ('prepared','registering','interleaved','replaced','before-prepared','after-installed','selection-prepared','selection-changed','composition-prepared','composition-active','expiry-locked','expiry-restored','aba-prepared','aba-input')
        p['result']=None if busy else clone(result);p['gate']='registering' if stage in ('registering','interleaved') else 'locked' if stage in ('expiry-locked','expiry-restored') else 'installed' if stage=='after-installed' else 'prepared' if busy else None
        p['fixtureActions']=clone(actions);p['nativeAdmission']=clone(trace)
        p['registry']=None if away else 'protected-12' if failure and replaced else 'protected-11' if replaced else 'protected-10'
        p['selection']=dict(id='protected-102' if failure and replaced and not away else 'protected-101' if replaced and not away else 'protected-100',account=C['ACCOUNT'],epoch=10 if away else epoch,viewId=3 if away else view)
        p['retainedAdoptions']=[adoption] if any(x['kind']=='registration-enqueued' for x in actions) and not ack else []
        p['registered']=dict(expected=review(enrolled[target]),editor=editor,laneId=lanes[target],sampledCurrent=not(scenario=='owner-replacement-channel' and stage=='recovered')) if registration else None
        p['retained']=[dict(retentionId=editor,scope=scope(target),writerId=uid(400),enrollment=uid(100),serial=1 if failed else writes,pending=local(NEWER) if failed else None,failedInputs=[local(NEWER)] if failed else [],running=False,parked=False,error='Storage' if failed else None)] if registration else []
        p['localLanes']=[dict(id=lanes[t],laneId=lanes[t],scope=scope(t),serial=(3 if nativecase else 2) if dirty and t==target else 1,pending=local(text,t=='thread') if dirty and t==target else None,failedInputs=[],running=False,error='ReviewRequired' if dirty and t==target else None,inventory=clone(inv[t] if dirty and t==target else initial[t])) for t in ('channel','thread') if not(done and t==target)]
        p['routes']={t:dict(error='Stale') if away else dict(kind='managed',identity=rebound['id'] if after('resumed') else installed['id'],serial=1 if failed else writes,writer=uid(400),enrollment=uid(100),registry_current=not(replaced and stage in ('returned','retried'))) if done and t==target else dict(kind='local',identity=lanes[t],serial=(3 if nativecase else 2) if dirty and t==target else 1,writer=None,enrollment=None,registry_current=False) for t in ('channel','thread')}
        status='Inactive' if away else 'NeedsLoad' if stage=='enrollment' or scenario=='owner-replacement-channel' and replaced or stage=='returned' else 'Ready'
        p['discovery']=shown(initial if stage in ('initial','dialog') else saved if failure and after('retried') else enrolled,status)
        controls={}
        if p['retainedAdoptions'] or done:controls['check']=busy
        controls['reload']=busy;managed=p['discovery']['entries'][target] is not None and p['discovery']['entries'][target]['kind']=='managed'
        if not managed and not done:controls['open']=busy
        if managed and not done:controls['adopt']=busy
        if failed:controls['retry']=busy
        if done and not away and not p['routes'][target]['registry_current']:controls['resume']=busy or failed
        if busy:controls['cancel']=False
        # Copy is asserted independently by semantic acceptance; synthetic source
        # obtains shared text to keep mutations focused on protocol boundaries.
        message=runpy.run_path(str(P/'protected_adoption_semantics.py'))['message'](scenario,stage)
        if stage=='dialog':pages=[T['page'](fixture,scenario,stage,k,target,generation) for k in ('channel','thread')]
        else:
            page=T['page'](fixture,'enroll-channel','initial','status',target,generation);page['buttons']={}
            for k,disabled in controls.items():b=T['button'](k,TEXT[k],disabled);b['id']='protected-adoption-'+k;page['buttons'][b['id']]=b
            page['text']=message+''.join(TEXT[k] for k in controls);pages=[page]
        records.append(dict(stage=stage,pages=pages,native=observed,composer=None if stage=='dialog' else g,durable=inv,projection=p))
    by={r['stage']:r for r in records}
    obs=lambda s:clone(by[s]['native'])
    d['nativeInitial']=clone(by['initial']['native']['native']);a=dict(expected=dict(scope='timeline',latestId='message',windowEnd=3),connected=True,scope='timeline',windowEnd=3,latestMounted=True,latestHeight=32,inputGeneration=0,scrollTop=100,scrollHeight=300,clientHeight=200,bottomGap=0,ready=True,atBottom=True);d['nativeInitial']['homeAnchor']=dict(landed=True,before=a,after=clone(a))
    attempts=[s for s in names if s in ('prepared','registering','before-prepared','after-installed','selection-prepared','composition-prepared','expiry-locked','aba-prepared')]
    for s in attempts:
        d[s+'Before']=obs(s);d[s+'Before'].update(readOnly=False,admission=None);d[s+'Click']=click('adopt')
    for key,s in [('enrollmentResult','enrollment'),('installResult','installed'),('refusal','refused'),('selectionResult','selection-refused'),('compositionResult','composition-refused'),('expiryResult','expiry-refused'),('abaResult','aba-refused'),('registeredResult','registered'),('recoveredResult','recovered'),('canceledBefore','canceled-before'),('canceledAfter','canceled-after'),('failedStatus','failed'),('retryResult','retried'),('resumeResult','resumed')]:
        if s in by:d[key]=clone(by[s]['projection']['result']);d[key+'Writable']=obs(s)
    if 'newer' in by or scenario=='late-input-channel':
        s='failed' if failure else 'interleaved' if scenario=='late-input-channel' else 'newer';d['typed']=dict(observed=obs(s),flushError='Storage' if failure else 'ReviewRequired' if scenario=='late-input-channel' else None)
    if failure:d['typedAfterRetry']=dict(observed=obs('newer'),flushError=None)
    if committed:d['oldLaneRefusal']=dict(laneId=lanes[target],beforeSerial=1,afterSerial=1,result='Stale')
    for k,c in [('check','check')] if scenario=='owner-replacement-channel' else [('cancelBefore','cancel'),('checkBefore','check'),('cancelAfter','cancel'),('checkAfter','check')] if scenario=='cancel-boundaries-channel' else [('failedCheck','check'),('retry','retry'),('resume','resume')] if failure else []:d[k]=click(c)
    for k,s in [('replacementReady','replaced'),('awayReady','away'),('returnReady','returned')]:
        if s in by:d[k]={**obs(s),'ready':True}
    if nativecase:
        d.update(selectionChange=obs('selection-changed'),compositionStart=obs('composition-active'),compositionEnd=obs('composition-refused'),expiredWritable=obs('expiry-restored'),expiryElapsedMs=6201,sameTextInput=dict(observed=obs('aba-input'),flushError='ReviewRequired'))
        v=clone(by['aba-input']['composer']);text=INITIAL+' temporary';v['native'].update(text=text,display=text,codePoints=[ord(c) for c in text],utf16Length=len(text),start=0,end=len(text));d['abaChangedInput']=dict(observed=T['observed'](v),flushError='ReviewRequired')
    if nativecase:d['compositionResultWritable']['native']['composing']=True
    def before(observation,original):observation['before']={k:original[k] for k in ('text','start','end','direction')}
    if 'typed' in d:before(d['typed']['observed'],by['registering' if scenario=='late-input-channel' else 'recovered' if scenario=='cancel-boundaries-channel' else 'installed']['native']['native'])
    if failure:before(d['typedAfterRetry']['observed'],by['resumed']['native']['native'])
    if nativecase:
        before(d['abaChangedInput']['observed'],by['aba-prepared']['native']['native']);before(d['sameTextInput']['observed'],d['abaChangedInput']['observed']['native'])
        for name,stage in [('selectionChange','selection-prepared'),('compositionStart','composition-prepared'),('compositionEnd','composition-active')]:before(d[name],by[stage]['native']['native'])
    value=dict(ok=True,fixture=fixture,scenario=scenario,phase='exercise',nonce=NONCE,pid=123,isolatedRoot=str(STORAGE),target=target,viewport=C['expected_viewport'](fixture),captures={n:n+'.png' for n in C['capture_names'](scenario)},snapshots=records,details=d,finalInventory=clone(final),finalProjection=clone(records[-1]['projection']),evidenceScope='isolated protected enrollment/native route adoption; no clear or network capability')
    def add_text_proofs(x):
        if isinstance(x,dict):
            if {'text','label','rect','disabled','hit','visible'}<=x.keys():
                r=x['rect'];x['textGeometry']=dict(clientWidth=r['width'],clientHeight=r['height'],scrollWidth=r['width'],scrollHeight=r['height'],rects=[dict(x=r['x']+6,y=r['y']+5,width=r['width']-12,height=r['height']-10)] if x['text'] else [])
            for y in list(x.values()):add_text_proofs(y)
        elif isinstance(x,list):
            for y in x:add_text_proofs(y)
    add_text_proofs(value)
    return value

def validate(v):return C['validate'](v['fixture'],v['scenario'],'exercise',NONCE,123,STORAGE,v)
