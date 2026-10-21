"""Closed original-row workflow details acceptance. No Start/run/authorization claim."""
import copy
import math
from pathlib import Path
import workflow_management_channel_workflow_details_ledger as L
from workflow_management_channel_workflow_details_ledger import STAGES
import workflow_management_channel_workflows as W
from workflow_management_delete import exact,require,integer
F=W.F;A=W.A

ABSENT={'browse','closed','closed-b','closed-info','late-info','closed-get','late-get','channel-successor','late-b','successor','late-settled','closed-final','returned'}
LOADING={'info-held','get-held','a-held','b-held','held'}
FAILED={'denied','not-found','failed','rate-limited'}
def blocked(case,stage):return case=='first-details-rate-limit' and stage in ('rate-limited','closed','reopened-blocked')
def stage_trigger(case,stage):return L.B if stage in ('details-b','b-ready','late-a','b-held') else L.A
def expected_status(stage):return 'loading' if stage in LOADING else 'failed' if stage in FAILED else 'idle' if stage in ('reopened-blocked','expired') else 'ready'
def quiet(q,observation):
    require(isinstance(q,dict) and q.get('observedMs')==500 and exact(q.get('before'),observation) and exact(q.get('after'),observation),'Actual bounded quiet changed state or replayed')
def original_owner(owner,ledger,destination,request_count=2):
    require(isinstance(owner,dict),'Original browse owner absent')
    integer(owner.get('id'),'Original browse identity',1)
    integer(owner.get('socket'),'Original captured socket generation',0)
    require(owner.get('account')==F.OWNER['account'] and owner.get('accountEpoch')==F.OWNER['epoch'] and owner.get('team')=='TWORKFLOW' and owner.get('channel')==A.DEST and owner.get('channelGeneration')==destination.get('channelGeneration'),'Original browse source scope changed')
    expected=dict(account=owner['account'],epoch=owner['accountEpoch'],team=owner['team'],channel=owner['channel'],channelGeneration=owner['channelGeneration'],tab='Workflows',socket=owner['socket'])
    require(len(ledger['requests'])==request_count and all(exact(r.get('owner'),expected) for r in ledger['requests'][-2:]),'Browse owner differs from independently retained original request owners')
def calculated_geometry(actual,expected):
    # JS Number serializes integral values without .0; serialized coordinates
    # can also round one ULP before Python recomputes subtraction/addition.
    # This bound applies ONLY to independently derived geometric arithmetic,
    # not retained observations, source identities, models, or raw receipts.
    if type(expected) in (int,float):
        return type(actual) in (int,float) and math.isfinite(actual) and math.isfinite(expected) and abs(actual-expected)<=1e-9
    if isinstance(expected,dict):
        return isinstance(actual,dict) and set(actual)==set(expected) and all(calculated_geometry(actual[k],v) for k,v in expected.items())
    if isinstance(expected,list):
        return isinstance(actual,list) and len(actual)==len(expected) and all(calculated_geometry(a,b) for a,b in zip(actual,expected))
    return type(actual) is type(expected) and actual==expected

