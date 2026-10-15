"""Strict disconnected insertion/ACK cohort; no preference executor coverage."""
from pathlib import Path
import runpy
import sys
import re

SCRIPTS=next(p/'scripts' for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'scripts/native_fixture.py').is_file())
sys.path.insert(0,str(SCRIPTS))
from native_check import expected_viewport
from native_fixture import DEFAULT_BINARY,ROOT,NativeFixture
M=runpy.run_path(str(SCRIPTS/'managed_draft_schedule_receipts.py'))
U=runpy.run_path(str(SCRIPTS/'check-slack-list-board-add-native.py'))
require,same,keys,proof,number=(M[k] for k in ('require','same','keys','proof','number'))
binary_sha,png_signature,validate_exit,normalize,validate_pointer_cleanup,closed_receipts,publish_ready,validate_ready=(U[k] for k in ('binary_sha','png_signature','validate_exit','normalize','validate_pointer_cleanup','closed_receipts','publish_ready','validate_ready'))
strict_equal=M['strict_equal']
FIXTURE='emoji-native';FIXTURES=(FIXTURE,FIXTURE+'-narrow')
FLAG='SUPER_PLATINUM_EMOJI_NATIVE_CHECK'
PREFIX,WAITING,STAGE='emoji-native-check: ','emoji-native-waiting: ','emoji-native-capture: '
STAGES=('initial','completion-offered','completion-standard','custom-offered','completion-custom','picker-selected','picker-cancelled','picker-unready','picker-ready','picker-inserted','guard-newer','guard-catalog','guard-account','guard-mount','guard-navigation','guard-composition')
ACTION_NAMES=('completion-standard','completion-custom','picker-cancelled','picker-inserted')+STAGES[10:]
BASE='\U0001f98a replace end';NEWER='Newer \u65e5\u672c\u8a9e \U0001f680 e\u0301'
QUERY='\U0001f98a :rocket';CUSTOM='\U0001f98a :party_fixture'
STANDARD='\U0001f98a \U0001f680 ';CUSTOM_INSERTED=CUSTOM+': ';PICKED='\U0001f98a \U0001f680 end'
CONTRACT_READY=Path(__file__).resolve().parent==SCRIPTS
def contract(fixture):
    require(fixture in FIXTURES,'Unknown emoji fixture')
    require(CONTRACT_READY,'Staged emoji cohort cannot launch before promotion')
def ready_tuple(fixture,nonce,pid):
    require(fixture in FIXTURES and isinstance(nonce,str) and re.fullmatch('[0-9a-f]{32}',nonce) and type(pid) is int and pid>0,'Invalid exact owner tuple')
    return dict(version=1,nonce=nonce,pid=pid,fixture=fixture)
def units(text):return len(text.encode('utf-16-le'))//2
def bounds(text):
    result={0};n=0
    for c in text:n+=units(c);result.add(n)
    return result
def snapshot(value):
    keys(value,{'text','anchor','focus','direction'},'wire/UTF16 snapshot')
    require(isinstance(value['text'],str),'Snapshot text missing')
    require(type(value['anchor']) is int and type(value['focus']) is int and value['anchor'] in bounds(value['text']) and value['focus'] in bounds(value['text']) and value['anchor']<=value['focus'],'Invalid UTF16 boundary')
    require(value['direction'] in ('none','forward','backward'),'Invalid selection direction')
def native(value,text=None):
    keys(value,{'text','codePoints','utf16Length','start','end','direction','scope','visit','catalog','preferences','focused','composing'},'actual native composer')
    snapshot(dict(text=value['text'],anchor=value['start'],focus=value['end'],direction=value['direction']))
    same(value['codePoints'],[ord(c) for c in value['text']],'Changed Unicode code points')
    same(value['utf16Length'],units(value['text']),'Wrong UTF16 length')
    for key in ('scope','visit'):require(isinstance(value[key],str) and value[key],'Missing native identity: '+key)
    for key in ('catalog','preferences'):require(isinstance(value[key],str),'Missing native authority: '+key)
    for key in ('focused','composing'):require(type(value[key]) is bool,'Invalid native boolean')
    if text is not None:same(value['text'],text,'Wrong actual inserted/preserved wire text')
