"""Independent Workflows browse oracle. Row invocation is outside this slice."""
import copy
from pathlib import Path
import workflow_management_confirmation_add as A
import workflow_management_first_publish as F
from workflow_management_delete import exact, require, integer

PREFIX=A.PREFIX+('selected',)
STAGES={
 'first-tab-success':PREFIX+('workflows','refreshed','returned'),
 'first-tab-empty':PREFIX+('workflows','empty','refreshed','returned'),
 'first-tab-failure':PREFIX+('workflows-failed','recovered','returned'),
 'first-tab-retirement':PREFIX+('workflows-held','switched','late-settled','returned'),
 'first-tab-rate-limit':PREFIX+('rate-limited','switched','reentered-blocked','expired','recovered','returned'),
}
IDS=['TR_REVIEW','FtDenied01','FtFolder01']
FEATURED=[dict(workflow_trigger_id='TR_REVIEW',title='Featured override unused'),dict(workflow_trigger_id='FtDenied01',title='Private featured \u4e2d')]
def channels(case=None):
    rows=A.channels()
    rows[0]['properties']=dict(channel_workflows=copy.deepcopy(FEATURED))
    rows[1]['properties']=dict(channel_workflows=[],tabz=[dict(id='workflows',type='workflows',label='')])
    if case=='first-tab-rate-limit':rows[0]['properties']['tabz']=[dict(id='workflows',type='workflows',label='')]
    return rows
CHANNELS={r['id']:r for r in channels()}
def bookmarks(channel):
    if channel==A.PRIVATE:return dict(ok=True,bookmarks=[],opaque_receipt='original-empty-workflows')
    return dict(ok=True,bookmarks=[A.bookmark(),
      dict(id='BkFolder01',channel_id=A.DEST,type='auto_gen_shortcut',shortcut_id='FtFolder01',title='Untrusted bookmark title',parent_id='BkParent01',rank='0'),
      dict(id='BkDenied01',channel_id=A.DEST,type='shortcut',shortcut_id='FtDenied01',title='Not private title authority',rank='b'),
      dict(id='BkDuplicate01',channel_id=A.DEST,type='shortcut',shortcut_id='TR_REVIEW',title='Duplicate later',rank='z'),
      dict(id='BkLegacy01',channel_id=A.DEST,type='legacy_workflow',shortcut_id='FtLegacy01',title='Legacy excluded',rank='-1'),
      dict(id='BkLink01',channel_id=A.DEST,type='link',link='https://example.test/ordinary',title='Ordinary excluded',rank='-2')],opaque_receipt='original-workflows-bookmarks')
def previews():
    return dict(ok=True,triggers=[
      dict(id='TR_REVIEW',type='shortcut',name='Published review \u4e2d',description='Actual preview description \u00e9',workflow=dict(workflow_id='WfREVIEW01',app_id='A_HOME'),opaque=dict(retained=[3,1])),
      dict(id='FtDenied01',error='access_denied',opaque='original-private'),
      dict(id='FtFolder01',type='shortcut',name='',description='',workflow=dict(workflow_id='WfFOLDER01',title='Folder workflow caf\u00e9',description='Actual workflow fallback'))],opaque_receipt='original-trigger-previews')
def rows():
    raw=previews()['triggers'];book=bookmarks(A.DEST)['bookmarks']
    def row(i,source,title,description):return dict(id=raw[i]['id'],kind='access-denied' if i==1 else 'available',title=title,description=description,source=source,previewRaw=raw[i])
    ready=lambda source:row(0,source,raw[0]['name'],raw[0]['description'])
    private=lambda source,title:row(1,source,title,'Only certain people can use this workflow')
    return ([ready(FEATURED[0]),private(FEATURED[1],FEATURED[1]['title'])],
      [row(2,book[1],'Folder workflow caf\u00e9','Actual workflow fallback'),ready(book[0]),private(book[2],'Private workflow')])
