"""Exact fixture-handler ordering and original native capture/lease accounting."""


def schedule(scenario):
    events=[];boundaries={}
    def add(*names):events.extend(names)
    def mark(stage):boundaries[stage]=list(events)
    add('current-load');mark('initial')
    add('open-click','current-load','dialog');mark('dialog')
    add('consent-click','prepared','enqueue');mark('enrollment')
    add('reload-click','current-load');mark('reloaded')
    fresh=True
    def start(stage,gate):
        nonlocal fresh
        add('adopt-click','current-load','install-captured')
        if fresh:add('registration-enqueued');fresh=False
        if gate=='registering':add('registration-reserved','gate:registering');mark(stage);return
        add('registered','install-prepared')
        if gate=='prepared':add('gate:prepared');mark(stage);return
        add('install-locked')
        if gate=='locked':add('gate:locked');mark(stage);return
        add('installed','gate:installed');mark(stage)
    if scenario=='late-input-channel':
        start('registering','registering');add('input');mark('interleaved');add('gate-released:registering');mark('refused')
    elif scenario=='native-refusals-channel':
        for name in ('selection','composition'):
            start(name+'-prepared','prepared');mark(name+('-changed' if name=='selection' else '-active'))
            add('gate-released:prepared');mark(name+'-refused')
        start('expiry-locked','locked');mark('expiry-restored');add('gate-released:locked');mark('expiry-refused')
        start('aba-prepared','prepared');add('input','input');mark('aba-input');add('gate-released:prepared');mark('aba-refused')
    elif scenario=='owner-replacement-channel':
        start('prepared','prepared');add('shell-replaced');mark('replaced');add('gate-released:prepared');mark('refused')
        add('adoption-check-click');mark('recovered')
    elif scenario=='cancel-boundaries-channel':
        start('before-prepared','prepared');add('gate-canceled:prepared','adoption-cancel-click');mark('canceled-before')
        add('adoption-check-click');mark('registered');start('after-installed','installed')
        add('gate-canceled:installed','adoption-cancel-click');mark('canceled-after')
        add('adoption-check-click','installation-recovered','installation-acknowledged');mark('recovered')
        add('input');mark('newer')
    else:
        start('prepared','prepared');add('gate-released:prepared','install-locked','installed','installation-acknowledged');mark('installed')
        add('input')
        if scenario=='failed-managed-write-channel':
            add('adoption-check-click','installation-recovered');mark('failed')
            add('shell-replaced');mark('away');add('shell-replaced');mark('returned')
            add('retry-input-click','input-retried','current-load');mark('retried')
            add('resume-click','current-load','rebind-prepared','rebound');mark('resumed');add('input')
        mark('newer')
    return boundaries


