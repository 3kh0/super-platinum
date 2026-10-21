"""Independent avatar crop geometry and closed synthetic-pointer evidence.

These checks never imply trusted OS input. Original DOM PointerEvents exercise
the renderer; the closed request ledger independently proves the sent crop.
"""
import math
from pathlib import Path
from workflow_management_delete import exact, require, integer

PREFIX = ('about', 'editor', 'draft', 'finish', 'details', 'uploaded', 'resized', 'dragged')
STAGES = {
    'first-avatar-crop-drag': PREFIX + ('cropped', 'confirmed', 'checked', 'done'),
    'first-avatar-crop-drag-retry': PREFIX + ('crop-rejected', 'cropped', 'confirmed', 'checked', 'done'),
    'first-avatar-crop-back-held': PREFIX + ('crop-held', 'back-details', 'reentered-crop', 'late-settled'),
    'first-avatar-crop-cancel-held': PREFIX + ('crop-held', 'crop-cancelled', 'cancelled', 'reopened-finish', 'reentered-crop', 'late-settled'),
}
HELD = ('first-avatar-crop-back-held', 'first-avatar-crop-cancel-held')
WIDE = 'first-avatar-crop-drag-retry'
ROOT = Path(__file__).resolve().parents[1]
WIDE_PATH = (ROOT / 'src/desktop/app_home/about/management/fixture/first_publish/avatar-wide.png').resolve()
WIDE_SHA256 = '44ad2c369510b635710fae04da0def0038267d06d0114a5960ddd19702bfd7f5'
WIDE_BYTES = 320369
CROP_STAGES = ('uploaded', 'resized', 'dragged', 'crop-rejected', 'crop-held', 'reentered-crop', 'late-settled')


def number(value):
    require(type(value) in (int, float) and math.isfinite(value), 'Non-finite crop coordinate')
    return value


def close(actual, expected, message='Crop geometry mismatch', tolerance=1e-7):
    require(abs(number(actual) - number(expected)) <= tolerance, message)


def js_round(value):
    # Source Math.round for admitted nonnegative image coordinates, not Python's
    # bankers rounding. Never silently accept negative/non-finite wire values.
    require(number(value) >= 0, 'Negative crop coordinate')
    return math.floor(value + 0.5)


def display_size(width, height):
    require(number(width) > 0 and number(height) > 0, 'Missing reply-image dimensions')
    scale = min(1, 460 / max(width, height))
    return width * scale, height * scale


def initial_selection(width, height):
    size = min(min(width, height), max(128, min(width, height) * .95))
    return dict(x=max(0, (width-size)/2), y=max(0, (height-size)/2), width=size, height=size)


def full_crop(selection, natural, display):
    x, y, width, height = (number(selection[k]) for k in ('x', 'y', 'width', 'height'))
    dw, dh = (number(display[k]) for k in ('width', 'height'))
    nw, nh = natural
    require(dw > 0 and dh > 0 and x >= 0 and y >= 0 and width > 0 and height > 0
            and x+width <= dw+1e-7 and y+height <= dh+1e-7, 'Crop escaped actual image')
    close(width, height, 'Non-square source selection')
    return dict(x=x*max(1,nw/dw), y=y*max(1,nh/dh), size=width*max(1,nw/dw))


def wire_crop(crop):
    return ','.join(str(js_round(crop[k])) for k in ('x', 'y', 'size'))


def expected_wire(case):
    return '128,64,178' if case == WIDE else '64,64,128'


def avatar_count(case, stage):
    if stage in ('about', 'editor', 'draft', 'finish', 'details'):
        return 0
    if stage in ('uploaded', 'resized', 'dragged'):
        return 1
    if case == WIDE:
        return 2 if stage == 'crop-rejected' else 3
    if case in HELD and stage in ('reentered-crop', 'late-settled'):
        return 3
    return 2


def held(case, stage):
    return case in HELD and stage in ('crop-held', 'back-details', 'crop-cancelled', 'cancelled', 'reopened-finish', 'reentered-crop')


ORDINALS = ('nw','n','ne','e','se','s','sw','w')


def selection_close(actual, expected, tolerance=1e-7):
    require(isinstance(actual,dict), 'Missing original crop selection')
    for key in ('x','y','width','height'):
        close(actual.get(key), expected[key], 'Independent selection '+key, tolerance)


def expected_selection(case, stage):
    dw,dh=display_size(*( (640,320) if case==WIDE else (256,256) ))
    initial=initial_selection(dw,dh)
    if stage in ('uploaded','reentered-crop','late-settled'): return initial
    if stage=='resized': return dict(initial,width=128.,height=128.)
    x,y=(92.,46.) if case==WIDE else (64.,64.)
    if stage=='crop-held':x+=8
    return dict(x=x,y=y,width=128.,height=128.)


