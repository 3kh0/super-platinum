"""Independent opt-in Feature evidence layered on the unchanged bookmark Add oracle."""
import copy
import json
import re
from urllib.parse import urlsplit, parse_qsl
import workflow_management_confirmation_add as A
import workflow_management_first_publish as F
from workflow_management_delete import exact, require, integer

CASES=('success','denied','rejected','readback-missing','readback-failure','held')
STAGES={'first-add-success':A.STAGES['first-add-success']}
OWNER=dict(F.OWNER)
METHOD='workflows.featured.add'
REASON='set_channel_workflow_as_workflow_actions'
NOTICE={
 'success':'Workflow featured in this channel.',
 'held':'Workflow featured in this channel.',
 'denied':None,
 'rejected':'Workflow added. Featuring could not be confirmed.',
 'readback-missing':'Workflow added. Featuring was acknowledged but is not yet visible in channel details.',
 'readback-failure':'Workflow added. Featuring was acknowledged, but channel details could not be verified.',
}

def acknowledgement(case):
    return dict(ok=False,error='not_allowed',opaque_receipt='original-feature-rejected') if case=='rejected' else dict(ok=True,opaque_receipt='original-feature-ack')

def channel(case):
    row=copy.deepcopy(A.CHANNELS[A.DEST])
    if case!='readback-missing':
        row['properties']={'channel_workflows':[dict(workflow_trigger_id='TR_REVIEW',title='Server featured review',opaque_featured={'source':'fresh-conversations-info'})]}
    return row

def preview():
    return dict(ok=True,triggers=[dict(id='TR_REVIEW',type='shortcut',name='Published review \u4e2d',description='Actual preview description \u00e9',workflow={'workflow_id':'WfREVIEW01','app_id':'A_HOME'},opaque={'retained':[3,1]})],opaque_receipt='original-feature-previews')

def read_literal(case,method):
    if method=='api.getFlannelHttpUrl':
        body=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
        raw=dict(ok=True,url='https://edgeapi.slack.com/cache/TWORKFLOW',ttl_seconds=60,opaque_receipt='original-feature-discovery')
    elif method=='permissions/info':
        body=dict(user_id='UWORKFLOW',channel_id=A.DEST,permissions=['MANAGE_CHANNEL_WORKFLOWS'])
        raw=dict(ok=True,allowed=[] if case=='denied' else ['MANAGE_CHANNEL_WORKFLOWS'],opaque_receipt='original-feature-permission')
    elif method=='conversations.info':
        body=[['channel',A.DEST],['include_num_members','false']]
        raw=dict(ok=True,channel=channel(case),opaque_receipt='original-feature-channel-readback')
    elif method=='bookmarks.list':
        body=[['channel',A.DEST],['include_folders','true'],['include_legacy_workflows','true'],['_x_reason','bookmarks-store/conditional-fetching']]
        raw=dict(ok=True,bookmarks=[A.bookmark()],opaque_receipt='original-feature-browse-bookmarks')
    elif method=='workflows.triggers.preview':
        body=[['trigger_ids','TR_REVIEW'],['_x_reason','hermes_trigger_previews_ufm']]
        raw=preview()
    else:raise RuntimeError('Unexpected Feature read method')
    result={'error':'offline'} if case=='readback-failure' and method=='conversations.info' else {'raw':raw}
    endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/permissions/info' if method=='permissions/info' else 'https://example.test/api/'+method
    return body,result,endpoint,method in ('bookmarks.list','conversations.info')

def request_proof(request):
    require(isinstance(request,dict) and request.get('method')==METHOD and request.get('tokenPresent') is True,'Wrong original Feature factory')
    require(exact(request.get('fields'),[['trigger_ids','TR_REVIEW'],['channel_id',A.DEST],['_x_reason',REASON]]),'Feature wire fields changed')
    url=urlsplit(request.get('url',''));require(url.scheme=='https' and url.netloc=='example.test' and url.path=='/api/'+METHOD and not url.fragment,'Feature request escaped original origin')
    pairs=parse_qsl(url.query,keep_blank_values=True);params=dict(pairs)
    expected={'_x_desktop_ia':'true','_x_foreground':'true','_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0','_x_version_ts':'capture-required','slack_route':'default'}
    require(len(params)==len(pairs) and set(params)==set(expected)|{'_x_id','_x_csid','fp'} and all(params[k]==v for k,v in expected.items()),'Feature original URL parameters changed')
    require(all(re.fullmatch('[0-9a-f]{32}',params[k]) for k in ('_x_id','_x_csid','fp')),'Feature original request identity missing')

