"""Strict evidence for the named, disconnected retained Owned Send cohort."""
import copy
from collections import Counter
import json
from pathlib import Path
import re
import runpy
from urllib.parse import parse_qsl, urlsplit, urlencode
import uuid

SCRIPTS = next(p / 'scripts' for p in Path(__file__).resolve().parents
               if (p / 'Cargo.toml').is_file() and (p / 'scripts/native_fixture.py').is_file())
BASE = runpy.run_path(str(SCRIPTS / 'protected_drafts_receipts.py'))
globals().update({k: BASE[k] for k in ('NativeFixture','DEFAULT_BINARY','ROOT','expected_viewport','sha','png',
    'validate_exit','normalize','validate_pointer_cleanup','closed_receipts','strict_equal','number','rectangle',
    'require','same','native','uuid4','keys','geometry','same_native','retention_id','local','encoded','review','SEND','SCHEDULE')})
STORAGE = runpy.run_path(str(SCRIPTS / 'protected_drafts_storage.py'))
unique_json = STORAGE['unique_json']
FIXTURES = ('protected-owned-send', 'protected-owned-send-narrow')
SCENARIOS = ('newer-before-prepare-channel', 'newer-before-prepare-thread')
FLAG = 'SUPER_PLATINUM_PROTECTED_OWNED_SEND_CHECK'
PREFIX, WAITING, STAGE = ('protected-owned-send-' + n + ': ' for n in ('check','waiting','stage'))
CONTRACT_READY = Path(__file__).resolve().parent == SCRIPTS
INITIAL = 'Protected owned draft'
REQUESTED = 'Frozen owned send \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301\nsecond line'
NEWER = 'Newer owned typing \u2014 \u65e5\u672c\u8a9e \U0001f680 e\u0301'
THREAD = '1719800400.000000'
ACK_TS = '1719800500.000001'
EVIDENCE_SCOPE = 'named isolated mounted Send; actual native clear and typed queue; synthetic socket Connected and HTTP response; no production selector, Source gap/drain, restart or broad C3 parity'


def require_contract():
    require(CONTRACT_READY, 'Owned Send preparation cannot launch before promotion into scripts')


def phases(scenario):
    require(scenario in SCENARIOS, 'Unknown Owned Send scenario')
    return ('exercise',)


def stages(scenario, phase='exercise'):
    require(phase in phases(scenario), 'Unknown Owned Send phase')
    return ('initial','input','cleared','newer','result')


capture_names = stages


def scope(account, target):
    return dict(account_id=account, key=dict(team='T1', channel='C2', thread=THREAD if target == 'thread' else None))


def integer(value, minimum=0):
    return type(value) is int and minimum <= value < 2**64


def seed_inventory(value, account, target):
    row = value.get('row', {})
    state, lineage = row.get('state', {}), row.get('lineage', {})
    require(uuid4(state.get('client_msg_id')) and uuid4(lineage.get('enrollment')), 'Missing actual seed identities')
    state = dict(scope=scope(account,target), client_msg_id=state['client_msg_id'], desired=encoded(INITIAL,target,target=='thread'),
                 local_revision=1, acknowledged_local_revision=0, acknowledged=None, workflow='Idle', tombstone=False,
                 retired_ids=[], consumption=None, check_deadline=None, check_ticket=None, observed_events={}, blocked_observation=None)
    # Empty enrollment creates clock 1; the first real Editor checkpoint adds
    # one helper touch and one canonical INSERT trigger. Verify changes row only.
    expected = dict(scope=scope(account,target), clock=3,
                    row=dict(scope=scope(account,target), revision=3, state=state,
                             lineage=dict(enrollment=lineage['enrollment'],verifiedEpoch=2,basis='Local')),
                    local=local(INITIAL,target=='thread'),blocked=None,alternates=[],origins=[],uncommitted=None)
    same(value, expected, 'Wrong complete initial Owned inventory')


