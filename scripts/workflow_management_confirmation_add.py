"""Independent closed confirmation Add oracle; no Feature/Share or Workflows-tab claim."""
import copy
from pathlib import Path
import workflow_management_first_publish as F
from workflow_management_delete import exact,require,integer
PREFIX=('about','editor','draft','finish','details','confirmed','checked')
STAGES={
 'first-add-no-selection':PREFIX+('done',),
 'first-add-success':PREFIX+('selected','added'),
 'first-add-duplicate':PREFIX+('selected','duplicate'),
 'first-add-rejected':PREFIX+('selected','rejected'),
 'first-add-retirement':PREFIX+('selected','add-held','successor','late-settled'),
 'first-add-unavailable':PREFIX+('unavailable','done'),
}
DEST='CADDDEST01';PRIVATE='CADDPRIV01';ORIGINAL='CAPPHOME1'
OWNER={k:F.OWNER[k] for k in ('account','epoch','team')}
def channels():
    def row(id,name,private=False):
        return dict(id=id,name=name,context_team_id='TWORKFLOW',is_channel=True,is_private=private,is_member=True,is_archived=False,is_im=False,is_mpim=False,is_general=False,is_ext_shared=False,is_org_shared=False,is_shared=False,is_read_only=False,is_thread_only=False,is_frozen=False,is_huddle=False,isUnknown=False,isNonExistent=False)
    rows=[row(DEST,'review-destination'),row(PRIVATE,'private-review',True),row('CADDARCH01','archived-review'),row('CADDLEFT01','left-review'),row('CADDREAD01','read-only-review')]
    rows[2]['is_archived']=True;rows[3]['is_member']=False;rows[4].update(is_read_only=True,is_general=True)
    return rows
CHANNELS={v['id']:v for v in channels()}
def bookmark():
    return dict(id='BkFIRST01',channel_id=DEST,type='shortcut',shortcut_id='TR_REVIEW',title=F.LOCAL_TITLE,link='https://slack.com/shortcuts/FIRSTFIXTURE',rank='a',opaque_original={'retained':True})
def rejection():return dict(ok=False,error='not_allowed',opaque_receipt='original-shortcut-rejected')
def fields(method,channel=DEST):
    args={'conversations.info':dict(channel=channel,include_num_members='false'),
          'bookmarks.list':dict(channel=DEST,include_folders='true',include_legacy_workflows='true',_x_reason='bookmarks-list-api'),
          'bookmarks.add':dict(channel=DEST,link=bookmark()['link'],shortcut_id='TR_REVIEW',title=F.LOCAL_TITLE,type='shortcut',_x_reason='channel-tab-add-bookmark'),
          'client.userBoot':dict(include_min_version_bump_check='1')}[method]
    return args

