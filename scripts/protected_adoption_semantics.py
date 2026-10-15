"""Exact state transitions for seven bounded local adoption contracts."""
import copy
import runpy
from pathlib import Path


def review(v):return {k:copy.deepcopy(v[k]) for k in ('row','local','blocked','alternates','origins','uncommitted')}


def rid(value):require(BASE['retention_id'](value),'Missing canonical retained process identity')


def field_scope(p,t):
    thread='Some("'+THREAD+'")' if t=='thread' else 'None'
    dest='None' if t=='thread' and p['thread'] is None else 'Some(DraftKey { team: "T1", channel: "C2", thread: '+thread+' })'
    return str(p['viewId'])+':'+str(p['epoch'])+':Some("'+p['account']+'"):'+dest+':'+str(p[t+'Generation'])


def phase_facts(scenario,stage):
    names=stages(scenario);i=names.index(stage)
    after=lambda s:s in names and i>=names.index(s)
    failed=scenario=='failed-managed-write-channel';native_case=scenario=='native-refusals-channel'
    dirty=scenario=='late-input-channel' and after('interleaved') or native_case and after('aba-input')
    installed=after('installed') or scenario=='cancel-boundaries-channel' and after('after-installed')
    writes=2 if failed and stage=='newer' else 1 if stage=='newer' or failed and after('retried') else 0
    error=failed and stage in ('failed','away','returned')
    routed=2 if native_case and dirty or failed and stage=='newer' else 1 if dirty or error or writes else 0
    text=FINAL if failed and stage=='newer' else NEWER if scenario=='late-input-channel' and dirty or failed and after('failed') or writes else INITIAL
    busy=stage in ('prepared','registering','interleaved','replaced','before-prepared','after-installed','selection-prepared','selection-changed','composition-prepared','composition-active','expiry-locked','expiry-restored','aba-prepared','aba-input')
    return dict(after=after,dirty=dirty,installed=installed,writes=writes,error=error,routed=routed,text=text,busy=busy)


def message(scenario,stage):
    if stage=='initial':return 'Review saved drafts, then continue editing with recovery.'
    if stage=='enrollment':return 'Enrollment finished. Reload saved drafts to see the current result.'
    if stage in ('reloaded','prepared','registering','interleaved','replaced','before-prepared','selection-prepared','selection-changed'):return 'Saved drafts reloaded.'
    if stage=='installed':return 'Recovery is ready for new input. Your draft and selection were kept.'
    if stage=='newer':return 'Editing resumed. Your draft and selection were kept.' if scenario=='failed-managed-write-channel' else 'Recovery is installed. Your draft was kept.' if scenario=='cancel-boundaries-channel' else 'Recovery is ready for new input. Your draft and selection were kept.'
    if stage in ('canceled-before',):return 'Stopped waiting. Check status to recover the earlier registration.'
    if stage in ('registered','after-installed'):return 'The earlier registration is retained. Continue editing only after a fresh review.'
    if stage=='canceled-after':return 'Recovery was already installed. Check status to see the retained result.'
    if stage=='recovered':return 'The earlier registration is retained. Continue editing only after a fresh review.' if scenario=='owner-replacement-channel' else 'Recovery is installed. Your draft was kept.'
    if stage in ('failed','away','returned'):return 'Recovery is installed. Your draft was kept.'
    if stage=='retried':return 'Your retained input was saved. Resume editing to restore its route.'
    if stage=='resumed':return 'Editing resumed. Your draft and selection were kept.'
    return 'The draft or its location changed. Check status before trying again.'


def displayed(inventory,status):
    return dict(status=status,entries={t:dict(kind='managed' if v['row'] else 'local',inventory=review(v) if v['row'] else v,uncommitted=None) if status=='Ready' else None for t,v in inventory.items()},pendingEnrollment=False)