def edited(before, text, broadcast):
    after = copy.deepcopy(before)
    after['clock'] += 2
    after['local'] = local(text,broadcast)
    after['row']['revision'] += 1
    state=after['row']['state']
    state['local_revision'] += 1
    state['desired']=encoded(text,'thread' if before['scope']['key']['thread'] else 'channel',broadcast)
    state['tombstone']=False
    return after


def consumed(before, actual):
    return BASE['consumed_inventory'](before,actual)


def recorded(newer, actual, operation):
    keys(actual, ('row','local','blocked','alternates','origins','uncommitted'), 'recorded full Review')
    identity=actual.get('row',{}).get('state',{}).get('client_msg_id')
    require(uuid4(identity) and identity != operation['client_msg_id'], 'Missing actual successor identity')
    expected=copy.deepcopy(newer)
    expected['row']['revision'] += 2  # actual Prepare then actual Record
    state=expected['row']['state']
    intent=state['consumption']
    state.update(client_msg_id=identity, consumption=None, acknowledged_local_revision=intent['local_revision']+1,
                 workflow='Idle',local_completion=dict(intent=intent,successor_client_msg_id=identity))
    expected['row']['lineage'].update(verifiedEpoch=0,basis='Unverified')
    same(actual,review(expected),'Recorded full inventory does not preserve newer input and frozen intent')
    return expected


def operation(value, intent):
    keys(value, ('id','scope','client_msg_id','local_revision','kind','snapshot','base','consume_id'), 'actual frozen operation')
    require(uuid4(value['id']) and value['id'] != intent['id'], 'Invalid actual operation identity')
    expected=dict(id=value['id'],scope=intent['scope'],client_msg_id=intent['client_msg_id'],local_revision=intent['local_revision'],
                  kind='Send',snapshot=intent['snapshot'],base=None,consume_id=intent['id'])
    same(value,expected,'Frozen Send differs from exact consumption')


def request(value, op, target):
    keys(value,('method','url','headers','fields','retrySafe','operation','rawResponse','delivered'),'prepared request')
    same(value['operation'],op,'Prepared request operation differs')
    for k,v in dict(method='POST',retrySafe=False,delivered=True).items():same(value[k],v,'Wrong request '+k)
    parsed=urlsplit(value['url'])
    same((parsed.scheme,parsed.netloc,parsed.path,parsed.fragment),('https','owned-send.slack.com','/api/chat.postMessage',''),'Wrong exact request endpoint')
    pairs=parse_qsl(parsed.query,keep_blank_values=True,strict_parsing=True)
    names=['_x_id','_x_csid','_x_desktop_ia','_x_foreground','_x_frontend_build_type','_x_gantry','_x_num_retries','_x_version_ts','fp','slack_route']
    same([p[0] for p in pairs],names,'Missing/duplicate/reordered query metadata')
    same(parsed.query,urlencode(pairs),'Noncanonical query encoding')
    expected=['','', 'true','true','current','true','0','capture-required','','default']
    for i,(_,v) in enumerate(pairs):
        if names[i] in ('_x_id','_x_csid','fp'):
            require(isinstance(v,str) and re.fullmatch('[0-9a-f]{12}4[0-9a-f]{3}[89ab][0-9a-f]{15}',v) is not None,'Invalid UUIDv4 request metadata')
        else:same(v,expected[i],'Changed stable request metadata')
    require(len({dict(pairs)[name] for name in ('_x_id','_x_csid','fp')})==3,'Request metadata identities alias')
    headers=[['sec-ch-ua-platform','"macOS"'],['Referer','https://app.slack.com/client/'],
        ['User-Agent','Mozilla/5.0 (Macintosh; Intel Mac OS X 26_6_0) AppleWebKit/537.36 (KHTML, like Gecko) Slack/4.50.143 Chrome/148.0.7778.265 Electron/42.4.1 Safari/537.36 AppleSilicon Sonic Slack_SSB/4.50.143'],
        ['sec-ch-ua','"Chromium";v="148", "Slack";v="4", "Not=A?Brand";v="99"'],
        ['Content-Type','application/x-www-form-urlencoded'],['sec-ch-ua-mobile','?0']]
    same(value['headers'],headers,'Actual prepared headers differ')
    fields=value['fields']
    require(isinstance(fields,list) and all(isinstance(v,list) and len(v)==2 and all(isinstance(s,str) for s in v) for v in fields),'Malformed form pairs')
    expected_fields=[['blocks',json.dumps(op['snapshot']['blocks'],ensure_ascii=False,separators=(',',':'),sort_keys=True)],['channel','C2'],['client_msg_id',op['client_msg_id']]]
    if target=='thread':expected_fields += [['thread_ts',THREAD],['reply_broadcast','true']]
    # serde_json::Value uses sorted object keys; the full exact UTF-8 string,
    # field order and every routing field are compared, not just decoded blocks.
    same(fields,expected_fields,'Actual frozen form differs (including blocks/routing/order)')
    message=dict(channel='C2',ts=ACK_TS,user='U0',client_msg_id=op['client_msg_id'])
    if target=='thread':message['thread_ts']=THREAD
    same(value['rawResponse'],dict(ok=True,channel='C2',ts=ACK_TS,message=message,fixtureEvidence=dict(retained=True)), 'Full synthetic raw response differs')