def exposure_proof(value,viewport,allow_clear=False):
    r=value.get('rect',{});x=value.get('exposure')
    require(isinstance(x,dict),'Actual background exposure absent')
    def box(left,top,right,bottom):return dict(left=left,top=top,right=right,bottom=bottom,width=max(0,right-left),height=max(0,bottom-top))
    v=box(max(0,r['left']),max(0,r['top']),min(viewport[0],r['right']),min(viewport[1],r['bottom']))
    require(calculated_geometry(x.get('viewport'),v),'Exposed viewport intersection changed')
    p=x.get('occluder');require(isinstance(p,dict) or allow_clear and p is None,'Actual details occluder absent')
    if p is not None:require(calculated_geometry(p,box(p['left'],p['top'],p['right'],p['bottom'])),'Invalid original pane rectangle')
    regions=[v] if v['width']>0 and v['height']>0 else []
    left=max(v['left'],p['left']) if p else 0;top=max(v['top'],p['top']) if p else 0;right=min(v['right'],p['right']) if p else 0;bottom=min(v['bottom'],p['bottom']) if p else 0
    if p is not None and right>left and bottom>top:
        regions=[b for b in [box(v['left'],v['top'],v['right'],top),box(v['left'],bottom,v['right'],v['bottom']),box(v['left'],top,left,bottom),box(right,top,v['right'],bottom)] if b['width']>0 and b['height']>0]
    require(calculated_geometry(x.get('regions'),regions),'Covered original region fabricated as exposed')
    hit=x.get('hitPoint');require(type(x.get('centerHit')) is bool and x.get('occluded') is (hit is None),'Actual hit/occlusion missing')
    require(value.get('connected') is True and value.get('visible') is (hit is not None),'Covered background visibility misreported')
    if x['centerHit']:
        cx=r['left']+r['width']/2;cy=r['top']+r['height']/2
        require(any(b['left']<=cx<=b['right'] and b['top']<=cy<=b['bottom'] for b in regions),'Covered center reported as hit')
    if hit is not None:
        require(isinstance(hit,dict) and set(hit)=={'x','y'} and any(calculated_geometry(hit,dict(x=b['left']+b['width']/2,y=b['top']+b['height']/2)) for b in regions),'Actual hitpoint not in exposed original region')
    return hit

def layout_proof(native,browse,viewport,rectangle):
    layout=native.get('detailsLayout');require(isinstance(layout,dict),'Actual pane/browse layout missing')
    if native.get('workflows') is not None:
        r=layout.get('browse');require(isinstance(r,dict) and r.get('width',0)>0 and r.get('height',0)>0,'Original browse layout absent')
    if native.get('details') is None:
        require(layout.get('pane') is None,'Detached details layout retained');return
    pane=layout.get('pane');require(isinstance(pane,dict),'Original details rectangle absent');rectangle(pane['rect'],viewport)
    require(isinstance(pane.get('position'),str),'Computed pane position absent')
    if viewport[0]<=780:
        require(pane['position']=='absolute','Narrow details squeezes original main pane')
        original=browse['detailsLayout']['browse'];actual=layout['browse']
        require(actual['width']==original['width'] and actual['left']==original['left'] and actual['right']==original['right'],'Narrow overlay changed underlying original browse width')

def successor_proof(case,stage,native,destination,owner,viewport,rectangle):
    successor=native.get('channelSuccessor')
    if stage not in ('channel-successor','late-b'):
        require(successor is None,'Unexpected channel successor outside actual sidebar transition');return
    require(case=='first-details-supersession' and isinstance(successor,dict),'Actual channel successor witness missing')
    require(destination.get('channel')==A.PRIVATE and destination.get('tab')=='Messages','Wrong actual successor channel/tab')
    integer(destination.get('channelGeneration'),'Actual successor channel generation',owner['channelGeneration']+1)
    require(destination.get('team')==owner['team'] and destination.get('account')==owner['account'] and destination.get('accountEpoch')==owner['accountEpoch'],'Sidebar successor changed original account/team')
    require(native.get('details') is None and native.get('workflows') is None and native.get('present') is False and native.get('modal') is None,'Old details/browse/management survived successor')
    require(successor.get('modalAbsent') is True and successor.get('name')=='private-review','Actual selected sidebar name/modal state')
    require(set(successor)=={'header','timeline','composer','active','name','modalAbsent'},'Unexpected successor evidence shape')
    ids={'header':'channel-details-trigger','timeline':'message-timeline','composer':'channel-composer','active':''}
    nodes=[]
    for key,identity in ids.items():
        node=successor.get(key,{});require(node.get('id')==identity and node.get('connected') is True,'Original successor '+key+' node absent')
        integer(node.get('nodeId'),'Original successor '+key+' identity',1);nodes.append(node['nodeId'])
    require(len(set(nodes))==4,'Successor reused unrelated node identity')
    header=successor['header'];require(header.get('text')=='#private-review' and header.get('visible') is True,'Actual successor header hidden/wrong')
    rectangle(header.get('rect'),viewport)
    custody=native.get('detailsCustody',{});require(custody.get('browse') is None and exact(custody.get('composer'),successor['composer']),'Actual successor composer differs from native custody')