def schedule(case,stage):
    if stage in PREFIX:return []
    reads=[('bookmarks.list',A.DEST),('workflows.triggers.preview',A.DEST)]
    if case=='first-tab-empty' and stage in ('empty','refreshed','returned'):
        reads.append(('bookmarks.list',A.PRIVATE))
        if stage in ('refreshed','returned'):reads.append(('bookmarks.list',A.PRIVATE))
    elif case in ('first-tab-success','first-tab-failure','first-tab-rate-limit') and stage in ('refreshed','recovered','returned'):
        reads+=reads.copy()
    return reads
def fields(method,channel):
    return dict(channel=channel,include_folders='true',include_legacy_workflows='true',_x_reason='bookmarks-store/conditional-fetching') if method=='bookmarks.list' else dict(trigger_ids=','.join(IDS),_x_reason='hermes_trigger_previews_ufm')
def ledger(case,stage,v):
    require(isinstance(v,dict) and v.get('networkCalls')==0,'Missing closed tab ledger')
    req,res,settles,release=(v.get(k) for k in ('requests','responses','settlements','releases'))
    expected=schedule(case,stage)
    require(all(isinstance(a,list) for a in (req,res,settles,release)) and len(req)==len(res)==len(expected),'Tab request count')
    for n,((method,channel),r,p) in enumerate(zip(expected,req,res),1):
        require(r.get('serial')==p.get('serial')==n and r.get('method')==method,'Tab exact request identity')
        pairs=r.get('fields');require(isinstance(pairs,list) and all(isinstance(x,list) and len(x)==2 for x in pairs),'Tab body pairs')
        require(len(dict(pairs))==len(pairs) and dict(pairs)==fields(method,channel) and r.get('retrySafe') is (method=='bookmarks.list'),'Tab exact source factory')
        owner=r.get('owner',{});require(all(exact(owner.get(k),A.OWNER[k]) for k in A.OWNER) and owner.get('channel')==channel and owner.get('tab')=='Workflows','Tab original source owner')
        integer(owner.get('channelGeneration'),'Tab generation',1);integer(owner.get('socket'),'Tab socket',0)
        result=dict(error='offline') if case=='first-tab-failure' and n==2 else dict(raw=previews() if method=='workflows.triggers.preview' else bookmarks(channel))
        if case=='first-tab-rate-limit' and n==2:result=dict(error='rate-limited',retryAfterSeconds=12)
        require(exact(p.get('result'),result),'Original tab raw response')
    held=case=='first-tab-retirement' and stage in ('workflows-held','switched')
    retired=case=='first-tab-retirement' and stage in ('switched','late-settled','returned')
    require(v.get('held')==([dict(serial=2,receiverRetired=retired)] if held else []),'Actual held receiver state')
    expected_settles=[dict(serial=n,returned=not(retired and n==2)) for n in range(1,len(req)+1) if not(case=='first-tab-retirement' and stage=='workflows-held' and n==2)]
    require(sorted(settles,key=lambda r:r['serial'])==expected_settles,'Original tab settlement/cancellation')
    require(release==([dict(serial=2,receiverRetired=True,delivered=False)] if case=='first-tab-retirement' and stage in ('late-settled','returned') else []),'Original retired release')
    require(isinstance(v.get('about'),list) and len(v['about'])==(4 if stage=='returned' else 0),'Actual About return reads')
    return v