def validate(fixture,scenario,phase,nonce,pid,storage,receipt,*,helpers):
    globals().update(helpers);helpers={**helpers,'review':review,'rid':rid}
    root=Path(__file__).parent
    storage_api=runpy.run_path(str(root/'protected_adoption_storage.py'))
    G=runpy.run_path(str(root/'protected_adoption_geometry.py'));G['install'](helpers)
    E=runpy.run_path(str(root/'protected_adoption_events.py'))
    keys(receipt,('ok','fixture','scenario','phase','nonce','pid','isolatedRoot','target','viewport','captures','snapshots','details','finalInventory','finalProjection','evidenceScope'),'terminal')
    require(fixture in FIXTURES and scenario in SCENARIOS and phase in phases(scenario),'Invalid fixture identity')
    target='thread' if scenario=='adopt-thread' else 'channel'
    for k,v in dict(ok=True,fixture=fixture,scenario=scenario,phase=phase,nonce=nonce,pid=pid,isolatedRoot=str(storage),target=target,viewport=expected_viewport(fixture),evidenceScope='isolated protected enrollment/native route adoption; no clear or network capability').items():same(receipt[k],v,'Terminal '+k)
    records=receipt['snapshots'];require(isinstance(records,list),'Missing ordered observations')
    same([r.get('stage') for r in records],list(stages(scenario,phase)),'Wrong capture-stage order')
    keys(receipt['captures'],capture_names(scenario,phase),'capture map')
    snapshots={r['stage']:r for r in records};initial=records[0]['durable'];p0=records[0]['projection'];d=receipt['details']
    keys(initial,('channel','thread'),'exact initial scopes')
    expected_final=storage_api['expected_inventory'](receipt,helpers)
    enrolled=copy.deepcopy(expected_final)
    for t in ('channel','thread'):
        empty=dict(scope=scope(t),clock=4,row=None,local=local(INITIAL,t=='thread'),blocked=None,alternates=[],origins=[],uncommitted=None)
        same(initial[t],empty,'Wrong exact local Host seed')
        enrolled[t]=copy.deepcopy(empty)
    row=copy.deepcopy(expected_final[target]['row']);row['revision']=1;row['state']['local_revision']=1;row['state']['desired']=encoded(INITIAL,target,target=='thread')
    enrolled[target].update(clock=5,row=row)
    generation=snapshots['dialog']['pages'][0]['generation'];rid(generation)
    same(d['enrollmentResult'],dict(kind='enrolled',current=True,account=ACCOUNT,expected=initial[target],review=review(enrolled[target])),'Real enrollment receipt')
    action_evidence=E['validate'](receipt,helpers)
    installed=action_evidence['installed'];adoption_id=action_evidence['adoptionId']
    registrations=[a for a in receipt['finalProjection']['fixtureActions'] if a['kind']=='registered']
    owner=next((r['projection']['retained'][0] for r in records if r['projection']['retained']),None)
    if owner:
        rid(owner['retentionId']);require(uuid4(owner['writerId']),'Invalid issued Writer UUID');same(owner['scope'],scope(target),'Editor scope');same(owner['enrollment'],row['lineage']['enrollment'],'Editor enrollment')
        if installed:same(installed['writerId'],owner['writerId'],'Installed receipt differs from retained issued Writer')
    local_ids={v['scope']['key']['thread'] and 'thread' or 'channel':v['id'] for v in p0['localLanes']}
    keys(local_ids,('channel','thread'),'initial local lanes');require(local_ids['channel']!=local_ids['thread'],'Scopes shared a local lane')
    for v in local_ids.values():rid(v)
    for r in records:
        keys(r,('stage','pages','native','composer','durable','projection'),'stage')
        stage=r['stage'];f=phase_facts(scenario,stage);p=r['projection']
        expected=copy.deepcopy(initial if stage in ('initial','dialog') else enrolled)
        if f['dirty']:expected[target]=copy.deepcopy(expected_final[target])
        elif f['writes']:
            expected[target]=BASE['changed'](enrolled[target],NEWER,target=='thread')
            if f['writes']==2:expected[target]=BASE['changed'](expected[target],FINAL,False)
        same(r['durable'],expected,'Whole Current inventory at '+stage)
        validate_projection(p,p0,initial,enrolled,expected,target,scenario,stage,f,owner,local_ids,adoption_id,installed,registrations)
        controls={}
        if p['retainedAdoptions'] or f['installed']:controls['check']=f['busy']
        controls['reload']=f['busy']
        shown=p['discovery']['entries'][target];has_enrollment=shown is not None and shown['kind']=='managed'
        if not has_enrollment and not f['installed']:controls['open']=f['busy']
        if has_enrollment and not f['installed']:controls['adopt']=f['busy']
        if f['error']:controls['retry']=f['busy']
        if f['installed'] and stage!='away' and not p['routes'][target]['registry_current']:controls['resume']=f['busy'] or f['error']
        if f['busy']:controls['cancel']=False
        G['record'](r,fixture,scenario,target,generation,f['text'],controls,message(scenario,stage))
        if stage=='expiry-locked':
            held=p['nativeAdmission'][-1]
            same(held['kind'],'lock','Locked observation has no actual native lock')
            same(r['native']['admission'],held['evidence']['token'],'Observed a different native admission')
            same(r['composer']['admission'],held['evidence']['token'],'Geometry measured a different native admission')
        same(r['native']['native']['scope'],p['fieldScopes'][target],'Native scope not actual Rust scope')
        same(r['native']['before'],{k:r['native']['native'][k] for k in ('text','start','end','direction')},'Observation altered native field')
    same(receipt['finalInventory'],records[-1]['durable'],'Final durable snapshot changed')
    same(receipt['finalProjection'],records[-1]['projection'],'Final renderer snapshot changed')
    validate_details(receipt,helpers,G,action_evidence)