def browse_proof(case,stage,native,destination,owner,viewport,rectangle,channels=None):
    tab=native.get('workflows');off=stage in ('channel-successor','late-b','successor','late-settled','returned')
    if off:require(tab is None,'Retired browse remained visible');return
    require(isinstance(tab,dict) and tab.get('panel',{}).get('connected') is True,'Original browse panel missing')
    p=tab.get('product',{});require(p.get('current') is True and p.get('status')=='ready' and p.get('serial')==1 and p.get('phase') is None,'Opening details changed browse acquisition')
    require(exact(p.get('owner'),owner),'Details changed original browse owner')
    require(destination.get('channel')==A.DEST and destination.get('tab')=='Workflows' and destination.get('channelGeneration')==owner['channelGeneration'],'Opening details navigated channel')
    require(all(exact(p.get('channelRaw',{}).get(k),v) for k,v in (channels or W.channels())[0].items()),'Original channel raw changed')
    require(exact(p.get('bookmarksRaw'),W.bookmarks(A.DEST)) and exact(p.get('previewsRaw'),W.previews()),'Browse original raw changed')
    f,b=W.rows();require(exact(p.get('featured'),f) and exact(p.get('bookmarked'),b),'Original collections changed')
    occluded=native.get('details') is not None or (native.get('detailsThread') or {}).get('pane',{}).get('displayed') is True
    require(p.get('reads')==[dict(serial=1,owner=owner['id'],method='bookmarks.list',raw=W.bookmarks(A.DEST)),dict(serial=1,owner=owner['id'],method='workflows.triggers.preview',raw=W.previews())],'Details triggered browse read')
    cool=blocked(case,stage);require(exact(p.get('cooldown'),dict(blocked=cool,retryAfterSeconds=12 if cool else None,indefinite=False)),'Shared cooldown lost')
    controls=tab.get('controls',[]);require(len(controls)==1 and controls[0].get('id')=='channel-workflows-refresh' and controls[0].get('disabled') is cool,'Underlying Refresh cooldown')
    require([e.get('text') for e in tab.get('feedback',[])]==(['Slack asked us to wait before refreshing workflows.'] if cool else []),'Underlying cooldown feedback')
    require(tab.get('tab',{}).get('pressed')=='true' and tab.get('empty')==[],'Actual selected nonempty tab')
    for section,group,rows in zip(tab.get('sections',[]),('featured','bookmarked'),(f,b)):
        require(section.get('kind')==group and section.get('role')=='list' and len(section.get('rows',[]))==len(rows),'Visible original collection')
        for index,(r,e) in enumerate(zip(section['rows'],rows)):
            available=e['kind']=='available'
            require(all(r.get(k)==e[v] for k,v in [('trigger','id'),('status','kind'),('title','title'),('description','description')]) and r.get('actions')==(1 if available else 0) and r.get('brokenImages')==0,'Original row metadata/actions')
            if available:require(r.get('opener',{}).get('id')==f'channel-workflow-open-{group}-{index}' and r['opener'].get('disabled') is False,'Exact available opener')
            else:require(r.get('opener') is None,'Private row granted opener')
            if occluded:exposure_proof(r,viewport)
            else:require(r.get('visible') is True,'Original row not measured visible')
            rectangle(r['rect'],viewport)
    require(len(tab.get('sections',[]))==2,'Original two collections missing')
    for c in controls+[tab['tab'],tab['header']]+tab['feedback']:
        if occluded:exposure_proof(c,viewport)
        else:require(c.get('visible') is True,'Original browse control not visible')
        rectangle(c['rect'],viewport)