def snapshot(case,row):
    from workflow_management_first_publish import observation
    obs=observation(row);native=obs['native'];product=obs['firstPublish']
    c=product.get('confirmation');m=native.get('modal')
    if not c or not m or m.get('pane')!='crop':return
    a=c['local']['avatar'];cp=a.get('cropper',{});dom=m.get('cropper',{})
    natural=(640,320) if case==WIDE else (256,256)
    dw,dh=display_size(*natural)
    require(cp.get('ready') is True and cp.get('interactionPending') is False,'Original cropper not settled/ready')
    integer(cp.get('generation'),'Cropper generation',1)
    require(exact(cp.get('originalImage'),dict(url='https://example.test/first-publish-upload.png',naturalWidth=natural[0],naturalHeight=natural[1])),'Original loaded reply-image identity')
    close(cp.get('display',{}).get('width'),dw);close(cp.get('display',{}).get('height'),dh)
    want=expected_selection(case,row['stage'])
    # Pointer endpoints originate in layout-quantized DOM rectangles. Exact
    # model movement is checked against those events below; this stage envelope
    # admits only one CSS layout quantum around the independent target.
    selection_close(cp.get('selection'),want,.02 if row['stage'] in ('dragged','crop-rejected','crop-held') else 1e-7)
    full=full_crop(cp['selection'],natural,dict(width=dw,height=dh))
    for key in ('x','y','size'):close(a.get('crop',{}).get(key),full[key],'Independent full-image '+key)
    require(dom.get('originalNodes') is True and str(dom.get('epoch'))==m.get('epoch'),'Native cropper original epoch')
    for key in ('surface','image','selection'):
        v=dom.get(key,{})
        require(v.get('id')=='workflow-finish-crop-'+key and v.get('connected') is True and v.get('savedSame') is True,'Original cropper '+key)
        integer(v.get('nodeId'),'Original cropper node',1)
    im=dom['image'];ir=im.get('rect',{})
    require(im.get('complete') is True and (im.get('naturalWidth'),im.get('naturalHeight'))==natural
        and isinstance(im.get('source'),str) and im['source'],'Actual loaded native image')
    close(ir.get('width'),dw,tolerance=.02);close(ir.get('height'),dh,tolerance=.02)
    selection_close(dom['selection'],cp['selection'],.02)
    sr=dom['selection']['rect']
    close(sr['left']-ir['left'],want['x'],tolerance=.02);close(sr['top']-ir['top'],want['y'],tolerance=.02)
    close(sr['width'],want['width'],tolerance=.02);close(sr['height'],want['height'],tolerance=.02)
    handles=dom.get('handles');require(isinstance(handles,list) and len(handles)==8,'Eight original resize handles')
    ids=[]
    for h,ordinal in zip(handles,ORDINALS):
        require(h.get('id')=='workflow-finish-crop-handle-'+ordinal and h.get('ordinal')==ordinal
            and h.get('connected') is True and h.get('savedSame') is True,'Original resize handle')
        integer(h.get('nodeId'),'Handle node',1);ids.append(h['nodeId'])
    require(len(set(ids))==8,'Duplicate original resize handle')
    require(a.get('pending') is (row['stage']=='crop-held'),'Held crop operation boundary')
    require(a.get('notice')==('Sorry! Something went wrong. Please try again.' if row['stage']=='crop-rejected' else None),'Crop notice boundary')


def preserved(before,after):
    require(exact(before['model'],after['model']),'Crop gesture changed parent model')
    require(exact(before['native']['fields'],after['native']['fields']),'Crop gesture changed parent buffers')
    b,a=before['firstPublish']['confirmation'],after['firstPublish']['confirmation']
    require(all(exact(b[k],a[k]) for k in ('id','visit','document','revision','copy','runDraft','managerDraft')),'Crop owner/settings changed')
    require(all(exact(b['local'][k],a['local'][k]) for k in ('title','description','iconUrl')),'Crop changed local details/icon without ACK')
    bm,am=before['native']['modal'],after['native']['modal']
    for key in ('nodeId',):require(bm[key]==am[key],'Original modal replaced')
    for key in ('title','description'):require(exact(bm['fields'][key],am['fields'][key]),'Original hidden field changed')
    require(exact(bm['upload'],am['upload']) and exact(bm['custody']['file'],am['custody']['file']),'Original selected File/control changed')
    require(bm['details']['nodeId']==am['details']['nodeId'],'Original Details pane replaced')