def ticket(raw, account):
    require(isinstance(raw,str),'Owned ticket must be the actual serialized object')
    value=unique_json(raw)
    keys(value,('version','nonce','account','marker','database'),'Owned ticket')
    same(value['version'],1,'Wrong ticket version');same(value['account'],account,'Ticket account differs')
    require(uuid4(value['nonce']),'Invalid owned nonce')
    for k in ('marker','database'):
        require(isinstance(value[k],list) and len(value[k])==2 and all(integer(v) for v in value[k]) and any(value[k]),'Invalid retained file identity')
    require(value['marker']!=value['database'],'Marker and database identities alias')
    return value


def native_trace(events, typed, cleared, target):
    require(isinstance(events,list),'Missing native admission history')
    same([e.get('kind') for e in events],['capture','lock','clear-ack','release-requested'],'Wrong actual native admission order')
    for e in events:keys(e,('kind','evidence'),'native event')
    before=dict(id=target+'-composer',scope=typed['scope'],visit=typed['visit'],input=1,composition=0,composing=False,
                wire=REQUESTED,start=typed['start'],end=typed['end'],direction=typed['direction'],
                display_start=typed['start'],display_end=typed['end'],display_len=typed['utf16Length'])
    token=events[1]['evidence'].get('token')
    require(isinstance(token,str) and re.fullmatch('[1-9][0-9]*',token) is not None,'Invalid native admission token')
    after={**before,'wire':'','start':0,'end':0,'direction':'forward','display_start':0,'display_end':0,'display_len':0}
    same(events,[dict(kind='capture',evidence=dict(before=before)),dict(kind='lock',evidence=dict(token=token,before=before)),
                 dict(kind='clear-ack',evidence=dict(token=token,before=before,after=after)),dict(kind='release-requested',evidence=dict(token=token))],
         'Actual native admission changed original scope/text/selection/generations')
    for k,v in dict(text='',display='',start=0,end=0,direction='forward',scope=before['scope'],visit=before['visit']).items():
        same(cleared[k],v,'Clear ACK differs from cleared DOM '+k)
    return before


