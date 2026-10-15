"""Fail-closed cold/refresh/controlled-reader receipt protocol; no native launches."""
from pathlib import Path
import json,math,re,runpy,sys
ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file())
SCRIPTS=ROOT/'scripts';sys.path.insert(0,str(SCRIPTS))
U=runpy.run_path(str(SCRIPTS/'emoji_native_receipts.py'))
for key in ('require','same','proof','number','binary_sha','png_signature','validate_exit','normalize','validate_pointer_cleanup','closed_receipts','publish_ready','validate_ready','expected_viewport','NativeFixture','DEFAULT_BINARY'):
    globals()[key]=U[key]
pointer_cleanup=runpy.run_path(str(SCRIPTS/'canvas_evidence.py'))['pointer_cleanup']
FIXTURES=tuple('emoji-lifecycle-'+p+'-64000'+suffix for p in ('colon','picker') for suffix in ('','-narrow'))
FLAG='SUPER_PLATINUM_EMOJI_LIFECYCLE'
PREFIX,WAITING,STAGE='emoji-lifecycle-check: ','emoji-lifecycle-waiting: ','emoji-lifecycle-capture: '
PROGRESS='emoji-lifecycle-progress: '
STAGES=('initial','cold-ready','refresh-ready','retired')
INVENTORY=json.loads(Path(__file__).with_name('emoji_lifecycle_inventory.json' if Path(__file__).resolve().parent==SCRIPTS else 'inventory.json').read_text(encoding='utf-8'))
def contract(fixture):
    require(fixture in FIXTURES,'Unknown exact cold fixture')
    require(Path(__file__).resolve().parent==SCRIPTS,'Staged cold cohort cannot launch before promotion')
def ready_tuple(fixture,nonce,pid):
    require(fixture in FIXTURES and isinstance(nonce,str) and re.fullmatch('[0-9a-f]{32}',nonce) and type(pid) is int and pid>0,'Invalid exact owner')
    return dict(version=1,fixture=fixture,nonce=nonce,pid=pid)
def finite(x):return type(x) in (int,float) and math.isfinite(x) and x>=0
# Only independently transported millisecond arithmetic uses this tolerance.
# Four ULPs per operand/result cover the observed decimal/f64 conversion residue;
# the 1e-9 ms cap refuses large-clock cancellation from widening acceptance.
ARITHMETIC_ABSOLUTE_CAP_MS = 1e-9

def endpoint_arithmetic(reported, start, end, label):
    require(all(finite(v) for v in (reported,start,end)) and end>=start,label+' numeric domain')
    expected=end-start
    bound=min(ARITHMETIC_ABSOLUTE_CAP_MS,
              4*(math.ulp(start)+math.ulp(end)+math.ulp(reported)+math.ulp(expected)))
    require(abs(reported-expected)<=bound,label)