def quiet(ex,after):
    q=ex.get('quiet',{})
    require(q.get('observedMs')==500 and exact(q.get('before'),q.get('after')) and exact(q.get('after'),after),'Crop bounded quiet changed or replayed work')


def gesture(case,ex,action):
    before,after=ex.get('before'),ex.get('after');require(isinstance(before,dict) and isinstance(after,dict),'Original gesture observations absent')
    preserved(before,after);require(exact(before['server'],after['server']),'Gesture dispatched request')
    g=ex.get('gesture',{});require(g.get('ok') is True and g.get('kind')=='synthetic-dom-pointer'
        and g.get('action')==action and g.get('sameNodes') is True,'Synthetic original gesture provenance')
    bm,am=before['native']['modal'],after['native']['modal'];bc,ac=bm['cropper'],am['cropper']
    owner=dict(visit=before['native']['scope']['visit'],document=before['native']['scope']['document'],revision=before['native']['scope']['revision'],confirmation=bm['owner'],epoch=bm['epoch'],pane='crop')
    require(exact(g.get('ownerBefore'),owner) and exact(g.get('ownerAfter'),owner),'Synchronous original gesture owner changed')
    require(int(am['epoch'])>int(bm['epoch']),'Completed original gesture did not advance epoch')
    require(before['firstPublish']['confirmation']['local']['avatar']['generation']==after['firstPublish']['confirmation']['local']['avatar']['generation'],'Gesture replaced operation generation')
    for key in ('surface','image','selection'):
        require(bc[key]['nodeId']==ac[key]['nodeId'],'Gesture remounted cropper')
    require([h['nodeId'] for h in bc['handles']]==[h['nodeId'] for h in ac['handles']],'Gesture replaced original handles')
    im=g.get('image',{});target=g.get('target',{});sb,sa=g.get('selectionBefore',{}),g.get('selectionAfter',{})
    require(im.get('id')==bc['image']['id'] and im.get('nodeId')==bc['image']['nodeId']
        and all(im.get(k)==bc['image'][k] for k in ('naturalWidth','naturalHeight','source')),'Gesture image identity')
    for key in ('width','height'):close(im['rect'][key],bc['image']['rect'][key],tolerance=.02)
    for point,dom in ((sb,bc['selection']),(sa,ac['selection'])):
        require(point.get('id')==dom['id'] and point.get('nodeId')==dom['nodeId'],'Gesture selection identity')
        selection_close(point,dom,.02)
        close(point['rect']['left']-im['rect']['left'],point['x'],tolerance=.000001)
        close(point['rect']['top']-im['rect']['top'],point['y'],tolerance=.000001)
        close(point['rect']['width'],point['width']);close(point['rect']['height'],point['height'])
    expected_target=next(h for h in bc['handles'] if h['ordinal']=='se') if action=='resize' else bc['selection']
    require(target.get('id')==expected_target['id'] and target.get('nodeId')==expected_target['nodeId'] and target.get('hit') is True,'Gesture began on wrong original target')
    tr=target['rect'];require(tr['width']>0 and tr['height']>0,'Gesture target has no rendered area')
    start=(tr['left']+tr['width']/2,tr['top']+tr['height']/2)
    original=before['firstPublish']['confirmation']['local']['avatar']['cropper']['selection']
    if action=='resize':
        requested={'size':96};delta=(96-sb['width'],96-sb['height'])
        want=dict(x=original['x'],y=original['y'],width=128.,height=128.)
    elif action=='drag':
        requested=dict(x=92,y=46) if case==WIDE else dict(x=64,y=64)
        delta=(requested['x']-sb['x'],requested['y']-sb['y'])
        want=dict(original,x=min(im['rect']['width']-original['width'],max(0,original['x']+delta[0])),y=min(im['rect']['height']-original['height'],max(0,original['y']+delta[1])))
    else:
        requested=dict(dx=8,dy=0);delta=(8,0)
        want=dict(original,x=min(im['rect']['width']-original['width'],max(0,original['x']+8)))
    require(exact(g.get('requested'),requested),'Unpinned gesture vector')
    selection_close(sa,want,.02)
    model_after=after['firstPublish']['confirmation']['local']['avatar']
    selection_close(model_after['cropper']['selection'],want)
    mapped=full_crop(want,(im['naturalWidth'],im['naturalHeight']),im['rect'])
    for key in ('x','y','size'):close(model_after['crop'][key],mapped[key],'Event-derived full-image '+key)
    events=g.get('events');require(isinstance(events,list) and len(events)==6,'Missing original pointer sequence')
    for e,typ,fraction in zip(events,['pointerdown']+['pointermove']*4+['pointerup'],[0,.25,.5,.75,1,1]):
        require(e.get('type')==typ and type(e.get('button')) is int and e['button']==0 and type(e.get('buttons')) is int and e['buttons']==(0 if typ=='pointerup' else 1)
            and e.get('pointerId')==41 and e.get('isTrusted') is False and type(e.get('defaultPrevented')) is bool,'False physical-pointer claim or wrong event sequence')
        close(e.get('clientX'),start[0]+delta[0]*fraction,tolerance=.000001)
        close(e.get('clientY'),start[1]+delta[1]*fraction,tolerance=.000001)
    quiet(ex,after)