def product_proof(case,stage,native,destination,viewport,rectangle):
    tab=native.get('workflows')
    mounted=stage not in PREFIX and stage not in ('switched','late-settled','returned')
    if not mounted:require(tab is None,'Stale Workflows panel mounted');return
    require(isinstance(tab,dict) and tab.get('panel',{}).get('connected') is True,'Actual tab panel missing')
    p=tab.get('product',{});channel=A.PRIVATE if case=='first-tab-empty' and stage in ('empty','refreshed') else A.DEST
    idle=stage in ('reentered-blocked','expired')
    blocked=stage in ('rate-limited','reentered-blocked')
    status='idle' if idle else 'failed' if stage in ('workflows-failed','rate-limited') else 'loading' if stage=='workflows-held' else 'ready'
    require(p.get('current') is True and p.get('status')==status and p.get('phase')==('previews' if status=='loading' else None),'Actual tab acquisition state')
    require(exact(p.get('cooldown'),dict(blocked=blocked,retryAfterSeconds=12 if blocked else None,indefinite=False)),'Actual Retry-After state')
    require(p.get('serial')==(0 if idle else 2 if stage in ('refreshed','recovered') and case!='first-tab-rate-limit' else 1),'Actual explicit read cycle')
    owner=p.get('owner')
    if idle:require(owner is None,'Blocked remount admitted an owner')
    else:
        require(isinstance(owner,dict) and owner.get('account')==F.OWNER['account'] and owner.get('accountEpoch')==F.OWNER['epoch'] and owner.get('team')=='TWORKFLOW' and owner.get('channel')==channel and owner.get('channelGeneration')==destination.get('channelGeneration'),'Original product owner scope')
        integer(owner.get('id'),'Actual product owner',1);integer(owner.get('socket'),'Actual product socket',0)
    require(destination.get('channel')==channel and destination.get('tab')=='Workflows','Tab actual route')
    channel_rows={r['id']:r for r in channels(case)}
    require(p.get('channelRaw') is None if idle else isinstance(p.get('channelRaw'),dict) and all(exact(p['channelRaw'].get(k),v) for k,v in channel_rows[channel].items()),'Canonical tab source fields changed')
    require(exact(p.get('bookmarksRaw'),None if idle else bookmarks(channel)),'Actual bookmarks original response')
    require(exact(p.get('previewsRaw'),previews() if status=='ready' and channel==A.DEST else None),'Actual preview response')
    read_rows=p.get('reads');require(isinstance(read_rows,list),'Actual product acquisition receipts missing')
    expected_methods=['bookmarks.list'] if channel==A.PRIVATE else ['bookmarks.list','workflows.triggers.preview']
    if status=='loading':expected_methods=['bookmarks.list']
    expected_methods*=p['serial']
    require([r.get('method') for r in read_rows]==expected_methods,'Product exact acquisition sequence')
    for index,r in enumerate(read_rows):
        cycle=1+index//(1 if channel==A.PRIVATE else 2)
        require(r.get('serial')==cycle,'Product original read cycle')
        if cycle==p['serial']:require(r.get('owner')==owner['id'],'Product current original owner receipt')
        rate_failed=stage=='rate-limited' and index==1
        failed=(case=='first-tab-failure' and index==1) or rate_failed
        require(exact(r.get('raw'),None if failed else bookmarks(channel) if r['method']=='bookmarks.list' else previews()),'Product original acquired raw')
        require(r.get('failure')==('rate-limited' if rate_failed else 'read failed' if failed else None),'Product failed read provenance')
        require(r.get('retryAfterSeconds')==(12 if rate_failed else None),'Product original Retry-After receipt')
    f,b=rows() if status=='ready' and channel==A.DEST else ([],[])
    require(exact(p.get('featured'),f) and exact(p.get('bookmarked'),b),'Projected source collections/order/private/title drift')
    t=tab.get('tab',{});require(t.get('text')=='Workflows' and t.get('pressed')=='true' and t.get('visible') is True,'Actual selected Workflows tab')
    rectangle(t['rect'],viewport)
    controls=tab.get('controls',[]);require([c.get('id') for c in controls]==['channel-workflows-refresh']+(['channel-workflows-retry'] if status in ('failed','idle') else []),'Actual browse controls')
    for c in controls:require(c.get('visible') is True and c.get('disabled') is (status=='loading' or blocked),'Browse control state');rectangle(c['rect'],viewport)
    sections=tab.get('sections');require(isinstance(sections,list) and len(sections)==2,'Collection observations missing')
    if status!='ready':require(sections==[None,None],'Failed/held data painted ready')
    else:
        for section,kind,expected in zip(sections,['featured','bookmarked'],[f,b]):
            require(section.get('kind')==kind and section.get('role')=='list' and len(section.get('rows',[]))==len(expected),'Painted collection count/order')
            for index,(r,e) in enumerate(zip(section['rows'],expected)):
                require(all(r.get(k)==e[v] for k,v in [('trigger','id'),('status','kind'),('title','title'),('description','description')]) and r.get('visible') is True and r.get('actions')==(1 if e['kind']=='available' else 0) and r.get('brokenImages')==0,'Actual painted row metadata/action/icon')
                if e['kind']=='available':require(r.get('opener',{}).get('id')==f'channel-workflow-open-{kind}-{index}' and r['opener'].get('disabled') is False,'Exact original available workflow opener')
                else:require(r.get('opener') is None,'Private workflow gained an opener')
                rectangle(r['rect'],viewport)
        require([e.get('text') for e in tab.get('empty',[])]==(['No featured workflows in this channel.','No bookmarked workflows in this channel.'] if channel==A.PRIVATE else []),'Actual empty state')
    text=' '.join(e.get('text','') for e in tab.get('feedback',[]))
    require(('Slack asked us to wait before refreshing workflows.' in text) if blocked else ('Retry to load workflows.' in text) if idle else ('Workflows could not load. Retry to refresh this channel.' in text) if status=='failed' else ('Loading workflows\u2026' in text) if status=='loading' else not text,'Actual browse feedback')