def attempt_proof(case,value):
    raw=acknowledgement(case)
    require(value.get('number')==1 and type(value.get('number')) is int and value.get('status')==200 and type(value.get('status')) is int,'Original Feature attempt status/number')
    require(value.get('outcome')==('rejected' if case=='rejected' else 'acknowledged') and value.get('mayHaveApplied') is (case!='rejected') and value.get('retryAfterMillis') is None,'Feature ACK classification changed')
    require(exact(value.get('raw'),raw),'Original Feature attempt raw changed')
    text=value.get('responseText');require(isinstance(text,str),'Feature original response text absent')
    try:parsed=json.loads(text)
    except (ValueError,TypeError):raise RuntimeError('Malformed original Feature response text')
    require(exact(parsed,raw),'Feature response text/raw mismatch')

def ledger(case,stage,server,*,held=False):
    require(case in CASES,'Unknown Feature case')
    v=server.get('confirmationAdd',{}).get('feature');require(isinstance(v,dict) and v.get('case')==case and v.get('networkCalls')==0,'Original closed Feature ledger')
    reads=v.get('reads');require(isinstance(reads,list),'Missing Feature read ledger')
    selected=stage in ('selected','added');submitted=stage=='added';sent=submitted and case!='denied'
    prefix=['api.getFlannelHttpUrl','permissions/info']*(0 if not selected else 1 if not sent else 2)
    methods=[r.get('method') for r in reads];require(methods[:len(prefix)]==prefix,'Feature fresh selection/preflight acquisition')
    tail=methods[len(prefix):]
    if not submitted:require(not tail,'Feature acquisition dispatched early')
    else:
        expected_readback=sent and case!='rejected' and not held
        require(tail.count('conversations.info')==int(expected_readback),'Feature independent readback inventory')
        browse=[m for m in tail if m!='conversations.info']
        pair=['bookmarks.list','workflows.triggers.preview']
        allowed=[pair] if case in ('denied','rejected','readback-failure') or held else [pair,pair*2]
        require(browse in allowed,'Feature browse replay/partial acquisition')
        if case=='held' and not held:require(browse==pair*2,'Held Feature did not refresh original ready destination')
    for i,r in enumerate(reads,1):
        body,result,endpoint,safe=read_literal(case,r.get('method'))
        require(r.get('serial')==i and type(r.get('serial')) is int and exact(r.get('owner'),OWNER),'Feature read serial/owner')
        require(exact(r.get('body'),body) and r.get('endpoint')==endpoint and r.get('retrySafe') is safe,'Feature exact read factory')
        require(exact(r.get('result'),result),'Original Feature read result changed')
    requests,responses,attempts,settlements,releases=(v.get(k) for k in ('requests','responses','attempts','settlements','releases'))
    require(all(isinstance(x,list) for x in (requests,responses,attempts,settlements,releases)),'Missing Feature dispatch/result/settlement inventory')
    require(len(requests)==len(responses)==int(sent),'Feature mutation replay or premature dispatch')
    if sent:
        require(requests[0].get('serial')==1 and exact(requests[0].get('owner'),OWNER),'Feature original mutation owner')
        request_proof(requests[0].get('request'))
        require(exact(responses[0],dict(serial=1,raw=acknowledgement(case))),'Original Feature response changed')
    require(len(attempts)==int(sent and not held),'Feature dispatched response confused with settlement')
    if attempts:attempt_proof(case,attempts[0])
    require(exact(settlements,[dict(serial=1,returned=True)] if sent and not held else []),'Feature original settlement')
    require(exact(v.get('held'),[1] if held else []),'Feature original receiver custody')
    require(exact(releases,[dict(serial=1,receiverRetired=False,delivered=True)] if case=='held' and sent and not held else []),'Feature original response release')
    require(v.get('acknowledged') is (sent and not held and case!='rejected'),'Feature server ACK state confused with raw response')
    return v