def avatar_receipts(case,row):
    p=row['proof'];s=p['server'];product=p['firstPublish']
    sent={r['serial']:r for r in s['requests'] if r['phase']=='avatar'}
    settled=[sent[x['serial']] for x in s['settlements'] if x['serial'] in sent]
    receipts=product.get('avatarReceipts')
    require(isinstance(receipts,list) and len(receipts)==len(settled),'Original avatar raw receipt inventory')
    for received,request in zip(receipts,settled):
        integer(received.get('id'),'Original avatar modal',1);integer(received.get('generation'),'Original avatar generation',1)
        require(received.get('failed') is False and exact(received.get('request'),request['request'])
            and exact(received.get('raw'),s['responses'][request['serial']-1]['result']['raw']),'Actual avatar response custody')
    local=(product.get('confirmation') or (product.get('attempts') or [{}])[0]).get('local')
    if local:
        current_id=(product.get('confirmation') or product['attempts'][0])['id']
        expected=[dict(kind='upload' if dict(r['request']['fields']).get('_x_reason')=='workflows_upload_workflow_avatar' else 'crop',raw=r['raw']) for r in receipts if r['id']==current_id]
        if case in HELD and row['stage']=='late-settled':expected=expected[:-1] if receipts[-1]['id']==current_id else expected
        require(exact(local['avatar']['receipts'],expected),'Late/foreign raw adopted into current local receipts')