def rate_exercise(stage,exercise,observation,previous):
    if stage in ('rate-limited','reentered-blocked'):
        b=exercise.get('blocked',{});a=b.get('activation',{});q=b.get('quiet',{})
        require(exact(b.get('before'),b.get('after')) and exact(b.get('after'),observation),'Disabled original controls changed state')
        require(a.get('ok') is True and [c.get('id') for c in a.get('controls',[])]==['channel-workflows-refresh','channel-workflows-retry'],'Original disabled control activation absent')
        actual=observation['native']['workflows']['controls']
        for control,seen in zip(a['controls'],actual):
            require(control.get('disabled') is True and control.get('nodeId')==seen.get('nodeId'),'Disabled activation replaced original control')
        require(q.get('observedMs')==500 and exact(q.get('before'),observation) and exact(q.get('after'),observation),'Disabled controls quiet replay')
    if stage in ('switched','reentered-blocked','expired'):
        require(exact(observation['server'],previous['server']),'Cooldown switch/remount/expiry sent a read')
    if stage=='expired':
        before=exercise['before'];after=exercise['after']
        require(exact(before,previous) and exact(after,observation),'Expiry lost original remounted owner')
        integer(exercise.get('waitedMs'),'Actual bounded expiry observation',1)
        require(exercise['waitedMs']<=20000,'Unbounded expiry wait')
        for key in ('model','firstPublish','server'):require(exact(before[key],after[key]),'Expiry changed '+key)
        old=copy.deepcopy(before['native']['workflows']['product']);old['cooldown']=dict(blocked=False,retryAfterSeconds=None,indefinite=False)
        require(exact(old,after['native']['workflows']['product']),'Expiry admitted owner or read')
    if stage=='recovered':require(exact(exercise['before'],previous),'Retry not admitted from actual expired panel')