def wire_native(value):return dict(text=value['text'],anchor=value['start'],focus=value['end'],direction=value['direction'])
def owner(value,n=None):
    keys(value,{'account','epoch','team','channel','view','channelGeneration','scope','mount','catalog','preferences'},'actual owner')
    same(value['account'],'emoji-native-disconnected','Wrong isolated account')
    same(value['team'],'T1','Wrong team');same(value['channel'],'C2','Wrong destination')
    for key in ('epoch','view','channelGeneration'):require(type(value[key]) is int and value[key]>=0,'Invalid owner generation')
    for key in ('scope','mount','catalog'):require(isinstance(value[key],str) and value[key],'Missing actual owner token')
    p=value['preferences'];keys(p,{'loaded','revision','raw'},'synthetic preferences')
    require(type(p['loaded']) is bool and type(p['revision']) is int and p['revision']>=0,'Invalid preference authority')
    same(p['raw'],{},'Unexpected synthetic preference mutation/writer')
    if n:
        same(value['scope'],n['scope'],'Native scope differs from current Rust owner')
        same(value['mount'],n['visit'],'Native mount differs from current Rust owner')
        same(value['catalog'],n['catalog'],'Native catalog differs from real worker publication')
        same(str(p['revision']),n['preferences'],'Native preference revision differs')
def action_proof(value,viewport,disabled=False):
    keys(value,{'ok','control','clickCalls','viewport','documentWidth'},'actual double choice')
    same(value['ok'],True,'Choice action failed');same(value['clickCalls'],2,'Missing duplicate native choice')
    same(value['viewport'],viewport,'Choice viewport mismatch');require(number(value['documentWidth']) and 0<value['documentWidth']<=viewport[0]+1,'Choice document overflow')
    proof(value['control'],viewport);same(value['control']['disabled'],disabled,'Wrong choice readiness')
def open_proof(value,viewport):
    keys(value,{'ok','control','clicked','viewport','documentWidth'},'actual picker open')
    M['action'](value,viewport)