def product_proof(case,stage,product,server):
    attempts=product.get('attempts',[])
    if not attempts:return
    v=(attempts[-1].get('confirmationAdd') or {}).get('feature')
    if stage not in ('selected','added'):return
    require(isinstance(v,dict) and v.get('attempted') is True and v.get('loading') is False,'Feature acquisition not settled')
    allowed=case!='denied';require(v.get('visible') is allowed and v.get('checked') is allowed,'Feature admission/checked state')
    require(v.get('notice') is None and v.get('count')==(0 if allowed else None),'Feature original existing count')
    op=v.get('operation')
    if stage!='added' or not allowed:require(op is None,'Unrequested Feature operation');return
    require(isinstance(op,dict),'Feature operation missing');integer(op.get('generation'),'Feature operation generation',1)
    require(exact(op.get('request'),server['confirmationAdd']['feature']['requests'][0]['request']),'Product Feature request differs from original dispatch')
    expected_status='not-started' if case=='rejected' else 'absent' if case=='readback-missing' else 'unavailable' if case=='readback-failure' else 'confirmed'
    require(op.get('outcome')==('rejected' if case=='rejected' else 'acknowledged') and op.get('readbackStatus')==expected_status,'Independent Feature mutation/readback outcome')
    require(exact(op.get('attempts'),server['confirmationAdd']['feature']['attempts']),'Product Feature attempt/raw custody')
    require(exact(op.get('raw'),None if case=='rejected' else acknowledgement(case)),'Feature transport result raw')
    raw=None if case in ('rejected','readback-failure') else read_literal(case,'conversations.info')[1]['raw']
    require(exact(op.get('readback'),raw),'Feature independent readback raw')

def checkbox_proof(v,allowed,checked,rectangle,viewport):
    require(isinstance(v,dict) and v.get('ok') is True and v.get('present') is allowed,'Feature checkbox presence')
    if not allowed:require(v.get('originalAbsent') is True,'Unexpected original Feature checkbox');return
    integer(v.get('nodeId'),'Feature original node',1)
    require(v.get('original') is True and v.get('connected') is True and v.get('type')=='checkbox' and v.get('checked') is checked and v.get('disabled') is False and v.get('indeterminate') is False,'Original Feature checkbox state')
    require(v.get('label')=='Feature this workflow in channel' and v.get('help')=='This adds it as a button to the message composer.','Feature source copy')
    require(v.get('visible') is True and v.get('hit') is True,'Feature checkbox not exposed');rectangle(v.get('rect'),viewport)
    point=v.get('hitPoint',{});r=v['rect'];require(r['left']<=point.get('x',-1)<=r['right'] and r['top']<=point.get('y',-1)<=r['bottom'],'Feature hitpoint outside original checkbox')

def selection_proof(case,exercise,row,rectangle,viewport):
    v=exercise.get('feature');require(isinstance(v,dict) and v.get('case')==case,'Feature selection evidence missing')
    before,after=v.get('before',{}),v.get('after',{});require(exact(after,F.observation(row)),'Feature selection after differs from original capture')
    require(exact(before.get('server'),after.get('server')),'Feature checkbox dispatched a request')
    for key in ('title','description','confirmationDestination'):
        require(exact(before.get('model',{}).get(key),after.get('model',{}).get(key)),'Feature selection changed retained document '+key)
    for scope in ('fields','modal'):
        left=before.get('native',{}).get(scope);right=after.get('native',{}).get(scope)
        require(isinstance(left,dict) and isinstance(right,dict),'Feature original native '+scope+' absent')
        if scope=='modal':
            require(left.get('nodeId')==right.get('nodeId') and left.get('originalNodes') is True and right.get('originalNodes') is True,'Feature modal remounted')
            require(left.get('upload',{}).get('nodeId')==right.get('upload',{}).get('nodeId'),'Feature original File input replaced')
            left,right=left.get('fields',{}),right.get('fields',{})
        for field in ('title','description'):
            for key in ('nodeId','value','selectionStart','selectionEnd'):
                require(exact(left.get(field,{}).get(key),right.get(field,{}).get(key)),'Feature changed original native field '+field+'/'+key)
    a,b=v.get('checkboxBefore'),v.get('checkboxAfter');allowed=case!='denied'
    checkbox_proof(a,allowed,False,rectangle,viewport);checkbox_proof(b,allowed,allowed,rectangle,viewport)
    activation=v.get('activation')
    if not allowed:require(activation is None,'Denied Feature clicked');return
    require(a['nodeId']==b['nodeId'] and int(b['epoch'])>int(a['epoch']),'Feature original node/epoch transition')
    require(isinstance(activation,dict) and activation.get('ok') is True and activation.get('kind')=='original-dom-click' and activation.get('nodeId')==a['nodeId'] and activation.get('checked') is True,'Original Feature click missing')
    require(exact(activation.get('click'),dict(isTrusted=False,targetNodeId=a['nodeId'])),'Feature synthetic DOM provenance changed')
    rectangle(activation.get('rect'),viewport)
    require(v.get('choiceAfter',{}).get('firstPublish',{}).get('attempts'), 'Original channel choice observation missing')