def product_proof(case,stage,model,native,ledger,owner,viewport,rectangle,thread_root=None):
    pane=native.get('details');product=model.get('workflowDetails')
    if stage in ABSENT:
        require(pane is None,'Retired details pane still rendered')
        if stage in ('channel-successor','late-b','returned') and isinstance(product,dict):require(product.get('current') is False and product.get('status')=='retired','Retired selection still current')
        else:require(product is None,'Closed original details selection retained')
        return
    require(isinstance(pane,dict) and exact(pane.get('product'),product),'DOM/model details observation mismatch')
    require(pane.get('originalNodes') is True and pane.get('pane',{}).get('connected') is True and pane['pane'].get('ariaLabel')=='Workflow details' and pane['pane'].get('role') is None,'Not the original secondary pane')
    require(pane.get('brokenImages')==0,'Broken details image')
    trigger=stage_trigger(case,stage);L.selection(product.get('selection'),trigger,owner,thread_root)
    require(product.get('current') is True and product.get('status')==expected_status(stage),'Details status/currentness')
    cool=blocked(case,stage);require(exact(product.get('cooldown'),dict(blocked=cool,retryAfterSeconds=12 if cool else None,indefinite=False)),'Details cooldown')
    selected=product['selection']['id'];requests=[r for r in ledger['requests'] if r['owner']['id']==selected]
    idle=stage in ('reopened-blocked','expired');attempt=0 if idle else max(r['attempt'] for r in requests)
    require(product.get('serial')==attempt,'Details actual read cycle')
    responses={r['serial']:r['result'] for r in ledger['responses']};returned={r['serial']:r['returned'] for r in ledger['settlements']}
    reads=[];trigger_raw=None;workflow_raw=None
    for r in requests:
        if returned.get(r['serial']) is not True:continue
        result=responses[r['serial']];value=dict(serial=r['attempt'],method=r['method'])
        if 'raw' in result:value['raw']=result['raw']
        elif result['error']=='rate-limited':value.update(failure='rate-limited',retryAfterSeconds=12)
        else:value['failure']='read failed'
        reads.append(value)
        if r['attempt']==attempt and 'raw' in result:
            if r['method']==L.INFO:trigger_raw=result['raw']
            else:workflow_raw=result['raw']
    require(exact(product.get('reads'),reads),'Product original info/get receipts')
    require(exact(product.get('triggerRaw'),trigger_raw) and exact(product.get('workflowRaw'),workflow_raw),'Product raw result altered')
    status=expected_status(stage);require(product.get('phase')==('workflow' if stage in ('get-held','b-held') else 'trigger' if status=='loading' else None),'Actual details read phase')
    require(exact(product.get('projection'),L.projection(trigger) if status=='ready' else None),'Details metadata precedence/complete steps')
    controls=pane.get('controls',[]);expected=['channel-workflow-details-close']+(['channel-workflow-details-retry'] if status in ('failed','idle') else [])
    require([c.get('id') for c in controls]==expected,'Unexpected details action or execution control')
    for c in controls:
        require(c.get('disabled') is (cool and c['id'].endswith('-retry')),'Original Close/Retry disabled state')
    contents={c['id']:c.get('text') for c in pane.get('content',[])}
    if status=='ready':expected_contents={'channel-workflow-details-title':L.projection(trigger)['title'],'channel-workflow-details-description':L.projection(trigger)['description']}
    elif status=='loading':expected_contents={'channel-workflow-details-loading':'Loading workflow details\u2026'}
    elif status=='failed':
        title,message=('Permission denied','You don\u2019t have permissions to view workflow details.') if stage=='denied' else ('Workflow not found','This workflow no longer exists.') if stage=='not-found' else ('Something\u2019s gone wrong','Oops, there was an error loading this workflow.')
        expected_contents={'channel-workflow-details-error-title':title,'channel-workflow-details-error-message':message}
    else:expected_contents={}
    require(contents==expected_contents,'Actual error/metadata text')
    require([s.get('text') for s in pane.get('steps',[])]==(['Review the original request'] if status=='ready' else []),'Rendered actual workflow steps')
    require(pane.get('headings',[None])[0]=='Workflow','Source details heading')
    for c in controls+pane.get('content',[])+pane.get('feedback',[])+pane.get('steps',[]):
        require(c.get('connected') is True and c.get('visible') is True,'Details individual control/text visibility');rectangle(c['rect'],viewport)
    opener=pane.get('opener',{});require(opener.get('id')==f"channel-workflow-open-{'featured' if trigger==L.A else 'bookmarked'}-0" and opener.get('connected') is True,'Original available opener custody')
    group='featured' if trigger==L.A else 'bookmarked'
    original=next(s for s in native['workflows']['sections'] if s['kind']==group)['rows'][0]['opener']
    require(opener.get('nodeId')==original.get('nodeId'),'Details origin replaced original browse opener')