def returned_server(server,before):
    for key in ('original','current','held','releases','owner','ownerCurrent','seed','confirmationAdd'):
        require(exact(server.get(key),before.get(key)),'Return changed existing source '+key)
    prefix=len(before['requests']);require(server['requests'][:prefix]==before['requests'] and server['responses'][:prefix]==before['responses'] and server['settlements'][:prefix]==before['settlements'],'Return rewrote prior ledger')
    req=server['requests'][prefix:];res=server['responses'][prefix:];expected=F.stage_state('first-success','editor')['reads']
    require([r['method'] for r in req]==expected,'Fresh Configuration exact read sequence')
    current=before['current']
    for n,(r,p) in enumerate(zip(req,res),prefix+1):
        require(r['serial']==p['serial']==n and r['phase']=='read' and r['applied'] is False and exact(r['owner'],A.OWNER),'Fresh return request owner')
        method=r['method'];F.read_wire(method,F.form(r))
        raw=(F.latest(current['workflow']) if method=='functions.workflows.getLatest' else dict(ok=True,triggers=[current['trigger']]) if method=='workflows.triggers.list' else dict(ok=True,trigger=current['trigger']) if method=='workflows.triggers.info' else current['copy'] if method=='functions.workflows.permissions.copy.list' else current['run'] if method=='workflows.triggers.permissions.list' else F.read_result(method)['raw'])
        require(exact(p['result'],dict(raw=raw)),'Fresh return raw response')
    require(server['settlements'][prefix:]==[dict(serial=n,returned=True) for n in range(prefix+1,prefix+len(req)+1)],'Fresh return settlement')
    about=server['channelWorkflows']['about']
    expected_about=[('conversations.info',dict(channel=A.ORIGINAL,return_app_home='true',_x_reason='app-home-open-api')),
      ('apps.about.info',dict(app_id='A_HOME',_x_reason='fetch-unknown-apps-about-info')),
      ('apps.profile.get',dict(app='A_HOME',_x_reason='fetch-all-apps-by-id')),
      ('experiments.getByUser',dict(ssb_platform='windows',ssb_release_channel='prod'))]
    for r,(method,fields_) in zip(about,expected_about):
        require(r.get('method')==method and dict(r.get('fields',[]))==fields_ and len(r['fields'])==len(fields_) and r.get('channel')==A.ORIGINAL,'Exact real About producer')
        integer(r.get('channelGeneration'),'Actual About generation',1)
        raw=r.get('raw',{})
        if method=='conversations.info':require(raw.get('ok') is True and raw.get('channel',{}).get('id')==A.ORIGINAL and raw.get('channel',{}).get('user')=='UHOME0001' and raw.get('app_home')==dict(app_id='A_HOME',app_installed_team_id='TWORKFLOW',home_tab_enabled=True,messages_tab_enabled=True,home_view_id='V_HOME'),'Original real Home/About source')
        elif method=='apps.about.info':require(raw==dict(ok=True,app_id='A_HOME',name='Review requests',owner_ids=['UWORKFLOW']),'Actual About identity')
        elif method=='apps.profile.get':require(raw==dict(ok=True,app_profile=dict(id='A_HOME',team_id='TWORKFLOW',is_installed=True,is_workflow_app=True)),'Actual About profile')
        else:require(raw==dict(ok=True,assignments=dict(agent_about_page=dict(group='on'),flexpane_content_parity=dict(group='on'))),'Actual About assignments')

def document_proof(stage,document,original,browse,product):
    require(all(exact(document.get(k),original.get(k)) for k in ('id','title','description')),'Original dirty document identity changed')
    if stage in PREFIX:
        expected=copy.deepcopy(original)
        if stage in ('confirmed','checked','selected'):expected['pending']=product['attempts'][0]['id']
        require(exact(document,expected),'Publication prefix changed original draft custody')
        return browse
    if browse is None:
        attempt=product['attempts'][0]
        require(attempt.get('complete') is True and attempt.get('resolved') is True and attempt.get('closed') is True and attempt.get('readback',{}).get('matches') is True,'Post-Add adoption lacks verified publication')
        integer(document.get('revision'),'Post-Add adoption revision',original['revision']+1)
        expected=copy.deepcopy(original)
        expected.update(pending=None,revision=document['revision'],editor=attempt['readback']['workflow'],trigger=attempt['readback']['trigger'])
        require(exact(document,expected),'Post-Add editor differs from verified publication readback')
        return copy.deepcopy(document)
    require(exact(document,browse),'Browse changed retained document/revision/editor')
    return browse