def validate_js(js,path):
    same(js['ok'],True,'Missing JS evidence')
    e=js['evidence'];same(e['errors'],[],'JS failed attempts cannot be excluded');same(e['overflow'],False,'JS overflow');same(e['done'],True,'Unfinished action')
    events=e['events'];require(isinstance(events,list) and 1<=len(events)<=256,'Invalid JS ledger')
    last=-1
    for i,event in enumerate(events):
        same(event['index'],i,'JS sequence gap');require(finite(event['at']) and event['at']>=last,'JS event clock went backwards');last=event['at']
        require(event['phase'] in ('setup','cold','refresh'),'Unknown JS phase')
        require(event['kind'] in ('query','receive','invalidate','accepted','armed','input','click','picker-ready','refresh-request','complete'),'Unexpected JS event')
    samples=e['samples'];same(len(samples),2,'Missing cold/refresh samples')
    for i,s in enumerate(samples):
        phase=('cold','refresh')[i];same(s['phase'],phase,'Wrong sample order');same(s['path'],path,'Wrong entry path')
        times=[s[k] for k in ('startMs','acceptedMs','raf1Ms','raf2Ms')]
        require(all(finite(v) for v in times) and times==sorted(times) and times[-1]-times[0]<15000,'Invalid sample clocks')
        endpoint_arithmetic(s['acceptedLatencyMs'],times[0],times[1],'Accepted endpoint arithmetic');endpoint_arithmetic(s['raf2LatencyMs'],times[0],times[3],'RAF arithmetic')
        same(s['inputTrusted'],False if phase=='cold' else None,'Wrong synthetic action kind')
        for key in ('scope','mount','preferences'):require(isinstance(s[key],str) and s[key],'Missing field authority')
        same(s['wire'],json.dumps([':ro' if path=='colon' else '',3 if path=='colon' else 0,3 if path=='colon' else 0,'forward'],separators=(',',':')),'Wire snapshot changed')
        same(s['result']['catalog'],f'{i+1}/local','Wrong accepted catalog')
        pe=[x for x in events if x['phase']==phase]
        starts=[x for x in pe if x['kind']==('refresh-request' if phase=='refresh' else 'input' if path=='colon' else 'click')]
        same(len(starts),1,'Missing/repeated action start');same(starts[0]['at'],s['startMs'],'Action clock binding')
        completed=[x for x in pe if x['kind']=='complete']
        same(len(completed),1,'Missing/repeated completion');same(completed[0]['at'],s['raf2Ms'],'Completion clock binding')
        if path=='colon':
            require(isinstance(s['owner'],str) and s['owner'].startswith(s['scope']+':mount:'),'Missing native controller owner')
            q=s['queries'];require(len(q)>=2 and all(type(v) is int and v>=0 for v in q) and q==sorted(set(q)),'Missing monotonic loading/requery chain')
            inv=s['invalidations'];require(len(inv)>=1 and all(x['result']['owner']==s['owner'] for x in inv),'Unlinked invalidation')
            same(s['result']['seq'],q[-1],'Accepted a nonlatest sequence')
            same(s['result']['kind'],'emoji_results','Accepted loading as result');same(s['result']['owner'],s['owner'],'Result owner')
            same(str(s['result']['preferences']),s['preferences'],'Preference revision changed')
            pe=[x for x in events if x['phase']==phase]
            queries=[x['value']['seq'] for x in pe if x['kind']=='query'];same(queries,q,'Query ledger/sample differ')
            accepted=[x for x in pe if x['kind']=='accepted'];require(any(x['value']['result'].get('kind')!='emoji_results' and x['value']['result']['choices']==[] for x in accepted),'Cold loading boundary missing')
            finals=[x for x in accepted if x['value']['result'].get('kind')=='emoji_results'];same(len(finals),1,'Repeated ready endpoint');same(finals[0]['value']['result'],s['result'],'Accepted DOM payload changed');same(finals[0]['at'],s['acceptedMs'],'Accepted clock binding')
        else:
            ready=[x for x in pe if x['kind']=='picker-ready']
            same(len(ready),1,'Missing/repeated picker ready');same(ready[0]['value'],s['result'],'Picker ready identity');same(ready[0]['at'],s['acceptedMs'],'Picker clock binding')
            names=s['result']['names'];require(len(names)==120 and len(set(names))==120 and 'rocket' in names,'Wrong ready picker rows')
    for key in ('scope','mount','preferences','wire'):same(samples[0][key],samples[1][key],'Cold/refresh field owner changed')
    return samples