def captured_observation(actual,captured):
    if exact(actual,captured):return True
    # Two passive probes sample this same header around individual scrollIntoView
    # calls. Permit only the early horizontal sample to converge to the later
    # independently observed same-node header. No model/identity/other geometry
    # is discarded, and uncorroborated motion is rejected.
    a=actual.get('native',{});b=captured.get('native',{})
    if not a.get('details') or not b.get('details'):return False
    early=a.get('destination',{}).get('header',{});final=b.get('destination',{}).get('header',{})
    later=a.get('workflows',{}).get('header',{})
    if not exact(later,b.get('workflows',{}).get('header')):return False
    if any(early.get(k)!=later.get(k) or final.get(k)!=later.get(k) for k in ('id','nodeId','text')):return False
    er=early.get('rect',{});fr=final.get('rect',{});lr=later.get('rect',{})
    if any(er.get(k)!=lr.get(k) or fr.get(k)!=lr.get(k) for k in ('top','bottom','width','height')):return False
    if any(fr.get(k)!=lr.get(k) for k in ('left','right')):return False
    width=a.get('viewport',[0])[0]
    if not (er.get('width',0)>0 and 0<=er.get('left',-1)<er.get('right',-1)<=width+1):return False
    if abs((er['right']-er['left'])-er['width'])>0.001:return False
    changed=copy.deepcopy(actual)
    changed['native']['destination']['header']['rect'].update(left=fr['left'],right=fr['right'])
    return exact(changed,captured)