def validate_events(events,actions):
    require(isinstance(events,list),'Missing actual observer ledger')
    presented={};chosen={};admitted=set();decisions={};acks=[];picker=[];picker_acks=[];picker_open=[]
    for index,e in enumerate(events):
        keys(e,{'sequence','stage','kind','value'},'observer ledger entry');same(e['sequence'],index,'Observer sequence gap')
        require(e['stage'] in ACTION_NAMES,'Unknown event stage')
        v=e['value'];kind=e['kind']
        if kind=='worker-present':
            keys(v,{'seq','owner','snapshot','catalog','preferences','choices'},'real worker result')
            snapshot(v['snapshot']);require(type(v['seq']) is int and v['seq']>=0 and type(v['preferences']) is int and v['preferences']>=0,'Worker counters')
            require(isinstance(v['owner'],str) and v['owner'] and isinstance(v['catalog'],str) and v['catalog'],'Worker identities')
            require(isinstance(v['choices'],list) and len(v['choices'])<=25,'Unbounded worker rows')
            for row in v['choices']:
                keys(row,{'id','label','detail'},'real worker row');require(all(isinstance(x,str) for x in row.values()) and row['id'],'Invalid worker row')
            presented[(v['owner'],v['catalog'],v['seq'])]=v
        elif kind=='completion-choose':
            keys(v,{'ticket'},'actual choice event');t=v['ticket']
            keys(t,{'serial','seq','owner','catalog','choice','snapshot'},'actual admission ticket');snapshot(t['snapshot'])
            require(type(t['serial']) is int and t['serial']>0 and type(t['seq']) is int,'Invalid ticket serial')
            require(e['stage'] not in chosen,'Duplicate native choice reached Rust')
            p=presented.get((t['owner'],t['catalog'],t['seq']));require(p is not None,'Choice not produced by actual worker')
            same(t['snapshot'],p['snapshot'],'Choice snapshot differs from ranked snapshot')
            require(any(r['id']==t['choice'] for r in p['choices']),'Choice not in presented rows')
            same(t['snapshot'],wire_native(actions[e['stage']]['before']),'Ticket differs from actual before field')
            before=actions[e['stage']]['before'];same(t['catalog'],before['catalog'],'Ticket differs from native catalog')
            same(str(p['preferences']),before['preferences'],'Worker used another preference revision')
            require(re.fullmatch(re.escape(before['scope'])+r':mount:[1-9][0-9]*',t['owner']) is not None,'Worker owner does not name actual composer scope')
            chosen[e['stage']]=t
        elif kind=='completion-admission':
            keys(v,{'answer'},'real admission result');a=v['answer']
            require(e['stage'] in chosen,'Admission preceded real choice')
            require(e['stage'] not in decisions,'Duplicate actual Rust decision')
            decisions[e['stage']]=a
            if a is not None:
                keys(a,{'kind','serial','seq','owner','catalog','choice','allowed','value','start','end','spacer'},'actual admission answer')
                same(a['kind'],'emoji_admit','Wrong admission kind');same(a['allowed'],True,'Invalid successful answer')
                for key in ('serial','seq','owner','catalog','choice'):same(a[key],chosen[e['stage']][key],'Admission changed ticket identity')
                name='party_fixture' if e['stage']=='completion-custom' else 'rocket'
                same(a['value'],':party_fixture:' if name=='party_fixture' else '\U0001f680','Wrong resolved insertion')
                same(a['start'],3,'Wrong trigger start');same(a['end'],units(CUSTOM if name=='party_fixture' else QUERY),'Wrong trigger end');same(a['spacer'],' ','Wrong insertion spacer')
                admitted.add(e['stage'])
        elif kind=='completion-ack':
            keys(v,{'ticket','after','validated','model'},'actual inserted ACK')
            require(e['stage'] in ('completion-standard','completion-custom'),'Stale choice inserted')
            require(e['stage'] in admitted,'ACK without actual successful admission')
            same(v['ticket'],chosen.get(e['stage']),'ACK did not consume actual choice ticket');same(v['validated'],True,'Raw ACK failed real Gate validation')
            same(v['after'],wire_native(actions[e['stage']]['after']),'ACK differs from actual final native snapshot')
            same(v['model'],v['after']['text'],'Normal input did not update model before observed ACK')
            acks.append(e['stage'])
        elif kind=='picker-open':
            keys(v,{'serial','snapshot'},'actual picker capture')
            require(type(v['serial']) is int and v['serial']>0,'Invalid captured picker serial');snapshot(v['snapshot'])
            same(e['stage'],'picker-cancelled' if not picker_open else 'picker-inserted','Unexpected picker revival/open')
            same(v['snapshot'],wire_native(actions[e['stage']]['before']),'Picker opened on wrong native snapshot')
            if picker_open:require(v['serial']>picker_open[-1]['serial'],'Picker reused consumed serial')
            picker_open.append(v)
        elif kind=='picker-choose':
            keys(v,{'serial','snapshot','value','guard'},'actual picker admission')
            same(e['stage'],'picker-inserted','Unready/cancelled picker consumed a selection')
            require(type(v['serial']) is int and v['serial']>0,'Invalid picker serial');snapshot(v['snapshot'])
            same(v['snapshot'],wire_native(actions['picker-inserted']['before']),'Picker changed captured selection')
            require(len(picker_open)==2,'Picker choice lacked actual capture');same(v['serial'],picker_open[-1]['serial'],'Picker readiness revived another serial')
            same(v['value'],'\U0001f680','Wrong picker resolution');keys(v['guard'],{'catalog','preferences'},'native picker fence')
            require(all(isinstance(x,str) and x for x in v['guard'].values()),'Missing picker authority');picker.append(v)
            same(v['guard'],{key:actions['picker-inserted']['after'][key] for key in ('catalog','preferences')},'Picker ACK used another native authority')
        elif kind=='picker-ack':
            keys(v,{'serial','confirmed','scopeCurrent','catalogCurrent'},'actual picker native ACK')
            require(len(picker)==1,'Picker ACK without unique actual choice');same(v['serial'],picker[0]['serial'],'Wrong picker serial')
            for k in ('confirmed','scopeCurrent','catalogCurrent'):same(v[k],True,'Picker ACK not currently authorized')
            picker_acks.append(v)
        else:raise RuntimeError('Unknown/timeout observer event: '+str(kind))
    same(acks,['completion-standard','completion-custom'],'Missing or replayed validated ACK')
    same(set(chosen),{'completion-standard','completion-custom',*STAGES[10:]},'Missing exercised admission fences')
    require(len(picker)==len(picker_acks)==1 and len(picker_open)==2,'Missing or repeated native picker ACK/capture')
    require('guard-catalog' in decisions and decisions['guard-catalog'] is None,'Catalog replacement did not finish actual denied Rust admission')
    return chosen,decisions