def add_ledger(case,stage,server,channel_rows=None):
    channel_rows=CHANNELS if channel_rows is None else channel_rows
    v=server.get('confirmationAdd');require(isinstance(v,dict),'Missing independent Add ledger')
    require(v.get('networkCalls')==0,'Add used live network')
    req,res,settle,release=(v.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(x,list) for x in (req,res,settle,release)),'Missing Add dispatch/result/settlement/release')
    serials=list(range(1,len(req)+1));require([x.get('serial') for x in req]==serials and [x.get('serial') for x in res]==serials,'Original Add serials')
    selected=stage in ('selected','added','duplicate','rejected','add-held','successor','late-settled','unavailable') or case=='first-add-unavailable' and stage=='done'
    submitted=stage in ('added','duplicate','rejected','add-held','successor','late-settled')
    listing=stage not in ('about','editor','draft','finish','details')
    listing_ids=list(channel_rows) if listing else []
    expected=['conversations.info']*len(listing_ids)+(['conversations.info'] if selected else [])+(['conversations.info','bookmarks.list'] if submitted else [])+(['bookmarks.add'] if submitted and case!='first-add-duplicate' else [])
    # A genuinely missing Authority can perform exactly one real cold BootRead.
    methods=[r.get('method') for r in req]
    if methods and methods[0]=='client.userBoot':expected=['client.userBoot']+expected
    require(methods==expected,'Actual Add read/write sequence or replay')
    for index,(r,response) in enumerate(zip(req,res)):
        offset=1 if methods and methods[0]=='client.userBoot' else 0
        listing_index=index-offset
        channel=listing_ids[listing_index] if 0<=listing_index<len(listing_ids) else DEST
        initial_info=0<=listing_index<len(listing_ids)
        method=r['method'];pairs=r.get('fields');require(isinstance(pairs,list) and all(isinstance(p,list) and len(p)==2 and all(isinstance(x,str) for x in p) for p in pairs),'Malformed Add wire pairs')
        require(len(dict(pairs))==len(pairs) and dict(pairs)==fields(method,channel),'Exact Add factory fields')
        mutation=method=='bookmarks.add';require(r.get('phase')==('mutation' if mutation else 'read') and exact(r.get('owner'),OWNER),'Wrong Add dispatch owner/phase')
        require(r.get('retrySafe') is (not mutation),'Unexpected Add factory retry bit')
        applied=mutation and case!='first-add-rejected';require(r.get('applied') is applied,'Server application confused with reply')
        if method=='client.userBoot': result={'raw':server['seed']['boot']}
        elif method=='conversations.info':result={'error':'offline'} if case=='first-add-unavailable' and not initial_info else {'raw':dict(ok=True,channel=channel_rows[channel],opaque_receipt='original-add-channel')}
        elif method=='bookmarks.list':result={'raw':dict(ok=True,bookmarks=[bookmark()] if case=='first-add-duplicate' else [],opaque_receipt='original-shortcut-list')}
        else:result={'raw':rejection() if case=='first-add-rejected' else dict(ok=True,bookmark=bookmark(),opaque_receipt='original-shortcut-added')}
        require(exact(response.get('result'),result),'Original Add raw reply changed')
    held=case=='first-add-retirement' and stage in ('add-held','successor')
    require(v.get('held')==([len(req)] if held else []),'Original Add held receiver')
    require(len({x.get('serial') for x in settle})==len(settle) and {x.get('serial') for x in settle}==set(serials)-({len(req)} if held else set()) and all(x.get('returned') is True for x in settle),'Actual Add settlement inventory')
    require(release==([dict(serial=len(req),receiverRetired=False,delivered=True)] if stage=='late-settled' else []),'Original dispatched Add release')
    require(exact(v.get('bookmarks'),[bookmark()] if submitted and case not in ('first-add-duplicate','first-add-rejected') else []),'Independent bookmark server state')
    return v

def seed_proof(server,channel_rows=None):
    channel_rows=channels() if channel_rows is None else channel_rows
    seed=server.get('seed',{});raw=seed.get('boot',{}).get('channels',[])
    require(exact(seed.get('unifiedUserClientEnabled'),{'TWORKFLOW':False}),'Original explicit non-unified credential input')
    require(len(raw)==len(channel_rows)+1 and raw[0].get('id')==ORIGINAL and exact(raw[1:],channel_rows),'Original canonical Boot channel inputs')
    require(all('fromAnotherTeam' not in v for v in raw[1:]),'HTTP field manufactured processed foreign authority')
    rows=seed.get('canonicalChannels');require(isinstance(rows,list),'Actual processed channel seed absent')
    for id in CHANNELS:
        found=[r for r in rows if r.get('id')==id]
        require(len(found)==1 and found[0].get('fromAnotherTeam') is False and found[0].get('name')==CHANNELS[id]['name'],'Actual processed canonical channel identity')

def embedded_settings(stage,product):
    if stage in ('finish','details'):
        require(isinstance((product.get('confirmation') or {}).get('runDraft'),dict),'Original embedded run settings unavailable')


def native_add(stage,native,rectangle,viewport):
    add=native.get('confirmationAdd')
    shown=stage in ('confirmed','checked','selected','unavailable','add-held')
    require((add is not None)==shown,'Actual confirmation Add visibility')
    if not shown:return
    require(add.get('originalNodes') is True,'Original confirmation selector binding')
    integer(int(add.get('epoch','0')),'Add epoch',1)
    for key in ('select','done','refresh'):
        c=add.get(key,{});integer(c.get('nodeId'),'Original Add control identity',1);require(c.get('visible') is True,'Actual Add control hidden');rectangle(c.get('rect'),viewport)
    options=add['select'].get('options');require(options==[dict(value='',text='No channel',disabled=False),dict(value=DEST,text='review-destination',disabled=False),dict(value=PRIVATE,text='private-review',disabled=False)],'Canonical eligible options / denied row admitted')
    selected=DEST if stage in ('selected','unavailable','add-held') else ''
    require(add.get('selected')==selected and add['select'].get('value')==selected,'Original actual selector value')
    require(add['done'].get('text')==('Add to Channel' if selected else 'Done'),'Source final action caption')
    require(add['done'].get('disabled') is (stage in ('unavailable','add-held')),'Add native action admission')
    require(add['select'].get('disabled') is (stage=='add-held'),'Original selector admission')