def validate(fixture, scenario, phase, nonce, pid, storage, receipt):
    keys(receipt,('ok','fixture','scenario','phase','nonce','pid','account','ownedTicket','isolatedRoot','target','viewport','captures','snapshots','details','attempts','actions','nativeAdmission',
                  'preparedRequests','realHttpRequests','syntheticConnectedEvent','syntheticSession','actualOwnedRoute','evidenceScope'),'terminal receipt')
    require(fixture in FIXTURES,'Unknown fixture');names=stages(scenario,phase)
    target='thread' if scenario.endswith('thread') else 'channel';other='channel' if target=='thread' else 'thread'
    require(uuid4(receipt['account']),'Invalid actual account UUID')
    account=receipt['account'];ticket(receipt['ownedTicket'],account)
    for k,v in dict(ok=True,fixture=fixture,scenario=scenario,phase=phase,nonce=nonce,pid=pid,isolatedRoot=str(storage),target=target,
                    viewport=expected_viewport(fixture),preparedRequests=1,realHttpRequests=0,syntheticConnectedEvent=True,syntheticSession=True,actualOwnedRoute=True,evidenceScope=EVIDENCE_SCOPE).items():
        same(receipt[k],v,'Wrong terminal identity '+k)
    require(isinstance(receipt['snapshots'],list),'Missing stage records')
    same([v.get('stage') for v in receipt['snapshots']],list(names),'Wrong stage order')
    snapshots={v['stage']:v for v in receipt['snapshots']}
    initial=snapshots['initial']['durable']
    keys(initial,('managedCount','localCount','uncommittedCount','scopes'),'full loaded inventory')
    keys(initial['scopes'],('channel','thread'),'inventory scope set')
    for t in ('channel','thread'):seed_inventory(initial['scopes'][t],account,t)
    same([initial[k] for k in ('managedCount','localCount','uncommittedCount')],[2,0,0],'Wrong initial inventory totals')
    typed=edited(initial['scopes'][target],REQUESTED,target=='thread')
    cleared=consumed(typed,snapshots['cleared']['durable']['scopes'][target])
    newer=edited(cleared,NEWER,False)
    intent=cleared['row']['state']['consumption']
    final_op=snapshots['result']['projection']['operation']
    operation(final_op['operation'],intent)
    committed=recorded(newer,final_op['recorded'],final_op['operation'])
    final=copy.deepcopy(newer);final['row']=None;final['clock']+=1 # actual StartLocal lease installation touch
    expected_by_stage=dict(initial=initial['scopes'][target],input=typed,cleared=cleared,newer=newer,result=final)
    details=receipt['details'];keys(details,('nativeInitial','input','send','newer'),'details')
    native(details['nativeInitial'],INITIAL,expected_viewport(fixture));SCHEDULE['home_anchor'](details['nativeInitial'])
    same_native(details['nativeInitial'],snapshots['initial']['geometry']['native'])
    for k,txt in (('input',REQUESTED),('newer',NEWER)):
        geometry(details[k],fixture,target,txt)
        same_native(details[k]['native'],snapshots[k]['geometry']['native'])
    SEND['controls'](details['send'],target,expected_viewport(fixture),True)
    for field in ('scope','visit'):
        same(details['send'][field],details['input']['native'][field],'Actual Send control has foreign '+field)
    before=native_trace(receipt['nativeAdmission'],details['input']['native'],snapshots['cleared']['geometry']['native'],target)
    same_native(details['newer']['native'],snapshots['result']['geometry']['native'])
    same(receipt['actions'],[dict(kind='input',key=scope(account,target)['key'],serial=2,local=local(REQUESTED,target=='thread')),
                             dict(kind='input',key=scope(account,target)['key'],serial=3,local=local(NEWER))],'Actual input routing differs')
    require(isinstance(receipt['attempts'],list) and len(receipt['attempts'])==1,'Actual request count differs')
    request(receipt['attempts'][0],final_op['operation'],target)
    same(final_op['attempt']['raw'],receipt['attempts'][0]['rawResponse'],'Parent lost or changed actual raw response')
    anchor=snapshots['initial']['projection']
    root_id=snapshots['cleared']['projection']['operation']['id']
    for record in receipt['snapshots']:
        keys(record,('stage','geometry','durable','projection'),'stage snapshot');stage=record['stage']
        expected=copy.deepcopy(initial);expected['scopes'][target]=expected_by_stage[stage]
        if stage=='result':expected.update(managedCount=1,localCount=1)
        same(record['durable'],expected,'Full durable inventory differs at '+stage)
        text={'initial':INITIAL,'input':REQUESTED,'cleared':'','newer':NEWER,'result':NEWER}[stage]
        # Send button focus may be retained immediately after clear. It is
        # observed explicitly; no claim of field focus is made at that stage.
        focused=record['geometry']['fieldFocused'] if stage=='cleared' else True
        require(type(focused)is bool,'Invalid focus observation')
        geometry(record['geometry'],fixture,target,text,focused=focused)
        validate_projection(record,initial,expected_by_stage,committed,before,target,account)
        projection=record['projection']
        for k in ('epoch','viewId'):same(projection[k],anchor[k],'Owner generation changed across stages')
        for t in ('channel','thread'):
            for k in ('retentionId','writerId','enrollment'):
                same(projection['editors'][t][k],anchor['editors'][t][k],'Actual retained Editor identity changed')
        if projection['operation'] is not None:same(projection['operation']['id'],root_id,'Root operation replaced')
        same(record['geometry']['native']['scope'],details['input']['native']['scope'],'Native scope changed')
        same(record['geometry']['native']['visit'],details['input']['native']['visit'],'Native mount changed')
    require(anchor['editors']['channel']['retentionId'] != anchor['editors']['thread']['retentionId'],'Retained Editor identities alias')