def destination_proof(case,v,*,held=False):
    require(isinstance(v,dict) and v.get('ok') is True and v.get('connected') is True and v.get('modalAbsent') is True and v.get('status')=='ready','Feature actual Workflows destination')
    expected='Featuring workflow\u2026' if held else NOTICE[case]
    require(v.get('notice')==expected,'Independent Feature outcome feedback missing')
    readback=v.get('readback');require(isinstance(readback,dict) and readback.get('current') is True and readback.get('status')=='ready','Original browse readback missing')
    require(readback.get('owner',{}).get('channel')==A.DEST and readback.get('owner',{}).get('team')=='TWORKFLOW','Feature browse owner mismatch')
    featured=case in ('success','held') and not held
    require([r.get('id') for r in readback.get('featured',[])]==(['TR_REVIEW'] if featured else []) and [r.get('id') for r in readback.get('bookmarked',[])]==['TR_REVIEW'],'ACK substituted for actual canonical featured collection')
    expected_rows=[dict(id='TR_REVIEW',status='available',title='Published review \u4e2d')]*(2 if featured else 1)
    require(exact(v.get('rows'),expected_rows),'Actual Feature rows/title differ from source preview')
    source=channel(case) if case in ('success','held','readback-missing') and not held else A.CHANNELS[A.DEST]
    actual=readback.get('channelRaw');require(isinstance(actual,dict) and all(exact(actual.get(k),v) for k,v in source.items()),'Canonical channel source fields changed')
    require(exact((actual.get('properties') or {}).get('channel_workflows',[]),(source.get('properties') or {}).get('channel_workflows',[])),'Canonical featured metadata was fabricated')
    require(exact(readback.get('bookmarksRaw'),read_literal(case,'bookmarks.list')[1]['raw']) and exact(readback.get('previewsRaw'),preview()),'Actual browse raw producers changed')

def final_proof(case,exercise,row):
    v=exercise.get('feature');require(isinstance(v,dict) and v.get('case')==case,'Final independent Feature evidence missing')
    require(exact(v.get('after'),F.observation(row)),'Feature final observation differs from original capture')
    destination_proof(case,v.get('destination'))
    owner=v['destination']['readback']['owner'];dest=row['model']['confirmationDestination']
    for key in ('account','accountEpoch','team','channel','channelGeneration'):
        require(exact(owner.get(key),dest.get(key)),'Feature browse original destination owner '+key)
    integer(owner.get('id'),'Feature browse owner identity',1)
    integer(owner.get('socket'),'Feature browse original socket',0)
    held=v.get('held')
    if case!='held':require(held is None,'Unexpected held Feature');return
    require(isinstance(held,dict),'Original Feature held proof missing')
    before=held.get('before',{});ledger(case,'added',before.get('server',{}),held=True);destination_proof(case,held.get('destination'),held=True)
    product=before.get('firstPublish',{});attempt=product.get('attempts',[])[-1];op=attempt.get('confirmationAdd',{}).get('operation',{})
    require(attempt.get('closed') is True and op.get('navigated') is True and op.get('outcome')=='acknowledged','Held Feature delayed original bookmark navigation')
    quiet=held.get('quiet',{});require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),before),'Held Feature did not remain quiet')
    require(exact(held.get('release'),dict(serial=1,receiverRetired=False,delivered=True)),'Wrong original Feature receiver released')
    final=row['model']['confirmationDestination'];start=before['model']['confirmationDestination']
    for key in ('account','accountEpoch','team','channel','channelGeneration','tab','documents'):
        require(exact(final.get(key),start.get(key)),'Feature response changed destination/draft '+key)

def validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,*,png=True,feature_case=None):
    require(case=='first-add-success' and feature_case in CASES and receipt.get('featureCase')==feature_case,'Feature exact opt-in case identity')
    A.validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,png=png)
    for row in receipt['stages']:
        proof=row['proof'];stage=row['stage'];ledger(feature_case,stage,proof['server']);product_proof(feature_case,stage,proof['firstPublish'],proof['server'])
        if stage=='selected':selection_proof(feature_case,proof['exercise'],row,rectangle,viewport)
        if stage=='added':final_proof(feature_case,proof['exercise'],row)
    return receipt