def validate(fixture,case,receipt,captures,viewport=None,rectangle=None,validate_png=None,png=True):
    require(case in STAGES and receipt.get('ok') is True and receipt.get('firstPublishCase')==case and receipt.get('fixture')==fixture,'Exact Workflows case')
    viewport=viewport or ([1024,640] if fixture.endswith('-narrow') else [1440,1000])
    require(receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Closed Workflows harness')
    stages=receipt.get('stages');require(isinstance(stages,list) and tuple(r.get('stage') for r in stages)==STAGES[case],'Exact tab stage order')
    require(set(receipt.get('captures',{}))==set(STAGES[case]),'Tab original capture inventory')
    parent=None;browse_document=None;checked=None;previous=None
    for row in stages:
        stage=row['stage'];proof=row['proof'];s=proof['server'];p=proof['firstPublish'];n=proof['native'];m=row['model'];exercise=proof['exercise'];d=m['confirmationDestination']
        channel_rows={r['id']:r for r in channels(case)}
        A.seed_proof(s,channels(case));ledger(case,stage,s['channelWorkflows'])
        phase=stage if stage in A.PREFIX else 'checked'
        A.add_ledger('first-add-success',stage if stage in PREFIX else 'added',s,channel_rows)
        if stage=='returned':returned_server(s,previous['proof']['server'])
        else:F.server_proof('first-success',phase,s)
        require(m.get('reads')==len(F.stage_state('first-success',phase)['reads'])+(12 if stage=='returned' else 0),'Exact parent read count')
        F.attempt_proof('first-success',phase,p,s,F.stage_state('first-success',phase))
        A.operation_proof('first-add-success',stage if stage in PREFIX else 'added',p)
        require(n.get('ok') is True and n.get('noOverflow') is True and n.get('viewport')==viewport and n.get('composing') is False,'Native original tab layout')
        require(exact(row['geometry'],dict(n,pngDimensions=row['geometry']['pngDimensions'])),'Native geometry not original observation')
        if stage in A.PREFIX:
            F.native_proof('first-success',stage,n,m,viewport,rectangle);A.embedded_settings(stage,p);F.confirmation_proof('first-success',stage,p,n,m)
        A.native_add(stage if stage in PREFIX else 'added',n,rectangle,viewport)
        product_proof(case,stage,n,d,viewport,rectangle)
        if case=='first-tab-rate-limit' and stage not in PREFIX and stage!='returned':rate_exercise(stage,exercise,F.observation(row),F.observation(previous))
        if stage not in ('about','editor'):
            docs=[v for v in d['documents'] if v['title']==F.TITLE and v['description']==F.DESCRIPTION];require(len(docs)==1,'Retained dirty original document absent')
            if parent is None:parent=docs[0]
            browse_document=document_proof(stage,docs[0],parent,browse_document,p)
        if stage=='checked':checked=row
        if stage not in PREFIX:
            require(p['attempts'][-1]['closed'] is True,'Original confirmation not closed')
            require(m['writes']==checked['model']['writes'] and m['networkCalls']==0,'Browse dispatched mutation/network')
        if stage in ('switched','late-settled'):
            require(n.get('workflows') is None and d['tab']=='Messages' and n['destination']['timeline']['connected'] is True and n['destination']['composer']['connected'] is True,'Actual successor Messages view missing')
        if 'quiet' in exercise:
            q=exercise['quiet'];require(q.get('observedMs')==500 and exact(q['before'],q['after']) and exact(q['after'],F.observation(row)),'Bounded quiet changed/refetched')
        if 'after' in exercise:require(exact(exercise['after'],F.observation(row)),'Actual exercise after mismatch')
        if stage=='late-settled':
            require(exercise['release']==dict(serial=2,receiverRetired=True,delivered=False),'Original canceled read release')
            for key in ('model','firstPublish','native'):require(exact(exercise['before'][key],exercise['after'][key]),'Late result changed successor '+key)
        if stage=='returned':
            require(d['channel']==A.ORIGINAL and d['tab']=='AppAbout' and n['present'] is True and n['originalNodes'] is True,'Real original editor return')
            require(all(exercise['remount'].get(k) is True for k in ('ok','originalDetached','newRoot','newTitle','newDescription')) and exercise['freshVisit']==m['visit'] and m['visit']!=checked['model']['visit'],'Fresh real About owner/remount')
            require(n['fields']['title']['value']==F.TITLE and n['fields']['description']['value']==F.DESCRIPTION,'Native returned dirty buffers')
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Owned tab capture path')
        if png:validate_png(path,row['geometry']['pngDimensions'])
        previous=row
    require(receipt.get('replacement') is None and exact(receipt['final'],F.observation(stages[-1])),'Final tab evidence mismatch')
    return receipt