def validate_rust(rust,samples,path,owner=None):
    same(rust['overflow'],False,'Rust overflow');events=rust['events'];require(isinstance(events,list) and len(events)<=256,'Rust ledger bound')
    allowed={'snapshot-decode','snapshot-buffer-release','local-worker-total','ensure','capture','build-and-picker','publish','displaced-ready-release','worker-returned','worker-present','canonical-refresh','controlled-reader-counts','controlled-final-reader-release'}
    for i,e in enumerate(events):
        require(e['kind'] in allowed,'Unexpected Rust event kind')
        same(e['seq'],i,'Rust sequence gap');require(e['phase'] in ('cold','refresh','retirement'),'Prewarmed catalog event')
        if 'milliseconds' in e['value']:require(finite(e['value']['milliseconds']),'Nonfinite/negative Rust span')
    for kind in ('capture','build-and-picker','publish','displaced-ready-release'):
        selected=[e for e in events if e['kind']==kind];same(len(selected),2,'Missing/repeated lifecycle '+kind)
        same([e['value']['ticket'] for e in selected],[1,2],'Lifecycle ticket order')
        same([e['phase'] for e in selected],['cold','refresh'],'Lifecycle phase order')
        for e in selected:require(finite(e['value']['milliseconds']),'Missing lifecycle duration')
    require(events and events[0]['kind']=='ensure','First actual catalog request absent')
    for key in ('serial','ready','building','attempted'):same(events[0]['value'][key],0,'First ensure was not cold')
    captures=[e['value'] for e in events if e['kind']=='capture']
    for i,e in enumerate(captures):
        same(e['count'],64000,'Canonical inventory changed');same(e['revision'],i+1,'Capture revision');same(e['account'],'emoji-lifecycle-disconnected','Capture account');same(e['team'],'T1','Capture team')
    for ticket in (1,2):
        positions=[next(e['seq'] for e in events if e['kind']==k and e['value'].get('ticket')==ticket) for k in ('capture','build-and-picker','publish','displaced-ready-release')]
        require(positions==sorted(positions),'Build/publication order')
    for e in events:
        if e['kind']=='build-and-picker':same(e['value']['ok'],True,'Catalog build failed')
    refresh=[e for e in events if e['kind']=='canonical-refresh'];same(len(refresh),1,'Missing/repeated canonical refresh')
    same(refresh[0]['value'],{'revision':2,'changed':['workspace_release_train_000000_celebration_custom_asset','workspace_release_train_000003_celebration_custom_asset']},'Refresh did not change exact metadata')
    cap2=next(e['seq'] for e in events if e['kind']=='capture' and e['value']['ticket']==2)
    pub1=next(e['seq'] for e in events if e['kind']=='publish' and e['value']['ticket']==1)
    require(pub1<refresh[0]['seq']<cap2,'Canonical refresh order')
    counts=[e for e in events if e['kind']=='controlled-reader-counts'];same(len(counts),1,'Missing/repeated observed reader count')
    same(counts[0]['value'],dict(loaded=1,catalog=1,picker=1),'Observed readers were not final')
    same(next(e['value']['counts'] for e in events if e['kind']=='displaced-ready-release' and e['value']['ticket']==1),None,'Cold replaced ready entry')
    final=[e for e in events if e['kind']=='controlled-final-reader-release'];same(len(final),1,'Missing controlled final release')
    v=final[0]['value'];same(v['countsBefore'],dict(loaded=1,catalog=1,picker=1),'Final reader not sole owner');same(v['weakDead'],True,'Final reader survived');same(v['includesPicker'],True,'Missing picker destruction');require(finite(v['milliseconds']),'Missing final drop span')
    require(counts[0]['seq']<final[0]['seq'],'Final release preceded count observation')
    same(len([e for e in events if e['kind']=='worker-returned']),2 if path=='colon' else 0,'Unexpected worker result retained')
    same(len([e for e in events if e['kind']=='worker-present']),2 if path=='colon' else 0,'Unexpected worker presentation retained')
    if path=='colon':
        for s in samples:
            matches=[e['value'] for e in events if e['kind']=='worker-returned' and e['value']['catalog']==s['result']['catalog'] and e['value']['seq']==s['result']['seq']]
            same(len(matches),1,'Accepted rows not traced to actual worker');same(matches[0]['choices'],s['result']['choices'],'DOM/controller rows differ from actual ranked worker')
            m=matches[0];same(m['owner'],s['owner'],'Worker/controller owner mismatch');same(str(m['preferences']),s['preferences'],'Worker/controller preference mismatch')
            wire=json.loads(s['wire']);same(m['snapshot'],dict(text=wire[0],anchor=wire[1],focus=wire[2],direction=wire[3]),'Worker/controller full snapshot mismatch')
            require(any(e['kind']=='worker-present' and e['value']['seq']==s['result']['seq'] and e['value']['catalog']==s['result']['catalog'] for e in events),'Worker failed final ownership guard')
    same(rust['inventory'],INVENTORY,'Exact original corpus inventory changed')
    validate_snapshot_spans(rust)