def validate_projection(p,p0,initial,enrolled,expected,target,scenario,stage,f,owner,local_ids,adoption_id,installed,registrations):
    keys(p,('account','epoch','viewId','team','channel','thread','channelGeneration','threadGeneration','fieldScopes','sessionPresent','transportPresent','cachePresent','isolated','book','channelComposer','threadComposer','discovery','registry','selection','routes','localLanes','retainedAdoptions','retained','registered','routed','refused','fixtureActions','nativeAdmission','result','gate'),'whole projection')
    away=stage=='away';replaced=scenario=='owner-replacement-channel' and f['after']('replaced') or scenario=='failed-managed-write-channel' and f['after']('away')
    for k,v in dict(account=OTHER if away else ACCOUNT,team='T1',channel='C2',thread=THREAD if target=='thread' else None,sessionPresent=False,transportPresent=False,cachePresent=False,isolated=not away,routed=f['routed'],refused=0).items():same(p[k],v,'Projection '+k)
    for k in ('epoch','viewId','channelGeneration','threadGeneration'):require(type(p[k]) is int and p[k]>=0,'Invalid generation')
    same(p['epoch'],p0['epoch']+(1 if away else 2 if scenario=='failed-managed-write-channel' and replaced else 0),'Owner epoch')
    if replaced:require(p['viewId']!=p0['viewId'],'Replacement reused view')
    else:
        for k in ('epoch','viewId','channelGeneration','threadGeneration'):same(p[k],p0[k],'Unexpected model navigation')
    same(p['fieldScopes'],{t:field_scope(p,t) for t in ('channel','thread')},'Field scope provenance')
    keys(p['selection'],('id','account','epoch','viewId'),'Host selection');rid(p['selection']['id'])
    if away:same(p['selection'],p0['selection'],'Away fabricated selected authority');same(p['registry'],None,'Away Registry retained authority')
    else:
        same({k:p['selection'][k] for k in ('account','epoch','viewId')},{k:p[k] for k in ('account','epoch','viewId')},'Host selection differs')
        rid(p['registry'])
        if replaced:require(p['selection']['id']!=p0['selection']['id'] and p['registry']!=p0['registry'],'Replaced selection/Registry revived')
        else:same(p['selection'],p0['selection'],'Selection changed');same(p['registry'],p0['registry'],'Registry changed')
    book={t:local(f['text'] if t==target else INITIAL,t=='thread') for t in ('channel','thread')}
    same(p['book'],book,'DraftBook input/sibling changed');same(p['channelComposer'],book['channel']['composer'],'Channel model changed');same(p['threadComposer'],book['thread']['composer'] if target=='thread' else local('')['composer'],'Thread model changed')
    by_scope={v.get('scope',{}).get('key',{}).get('thread') and 'thread' or 'channel':v for v in p['localLanes']}
    require(len(by_scope)==len(p['localLanes']),'Duplicate local retention scopes')
    keys(by_scope,[t for t in ('channel','thread') if not(f['installed'] and t==target)],'Retained local lanes')
    for t,v in by_scope.items():
        dirty=f['dirty'] and t==target
        wanted=dict(id=local_ids[t],laneId=local_ids[t],scope=scope(t),serial=(3 if scenario=='native-refusals-channel' else 2) if dirty else 1,pending=local(f['text'],t=='thread') if dirty else None,failedInputs=[],running=False,error='ReviewRequired' if dirty else None,inventory=expected[t] if dirty else initial[t])
        same(v,wanted,'Original local lane lost or input changed')
    registered=bool([a for a in p['fixtureActions'] if a['kind']=='registered'])
    if registered:
        serial=1 if f['error'] else f['writes']
        wanted=dict(retentionId=owner['retentionId'],scope=scope(target),writerId=owner['writerId'],enrollment=owner['enrollment'],serial=serial,pending=local(NEWER) if f['error'] else None,failedInputs=[local(NEWER)] if f['error'] else [],running=False,parked=False,error='Storage' if f['error'] else None)
        same(p['retained'],[wanted],'Actual retained Editor identity/input/serial differs')
        same(p['registered'],dict(expected=review(enrolled[target]),editor=owner['retentionId'],laneId=local_ids[target],sampledCurrent=not(scenario=='owner-replacement-channel' and stage=='recovered')),'Stored registration changed')
    else:same(p['retained'],[],'Unrequested Editor');same(p['registered'],None,'Registration fabricated')
    dispatched=any(a['kind']=='registration-enqueued' for a in p['fixtureActions']);ack=any(a['kind']=='installation-acknowledged' for a in p['fixtureActions'])
    same(p['retainedAdoptions'],[adoption_id] if dispatched and not ack else [],'Registration slot replay/eviction')
    routes={}
    for t in ('channel','thread'):
        if away:routes[t]={'error':'Stale'}
        elif f['installed'] and t==target:
            rebound=next((a['receipt'] for a in p['fixtureActions'] if a['kind']=='rebound'),None)
            routes[t]=dict(kind='managed',identity=(rebound or installed)['id'],serial=1 if f['error'] else f['writes'],writer=owner['writerId'],enrollment=owner['enrollment'],registry_current=not(replaced and stage in ('returned','retried')))
        else:routes[t]=dict(kind='local',identity=local_ids[t],serial=(3 if scenario=='native-refusals-channel' else 2) if f['dirty'] and t==target else 1,writer=None,enrollment=None,registry_current=False)
    same(p['routes'],routes,'Route/Registry/serial does not match actual handoff')
    status='Inactive' if away else 'NeedsLoad' if stage=='enrollment' or scenario=='owner-replacement-channel' and replaced or stage=='returned' else 'Ready'
    shown=initial if stage in ('initial','dialog') else enrolled
    if scenario=='failed-managed-write-channel' and f['after']('retried'):
        shown=copy.deepcopy(enrolled);shown[target]=BASE['changed'](enrolled[target],NEWER,False)
    same(p['discovery'],displayed(shown,status),'Current discovery/presentation lifecycle changed')
    gate='registering' if stage in ('registering','interleaved') else 'locked' if stage in ('expiry-locked','expiry-restored') else 'installed' if stage=='after-installed' else 'prepared' if f['busy'] else None
    same(p['gate'],gate,'Wrong actual timing gate')