def exercise_proof(case,stage,e,observation,previous):
    if 'after' in e:require(captured_observation(e['after'],observation),'Details exercise after is not original capture')
    if 'quiet' in e:quiet(e['quiet'],observation)
    if stage=='browse':
        v=e.get('private',{});require(exact(v.get('before'),observation) and exact(v.get('after'),observation),'Private activation changed original state')
        require(v.get('activation',{}).get('ok') is True and v['activation'].get('actions')==0 and v['activation'].get('trigger')=='FtDenied01','Actual inert private activation absent');quiet(v.get('quiet'),observation)
        require(v['activation'].get('nodeId')==observation['native']['workflows']['sections'][0]['rows'][1].get('nodeId'),'Private activation lost original node')
        return
    if 'before' in e and stage!='returned':
        # Cancellation may settle between two observations; original request/raw is fixed.
        for k in ('model','firstPublish','native'):require(exact(e['before'][k],previous[k]),'Action lost prior original '+k)
        for k in ('requests','responses'):require(exact(e['before']['server']['channelWorkflows']['details'][k],previous['server']['channelWorkflows']['details'][k]),'Original request/raw changed before action')
    action={'details-a':'channel-workflow-open-featured-0','details-b':'channel-workflow-open-bookmarked-0','denied':'channel-workflow-open-featured-0','info-held':'channel-workflow-open-featured-0','get-held':'channel-workflow-open-featured-0','a-held':'channel-workflow-open-featured-0','b-ready':'channel-workflow-open-bookmarked-0','b-held':'channel-workflow-open-bookmarked-0','held':'channel-workflow-open-featured-0','not-found':'channel-workflow-details-retry','failed':'channel-workflow-details-retry','recovered':'channel-workflow-details-retry',**{k:'channel-workflow-details-close' for k in ('closed','closed-b','closed-info','closed-get','closed-final')}}.get(stage)
    if action is not None:
        require(e.get('action')==action,'Actual details action missing or wrong')
        before=e['before']['model'].get('workflowDetails');after=observation['model'].get('workflowDetails')
        if action.startswith('channel-workflow-open-'):
            activation=e.get('activation',{});tab=e['before']['native']['workflows']
            original=next((r['opener'] for group in tab['sections'] for r in group['rows'] if (r.get('opener') or {}).get('id')==action),None)
            require(original is not None and activation.get('ok') is True and activation.get('id')==action and activation.get('nodeId')==original['nodeId'],'Actual original exposed-button activation missing')
            require(exact(activation.get('owner'),tab['product']['owner']) and activation.get('serial')==tab['product']['serial'],'Activated row source owner changed')
            if activation.get('exposure',{}).get('occluder') is None:
                require(e['before']['native'].get('details') is None,'Original details occluder omitted')
            require(exposure_proof(dict(activation,connected=True,visible=True),observation['native']['viewport'],allow_clear=e['before']['native'].get('details') is None) is not None,'Covered original opener actuated')
            require(isinstance(after,dict) and (before is None or after['selection']['id']>before['selection']['id']),'Row click reused prior selection authority')
        elif action.endswith('-retry'):
            require(exact(after['selection'],before['selection']) and after['serial']==before['serial']+1,'Retry lost original row or replayed more than once')
    if 'release' in e:
        require(e['release'].get('receiverRetired') is True and e['release'].get('delivered') is False,'Original held receiver not retired')
        for k in ('model','firstPublish','native'):require(exact(e['before'][k],observation[k]),'Late reply changed successor '+k)
    if e.get('action')=='channel-workflow-details-close':
        require(observation['native'].get('detailsFocus')==e['before']['native']['details']['opener']['id'],'Close did not restore original opener focus')
        require(exact(e['before']['native']['detailsCustody'],observation['native']['detailsCustody']),'Close replaced underlying original nodes')
    if stage in ('rate-limited','reopened-blocked'):
        b=e.get('blocked',{});require(exact(b.get('before'),observation) and exact(b.get('after'),observation),'Disabled Retry changed state');quiet(b.get('quiet'),observation)
        a=b.get('activation',{});control=observation['native']['details']['controls'][1]
        require(a.get('ok') is True and a.get('disabled') is True and a.get('id')==control['id'] and a.get('nodeId')==control['nodeId'],'Actual disabled Retry identity')
        require(exact(e.get('opening',{}).get('before'),previous) and exact(e['opening'].get('after'),observation),'Actual cooldown row opening')
        require(e['opening'].get('action')=='channel-workflow-open-featured-0','Cooldown direct model action substituted')
    if stage=='expired':
        integer(e.get('waitedMs'),'Bounded actual expiry wait',1);require(e['waitedMs']<=21000,'Unbounded cooldown wait')
        require(exact(e['before']['server'],observation['server']),'Expiry sent read or mutation')
        require(observation['model']['workflowDetails']['serial']==0,'Expiry automatically admitted read')
    if stage not in ('channel-successor','late-b','successor','late-settled','returned'):
        require(exact(observation['native']['detailsCustody'],previous['native']['detailsCustody']),'Details changed underlying original browse/thread/composer nodes')
        require(exact(observation['model']['detailsCustody'],previous['model']['detailsCustody']),'Details changed secondary model custody')

