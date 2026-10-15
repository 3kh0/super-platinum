"""Pure, strict slice-1 observation decoding. No native operations or timing repair."""
import json
import math
import re
from pathlib import Path
import statistics
import struct

MAGIC = 0x5350454D4A523031
CAPACITY, WORDS, HEADER_WORDS = 65536, 16, 12
BYTES = (HEADER_WORDS + CAPACITY * WORDS) * 8
ROLES = ('W1', 'B1', 'W2', 'B2', 'W3', 'B3')
FIELDS = ('published', 'at', 'kind', 'generation', 'child', 'context', 'thread',
          'role', 'write', 'mount', 'action', 'related', 'r0', 'r1', 'r2', 'r3')
CONTEXTS = {0:'unknown',1:'local-displaced',2:'remote-displaced-shell-write',
    3:'local-rejected-return',4:'remote-rejected-return',5:'query-slot-replace',
    6:'gate-selection-replace',7:'worker-return-continuation',8:'worker-catalog-replace',
    9:'worker-waiting-drop',10:'worker-output-drop',11:'picker-effect',
    12:'picker-render',14:'account-install-shell-write'}

def require(ok, message):
    if not ok:
        raise ValueError(message)

def fnv(value):
    result = 0xcbf29ce484222325
    for byte in value.encode():
        result = ((result ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return result

def integer(value):
    return type(value) is int and 0 <= value < 1 << 64

def _layout(data, identity):
    require(integer(identity['pid']) and identity['pid']>0 and type(identity['nonce']) is str
            and re.fullmatch('[0-9a-f]{32}',identity['nonce']) and identity['process'] in ROLES,'Trace identity types')
    require(len(data) == BYTES, 'Trace byte length')
    h = struct.unpack_from('<12Q', data)
    nonce = identity['nonce']
    require(h[:3] == (MAGIC, 1, CAPACITY), 'Trace schema')
    require(h[3] <= CAPACITY and h[4] == 0, 'Trace overflow')
    require(h[5] > 0 and h[7] == 0, 'Serial thread/header reserved')
    require(h[8:] == (identity['pid'], int(nonce[:16],16), int(nonce[16:],16),
                     ROLES.index(identity['process'])+1), 'Trace ownership')
    events=[]
    for i in range(h[3]):
        row=struct.unpack_from('<16Q',data,(HEADER_WORDS+i*WORDS)*8)
        require(row[0] == i+1, 'Unpublished/duplicated trace slot')
        require(row[12:] == (0,0,0,0), 'Trace slot reserved')
        e=dict(zip(FIELDS,row))
        require(1 <= e['kind'] <= 23 and e['context'] in CONTEXTS, 'Unknown trace kind/context')
        require(e['thread'] > 0 and e['role'] == (1 if e['thread']==h[5] else 2), 'Fabricated thread role')
        require(e['write'] in (0,1,2) and e['child'] in (0,1,2), 'Invalid write/child')
        require(e['generation']==0 or (e['generation']>>48)<=3 and e['generation']&3 in (1,2), 'Generation encoding')
        events.append(e)
    require(not any(data[(HEADER_WORDS+h[3]*WORDS)*8:]), 'Unexpected unpublished tail')
    return {'header':list(h),'events':events}

def decode(data,identity):
    value=_layout(data,identity)
    validate_events(value['events'])
    return value

def validate_events(events):
    threads={};stacks={};children={};builds={}
    for e in events:
        thread=e['thread'];at=e['at'];stack=stacks.setdefault(thread,[])
        require(at >= threads.get(thread,0), 'Per-thread clock reversed')
        threads[thread]=at
        location=(e['context'],e['write'],e['mount'])
        if e['kind']==20:
            require(e['related']==e['context'], 'Scope entered another context')
            stack.append(location)
        else:
            require(location == (stack[-1] if stack else (0,0,0)), 'Fabricated lexical context/guard')
        if e['kind']==21:
            require(bool(stack), 'Unmatched scope exit')
            stack.pop()
            require(e['related']==(stack[-1][0] if stack else 0), 'Scope restoration changed')
        require(e['write']!=1 or e['context'] in (2,14), 'Uninstrumented shell write claim')
        require(e['context'] not in (2,14) or e['write']==1, 'Actual shell guard missing')
        if e['context'] in (1,2,3,4,5,6,7,11,12,14):require(e['role']==1,'Serial source context on another thread')
        if e['context']==8:require(e['role']==2 and e['write']==2,'Worker replacement not on observed nonserial thread')
        if e['kind'] in (1,10,11,14,15,23):require(e['role']==2,'Actual producer/worker event on serial thread')
        if e['kind'] in (7,8,9,12,13,16,18,19,22):require(e['role']==1,'Actual continuation/control event on another thread')
        if e['kind'] in (10,11):
            values=builds.setdefault(e['generation'],[]);values.append(e['kind'])
            require(values in ([10],[10,11]), 'Build repeated/reversed')
        if e['kind'] in (1,2,3,4,5):
            require(e['generation']!=0 and e['child'] in (1,2), 'Payload identity missing')
            key=(e['generation'],e['child']);values=children.setdefault(key,[]);values.append(e)
            require([v['kind'] for v in values]==list(range(1,len(values)+1)), 'Repeated/missing/reordered destructor')
            if len(values)>2:
                require(all(e[k]==values[1][k] for k in ('thread','role','context','write','mount')), 'Destructor migrated context/thread')
        if e['kind']==17:
            raise ValueError('Native observer failure '+str(e['related']))
    # Open scopes/builds/children are retained as incomplete on process teardown;
    # never silently balance or treat process exit as completion.

def validate_actions(schedule, terminal, trace):
    require(terminal.get('ok') is True, 'No successful close-ready endpoint')
    role=terminal['ready']['process']
    require(role in ROLES and all(integer(terminal['ready'][key]) for key in ('version','pid','creation')),'Terminal readiness types')
    require(integer(terminal['index']),'Terminal index type')
    p=next(p for p in schedule['processes'] if p['process']==role)
    operations=[x for x in p['ordered_actions'] if x['kind']!='cutoff']
    expected=operations[:-1]; actions=terminal['actions'];events=trace['events']
    require(len(actions)==len(expected), 'Action inventory differs')
    require(terminal['action']==operations[-1]['id'] and terminal['index']==len(operations), 'Close action identity')
    starts=[e for e in events if e['kind']==7];ends=[e for e in events if e['kind']==8]
    require([e['action'] for e in starts]==list(range(1,len(operations)+1)), 'Native start inventory')
    require([e['related'] for e in starts]==list(range(1,len(operations)+1)), 'Native start binding')
    require([e['action'] for e in ends]==list(range(1,len(operations))), 'Native end inventory')
    require([e['related'] for e in ends]==list(range(1,len(operations))), 'Native end binding')
    for i,(action,op) in enumerate(zip(actions,expected),1):
        require(integer(action['index']),'JS action index type')
        require(action.get('ok') is True and action['id']==op['id'] and action['index']==i, 'JS action identity')
        times=[action[k] for k in ('start','ackAt','acceptedAt','raf1','raf2')]
        require(all(type(t) in (float,int) and math.isfinite(t) and t>=0 for t in times)
                and times==sorted(times) and times[-1]-times[0]<=5000, 'JS endpoint clock/deadline')
        require(0 <= ends[i-1]['at']-starts[i-1]['at'] <= 5_000_000_000, 'Native endpoint deadline')
        # Both durations remain on their own clocks. No one-way bridge estimate.
    js=terminal['js'];require(js.get('ok') is True,'Missing JS observation')
    js=js['evidence'];require(js['overflow'] is False and js['active'] is None,'JS overflow/unfinished action')
    je=js['events'];require(len(je)<=16384,'JS bound')
    require(all(integer(e['index']) for e in je),'JS event index type')
    require([e['index'] for e in je]==list(range(len(je))),'JS sequence')
    require([e['action'] for e in je if e['kind']=='start']==[x['id'] for x in expected], 'JS start inventory')
    require([e['action'] for e in je if e['kind']=='end']==[x['id'] for x in expected], 'JS end inventory')
    require(all(e['kind'] in ('start','end','query','receive','invalidate','accepted') for e in je),'Unknown JS event')
    require(all(type(e['at']) in (int,float) and math.isfinite(e['at']) and e['at']>=0 for e in je),'JS ledger clock')
    require([e['at'] for e in je]==sorted(e['at'] for e in je),'JS ledger time reversed')
    account=0;revision=1
    for action,op in zip(actions,expected):
        sub=op.get('subcase','');verb=op['id'].split('/')[-1]
        if (sub,verb)==('refresh.closed','refresh'):revision=2
        if (sub,verb)==('refresh.picker','refresh'):revision=3
        if (sub,verb)==('reject.local','start'):revision=4
        if (sub,verb)==('reject.local','invalidate'):revision=5
        if (sub,verb)==('reject.remote','invalidate'):revision=6
        if (sub,verb)==('account.reset','replace'):account+=1;revision=1
        if verb=='catalog-start':revision=2
        validate_authority(action,op,je,account,revision)
    targets=terminal['targets'];subcases=[s for c in p['cycles'] for s in c['subcases']]
    require(all(integer(g) and g>0 for g in targets.values()),'Target generation type')
    require(set(targets)=={s['id'] for s in subcases}|{role+'/shutdown'},'Target inventory')
    require(len({fnv(k) for k in targets})==len(targets),'Target hash collision')
    bindings=[(e['related'],e['generation'],e['child']) for e in events if e['kind']==9]
    expected_bindings=[(fnv(k),g,c) for k,g in targets.items() for c in (1,2)]
    require(sorted(bindings)==sorted(expected_bindings),'Target binding repeated/missing')
    validate_target_authority(p,terminal,events,operations)
    cutoffs=[e for e in events if e['kind']==18]
    require(len(cutoffs)==21,'Cutoff inventory')
    for s,cut in zip(subcases,cutoffs):
        require((cut['related'],cut['generation'])==(fnv(s['id']),targets[s['id']]),'Cutoff target')
        final=next(i for i,x in enumerate(operations) if x['id']==s['actions'][-1])
        require(cut['at']>=starts[final]['at']+5_000_000_000,'Premature cutoff')
    remote=[e for e in events if e['kind']==19]
    require(len(remote)==3,'Synthetic remote response inventory')
    rejects=[e for e in events if e['kind']==13]
    fault_ids=[targets[s['id']] for s in subcases if s['classification']=='fault']
    require(sorted(e['generation'] for e in rejects)==sorted(fault_ids),'Rejection inventory')
    for generation in fault_ids:
        require(sum(e['kind']==14 and e['generation']==generation for e in events)==1,'Fault hold missing')
        require(sum(e['kind']==15 and e['generation']==generation for e in events)==1,'Fault release missing')
    require(all(e['related']==0 for e in events if e['kind']==15),'Failed hold release')
    shutdown=targets[role+'/shutdown'];close_start=starts[-1]
    if role.startswith('W'):
        waiting=[e for e in events if e['kind'] in (16,23) and e['at']<=close_start['at']]
        query=next(a for a in actions if a['id']==role+'/setup/shutdown/query')
        require(bool(waiting) and waiting[-1]['kind']==16 and waiting[-1]['generation']==shutdown
                and waiting[-1]['related']==fnv(query['accepted']['owner']),'Current worker was not actually waiting at close')
    else:
        held=[e for e in events if e['kind']==14 and e['generation']==shutdown]
        released=[e for e in events if e['kind']==15 and e['generation']==shutdown]
        constructed=[e for e in events if e['kind']==1 and e['generation']==shutdown]
        require(len(held)==1 and held[0]['at']<=close_start['at'],'B output was not held before close')
        require({e['child'] for e in constructed}=={1,2} and all(e['at']<=held[0]['at'] for e in constructed),'B hold lacks actual constructed children')
        # Missing release is a censored shutdown, not invented success.
        require(len(released)<=1 and (not released or released[0]['at']>=close_start['at']),'Early/repeated B release')
    return p,starts

def validate_authority(action,op,events,account,revision):
    """Re-evaluate recorded authority, independently from driver success booleans."""
    ack=action['ack'];require(ack.get('ok') is True and ack['id']==op['id'] and ack['nativeComplete'] is True,'Native acknowledgement binding')
    require(integer(ack['generation']),'Native generation type')
    state=ack['state'];require(state['isolated'] is True and state['team']=='T1','Fixture isolation lost')
    require(integer(state['epoch']) and integer(state['catalog']['revision']) and integer(state['prefs']['revision']),'Native owner/revision integer types')
    ready=state['catalog'].get('ready')
    if ready is not None:
        require(integer(ready['generation']) and integer(ready['revision']) and type(ready['token']) is str,'Ready identity types')
    building=state['catalog'].get('building')
    if building is not None:require(integer(building['ticket']) and integer(building['revision']),'Building identity types')
    require(state['account']==f'emoji-retirement-{account}' and state['epoch']==account,'Native owner changed')
    require(state['catalog']['revision']==revision and state['prefs']['loaded'] is True,'Native revision/preferences changed')
    witnesses=action['witnesses'];require(len(witnesses)==3,'Accepted/two-RAF witnesses missing')
    sub=op.get('subcase','');verb=op['id'].split('/')[-1];field='thread' if sub=='completion.thread' else 'channel'
    field_id=field+'-composer';before=action['before'][field]
    query=verb=='query';empty=verb in ('account-clear','boot-ready')
    opened=verb in ('open','seed-picker-open') or (sub,verb)==('refresh.picker','refresh')
    picker_close=verb=='seed-picker-close' or (sub,verb)==('refresh.picker','close')
    require(action['after']['channel']==witnesses[-1]['channel'] and action['after']['thread']==witnesses[-1]['thread'],'Final witness changed')
    for w in witnesses:
        require(w['focused'] is True and w['visibility']=='visible','Unfocused/hidden endpoint')
        f=w[field]
        if (sub,verb)==('completion.thread','close'):
            require(w['thread'] is None and w['channel']['mount']==action['before']['channel']['mount'],'Thread close authority')
            continue
        require(f is not None and f['connected'] is True,'Missing endpoint field')
        require(all(type(f[k]) is str and f[k] for k in ('scope','mount')),'Field owner types')
        require(type(f['wire']) is list and len(f['wire'])==4 and type(f['wire'][0]) is str
                and all(integer(x) for x in f['wire'][1:3]) and f['wire'][3] in ('none','forward','backward'),'Wire identity types')
        if query or empty or opened or picker_close or verb=='dismiss' or sub=='refresh.closed':
            require(f['scope']==before['scope'] and f['mount']==before['mount'],'Same-mount operation changed authority')
        if query or empty:
            require(w['active']==field_id and f['wire']==([':ro',3,3,'forward'] if query else ['',0,0,'forward']),'Input wire/focus changed')
            q=[e for e in events if e['kind']=='query' and e['value']['field']==field_id and e['at']<=action['acceptedAt']]
            require(bool(q),'No actual query');q=q[-1]['value']
            require(integer(q['seq']),'Query sequence type')
            require(q['snapshot']==json.dumps(f['wire'],separators=(',',':')),'Actual full query snapshot changed')
            if query:
                a=[e for e in events if e['kind']=='accepted' and e['action']==op['id'] and e['value']['field']==field_id and e['value']['result'].get('kind')=='emoji_results' and e['value']['valid'] is True and e['at']<=action['acceptedAt']]
                require(bool(a),'No actual accepted emoji result');a=a[-1]['value'];r=a['result']
                require(integer(r['seq']) and integer(action['accepted']['seq']) and type(r['preferences']) is str,'Result identity types')
                require(a['owner']==q['owner'] and r['seq']==q['seq'] and a['snapshot']==q['snapshot'],'Accepted query/owner/sequence mismatch')
                require(r['owner']==a['owner'] and r['catalog']==f['catalog']==state['catalog']['ready']['token'] and str(r['preferences'])==f['preferences']==str(state['prefs']['revision']),'Accepted catalog/prefs owner mismatch')
                require(action['accepted']=={'owner':a['owner'],'seq':r['seq'],'catalog':r['catalog'],'at':a['at']},'Reported accepted identity differs')
                popup=w['popup'];require(popup is not None and popup['connected'] is True and popup['hidden'] is False,'Accepted popup unavailable')
                require(popup['rows']==[{'label':c['label'],'detail':c['detail']} for c in r['choices']],'Actual DOM differs from results')
                require(any(c['label']==':rocket:' or c['label'].endswith(' :rocket:') for c in popup['rows']),'Rocket missing')
            else:
                r=[e for e in events if e['kind']=='receive' and e['value']['field']==field_id and e['at']<=action['acceptedAt']]
                require(bool(r),'No empty result');r=r[-1]['value']
                require(integer(r['result']['seq']) and integer(r['seq']),'Empty result sequence types')
                require(r['owner']==q['owner'] and r['result']['seq']==q['seq'] and r['snapshot']==q['snapshot'] and not r['result'].get('choices'),'Empty handshake mismatch')
        if opened:
            picker=w['picker'];require(picker is not None and picker['ready']=='true' and w['active']=='emoji-query','Picker readiness/focus')
            require(picker['catalog']==f['catalog']==state['catalog']['ready']['token'],'Picker owner changed')
            rows=picker['rows'];names=[x['name'] for x in rows]
            require(len(rows)==120 and len(set(names))==120 and 'rocket' in names and all(x['disabled'] is False for x in rows),'Picker row readiness/inventory')
            require(action['accepted']=={'picker':True,'catalog':picker['catalog'],'names':names},'Picker accepted identity differs')
        if picker_close:
            require(w['picker'] is None and w['active']==field_id and f['wire']==before['wire'],'Picker close authority')
        if verb=='dismiss':
            require(w['popup'] and w['popup']['hidden'] is True and f['wire']==before['wire'],'Dismiss changed draft/popover')
        if verb in ('navigate','thread-channel','replace'):
            require(w['channel']['scope']==state['channelScope'] and w['channel']['mount']!=action['before']['channel']['mount'] and w['picker'] is None,'Navigation mount not replaced')
        if verb=='thread-open':
            require(w['thread'] and w['thread']['scope']==state['threadScope'] and w['thread']['wire'][0]=='' and w['active']=='thread-composer','Actual thread opening failed')
        if sub=='refresh.closed':
            require(w['picker'] is None and w['popup'] and w['popup']['hidden'] is True and f['catalog']==state['catalog']['ready']['token'] and f['catalog']!=before['catalog'],'Closed refresh endpoint')

def validate_target_authority(process,terminal,events,operations):
    actions=terminal['actions'];by_id={a['id']:a for a in actions};targets=terminal['targets']
    positions={op['id']:i+1 for i,op in enumerate(operations)}
    for cycle in process['cycles']:
        for subcase in cycle['subcases']:
            name=subcase['subcase'];key=subcase['id'];generation=targets[key]
            remote=name=='reject.remote'
            require(generation>>48==cycle['account_ordinal'] and generation&3==(2 if remote else 1),'Target account/branch differs')
            bind_verb='refresh' if name=='refresh.closed' else 'start' if name.startswith('reject.') else 'open' if name in ('refresh.picker','account.reset') else 'query'
            bind_id=key+'/'+bind_verb;action=by_id[bind_id];index=positions[bind_id]
            require(all(e['action']==index for e in events if e['kind']==9 and e['related']==fnv(key)),'Target binding operation differs')
            if name=='refresh.closed':
                previous=actions[index-2]['ack']['state']['catalog']['ready']
                require(integer(previous['generation']) and generation==previous['generation'],'Refresh target is not old ready allocation')
            elif name.startswith('reject.'):
                require(generation==action['ack']['generation'],'Fault target is not actual held output')
                require(by_id[key+'/release']['ack']['generation']==generation,'Released another output')
            else:
                current=action['ack']['state']['catalog']['ready']
                require(integer(current['generation']) and generation==current['generation'],'Target is not actual accepted ready allocation')
                require(action['accepted']['catalog']==current['token'],'Target token differs from actual accepted result')
            ticket=(generation&((1<<48)-1))>>2
            require(ticket>0,'Zero target ticket')
            if not name.startswith('reject.'):
                native=previous if name=='refresh.closed' else current
                require(native['token']==f'{ticket}/local','Target ticket/token mismatch')
    key=process['process']+'/shutdown';generation=targets[key]
    require(generation>>48==3 and generation&3==1,'Shutdown generation owner/branch')
    verb='catalog-start' if process['process'].startswith('B') else 'query'
    action=by_id[process['process']+'/setup/shutdown/'+verb]
    if verb=='catalog-start':require(generation==action['ack']['generation'],'B target differs from actual held output')
    else:require(generation==action['ack']['state']['catalog']['ready']['generation'],'W target differs from accepted query')

def samples(schedule,terminal,trace):
    p,starts=validate_actions(schedule,terminal,trace);events=trace['events']
    operations=[x for x in p['ordered_actions'] if x['kind']!='cutoff']
    children={}
    for e in events:
        if e['kind'] in (1,2,3,4,5):children.setdefault((e['generation'],e['child']),[]).append(e)
    result=[]
    for cycle in p['cycles']:
        for s in cycle['subcases']:
            generation=terminal['targets'][s['id']]
            first=next(i for i,x in enumerate(operations) if x['id']==s['actions'][0])
            final=next(i for i,x in enumerate(operations) if x['id']==s['actions'][-1])
            lower=starts[first]['at'];upper=starts[final]['at']+5_000_000_000
            for child in (1,2):
                chain=children.get((generation,child),[])
                require(bool(chain),'Target child never constructed')
                status='missing-terminal'
                if len(chain)==5:
                    status='completed' if lower<=chain[1]['at'] and chain[-1]['at']<=upper else 'outside-cutoff'
                    if status=='completed' and chain[1]['context']==0:status='unknown-context'
                row=dict(process=p['process'],cycle=cycle['cycle'],subcase=s['subcase'],classification=s['classification'],generation=generation,child=child,status=status,lower_ns=lower,cutoff_ns=upper)
                if len(chain)==5:
                    row.update(context=chain[1]['context'],thread=chain[1]['thread'],role=chain[1]['role'],write=chain[1]['write'],
                               outer_ns=chain[4]['at']-chain[1]['at'],payload_ns=chain[3]['at']-chain[2]['at'])
                    row['serial_ns']=row['outer_ns'] if row['role']==1 else 0
                result.append(row)
    # Every started producer owes both children (or an explicit failed process).
    # A shutdown before construction is not a magically empty inventory.
    created=set(children)|{(e['generation'],child) for e in events if e['kind']==10 for child in (1,2)}
    complete={key for key,chain in children.items() if len(chain)==5}
    close=[e for e in events if e['kind']==22]
    require(len(close)<=1,'Repeated actual CloseRequested')
    shutdown_deadline=close[0]['at']+5_000_000_000 if close else None
    terminal_complete={key for key in complete if shutdown_deadline is not None and children[key][-1]['at']<=shutdown_deadline}
    return {'rows':result,'shutdown':{'created':[list(k) for k in sorted(created)],
        'missing':[list(k) for k in sorted(created-terminal_complete)],'complete':bool(close) and created==terminal_complete,
        'native_close_ns':close[0]['at'] if close else None,'cutoff_ns':shutdown_deadline,
        'missing_close_handler':not bool(close)}}

def aggregate(receipts):
    require([r['process'] for r in receipts]==list(ROLES),'Six fixed processes required')
    rows=[x for r in receipts for x in r['analysis']['rows']]
    require(len(rows)==252,'All 126 ordinary/fault child pairs required')
    groups=[]
    keys=sorted({(x['subcase'],x['child'],x.get('context',0)) for x in rows if x['classification']=='ordinary'})
    for subcase,child,context in keys:
        selected=[x for x in rows if (x['subcase'],x['child'],x.get('context',0))==(subcase,child,context)]
        groups_by_process={role:[x for x in selected if x['process']==role] for role in ROLES}
        eligible=context!=0 and all(len(v)==3 and {x['cycle'] for x in v}=={1,2,3}
            and all(x['status']=='completed' for x in v) for v in groups_by_process.values())
        group=dict(subcase=subcase,child=child,context=context,eligible=eligible)
        if eligible:
            medians=[statistics.median(x['serial_ns'] for x in groups_by_process[role]) for role in ROLES]
            serial_processes=sum(any(x['role']==1 and x['payload_ns']>0 for x in v) for v in groups_by_process.values())
            group.update(process_medians_ns=medians,median_ns=statistics.median(medians),maximum_ns=max(x['serial_ns'] for x in selected),serial_processes=serial_processes)
            group['screen']=group['median_ns']>=5_000_000 and serial_processes>=2
        else:group['screen']=False
        groups.append(group)
    return {'groups':groups,'ordinary_screen':any(g['screen'] for g in groups),
        'shutdown_complete':all(r['analysis']['shutdown']['complete'] for r in receipts),
        'limitations':['Synthetic diagnostic lifecycle; not natural production timing.',
            'Unknown, late and missing terminal releases are censored, never zero.',
            'No retirement implementation or recommendation follows automatically.']}