def validate_native(fixture,nonce,pid,directory,r,edges,expected_dimensions=None):
    same(r['ok'],True,'Native failed attempt retained');same(r['version'],1,'Protocol');same(r['fixture'],fixture,'Fixture');same(r['nonce'],nonce,'Nonce');same(r['pid'],pid,'PID')
    path='colon' if '-colon-' in fixture else 'picker';same(r['path'],path,'Path')
    for key in ('noSession','noTransport','noCache','controlledFinalReader','syntheticInput'):same(r[key],True,'Missing isolation/proof '+key)
    for key in ('serial','ready','building','attempted'):same(r['coldState'][key],0,'Catalog prewarmed')
    samples=validate_js(r['js'],path);validate_rust(r['rust'],samples,path,r['owner'])
    o=r['owner'];same(o['account'],'emoji-lifecycle-disconnected','Wrong native account');same(o['team'],'T1','Wrong native team');same(o['channel'],'C2','Wrong destination')
    same(o['preferences']['loaded'],True,'Preferences not explicitly loaded');same(o['preferences']['raw'],{},'Unexpected preferences')
    for sample in samples:
        same(sample['scope'],o['scope'],'JS/Rust scope mismatch');same(sample['mount'],o['mount'],'JS/Rust mount mismatch');same(sample['preferences'],str(o['preferences']['revision']),'JS/Rust preferences mismatch')
    for e in r['rust']['events']:
        if e['kind']=='capture':
            for key in ('account','epoch','team'):same(e['value'][key],o[key],'Capture owner differs: '+key)
    same(r['cold']['evidence']['samples'],samples[:1],'Cold endpoint rewritten');same(r['refresh']['evidence']['samples'],samples,'Refresh endpoint rewritten')
    same(edges,[dict(stage=s,edge=e) for s in STAGES for e in ('before','after')],'Capture ordering')
    same([c['stage'] for c in r['captures']],list(STAGES),'Capture stages');dimensions={};viewport=expected_viewport(fixture)
    for capture in r['captures']:
        p=Path(capture['path']);require(p.resolve()==directory.resolve()/(capture['stage']+'.png') and p.is_file() and not p.is_symlink(),'PNG path ownership')
        dimensions[capture['stage']]=png_signature(p)
        if expected_dimensions is not None:same(dimensions[capture['stage']],expected_dimensions,'Owned PNG dimensions')
        g=capture['geometry'];same(g['ok'],True,'Geometry failed');same(g['viewport'],viewport,'Viewport changed');require(0<g['documentWidth']<=viewport[0]+1,'Document overflow')
        for p in g['controls'].values():proof(p,viewport)
        same(capture['owner'],r['owner'],'Capture owner changed')
    return dimensions
def validate_closed_log(log,expected,directory,native,expected_dimensions=None):
    same(closed_receipts(log,WAITING),[expected],'Closed readiness log')
    same(closed_receipts(log,PREFIX),[native],'Closed terminal log')
    progress=closed_receipts(log,PROGRESS);same([p['phase'] for p in progress],['cold','refresh'],'Closed progress ledger')
    for p,kind in zip(progress,('cold','refresh')):
        for key in ('fixture','nonce','pid'):same(p[key],expected[key],'Partial ledger identity changed')
        same(p['js'],native[kind],'Partial endpoint rewritten')
    return validate_native(expected['fixture'],expected['nonce'],expected['pid'],directory,native,closed_receipts(log,STAGE),expected_dimensions)

SNAPSHOT_KINDS=('snapshot-decode','snapshot-buffer-release','local-worker-total')
def validate_snapshot_spans(rust,expected_arm=None):
    events=rust['events']
    modes=[]
    for kind in SNAPSHOT_KINDS:
        selected=[e for e in events if e['kind']==kind]
        same(len(selected),2,'Missing/repeated snapshot phase '+kind)
        same([e['phase'] for e in selected],['cold','refresh'],'Snapshot phase order')
        for ticket,e in enumerate(selected,1):
            v=e['value'];keys={'ticket','revision','mode','milliseconds'}
            if kind!='snapshot-buffer-release':keys.add('ok');same(v.get('ok'),True,'Snapshot worker failed')
            same(set(v),keys,'Snapshot span key inventory')
            require(type(v['ticket']) is int and v['ticket']==ticket and type(v['revision']) is int and v['revision']==ticket,'Snapshot integer identity')
            require(v['mode'] in ('current','packed'),'Snapshot mode')
            require(finite(v['milliseconds']),'Snapshot finite span')
            modes.append(v['mode'])
    require(len(set(modes))==1,'Mixed snapshot modes')
    if expected_arm is not None:same(modes[0],expected_arm,'Snapshot mode differs from sealed binary arm')
    for ticket in (1,2):
        kinds=('capture','snapshot-decode','snapshot-buffer-release','build-and-picker','local-worker-total','publish','displaced-ready-release')
        selected=[next(e for e in events if e['kind']==kind and e['value'].get('ticket')==ticket) for kind in kinds]
        require([e['seq'] for e in selected]==sorted(e['seq'] for e in selected),'Snapshot execution order')
        total=selected[4]['value']['milliseconds']
        require(all(total>=e['value']['milliseconds'] for e in selected[1:4]),'Inclusive worker shorter than an inner phase')
    return modes[0]