def validate(receipt,h):
    same,require,keys=(h[k] for k in ('same','require','keys'))
    records=receipt['snapshots'];scenario=receipt['scenario'];target=receipt['target'];details=receipt['details']
    expected=schedule(scenario);final=receipt['finalProjection']['fixtureActions']
    def token(e):return e['kind']+(':'+e['phase'] if e['kind'].startswith('gate') else '')
    previous=[];native_previous=[];previous_projection=None
    for r in records:
        actions=r['projection']['fixtureActions'];trace=r['projection']['nativeAdmission']
        require(isinstance(actions,list) and len(actions)<=256,'Unbounded fixture ledger')
        same([token(v) for v in actions],expected[r['stage']],'Handler sequence differs at '+r['stage'])
        same(actions[:len(previous)],previous,'Fixture actions changed retrospectively')
        for event in actions[len(previous):]:
            p=r['projection']
            if event['kind']=='current-load':same({k:event[k] for k in ('epoch','viewId')},{k:p[k] for k in ('epoch','viewId')},'Current load used an earlier owner')
            if event['kind']=='shell-replaced':
                old=previous_projection;require(old is not None,'Replacement has no observed predecessor')
                same(event['before'],dict(account=old['account'],epoch=old['epoch'],viewId=old['viewId'],channelDraft=old['book']['channel'],threadDraft=old['book']['thread'],channel=old['channelComposer'],thread=old['threadComposer']),'Replacement changed the retained source draft')
                same({k:event[k] for k in ('afterEpoch','afterViewId','afterAccount')},dict(afterEpoch=p['epoch'],afterViewId=p['viewId'],afterAccount=p['account']),'Replacement result is not the actual current owner')
        same(trace[:len(native_previous)],native_previous,'Native evidence changed retrospectively')
        count=lambda kind:sum(a['kind']==kind for a in actions)
        native_count=lambda kind:sum(a['kind']==kind for a in trace)
        same(native_count('capture'),count('dialog')+count('install-captured')+count('rebind-prepared'),'Capture outside actual handler at '+r['stage'])
        locks=count('enqueue')+count('install-locked')+count('rebound')
        same(native_count('lock'),locks,'Lock outside actual handler at '+r['stage'])
        same(native_count('release-requested'),locks-int(r['stage'] in ('expiry-locked','expiry-restored')),'Release-request lifecycle at '+r['stage'])
        previous=actions;native_previous=trace;previous_projection=r['projection']
    same(final,previous,'Final fixture ledger differs');same(receipt['finalProjection']['nativeAdmission'],native_previous,'Final native trace differs')
    named=lambda kind:[e for e in final if e['kind']==kind]
    enrollment=records[2]['durable'][target];seed=records[0]['durable'][target]
    review=h['review'](enrollment);generation=records[1]['pages'][0]['generation']
    attempts={};adoption_id=None;installed=None;prepared={};registry=records[0]['projection']['registry'];input_index=0;registration=None;rebind_prepared=None;active_attempt=None
    def installed_receipt(v,expected,serial,predecessor,attempt_id):
        keys(v,('id','adoptionId','installAttempt','expected','installedEditorSerial','reboundFrom','scope','laneId','laneSerial','editor','writerId','enrollment','serial'),'sealed installation receipt')
        h['rid'](v['id']);require(h['uuid4'](v['writerId']),'Installation writer is not issued UUID')
        for k,x in dict(adoptionId=adoption_id,installAttempt=attempt_id,expected=expected,installedEditorSerial=serial,reboundFrom=predecessor,scope=h['scope'](target),laneId=registration['laneId'],laneSerial=1,editor=registration['editor'],enrollment=review['row']['lineage']['enrollment'],serial=serial).items():same(v[k],x,'Installation '+k)
        if predecessor:
            require(v['id']!=predecessor,'Rebind reused old installation identity')
            same(v['writerId'],installed['writerId'],'Rebind changed issued writer')
    for e in final:
        kind=e['kind']
        if kind!='adopt-click' and 'attempt' in e:same(e['attempt'],active_attempt,'Event belongs to an earlier install attempt')
        if kind in ('open-click','reload-click','adoption-check-click','retry-input-click','resume-click'):keys(e,('kind',),'handler event')
        elif kind=='current-load':
            keys(e,('kind','epoch','viewId'),'load');require(type(e['epoch']) is int and type(e['viewId']) is int,'Missing load owner')
            require(any(e['epoch']==r['projection']['epoch'] and e['viewId']==r['projection']['viewId'] for r in records),'Unobserved load owner')
        elif kind=='dialog':
            keys(e,('kind','id','displayed','pending'),'dialog');same(e['id'],generation,'Dialog generation');same(e['pending'],[],'Unexpected pending enrollment input');same(e['displayed'],records[1]['projection']['discovery'],'Displayed inventory changed')
        elif kind=='consent-click':same(e,dict(kind=kind,id=generation),'Consent identity')
        elif kind in ('prepared','enqueue'):same(e,dict(kind=kind,expected=seed),'Original whole enrollment inventory')
        elif kind=='adopt-click':
            keys(e,('kind','attempt'),'adopt click');h['rid'](e['attempt']);require(e['attempt'] not in attempts,'Install attempt reused');attempts[e['attempt']]=[]
            active_attempt=e['attempt']
        elif kind=='install-captured':
            keys(e,('kind','attempt','expected','registry'),'capture binding');require(e['attempt'] in attempts,'Unowned capture');same(e['expected'],review,'Capture rebased immutable Review');same(e['registry'],registry,'Original Registry changed')
        elif kind=='registration-enqueued':
            keys(e,('kind','attempt','adoptionId','dispatchHeldByOwnerGate'),'registration');require(adoption_id is None,'Registration replay');h['rid'](e['adoptionId']);adoption_id=e['adoptionId'];same(e['dispatchHeldByOwnerGate'],scenario=='late-input-channel','Wrong registration delay claim')
            require(e['attempt'] in attempts,'Registration without actual install click')
        elif kind=='registered':
            keys(e,('kind','attempt','adoptionId','current','editor','laneId','expected'),'registered');same(e['adoptionId'],adoption_id,'Different registration');same(e['current'],True,'Registration was already stale');same(e['expected'],review,'Registered original Review');h['rid'](e['editor']);same(e['laneId'],records[0]['projection']['routes'][target]['identity'],'Wrong original lane')
            require(e['attempt'] in attempts,'Registered unrequested attempt')
            if registration:same({k:e[k] for k in ('editor','laneId','expected')},{k:registration[k] for k in ('editor','laneId','expected')},'Retry replaced original registration')
            registration=e
        elif kind=='registration-reserved':
            keys(e,('kind','adoptionId','retentionId'),'real pending registration reservation')
            same(e['adoptionId'],adoption_id,'Reservation belongs to another adoption')
            h['rid'](e['retentionId'])
            require(scenario=='late-input-channel','Unexpected reserved-slot timing observation')
        elif kind=='install-prepared':
            keys(e,('kind','attempt','installAttempt','adoptionId'),'prepared');h['rid'](e['installAttempt']);require(e['installAttempt'] not in prepared.values(),'Install visit reused');same(e['adoptionId'],adoption_id,'Prepared other adoption');prepared[e['attempt']]=e['installAttempt']
        elif kind=='install-locked':same(e,dict(kind=kind,attempt=e['attempt'],installAttempt=prepared[e['attempt']],adoptionId=adoption_id),'Original prepared attempt changed')
        elif kind.startswith('gate'):keys(e,('kind','phase'),'timing gate')
        elif kind=='installed':
            keys(e,('kind','attempt','receipt'),'installed');require(installed is None,'Installation replay');installed=e['receipt'];same(installed['installAttempt'],prepared[e['attempt']],'Installed unrelated native attempt');same(installed['adoptionId'],adoption_id,'Installed other registration');same(installed['expected'],review,'Installed Review rebased');same(installed['reboundFrom'],None,'Initial receipt invented rebind')
            installed_receipt(installed,review,0,None,prepared[e['attempt']])
        elif kind=='installation-acknowledged':same(e,dict(kind=kind,receiptId=installed['id']),'Acknowledged different installation')
        elif kind=='installation-recovered':same(e,dict(kind=kind,receipt={**installed,'serial':1 if scenario=='failed-managed-write-channel' else 0}),'Recovered a different completed installation')
        elif kind=='adoption-cancel-click':
            keys(e,('kind','installed'),'cancel');same(e['installed'],installed,'Cancellation changed completed state')
        elif kind=='input':
            keys(e,('kind','key','serial','local'),'input');same(e['key'],h['scope'](target)['key'],'Input escaped target');require(type(e['serial']) is int and e['serial'] in (1,2,3),'Wrong input serial')
            input_index+=1
            preserved=scenario in ('late-input-channel','native-refusals-channel')
            text=(h['INITIAL']+' temporary' if input_index==1 else h['INITIAL']) if scenario=='native-refusals-channel' else h['FINAL'] if input_index==2 else h['NEWER']
            same(e['serial'],input_index+int(preserved),'Actual input serial progression')
            same(e['local'],h['local'](text,target=='thread'),'Actual routed input differs')
        elif kind=='shell-replaced':
            keys(e,('kind','mode','before','afterEpoch','afterViewId','afterAccount'),'replacement');keys(e['before'],('account','epoch','viewId','channelDraft','threadDraft','channel','thread'),'replacement source')
            require(e['mode'] in ('same-epoch','away','return'),'Unknown replacement');require(e['afterViewId']!=e['before']['viewId'],'Shell view revived');same(e['afterEpoch'],e['before']['epoch']+(0 if e['mode']=='same-epoch' else 1),'Wrong replacement epoch');same(e['afterAccount'],h['OTHER'] if e['mode']=='away' else h['ACCOUNT'],'Wrong replacement account')
        elif kind=='input-retried':
            keys(e,('kind','retentionId','receiptId','serialBefore','serialAfter','review','pending','failedInputs'),'exact retry');same(e['receiptId'],installed['id'],'Retry replaced installation');same(e['retentionId'],installed['editor'],'Retry replaced Editor');same(e['serialBefore'],1,'Retry original serial');same(e['serialAfter'],1,'Retry replayed serial');same(e['pending'],None,'Retry still pending');same(e['failedInputs'],[],'Retry lost command');same(e['review'],h['review'](next(r for r in records if r['stage']=='retried')['durable'][target]),'Retry result full Review')
        elif kind=='rebind-prepared':
            keys(e,('kind','installAttempt','priorReceipt','editor'),'rebind preparation');h['rid'](e['installAttempt']);same(e['priorReceipt'],installed['id'],'Rebind original receipt');same(e['editor'],installed['editor'],'Rebind replaced Editor')
            require(e['installAttempt'] not in prepared.values(),'Rebind reused prior native visit');rebind_prepared=e['installAttempt']
        elif kind=='rebound':
            keys(e,('kind','priorReceipt','receipt'),'rebound');same(e['priorReceipt'],installed['id'],'Rebind predecessor');same(e['receipt']['reboundFrom'],installed['id'],'Missing rebind link');same(e['receipt']['editor'],installed['editor'],'Rebind minted Editor');same(e['receipt']['writerId'],installed['writerId'],'Rebind minted Writer')
            installed_receipt(e['receipt'],h['review'](next(r for r in records if r['stage']=='retried')['durable'][target]),1,installed['id'],rebind_prepared)
        else:raise RuntimeError('Unexpected fixture work: '+kind)
    # Full native trace contains only actual capture/lock/release requests; no clear.
    captures=[];active=None;tokens=set()
    for e in native_previous:
        keys(e,('kind','evidence'),'native event');v=e['evidence']
        if e['kind']=='capture':
            require(active is None,'Capture replaced active native lease');keys(v,('before',),'captured snapshot');captures.append(v['before'])
            s=v['before'];keys(s,('id','scope','visit','input','composition','composing','wire','start','end','direction','display_start','display_end','display_len'),'native snapshot')
            same(s['id'],target+'-composer','Native target');same(s['composing'],False,'Capture during composition');require(type(s['input']) is int and s['input']>=0 and type(s['composition']) is int and s['composition']>=0,'Invalid native counters')
            same(s['display_start'],s['start'],'Native display caret');same(s['display_end'],s['end'],'Native display caret');same(s['display_len'],len(s['wire'].encode('utf-16-le'))//2,'Native wire length')
        elif e['kind']=='lock':
            keys(v,('token','before'),'real native lock');require(active is None and captures,'Unowned native lock');same(v['before'],captures[-1],'Native lock recaptured original evidence');require(isinstance(v['token'],str) and v['token'].isdigit() and int(v['token'])>0 and v['token'] not in tokens,'Native token reused');tokens.add(v['token']);active=v['token']
        elif e['kind']=='release-requested':same(v,dict(token=active),'Unowned native release');require(active is not None,'Release without lease');active=None
        else:raise RuntimeError('Unexpected clear/native event '+e['kind'])
    same(active,None,'Native lease still outstanding')
    expected_captures=1+len(named('install-captured'))+len(named('rebind-prepared'))
    same(len(captures),expected_captures,'Extra or missing original capture')
    expected_locks=1+len(named('install-locked'))+len(named('rebound'))
    same(len(tokens),expected_locks,'Extra or missing actual lock')
    attempt_stages=[r['stage'] for r in records if r['stage']+'Before' in details]
    sources=[records[1]['native']['native']]+[details[s+'Before']['native'] for s in attempt_stages]
    if named('rebound'):sources.append(next(r for r in records if r['stage']=='returned')['native']['native'])
    same(len(sources),len(captures),'Capture source coverage')
    for captured,source in zip(captures,sources):
        for key,source_key in [('scope','scope'),('visit','visit'),('wire','text'),('start','start'),('end','end'),('direction','direction')]:same(captured[key],source[source_key],'Captured original native '+key)
    return dict(adoptionId=adoption_id,installed=installed,captures=captures)