def validate_stage(case,row,previous,rows):
    from workflow_management_first_publish import observation
    snapshot(case,row)
    stage=row['stage'];after=observation(row);ex=row['proof'].get('exercise',{})
    if stage in ('about','editor','draft','finish','details','confirmed','checked','done'):return
    require(isinstance(ex.get('before'),dict) and exact(ex.get('after'),after),'Original crop action observation')
    before=ex['before']
    if stage in ('resized','dragged'):
        require(exact(before,observation(previous)),'Gesture did not begin at original prior stage')
        gesture(case,ex,'resize' if stage=='resized' else 'drag')
        return
    if stage=='crop-held':
        require(exact(before,observation(previous)),'Held Save replaced prior dragged selection')
        dispatched=ex.get('dispatched');pending=ex.get('pendingGesture',{})
        require(isinstance(dispatched,dict) and exact(pending.get('before'),dispatched)
            and exact(pending.get('after'),after),'Pending edit did not use actual dispatched state')
        require(len(dispatched['server']['requests'])==len(before['server']['requests'])+1,'Held Save dispatch count')
        req=dispatched['server']['requests'][-1]
        require(req['method']=='avatars.crop' and dict(req['request']['fields'])['cropbox']==expected_wire(case),'Held request original crop tuple')
        require(dispatched['server']['held']==[req['serial']] and req['serial'] not in [v['serial'] for v in dispatched['server']['settlements']],'Original crop receiver not held')
        require(dispatched['firstPublish']['confirmation']['local']['avatar']['pending'] is True,'Pending Save not retained')
        gesture(case,pending,'pending-drag');quiet(ex,after)
        require(exact(dispatched['server'],after['server']),'Pending interaction rewrote dispatched tuple or replayed Save')
        return
    if stage in ('uploaded','reentered-crop'):
        require(exact(before['model'],after['model']) and exact(before['native']['fields'],after['native']['fields']),'Chooser changed parent draft')
        b,a=before['native']['modal'],after['native']['modal']
        require(b['nodeId']==a['nodeId'] and b['upload']['nodeId']==a['upload']['nodeId'],'Chooser remounted original controls')
        require(b['custody']['file']['selectedIdentity']!=a['custody']['file']['selectedIdentity'],'Real new chooser File identity absent')
        require(len(after['server']['requests'])==len(before['server']['requests'])+1 and after['server']['requests'][-1]['method']=='avatars.upload','Actual chooser upload missing')
        if stage=='reentered-crop':require(exact(before,observation(previous)),'Reentry borrowed another original')
        return
    if stage in ('crop-rejected','cropped'):
        require(exact(before,observation(previous)),'Crop Save/retry did not use prior original selection')
        require(exact(before['model'],after['model']) and exact(before['native']['fields'],after['native']['fields']),'Crop changed original metadata')
        b,a=before['firstPublish']['confirmation'],after['firstPublish']['confirmation']
        require(all(exact(b[k],a[k]) for k in ('id','visit','document','revision','copy','runDraft','managerDraft')),'Crop Save borrowed owner/settings')
        require(all(exact(b['local'][k],a['local'][k]) for k in ('title','description')),'Crop Save changed local text')
        require(exact(before['native']['modal']['custody']['file'],after['native']['modal']['custody']['file']),'Crop Save replaced File')
        require(len(after['server']['requests'])==len(before['server']['requests'])+1,'Explicit Save was not one request')
        require(wire_crop(b['local']['avatar']['crop'])==expected_wire(case),'Crop request did not use independently mapped full-image tuple')
        if stage=='crop-rejected':
            preserved(before,after)
            require(exact(b['local']['avatar']['uploaded'],a['local']['avatar']['uploaded']),'Rejection lost original image')
            quiet(ex,after)
        else:require(a['local']['iconUrl']=='https://example.test/first-publish-crop.png','ACK did not adopt original crop URL')
        return
    if stage in ('back-details','crop-cancelled'):
        require(exact(before,observation(previous)) and exact(before['server'],after['server']),'Pending abandonment dispatched or changed sender')
        b,a=before['firstPublish']['confirmation'],after['firstPublish']['confirmation']
        require(all(exact(b[k],a[k]) for k in ('id','visit','document','revision','copy','runDraft','managerDraft')),'Abandonment replaced original modal/settings')
        require(all(exact(b['local'][k],a['local'][k]) for k in ('title','description','iconUrl')) and a['local']['iconUrl']=='','Abandoned crop adopted icon/details')
        require(a['local']['avatar']['generation']>b['local']['avatar']['generation'] and a['local']['avatar']['pending'] is False
            and a['local']['avatar']['uploaded'] is None and a['local']['avatar']['notice'] is None,'Abandonment did not retire pending generation')
        require(exact(before['native']['modal']['custody']['file'],after['native']['modal']['custody']['file']),'Abandonment replaced original File')
        require(ex.get('action')==('workflow-finish-crop-back' if stage=='back-details' else 'workflow-finish-crop-cancel'),'Wrong pending pane action')
        return
    if stage in ('cancelled','reopened-finish'):
        require(exact(before,observation(previous)) and exact(before['server'],after['server']),'Whole modal transition dispatched work')
        require(all(exact(before['firstPublish']['document'][k],after['firstPublish']['document'][k]) for k in ('id','revision','title','description','editor','trigger','pending')),'Whole modal transition lost parent document')
        if stage=='reopened-finish':
            require(all(ex.get('remount',{}).get(k) is True for k in ('ok','originalDetached','newRoot','newTitle','newDescription','newUpload')),'Actual successor modal remount missing')
        return
    if stage=='late-settled':
        require(exact(before,observation(previous)) and exact(before['model'],after['model']) and exact(before['native'],after['native']),'Late crop changed current native/model')
        b,a=before['firstPublish'],after['firstPublish']
        require(exact({k:v for k,v in b.items() if k!='avatarReceipts'},{k:v for k,v in a.items() if k!='avatarReceipts'}),'Late crop adopted into successor local state')
        require(len(a['avatarReceipts'])==len(b['avatarReceipts'])+1 and exact(a['avatarReceipts'][:-1],b['avatarReceipts']),'Late actual raw receipt missing')
        held_row=next(v for v in rows if v['stage']=='crop-held');original=held_row['proof']['exercise']['dispatched']['firstPublish']['confirmation']
        reply=a['avatarReceipts'][-1]
        require(reply['id']==original['id'] and reply['generation']==original['local']['avatar']['generation'],'Late receipt lost original operation owner')
        require(reply['generation']!=a['confirmation']['local']['avatar']['generation'],'Late reply borrowed successor operation')
        require(exact(before['server']['requests'],after['server']['requests']) and exact(before['server']['responses'],after['server']['responses'])
            and exact(before['server']['current'],after['server']['current']),'Late release manufactured request/response/server mutation')
        require(exact(ex.get('release'),after['server']['releases'][-1]),'Original held sender release missing')
        quiet(ex,after)
        return
    raise RuntimeError('Unvalidated cropper stage '+stage)