def validate_controller(events,actions,chosen,decisions):
    order=('completion-standard','completion-custom','guard-newer','guard-catalog','composition-start','guard-composition','composition-end')
    require(isinstance(events,list) and len(events)==len(order),'Missing/repeated completed native controller decisions')
    for index,(event,stage) in enumerate(zip(events,order)):
        keys(event,{'sequence','kind','owner','scope','mount','composing','focused','snapshot','result','accepted'},'actual controller observation')
        same(event['sequence'],index,'Controller observer sequence gap')
        name='guard-composition' if stage.startswith('composition-') else stage
        a=actions[name];t=chosen[name];n=a['after'];is_decision=not stage.startswith('composition-')
        same(event['kind'],'admission' if is_decision else stage,'Wrong controller event order')
        same(event['owner'],t['owner'],'Controller event changed actual worker owner')
        same(event['scope'],n['scope'],'Controller observed another scope');same(event['mount'],n['visit'],'Controller observed another mount')
        same(event['focused'],True,'Guard was bypassed through loss of native focus')
        same(event['composing'],stage in ('composition-start','guard-composition'),'Actual composition listener state differs')
        snapshot(event['snapshot']);same(event['snapshot'],wire_native(n),'Completed controller decision changed native text/caret/direction')
        if is_decision:
            require(stage in decisions,'Native result missing actual completed Rust decision')
            result={k:t[k] for k in ('serial','seq','owner','catalog','choice')}
            result['allowed']=decisions[stage] is not None
            same(event['result'],result,'Native decision did not consume actual Rust reply')
            same(event['accepted'],stage.startswith('completion-'),'Native guard inserted or success was not accepted')
        else:
            same(event['result'],None,'Composition observation fabricated reply');same(event['accepted'],None,'Composition observation fabricated admission')
def validate(fixture,nonce,pid,receipt,platform=sys.platform):
    try:_validate(fixture,nonce,pid,receipt,platform)
    except (KeyError,TypeError,ValueError,AttributeError,IndexError,OverflowError) as e:raise RuntimeError('Malformed emoji receipt: '+str(e)) from e