def validate(fixture,case,receipt,captures,viewport=None,rectangle=None,validate_png=None,png=True):
    require(case in STAGES and receipt.get('ok') is True and receipt.get('firstPublishCase')==case and receipt.get('fixture')==fixture,'Exact details native case')
    threaded=receipt.get('threadCustody',False)
    require(type(threaded) is bool and (not threaded or case in ('first-details-success','first-details-errors','first-details-close-held')),'Invalid thread fixture mode')
    if threaded:
        import workflow_management_channel_workflow_thread as T
    channels=T.channels() if threaded else W.channels()
    thread_root=T.ROOT if threaded else None
    viewport=viewport or ([1024,640] if fixture.endswith('-narrow') else [1440,1000])
    require(receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Closed details harness')
    setup=receipt.get('setup');rows=receipt.get('stages')
    require(isinstance(setup,list) and tuple(r.get('stage') for r in setup)==L.SETUP,'Actual retained setup missing')
    require(isinstance(rows,list) and tuple(r.get('stage') for r in rows)==STAGES[case] and set(receipt.get('captures',{}))==set(STAGES[case]),'Exact details capture stages')
    parent=None;document=None;owner=None;browse=None;previous=None;parent_nodes=None;modal_nodes=None
    for row in setup+rows:
        stage=row['stage'];setup_stage=stage in L.SETUP;p=row['proof'];s=p['server'];product=p['firstPublish'];native=p['native'];m=row['model'];destination=m['confirmationDestination'];e=p['exercise'];observation=F.observation(row)
        retired=case=='first-details-account-retirement' and stage in ('successor','late-settled')
        A.seed_proof(s,channels);channel_rows={v['id']:v for v in channels}
        A.add_ledger('first-add-success',stage if setup_stage else 'added',s,channel_rows)
        phase=stage if stage in A.PREFIX else 'checked'
        if retired:
            require(s.get('ownerCurrent') is False and destination.get('account')=='00000000-0000-4000-8000-000000000099' and destination.get('accountEpoch')==F.OWNER['epoch']+1,'Actual account replacement missing')
            for k in ('original','current','requests','responses','settlements','releases','seed','confirmationAdd'):require(exact(s[k],browse['proof']['server'][k]),'Account replacement rewrote original '+k)
            require(exact(product['attempts'],browse['proof']['firstPublish']['attempts']) and product.get('visible') is False,'Retired publication receipt changed')
        elif stage=='returned':W.returned_server(s,previous['server'])
        else:F.server_proof('first-success',phase,s)
        if not retired:F.attempt_proof('first-success',phase,product,s,F.stage_state('first-success',phase))
        A.operation_proof('first-add-success',stage if setup_stage else 'added',product)
        require(native.get('ok') is True and native.get('noOverflow') is True and native.get('viewport')==viewport and native.get('composing') is False,'Original details native layout/capture')
        custody=native.get('detailsCustody');require(isinstance(custody,dict) and set(custody)=={'browse','thread','composer','threadComposer'},'Explicit underlying native custody absent')
        # The closed destination has no history producer. This pins honest absence,
        # not active-thread preservation (which remains outside native acceptance).
        if not threaded or setup_stage or stage=='returned':
            require(custody['thread'] is None and custody['threadComposer'] is None,'Unexpected synthetic mounted thread')
        model_custody=m.get('detailsCustody',{});require(model_custody.get('threadRoot')==(thread_root if not setup_stage and stage!='returned' else None) and all(model_custody.get(k) is None for k in ('profileUser','groupId')),'Unexpected details model secondary owner')
        integer(model_custody.get('threadGeneration'),'Actual thread generation',0)
        if setup_stage:
            if native.get('fields'):
                nodes=tuple(native['fields'][k]['nodeId'] for k in ('title','description'))
                if stage=='editor':parent_nodes=nodes
                require(nodes==parent_nodes,'Setup replaced original parent metadata controls')
            if native.get('modal'):
                modal=native['modal'];nodes=(modal['nodeId'],modal['fields']['title']['nodeId'],modal['fields']['description']['nodeId'],modal['upload']['nodeId'])
                if modal_nodes is None:modal_nodes=nodes
                require(nodes==modal_nodes,'Setup replaced original Finish Up fields/File node')
        if stage in A.PREFIX:
            F.native_proof('first-success',stage,native,m,viewport,rectangle);A.embedded_settings(stage,product);F.confirmation_proof('first-success',stage,product,native,m)
        A.native_add(stage if setup_stage else 'added',native,rectangle,viewport)
        successor_proof(case,stage,native,destination,owner,viewport,rectangle)
        if setup_stage:
            W.ledger('first-tab-success',stage,s['channelWorkflows'])
            require(native.get('workflows') is None and native.get('details') is None and m.get('workflowDetails') is None,'Details escaped real setup')
            require(exact(row['geometry'],native),'Setup geometry not actual native observation')
        else:
            if stage=='browse':
                browse=row;owner=native['workflows']['product']['owner'];W.ledger('first-tab-success','refreshed' if threaded else 'workflows',s['channelWorkflows']);original_owner(owner,s['channelWorkflows'],destination,4 if threaded else 2)
            else:
                for k in ('requests','responses','settlements','held','releases','networkCalls'):require(exact(s['channelWorkflows'][k],browse['proof']['server']['channelWorkflows'][k]),'Details changed browse ledger '+k)
                require(len(s['channelWorkflows']['about'])==(4 if stage=='returned' else 0),'Unexpected About reads')
            browse_proof(case,stage,native,destination,owner,viewport,rectangle,channels)
            layout_proof(native,browse['proof']['native'],viewport,rectangle)
            product_proof(case,stage,m,native,s['channelWorkflows']['details'],owner,viewport,rectangle,thread_root)
            exercise_proof(case,stage,e,observation,previous)
            require(exact(row['geometry'],dict(native,pngDimensions=row['geometry']['pngDimensions'])),'Geometry not exact native sample')
            path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Owned details capture path')
            if png:validate_png(path,row['geometry']['pngDimensions'])
        L.ledger(case,stage,s['channelWorkflows']['details'],owner,thread_root)
        if stage not in ('about','editor'):
            docs=[d for d in destination['documents'] if d['title']==F.TITLE and d['description']==F.DESCRIPTION];require(len(docs)==1,'Original dirty workflow draft absent')
            if parent is None:parent=copy.deepcopy(docs[0])
            document=W.document_proof(stage,docs[0],parent,document,product)
        if not setup_stage:
            require(m.get('writes')==setup[-1]['model']['writes'] and m.get('networkCalls')==0,'Details dispatched mutation/live call')
            if not retired:require(m.get('reads')==setup[-1]['model']['reads']+(12 if stage=='returned' else 0),'Details escaped dedicated read adapter')
        if stage=='returned':
            require(destination['channel']==A.ORIGINAL and destination['tab']=='AppAbout' and native['present'] is True and native['originalNodes'] is True,'Real About original draft return')
            require(all(e.get('remount',{}).get(k) is True for k in ('ok','originalDetached','newRoot','newTitle','newDescription')) and e['freshVisit']==m['visit'] and m['visit']!=setup[-1]['model']['visit'],'Fresh About owner not observed')
            require(native['fields']['title']['value']==F.TITLE and native['fields']['description']['value']==F.DESCRIPTION,'Returned Unicode native draft')
        previous=observation
    if case=='first-details-account-retirement':require(receipt.get('replacement',{}).get('foreignDocumentHidden') is True,'Account replacement proof absent')
    else:require(receipt.get('replacement') is None,'Unexpected account replacement')
    require(exact(receipt.get('final'),F.observation(rows[-1])),'Final details observation mismatch')
    if threaded:T.validate(receipt,viewport,rectangle)
    return receipt