def validate_details(receipt,h,G,evidence):
    scenario=receipt['scenario'];target=receipt['target'];fixture=receipt['fixture'];d=receipt['details'];records={r['stage']:r for r in receipt['snapshots']}
    generation=records['dialog']['pages'][0]['generation']
    common={'nativeInitial','open','consent','enrollmentResult','enrollmentResultWritable','reload'}
    attempts={'adopt-channel':['prepared'],'adopt-thread':['prepared'],'late-input-channel':['registering'],'native-refusals-channel':['selection-prepared','composition-prepared','expiry-locked','aba-prepared'],'owner-replacement-channel':['prepared'],'cancel-boundaries-channel':['before-prepared','after-installed'],'failed-managed-write-channel':['prepared']}[scenario]
    for a in attempts:common.update((a+'Before',a+'Click'))
    extras={'adopt-channel':{'installResult','installResultWritable','oldLaneRefusal','typed'},'adopt-thread':{'installResult','installResultWritable','oldLaneRefusal','typed'},'late-input-channel':{'typed','refusal','refusalWritable'},'native-refusals-channel':{'selectionChange','selectionResult','selectionResultWritable','compositionStart','compositionEnd','compositionResult','compositionResultWritable','expiredWritable','expiryElapsedMs','expiryResult','expiryResultWritable','abaChangedInput','sameTextInput','abaResult','abaResultWritable'},'owner-replacement-channel':{'replacementReady','refusal','refusalWritable','check','recoveredResult','recoveredResultWritable'},'cancel-boundaries-channel':{'cancelBefore','canceledBefore','canceledBeforeWritable','checkBefore','registeredResult','registeredResultWritable','cancelAfter','canceledAfter','canceledAfterWritable','checkAfter','recoveredResult','recoveredResultWritable','oldLaneRefusal','typed'},'failed-managed-write-channel':{'installResult','installResultWritable','oldLaneRefusal','typed','failedCheck','failedStatus','failedStatusWritable','awayReady','returnReady','retry','retryResult','retryResultWritable','resume','resumeResult','resumeResultWritable','typedAfterRetry'}}[scenario]
    keys(d,common|extras,'case evidence')
    native(d['nativeInitial'],INITIAL,expected_viewport(fixture));BASE['SCHEDULE']['home_anchor'](d['nativeInitial'])
    for k in ('text','display','codePoints','utf16Length','scope','visit'):
        same(d['nativeInitial'][k],records['initial']['native']['native'][k],'Home mount differs from initial selected field')
    for name,control in [('open','open'),('consent','submit'),('reload','reload')]+[(a+'Click','adopt') for a in attempts]+[(n,c) for n,c in [('check','check'),('failedCheck','check'),('cancelBefore','cancel'),('cancelAfter','cancel'),('checkBefore','check'),('checkAfter','check'),('retry','retry'),('resume','resume')] if n in d]:
        G['click'](d[name],control,fixture,generation if control=='submit' else None)
    for a in attempts:
        G['observation'](d[a+'Before'],fixture,target,INITIAL)
        BASE['same_native'](d[a+'Before']['native'],records[a]['native']['native'])
    for name in ('typed','typedAfterRetry','abaChangedInput','sameTextInput'):
        if name not in d:continue
        v=d[name];keys(v,('observed','flushError'),'actual input')
        text=FINAL if name=='typedAfterRetry' else INITIAL+' temporary' if name=='abaChangedInput' else INITIAL if name=='sameTextInput' else NEWER
        G['observation'](v['observed'],fixture,target,text);same(v['observed']['fieldFocused'],True,'Input lost field focus')
        n=v['observed']['native'];same(n['start'],len(text[:text.index(chr(0x1f680))].encode('utf-16-le'))//2 if chr(0x1f680) in text else 0,'Input selection start');same(n['end'],len(text.encode('utf-16-le'))//2,'Input selection end');same(n['direction'],'backward','Input selection direction')
        error='ReviewRequired' if scenario in ('late-input-channel','native-refusals-channel') else 'Storage' if scenario=='failed-managed-write-channel' and name=='typed' else None
        same(v['flushError'],error,'Wrong typed worker result')
        stage='newer' if name=='typedAfterRetry' else 'aba-input' if name=='sameTextInput' else 'failed' if scenario=='failed-managed-write-channel' else 'interleaved' if scenario=='late-input-channel' else 'newer'
        if name!='abaChangedInput':BASE['same_native'](n,records[stage]['native']['native'])
        original=(d['abaChangedInput']['observed']['native'] if name=='sameTextInput' else records['aba-prepared']['native']['native'] if name=='abaChangedInput' else records['resumed']['native']['native'] if name=='typedAfterRetry' else records['registering' if scenario=='late-input-channel' else 'recovered' if scenario=='cancel-boundaries-channel' else 'installed']['native']['native'])
        same(v['observed']['before'],{k:original[k] for k in ('text','start','end','direction')},'Input did not start from the actual preceding field')
        for k in ('scope','visit'):same(n[k],original[k],'Input moved to a different native owner')
    for name,v in d.items():
        if name.endswith('Writable'):
            G['observation'](v,fixture,target,v['native']['text'],expected_composing=scenario=='native-refusals-channel' and name=='compositionResultWritable')
    for name in ('refusal','selectionResult','compositionResult','expiryResult','abaResult'):
        if name not in d:continue
        keys(d[name],('kind','attempt','error'),'install refusal');same(d[name]['kind'],'install-refused','Refusal installed');rid(d[name]['attempt']);same(d[name]['error'],'ReviewRequired' if name in ('selectionResult','abaResult') else 'Stale','Actual refusal reason')
    result_stages={'enrollmentResult':'enrollment','installResult':'installed','refusal':'refused','selectionResult':'selection-refused','compositionResult':'composition-refused','expiryResult':'expiry-refused','abaResult':'aba-refused','registeredResult':'registered','recoveredResult':'recovered','canceledBefore':'canceled-before','canceledAfter':'canceled-after','failedStatus':'failed','retryResult':'retried','resumeResult':'resumed'}
    for name,stage in result_stages.items():
        if name not in d:continue
        same(d[name],records[stage]['projection']['result'],'UI result and actual snapshot differ')
        if 'attempt' in d[name]:
            actual=[a['attempt'] for a in records[stage]['projection']['fixtureActions'] if a['kind']=='adopt-click']
            require(bool(actual),'Result without an actual install handler')
            same(d[name]['attempt'],actual[-1],'Result belongs to a different install attempt')
        BASE['same_native'](d[name+'Writable']['native'],records[stage]['native']['native'])
    for stage,r in records.items():
        if stage in ('initial','dialog'):same(r['projection']['result'],None,'Result before actual consent')
        elif stage=='reloaded':same(r['projection']['result'],d['enrollmentResult'],'Reload invented an outcome')
        elif phase_facts(scenario,stage)['busy']:same(r['projection']['result'],None,'Pending gate fabricated completed result')
        elif stage=='newer':same(r['projection']['result'],d['resumeResult' if scenario=='failed-managed-write-channel' else 'recoveredResult' if scenario=='cancel-boundaries-channel' else 'installResult'],'Input invented another installation result')
        elif stage in ('away','returned'):same(r['projection']['result'],d['failedStatus'],'Owner navigation rewrote retained result')
    if 'installResult' in d:
        v=d['installResult'];same(v,dict(kind='installed',attempt=v.get('attempt'),alreadyInstalled=False,receipt=evidence['installed']),'Actual install result')
        same(v['attempt'],next(a['attempt'] for a in receipt['finalProjection']['fixtureActions'] if a['kind']=='installed'),'Result attempt')
    if 'canceledBefore' in d:same(d['canceledBefore'],dict(kind='canceled',installed=None),'Precommit cancellation installed')
    if 'canceledAfter' in d:same(d['canceledAfter'],dict(kind='canceled',installed=evidence['installed']),'Postcommit cancellation rolled back')
    if 'registeredResult' in d or scenario=='owner-replacement-channel':
        name='registeredResult' if 'registeredResult' in d else 'recoveredResult';v=d[name]
        reg=records['before-prepared' if name=='registeredResult' else 'prepared']['projection']['registered']
        same(v,dict(kind='registered',adoptionId=evidence['adoptionId'],current=scenario!='owner-replacement-channel',editor=reg['editor'],laneId=reg['laneId'],expected=reg['expected'],statusOnly=True),'Recovered registration identity/current')
    if 'recoveredResult' in d and scenario=='cancel-boundaries-channel':same(d['recoveredResult'],dict(kind='installed',receipt=evidence['installed'],statusOnly=True),'Recovered installation differs')
    if 'failedStatus' in d:same(d['failedStatus'],dict(kind='installed',receipt={**evidence['installed'],'serial':1},statusOnly=True),'Failed input status lost installed receipt')
    if 'retryResult' in d:same(d['retryResult'],next(a for a in receipt['finalProjection']['fixtureActions'] if a['kind']=='input-retried'),'Retry result not actual immutable command')
    if 'resumeResult' in d:same(d['resumeResult'],next(a for a in receipt['finalProjection']['fixtureActions'] if a['kind']=='rebound'),'Rebind result not actual retained receipt')
    if 'oldLaneRefusal' in d:
        v=d['oldLaneRefusal'];same(v,dict(laneId=evidence['installed']['laneId'],beforeSerial=1,afterSerial=1,result='Stale'),'Old cloned lane accepted input')
    if scenario=='native-refusals-channel':
        require(number(d['expiryElapsedMs']) and d['expiryElapsedMs']>=6200,'Lease expiry was not elapsed')
        for name in ('selectionChange','compositionStart','compositionEnd'):
            G['observation'](d[name],fixture,target,INITIAL,expected_composing=name=='compositionStart')
        for name,stage,prior in [('selectionChange','selection-changed','selection-prepared'),('compositionStart','composition-active','composition-prepared'),('compositionEnd','composition-refused','composition-active')]:
            BASE['same_native'](d[name]['native'],records[stage]['native']['native'])
            same(d[name]['before'],{k:records[prior]['native']['native'][k] for k in ('text','start','end','direction')},'Native interaction started from a foreign field')
        BASE['same_native'](d['expiredWritable']['native'],records['expiry-restored']['native']['native'])
        same(d['selectionChange']['native']['direction'],'forward','Direction refusal not exercised')
        same([v['input'] for v in evidence['captures']],[0,0,0,0,0],'Unexpected pre-ABA native input')
        same([v['composition'] for v in evidence['captures']],[0,0,0,2,2],'Composition counters not exercised')
        require(len(evidence['captures'])==5,'Original capture reused/rearmed')
    # Every observed install keeps the original native bytes/selection. Rebind
    # compares its returned mount, never an unrelated earlier visit.
    if 'installed' in records:BASE['same_native'](records['prepared']['native']['native'],records['installed']['native']['native'])
    if 'resumed' in records:BASE['same_native'](records['returned']['native']['native'],records['resumed']['native']['native'])
    for name,stage in [('replacementReady','replaced'),('awayReady','away'),('returnReady','returned')]:
        if name in d:
            v=d[name];same(v.get('ready'),True,'Replacement not ready');copy_v={k:x for k,x in v.items() if k!='ready'}
            G['observation'](copy_v,fixture,target,records[stage]['native']['native']['text']);BASE['same_native'](v['native'],records[stage]['native']['native'])
            require(v['native']['visit']!=records['initial']['native']['native']['visit'],'Actual mount reused')
    if 'returnReady' in d:
        require(d['returnReady']['native']['visit']!=d['awayReady']['native']['visit'],'Return reused away mount')
        require(records['returned']['projection']['viewId']!=records['away']['projection']['viewId'],'Return reused away ShellState')