def _validate(fixture,nonce,pid,r,platform):
    ready_tuple(fixture,nonce,pid);viewport=expected_viewport(fixture,platform)
    for key,value in dict(ok=True,fixture=fixture,nonce=nonce,pid=pid,viewport=viewport,noSession=True,noTransport=True,noCache=True,preparedRequests=0,attempts=[],syntheticInput=True,syntheticPreferences=True,preferenceWriterCovered=False).items():same(r[key],value,'Wrong terminal field: '+key)
    same(r['messagesInitial'],r['messagesFinal'],'Emoji insertion changed messages or pending delivery')
    keys(r['messagesInitial'],{'channels','threadCount','pendingDeliveryCount'},'message inventory')
    require(isinstance(r['messagesInitial']['channels'],dict),'Missing all channel inventory')
    same(r['messagesInitial']['threadCount'],0,'Unexpected thread cohort');same(r['messagesInitial']['pendingDeliveryCount'],0,'Unexpected pending delivery')
    for bag in r['messagesInitial']['channels'].values():
        keys(bag,{'messages','pending'},'message bag');require(isinstance(bag['messages'],list) and isinstance(bag['pending'],list),'Invalid message bag')
    require(isinstance(r['actions'],list) and len(r['actions'])==len(ACTION_NAMES),'Missing ordered actions')
    actions={}
    for a,name in zip(r['actions'],ACTION_NAMES):
        same(a['kind'],name,'Wrong action order');actions[name]=a
        native(a['before']);native(a['after'])
        if name.startswith('completion-'):
            keys(a,{'kind','choice','before','after'},'completion action');action_proof(a['choice'],viewport)
            native(a['before'],QUERY if name=='completion-standard' else CUSTOM);native(a['after'],STANDARD if name=='completion-standard' else CUSTOM_INSERTED)
            same(a['after']['start'],units(a['after']['text']),'Insertion caret');same(a['after']['end'],a['after']['start'],'Noncollapsed final selection');same(a['after']['direction'],'forward','Wrong insertion direction')
        elif name=='picker-cancelled':
            keys(a,{'kind','open','escape','before','after'},'picker cancellation');open_proof(a['open'],viewport)
            keys(a['escape'],{'ok','prevented'},'actual Escape');same(a['escape']['ok'],True,'Escape failed');require(type(a['escape']['prevented']) is bool,'Missing Escape facts')
            same(wire_native(a['before']),dict(text=BASE,anchor=3,focus=10,direction='backward'),'Missing actual selected range')
            same(wire_native(a['after']),wire_native(a['before']),'Cancel changed selection');same(a['after']['focused'],True,'Cancel did not restore field focus')
        elif name=='picker-inserted':
            keys(a,{'kind','open','blocked','choice','before','after'},'picker readiness');open_proof(a['open'],viewport)
            action_proof(a['blocked'],viewport,True);action_proof(a['choice'],viewport)
            same(wire_native(a['before']),dict(text=BASE,anchor=3,focus=10,direction='backward'),'Picker selection changed before ready')
            native(a['after'],PICKED);same(wire_native(a['after']),dict(text=PICKED,anchor=5,focus=5,direction='forward'),'Wrong exact picker edit')
        else:
            keys(a,{'kind','choice','before','after','ownerBefore','ownerAfter','composition'},'stale admission action');action_proof(a['choice'],viewport)
            same(a['composition'],dict(ok=True,type='compositionstart',data='',bubbles=True,isTrusted=False) if name=='guard-composition' else None,'Missing actual composition event facts')
            native(a['before'],QUERY);native(a['after'],NEWER if name=='guard-newer' else QUERY)
            if name in ('guard-newer','guard-catalog','guard-composition'):
                same(a['before']['focused'],True,'Guard did not start focused');same(a['after']['focused'],True,'Same-mount guard lost focus')
                expected=dict(text=NEWER,anchor=units(NEWER),focus=units(NEWER),direction='forward') if name=='guard-newer' else wire_native(a['before'])
                same(wire_native(a['after']),expected,'Same-mount guard changed actual selection')
            b,c=a['ownerBefore'],a['ownerAfter'];owner(b,a['before']);owner(c,a['after'])
            if name=='guard-catalog':
                require(b['catalog']!=c['catalog'],'Catalog never replaced');same(b['scope'],c['scope'],'Catalog case changed composer')
                same(c,dict(b,catalog=c['catalog']),'Catalog guard changed unrelated ownership')
            elif name in ('guard-account','guard-mount','guard-navigation'):
                key={'guard-account':'epoch','guard-mount':'view','guard-navigation':'channelGeneration'}[name]
                same(c[key],b[key]+(2 if name=='guard-navigation' else 1),'Wrong ownership transition')
                require(b['scope']!=c['scope'] and b['mount']!=c['mount'],'Old native owner reused')
                expected=dict(b,scope=c['scope'],mount=c['mount']);expected[key]=c[key]
                if name=='guard-account':
                    require(b['catalog']!=c['catalog'],'Account retirement reused old catalog');expected['catalog']=c['catalog']
                same(c,expected,'Ownership guard changed unrelated identity')
            else:same(b,c,'Input guard unexpectedly changed Rust owner')
    chosen,decisions=validate_events(r['events'],actions)
    validate_controller(r['controllerEvents'],actions,chosen,decisions)
    require(isinstance(r['geometry'],list) and len(r['geometry'])==len(STAGES),'Missing stage geometry')
    texts={s:QUERY for s in STAGES};texts.update(initial=BASE,**{'completion-offered':QUERY,'completion-standard':STANDARD,'custom-offered':CUSTOM,'completion-custom':CUSTOM_INSERTED,'picker-selected':BASE,'picker-cancelled':BASE,'picker-unready':BASE,'picker-ready':BASE,'picker-inserted':PICKED,'guard-newer':NEWER})
    for row,stage in zip(r['geometry'],STAGES):
        keys(row,{'ok','stage','surface','controls','native','viewport','documentWidth','pickerReady','pickerNode','owner','model'},'native capture geometry')
        same(row['stage'],stage,'Wrong captured stage');same(row['ok'],True,'Failed geometry');same(row['viewport'],viewport,'Wrong viewport')
        require(number(row['documentWidth']) and 0<row['documentWidth']<=viewport[0]+1,'Document overflow')
        surface='completion' if stage in ('completion-offered','custom-offered') else 'picker' if stage in ('picker-selected','picker-unready','picker-ready') else 'composer'
        same(row['surface'],surface,'Wrong native surface')
        expected={'panel','query','choice'} if surface=='picker' else {'field','footer','popup','choice'} if surface=='completion' else {'field','footer'}
        keys(row['controls'],expected,'bounded visible controls')
        for control in row['controls'].values():proof(control,viewport)
        for child,parent in ([('query','panel'),('choice','panel')] if surface=='picker' else [('field','footer')]+([('choice','popup')] if surface=='completion' else [])):
            a,b=row['controls'][child]['rect'],row['controls'][parent]['rect']
            require(all(a[x]>=b[x]-1 and a[x]+a[w]<=b[x]+b[w]+1 for x,w in [('x','width'),('y','height')]),'Clipped control within surface')
        same(row['pickerReady'],stage!='picker-unready' if surface=='picker' else None,'Wrong picker readiness')
        if surface=='picker':require(isinstance(row['pickerNode'],str) and row['pickerNode'],'Missing actual picker node identity')
        else:same(row['pickerNode'],None,'Nonpicker retained dialog')
        if surface=='picker':same(row['controls']['choice']['disabled'],stage=='picker-unready','Unready picker selectable')
        native(row['native'],texts[stage]);owner(row['owner'],row['native'])
        same(row['owner']['preferences']['loaded'],stage!='picker-unready','Picker readiness differs from authoritative preference state')
        same(row['owner']['preferences']['revision'],0 if stage=='picker-unready' else 1,'Unexpected isolated preference revision')
        keys(row['model'],{'text','selection','reply_broadcast'},'normal composer input model')
        same(row['model']['text'],row['native']['text'],'Native input did not reach normal model');same(row['model']['reply_broadcast'],False,'Channel broadcast changed')
        if row['model']['selection'] is not None:
            keys(row['model']['selection'],{'start','end'},'model byte selection')
            allowed={len(row['native']['text'][:i].encode()) for i in range(len(row['native']['text'])+1)}
            require(all(type(v) is int and v in allowed for v in row['model']['selection'].values()),'Invalid model byte boundary')
    by_stage={row['stage']:row for row in r['geometry']}
    same(by_stage['picker-unready']['pickerNode'],by_stage['picker-ready']['pickerNode'],'Readiness replaced/revived another native dialog')
    require(by_stage['picker-selected']['pickerNode']!=by_stage['picker-ready']['pickerNode'],'Cancelled dialog revived')
    require(isinstance(r['limitations'],list) and len(r['limitations'])>=4,'Missing explicit cohort limitations')
def validate_native(fixture,nonce,pid,directory,receipt,stages_seen,platform=sys.platform,expected_dimensions=None):
    validate(fixture,nonce,pid,receipt,platform)
    same(set(receipt['captures']),set(STAGES),'Missing or extra capture')
    same(stages_seen,[dict(stage=s,edge=e) for s in STAGES for e in ('before','after')],'Missing/reordered capture checkpoints')
    dimensions={}
    for stage in STAGES:
        path=directory/(stage+'.png');same(receipt['captures'][stage],str(path),'Capture escaped owned directory')
        dimensions[stage]=png_signature(path,expected_dimensions)
    return dimensions
def validate_closed_log(path,expected,directory,admitted,expected_dimensions=None):
    waiting,receipts=closed_receipts(path,WAITING),closed_receipts(path,PREFIX)
    require(len(waiting)==len(receipts)==1,'Closed log duplicate/missing ownership or terminal')
    validate_ready(waiting[0],expected);same(receipts[0],admitted,'Closed terminal changed')
    return validate_native(expected['fixture'],expected['nonce'],expected['pid'],directory,receipts[0],closed_receipts(path,STAGE),expected_dimensions=expected_dimensions)