def operation_proof(case,stage,product):
    attempts=product.get('attempts',[])
    if stage not in ('confirmed','checked','selected','unavailable','done','added','duplicate','rejected','add-held','successor','late-settled'):return
    a=attempts[0];add=a.get('confirmationAdd');require(isinstance(add,dict),'Actual result Add observer missing')
    submitted=stage in ('added','duplicate','rejected','add-held','successor','late-settled')
    op=add.get('operation');require((op is not None)==submitted,'Unrequested Add operation')
    if not submitted:return
    outcome='in-flight' if stage in ('add-held','successor') else 'duplicate' if case=='first-add-duplicate' else 'rejected' if case=='first-add-rejected' else 'acknowledged'
    require(op.get('outcome')==outcome and op.get('channel')==DEST and op.get('finished') is (stage not in ('add-held','successor')),'Original Add outcome')
    require(op.get('navigated') is (stage=='added') and op.get('canonical') is (stage=='added'),'Late/rejected Add gained navigation or canonical authority')
    raw=None if stage in ('add-held','successor') or case=='first-add-duplicate' else rejection() if case=='first-add-rejected' else dict(ok=True,bookmark=bookmark(),opaque_receipt='original-shortcut-added')
    require(exact(op.get('raw'),raw),'Add operation raw acknowledgement')
    normalized=dict(id='BkFIRST01',channelId=DEST,type='shortcut',shortcutId='TR_REVIEW',title=F.LOCAL_TITLE,url=bookmark()['link'],rank='a') if outcome=='acknowledged' else None
    require(exact(op.get('normalized'),normalized),'Accepted raw bookmark normalization changed')
    if case=='first-add-duplicate':require(op.get('request') is None,'Duplicate dispatched Add')
    else:
        args=fields('bookmarks.add').copy();args.pop('_x_reason')
        require(exact(op.get('request'),dict(method='bookmarks.add',arguments=args)),'Actual Add mutation plan')
    if stage not in ('add-held','successor'):require(a.get('closed') is True,'Source finally close omitted')


def validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,*,png=True):
    require(case in STAGES and receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('firstPublishCase')==case,'Add native terminal identity')
    require(all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')) and receipt.get('viewport')==viewport,'Closed Add harness identity')
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r.get('stage') for r in rows)==STAGES[case],'Exact Add stage sequence')
    paths=receipt.get('captures');require(isinstance(paths,dict) and set(paths)==set(STAGES[case]),'Add capture inventory')
    original_parent=None;original_modal=None;checked=None;previous=None
    for row in rows:
        stage=row['stage'];proof=row.get('proof',{});server=proof.get('server',{});product=proof.get('firstPublish',{});model=row.get('model',{});native=proof.get('native',{});exercise=proof.get('exercise',{})
        seed_proof(server);add_ledger(case,stage,server)
        retired=stage in ('successor','late-settled')
        phase=stage if stage in PREFIX else 'checked'
        if retired:
            require(checked is not None and server.get('ownerCurrent') is False,'Original account did not retire')
            for key in ('original','current','requests','responses','settlements','releases','held','owner','seed','networkCalls'):
                require(exact(server.get(key),checked['proof']['server'].get(key)),'Retirement changed publication ledger '+key)
            spec=F.stage_state('first-success','checked')
        else:spec=F.server_proof('first-success',phase,server)
        require(model.get('networkCalls')==0 and model.get('reads')==len(spec['reads']) and model.get('writes')==spec['writes'],'Original publication IO changed during Add')
        F.attempt_proof('first-success',phase,product,server,spec)
        require(exact(row.get('geometry'),dict(native,pngDimensions=row.get('geometry',{}).get('pngDimensions'))),'Actual Add geometry differs from observation')
        require(native.get('ok') is True and native.get('noOverflow') is True and native.get('viewport')==viewport and native.get('composing') is False,'Native Add layout or composition')
        for item in native.get('controls',[])+native.get('feedback',[])+native.get('outcomes',[]):
            require(item.get('visible') is True and item.get('hit') is True,'Original control hidden');rectangle(item.get('rect'),viewport)
        if stage in PREFIX:
            F.native_proof('first-success',stage,native,model,viewport,rectangle)
            embedded_settings(stage,product)
            F.confirmation_proof('first-success',stage,product,native,model)
        elif stage not in ('added','successor','late-settled'):
            require(native.get('present') is True and native.get('originalNodes') is True,'Original editor replaced')
            for key,expected in [('title',F.TITLE),('description',F.DESCRIPTION)]:
                require(model.get(key)==expected and native.get('fields',{}).get(key,{}).get('value')==expected,'Parent draft buffer lost')
            if stage in ('done','duplicate','rejected'):require(native.get('modal') is None,'Source finally close not painted')
            else:require(native.get('modal',{}).get('originalNodes') is True,'Original Add modal replaced')
        native_add(stage,native,rectangle,viewport);operation_proof(case,stage,product)
        d=model.get('confirmationDestination',{});require(isinstance(d.get('documents'),list),'Retained document witness absent')
        if stage not in ('about','editor'):
            docs=[v for v in d['documents'] if v.get('title')==F.TITLE and v.get('description')==F.DESCRIPTION]
            require(len(docs)==1,'Original Unicode dirty document not retained')
            if original_parent is None:original_parent=docs[0]['id']
            require(docs[0]['id']==original_parent,'Original document identity changed')
        if native.get('fields'):
            ids=tuple(native['fields'][k]['nodeId'] for k in ('title','description'))
            if stage=='editor':parent_nodes=ids
            require(ids==parent_nodes,'Original parent DOM remounted')
        if native.get('modal'):
            m=native['modal'];ids=(m['nodeId'],m['fields']['title']['nodeId'],m['fields']['description']['nodeId'],m['upload']['nodeId'])
            if original_modal is None:original_modal=ids
            require(ids==original_modal,'Original modal field/File remounted')
        if stage=='added':
            dest=native.get('destination',{});require(dest.get('modalAbsent') is True and dest.get('header',{}).get('text')=='#review-destination' and dest.get('header',{}).get('visible') is True,'Actual native channel destination missing')
            rectangle(dest['header']['rect'],viewport)
            require(dest.get('workflows',{}).get('connected') is True and dest.get('tab',{}).get('pressed')=='true' and dest.get('tab',{}).get('visible') is True,'Actual Workflows destination missing')
            require(d.get('channel')==DEST and d.get('team')=='TWORKFLOW' and d.get('tab')=='Workflows','Actual source Workflows route')
            items=d.get('bookmarks',{}).get('items',[]);require(len(items)==1 and items[0].get('id')=='BkFIRST01' and items[0].get('shortcutId')=='TR_REVIEW' and items[0].get('channel')==DEST,'Accepted canonical shortcut bookmark missing')
        elif stage not in ('successor','late-settled'):require(d.get('channel')==ORIGINAL,'Unaccepted Add navigated')
        if stage in ('selected','unavailable'):
            selection=exercise.get('selection',exercise);require(selection.get('value')==DEST and exact(selection.get('after'),F.observation(row)),'Original selection exercise')
        if stage=='unavailable':require('This channel could not be verified. Refresh channel choices.' in [v.get('text') for v in native.get('feedback',[])],'Actual acquisition failure hidden')
        if 'quiet' in exercise:
            quiet=exercise['quiet'];require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),F.observation(row)),'Bounded Add quiet proof')
        if stage in ('done','added','duplicate','rejected','late-settled'):
            require(exact(exercise.get('after'),F.observation(row)),'Actual Add final exercise differs')
        if stage=='done':require(exact(exercise['before']['server'],exercise['after']['server']),'Done dispatched work')
        if stage=='checked':
            require(exact(exercise.get('after'),F.observation(row)) and exercise['after']['model']['reads']==exercise['before']['model']['reads']+11 and exercise['after']['model']['writes']==exercise['before']['model']['writes'],'Independent actual publication readback')
            checked=row
        if retired:
            require(native.get('modal') is None and native.get('fields') is None and [v['id'] for v in native.get('controls',[])]==['workflow-configuration'],'Original modal survived account replacement')
            require(d.get('account')=='00000000-0000-4000-8000-000000000099' and d.get('accountEpoch')==F.OWNER['epoch']+1,'Actual successor account identity')
        if stage=='late-settled':
            for key in ('model','native'):require(exact(exercise['before'][key],exercise['after'][key]),'Late Add changed successor '+key)
            require(exercise.get('release')==server['confirmationAdd']['releases'][0],'Original late release mismatch')
        path=Path(paths[stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Capture path ownership')
        if png:validate_png(path,row['geometry']['pngDimensions'])
        previous=row
    require(exact(receipt.get('final'),F.observation(rows[-1])),'Final Add observation changed')
    require((receipt.get('replacement') is not None)==(case=='first-add-retirement'),'Unexpected account replacement')
    return receipt