def validate_projection(record, initial, inventories, committed, native_before, target, account):
    p=record['projection'];stage=record['stage'];final=stage=='result'
    keys(p,('account','epoch','viewId','current','sourceCurrent','bindingCurrent','workerCurrent','syntheticSession','cachePresent','channelComposer','threadComposer','book','editors','operation','retainedJobs','routed','refused'),'projection')
    for k,v in dict(account=account,current=True,sourceCurrent=True,bindingCurrent=True,workerCurrent=True,syntheticSession=True,cachePresent=False,refused=0,
                    routed=0 if stage=='initial' else 1 if stage in ('input','cleared') else 2).items():same(p[k],v,'Wrong projection '+k)
    require(integer(p['epoch'],1) and integer(p['viewId']),'Invalid owner generations')
    thread='Some("'+THREAD+'")' if target=='thread' else 'None'
    prefix=f'{p["viewId"]}:{p["epoch"]}:Some("{account}"):Some(DraftKey {{ team: "T1", channel: "C2", thread: {thread} }}):'
    native_scope=record['geometry']['native']['scope']
    require(isinstance(native_scope,str) and re.fullmatch(re.escape(prefix)+'(?:0|[1-9][0-9]*)',native_scope) is not None,
            'Native field does not belong to actual Rust account/destination')
    keys(p['editors'],('channel','thread'),'editors');keys(p['book'],('channel','thread'),'draft book')
    for t in ('channel','thread'):
        inv=inventories[stage] if t==target else initial['scopes'][t]
        editor=p['editors'][t]
        keys(editor,('retentionId','writerId','enrollment','serial','local','managed','uncommitted','pending','running','parked','error','failedInputs'),'Editor')
        require(retention_id(editor['retentionId']) and uuid4(editor['writerId']),'Invalid retained Editor/writer identity')
        same(editor['enrollment'],initial['scopes'][t]['row']['lineage']['enrollment'],'Editor enrollment changed')
        same(editor['serial'],1 if t!=target or stage=='initial' else 2 if stage in ('input','cleared') else 3,'Wrong actual Editor serial')
        for k,v in dict(local=inv['local'],managed=None if inv['row'] is None else review(inv),uncommitted=None,pending=None,running=False,parked=False,error=None,failedInputs=[]).items():
            same(editor[k],v,'Wrong settled Editor '+k+' at '+stage)
        # Home(Channel) legitimately has no mounted thread composer; its book
        # remains exact. For a mounted thread, both visible draft values apply.
        composer=local('')['composer'] if t=='thread' and target=='channel' else inv['local']['composer']
        same(p[t+'Composer'],composer,'Shell composer differs at '+stage)
        same(p['book'][t],None if not inv['local']['composer']['text'] else inv['local'],'Book input differs at '+stage)
    require(p['editors']['channel']['writerId']!=p['editors']['thread']['writerId'],'Scope writers alias')
    require(isinstance(p['retainedJobs'],list) and 0<len(p['retainedJobs'])<=64,'Invalid retained job inventory')
    families=Counter()
    for job in p['retainedJobs']:
        keys(job,('family','generation'),'retained job metadata')
        require(job['family'] in ('protected-inventory','protected-local-inventory','protected-review',
                'protected-enroll-local','protected-stream-state','protected-open-stream','protected-verify-local',
                'protected-register-writer','protected-checkpoint','protected-consume-send','protected-prepare',
                'protected-claim-dispatch','protected-record','protected-release','protected-start-local'),
                'Unexpected actual retained job family')
        families[job['family']]+=1
        same(job['generation'],1,'Unexpected route fence generation')
    # Counts describe retained reservations, not proof that a reserved child
    # executed. The independent operation/HTTP/storage evidence proves execution.
    expected_jobs={'protected-enroll-local':2,'protected-stream-state':1,'protected-open-stream':1,
                   'protected-verify-local':2,'protected-register-writer':2,
                   'protected-checkpoint':2 if stage=='initial' else 3 if stage in ('input','cleared') else 4}
    expected_jobs.update({name:0 if stage in ('initial','input') else 1 for name in
                         ('protected-consume-send','protected-prepare','protected-claim-dispatch','protected-record')})
    expected_jobs.update({name:1 if final else 0 for name in ('protected-release','protected-start-local')})
    for family,count in expected_jobs.items():same(families[family],count,'Wrong retained family count '+family+' at '+stage)
    op=p['operation']
    if stage in ('initial','input'):
        same(op,None,'Operation predates actual Send click');return
    keys(op,('id','scope','epoch','serial','local','book','live','phase','status','native','review','consumed','operation','recorded','attempt','publication','releasePresent','localLeasePresent','handoff'),'root operation')
    require(integer(op['id'],1),'Missing retained operation identity')
    for k,v in dict(scope=scope(account,target),epoch=p['epoch'],serial=2,local=local(REQUESTED,target=='thread'),book=local(REQUESTED,target=='thread'),live=True,native=native_before,
                    review=review(inventories['input']),consumed=dict(before=review(inventories['input']),after=review(inventories['cleared']),serial=2,nativeClear='Cleared')).items():
        same(op[k],v,'Retained root evidence differs '+k)
    same(op['phase'],'StartLocal' if final else 'NativeClear','Wrong actual operation phase')
    same(op['status'],'Finished' if final else 'Running','Wrong actual operation status')
    for k in ('operation','recorded','attempt'):
        if not final:same(op[k],None,'Work crossed post-clear hold: '+k)
    for k in ('releasePresent','localLeasePresent'):same(op[k],final,'Wrong exact handoff evidence')
    same(op['handoff'],{'Ok':None} if final else None,'Wrong handoff result')
    pub=dict(initialInventory=True,pendingSend=True,sendHelperInvoked=True,inventory=True,queue=False,afterReleaseInventory=True) if final else dict.fromkeys(('initialInventory','pendingSend','sendHelperInvoked','inventory','queue','afterReleaseInventory'))
    same(op['publication'],pub,'Actual publication decisions differ')
    if final:
        raw=op['attempt']['raw'];keys(op['attempt'],('dispatched','raw','outcome','issue'),'full retained Attempt')
        same(op['attempt']['dispatched'],True,'HTTP was not dispatched');same(op['attempt']['issue'],None,'Unexpected decoder issue')
        same(op['attempt']['outcome'],f'Sent {{ client_msg_id: "{op["operation"]["client_msg_id"]}", draft_id: None, ts: "{ACK_TS}" }}','Actual typed positive outcome differs')
        same(raw['fixtureEvidence'],{'retained':True},'Raw unsupported response fields lost')


def validate_closed_storage(storage, receipt, *, temporary_root):
    module=runpy.run_path(str(Path(__file__).with_name('protected_owned_send_storage.py')))
    return module['validate'](storage,receipt,globals(),temporary_root=temporary_root)
