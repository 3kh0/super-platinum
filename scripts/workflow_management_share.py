"""Independent closed Share evidence; queue closure is never an ACK oracle."""
import json
import re
from pathlib import Path
from urllib.parse import urlsplit, parse_qsl
import workflow_management_confirmation_add as A
import workflow_management_first_publish as F
from workflow_management_delete import exact, require, integer

CASES=('success','rejected','lost-response','held','destination-failure')
STAGES={c:A.PREFIX+('share-open','share-edited')+(
    ('share-held','share-cancelled','late-settled','destination') if c=='held' else
    ('share-unavailable','share-cancelled') if c=='destination-failure' else
    ('share-settled','destination')) for c in CASES}
MESSAGE='Share review \u4e2d\nPlease take a look.'
URL='https://slack.com/shortcuts/FIRSTFIXTURE'
SERVER_TS='1781524999.000001'
OWNER=dict(F.OWNER)

def blocks():
    return [{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[
        {'type':'text','text':MESSAGE+'\n'},{'type':'link','url':URL}]}]}]

def raw_reply(case,client_id,*,destination=A.DEST):
    if case=='rejected':
        return dict(ok=False,error='restricted_action_read_only_channel',opaque_receipt='original-share-rejected')
    return dict(ok=True,channel=destination,ts=SERVER_TS,message=dict(type='message',user='UWORKFLOW',
        ts=SERVER_TS,text=MESSAGE+'\n'+URL,blocks=blocks(),client_msg_id=client_id),opaque_receipt='original-share-ack')

def request_proof(v,*,destination=A.DEST):
    require(v.get('method')=='chat.postMessage' and v.get('tokenPresent') is True,'Share exact typed method/token')
    fields=v.get('fields');require(isinstance(fields,list) and len(fields)==8,'Share exact field inventory')
    require([x[0] for x in fields]==['channel','ts','type','blocks','unfurl','client_msg_id','include_channel_perm_error','_x_reason'],'Share field ordering/unknown field')
    f=dict(fields);client_id=f['client_msg_id'];ts=f['ts']
    require(re.fullmatch(r'[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}',client_id) is not None,'Original Share message ID')
    require(re.fullmatch(r'\d+\.\d{6}',ts) is not None,'Original Share local timestamp')
    require(f['channel']==destination and f['type']=='message' and f['include_channel_perm_error']=='true' and f['_x_reason']=='webapp_message_send','Share scalar fields')
    require(exact(json.loads(f['blocks']),blocks()) and exact(json.loads(f['unfurl']),[{'url':URL}]),'Share rich blocks/link changed')
    u=urlsplit(v.get('url',''));p=parse_qsl(u.query,keep_blank_values=True);q=dict(p)
    fixed={'_x_desktop_ia':'true','_x_foreground':'true','_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0','_x_version_ts':'capture-required','slack_route':'default'}
    require(u.scheme=='https' and u.netloc=='example.test' and u.path=='/api/chat.postMessage' and not u.fragment,'Share escaped original origin')
    require(len(p)==len(q) and set(q)==set(fixed)|{'_x_id','_x_csid','fp'} and all(q[k]==v for k,v in fixed.items()),'Share original query fields')
    require(all(re.fullmatch('[0-9a-f]{32}',q[k]) for k in ('_x_id','_x_csid','fp')),'Share request trace identity')
    return client_id,ts

def ledger(case,stage,server,opened=None):
    v=server.get('confirmationAdd',{}).get('share');require(isinstance(v,dict) and v.get('case')==case and v.get('networkCalls')==0,'Closed Share ledger missing')
    require(exact(v.get('retrySchedule'),dict(kind='closed-fixture-source-minimum',delayMillis=1000,maxAttempts=3)),'Share retry schedule claim')
    reads=v.get('reads');require(isinstance(reads,list),'Share actual read ledger')
    for i,r in enumerate(reads,1):
        require(type(r.get('serial')) is int and r.get('serial')==i and exact(r.get('owner'),OWNER),'Share read original owner/serial')
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Share read fields')
        channel=body[0][1];require(channel in A.CHANNELS,'Share unseeded channel')
        require(exact(body,[['channel',channel],['include_num_members','false']]) and r.get('method')=='conversations.info' and r.get('endpoint')=='https://example.test/api/conversations.info' and r.get('retrySafe') is True,'Share exact destination factory')
        raw={'error':'offline'} if case=='destination-failure' and channel==A.DEST else {'raw':dict(ok=True,channel=A.CHANNELS[channel],opaque_receipt='original-share-channel')}
        require(exact(r.get('result'),raw),'Share destination raw/error changed')
    prefix=stage in A.PREFIX
    pre=stage in ('share-open','share-edited') or prefix
    sent=not pre and case!='destination-failure'
    held=case=='held' and stage in ('share-held','share-cancelled')
    if prefix:require(reads==[],'Share reads before entry')
    if stage=='share-open':
        ids=[r['body'][0][1] for r in reads];require(len(ids)==len(set(ids)) and len(ids)<=5,'Share initial listing replay')
    if opened is not None and not prefix:
        initial=opened['proof']['server']['confirmationAdd']['share']['reads']
        require(exact(reads[:len(initial)],initial),'Share listing receipts changed')
        require(len(reads)==len(initial)+(0 if pre else 1),'Share fresh Send read/replay')
        if not pre:require(reads[-1]['body'][0][1]==A.DEST,'Share fresh destination changed')
    req,res,attempts,settle,release=(v.get(k) for k in ('requests','responses','attempts','settlements','releases'))
    require(all(isinstance(x,list) for x in (req,res,attempts,settle,release)),'Share lifecycle arrays')
    count=(3 if case=='lost-response' else 1) if sent else 0
    require(len(req)==len(res)==count and v.get('started') is sent,'Share send count or replay')
    require(len(attempts)==len(settle)==(0 if held else count),'Share returned attempt inventory')
    require(exact(v.get('held'),[1] if held else []),'Share held receiver inventory')
    expected_release=[dict(serial=1,receiverRetired=False,delivered=True)] if case=='held' and sent and not held else []
    require(exact(release,expected_release),'Original Share held sender release')
    client_id=local_ts=None
    for i,(r,response) in enumerate(zip(req,res),1):
        require(type(r.get('serial')) is int and r.get('serial')==i and exact(r.get('owner'),OWNER),'Share dispatch owner/serial')
        cid,ts=request_proof(r.get('request',{}));client_id=client_id or cid;local_ts=local_ts or ts
        require((cid,ts)==(client_id,local_ts) and exact(r['request'],req[0]['request']),'Retry replaced original request/message identity')
        raw=raw_reply(case,cid)
        result=dict(serial=i,error='transport',applied=True) if case=='lost-response' else dict(serial=i,raw=raw,applied=case!='rejected')
        require(exact(response,result),'Share raw response fabricated/changed')
        if held:continue
        a=attempts[i-1];require(type(a.get('number')) is int and a.get('number')==i and exact(a.get('status'),None if case=='lost-response' else 200),'Share attempt status')
        outcome='unknown' if case=='lost-response' else 'rejected' if case=='rejected' else 'acknowledged'
        require(a.get('outcome')==outcome and a.get('mayHaveApplied') is (case!='rejected') and a.get('retryAfterMillis') is None,'Share outcome confused with modal closure')
        require(exact(a.get('raw'),None if case=='lost-response' else raw),'Share raw ACK custody')
        if case=='lost-response':require(a.get('responseText') is None,'Lost reply invented body')
        else:require(exact(json.loads(a.get('responseText','null')),raw),'Original Share text/raw mismatch')
        require(exact(settle[i-1],dict(serial=i,returned=True)),'Share settlement missing')
    applied=[] if not sent or case=='rejected' else [raw_reply('success',client_id)['message']]
    require(exact(v.get('applied'),applied),'Server application confused with receipt')
    return client_id,local_ts

def activation(v,kind,viewport,rectangle):
    require(isinstance(v,dict) and v.get('ok') is True and v.get('kind')==kind,'Actual original Share activation')
    integer(v.get('nodeId'),'Original Share node ID',1);rectangle(v.get('rect'),viewport)
    r=v['rect'];p=v.get('hitPoint',{});require(r['left']<=p.get('x',-1)<=r['right'] and r['top']<=p.get('y',-1)<=r['bottom'],'Share exposed hit point')
    require(exact(v.get('event'),dict(type='input' if kind=='text' else 'change' if kind=='select' else 'click',isTrusted=False,targetNodeId=v['nodeId'])),'Share DOM input provenance')
    if kind=='text':require(v.get('value')==MESSAGE and v.get('start')==7 and v.get('end')==14,'Actual Share buffer/selection')

def custody(before,after,*,navigation=False):
    a=before['model']['shareCustody'];b=after['model']['shareCustody']
    keys=('composer','threadComposer','composerAttachments','threadAttachments','threadRoot')+(() if navigation else ('threadGeneration',))
    for k in keys:
        require(exact(a.get(k),b.get(k)),'Share consumed unrelated composer '+k)
    if not navigation:require(exact(a.get('drafts'),b.get('drafts')),'Share changed unrelated stored drafts')
    else:
        for d in a.get('drafts',[]):require(d in b.get('drafts',[]),'Navigation lost original draft')

def message_proof(case,stage,row,client_id,local_ts,*,destination=A.DEST):
    c=row['model']['shareCustody'];messages=c.get('messages') or [];pending=c.get('pending') or [];delivery=c.get('delivery') or {}
    if client_id is None:require(messages==[] and pending==[] and delivery=={},'Unsent Share queued a message');return
    require(len(messages)==1,'Share duplicate/lost message row');m=messages[0]
    acknowledged=case in ('success','held') and stage not in ('share-held','share-cancelled')
    require(m.get('client_msg_id')==client_id and m.get('ts')==(SERVER_TS if acknowledged else local_ts) and m.get('channel')==destination and m.get('user')=='UWORKFLOW','Share actual message identity/sender')
    require(m.get('text')==MESSAGE+'\n'+URL and exact(m.get('blocks'),blocks()),'Share actual rich message content')
    require(exact(pending,[] if acknowledged else [local_ts]),'Share pending queue lost/duplicated')
    expected={} if acknowledged or stage in ('share-held','share-cancelled') else {local_ts:'Failed' if case=='rejected' else 'Unconfirmed'}
    require(exact(delivery,expected),'Failed/lost Share queue status')

def operation_proof(case,stage,q,server,*,destination=A.DEST):
    ledger=server['confirmationAdd']['share'];op=q.get('operation');sent=bool(ledger['requests'])
    require((op is not None)==sent,'Share operation without actual dispatch or lost pending operation')
    if not sent:return
    held=case=='held' and stage in ('share-held','share-cancelled')
    expected='sending' if held else 'unknown' if case=='lost-response' else 'rejected' if case=='rejected' else 'acknowledged'
    require(op.get('outcome')==expected and op.get('finished') is (not held) and op.get('queued') is True,'Share product queue outcome/closure confused')
    require(exact(op.get('request'),ledger['requests'][0]['request']) and exact(op.get('attempts'),ledger['attempts']),'Share product original request/attempt custody')
    cid,ts=request_proof(op['request'],destination=destination)
    require(exact(op.get('message'),dict(channel=destination,clientMsgId=cid,localTs=ts,blocks=blocks(),url=URL,text=MESSAGE+'\n'+URL)),'Share immutable message changed')
    require(exact(op.get('raw'),None if held or case=='lost-response' else raw_reply(case,cid,destination=destination)),'Share product raw result changed')

def validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,*,png=True,share_case=None,share_drafts=False,share_cancel=False,share_resend_case=None,share_posting_case=None, share_dm_case=None,share_person_case=None,share_search_case=None,share_multiple_case=None):
    import workflow_management_share_multiple as multiple
    multiple.mode(share_multiple_case,share_case,share_drafts,share_cancel,share_resend_case,share_posting_case,share_dm_case,share_person_case,share_search_case)
    multiple.identity(receipt,share_multiple_case)
    if share_multiple_case is not None:
        return multiple.validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,png=png,multiple_case=share_multiple_case)
    import workflow_management_share_search as search
    search.mode(share_search_case,share_case,share_drafts,share_cancel,share_resend_case,share_posting_case,share_dm_case,share_person_case)
    search.identity(receipt,share_search_case)
    if share_search_case is not None:
        return search.validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,png=png,search_case=share_search_case)
    import workflow_management_share_person as person
    person.mode(share_person_case,share_case,share_drafts,share_cancel,share_resend_case,share_posting_case,share_dm_case)
    person.identity(receipt,share_person_case)
    if share_person_case is not None:
        return person.validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,png=png,person_case=share_person_case)
    import workflow_management_share_dm as dm
    dm.mode(share_dm_case, share_case, share_drafts, share_cancel, share_resend_case, share_posting_case)
    dm.identity(receipt, share_dm_case)
    if share_dm_case is not None:
        return dm.validate(fixture, case, receipt, captures, viewport, rectangle, validate_png, png=png, dm_case=share_dm_case, share_case=share_case)
    import workflow_management_share_posting as posting
    posting.mode(share_posting_case,share_case,share_drafts,share_cancel,share_resend_case)
    posting.identity(receipt,share_posting_case)
    require(receipt.get('shareCancelCheck',False) is share_cancel,'Local Share Cancel mode identity')
    require(not share_cancel or share_drafts and share_case in ('rejected','lost-response'),'Local Share Cancel requires failed Share with draft custody')
    require(share_cancel or receipt.get('shareCancel') is None,'Unrequested local Share Cancel evidence')
    require(receipt.get('shareResendCheck',False) is (share_resend_case is not None) and receipt.get('shareResendCase')==share_resend_case,'Share Resend mode identity')
    require(share_resend_case is None or share_resend_case in ('success','rejected','held','suppressed') and share_drafts and not share_cancel and share_case==('rejected' if share_resend_case=='suppressed' else 'lost-response'),'Share Resend mode scope')
    require(share_resend_case is not None or receipt.get('shareResend') is None,'Unrequested Share Resend evidence')
    require(receipt.get('shareDraftCustody',False) is share_drafts,'Share draft mode identity')
    require(share_drafts or receipt.get('shareDrafts') is None,'Unexpected Share draft evidence')
    if share_drafts:
        import workflow_management_share_drafts as drafts
        drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    require(case=='first-add-success' and share_case in CASES and receipt.get('shareCase')==share_case,'Share explicit case identity')
    require(receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('firstPublishCase')==case,'Share native terminal identity')
    require(all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')) and receipt.get('viewport')==viewport,'Share closed setup')
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r.get('stage') for r in rows)==STAGES[share_case],'Exact Share stages')
    require(set(receipt.get('captures',{}))==set(STAGES[share_case]),'Share PNG inventory')
    checked=opened=edited=None;parent_ids=None;share_ids=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];product=p['firstPublish'];native=p['native'];model=row['model'];ex=p['exercise'];phase=stage if stage in A.PREFIX else 'checked'
        A.seed_proof(server);A.add_ledger(case,phase,server);spec=F.server_proof('first-success',phase,server,posting_case=share_posting_case)
        require(model.get('reads')==len(spec['reads']) and model.get('writes')==spec['writes'] and model.get('networkCalls')==0,'Share changed publication IO')
        if stage in A.PREFIX:
            F.attempt_proof('first-success',stage,product,server,spec);F.native_proof('first-success',stage,native,model,viewport,rectangle,passive=(lambda item:drafts.control(receipt,row,item,viewport,rectangle)) if share_drafts else None);F.confirmation_proof('first-success',stage,product,native,model);A.embedded_settings(stage,product)
        require(exact(row['geometry'],dict(native,pngDimensions=row['geometry'].get('pngDimensions'))),'Share geometry not actual native sample')
        require(native.get('ok') is True and native.get('noOverflow') is True and native.get('composing') is False and native.get('viewport')==viewport,'Share native original state')
        for item in native.get('controls',[])+native.get('feedback',[]):
            if share_drafts:drafts.control(receipt,row,item,viewport,rectangle)
            else:require(item.get('visible') is True and item.get('hit') is True,'Share hidden control');rectangle(item['rect'],viewport)
        if native.get('fields'):
            ids=[native['fields'][k]['nodeId'] for k in ('title','description')]
            if stage=='editor':parent_ids=ids
            require(ids==parent_ids and native.get('originalNodes') is True,'Share replaced original metadata nodes')
        docs=model.get('confirmationDestination',{}).get('documents',[])
        if stage not in ('about','editor'):require(len([d for d in docs if d.get('title')==F.TITLE and d.get('description')==F.DESCRIPTION])==1,'Share lost Unicode parent draft')
        cid,ts=ledger(share_case,stage,server,opened)
        if stage not in A.PREFIX:
            if share_drafts:
                drafts.stage(receipt,row,checked)
            else:
                custody(F.observation(checked),F.observation(row),navigation=stage=='destination')
            a=product['attempts'][-1];q=a.get('confirmationShare');require(isinstance(q,dict),'Actual Share observer')
            operation_proof(share_case,stage,q,server)
            require(q.get('loading') is False and q.get('bound') is True,'Share original channel acquisition/binding unfinished')
            expected_options=person.options(q,server)
            choices=q.get('rows');require(isinstance(choices,list) and sorted(r.get('id') for r in choices)==sorted((A.DEST,A.PRIVATE)),'Share eligible/negative channel choices')
            for choice in choices:
                expected=dict(A.CHANNELS[choice['id']],fromAnotherTeam=False)
                require(choice.get('label')==expected['name'] and exact(choice.get('raw'),expected),'Share raw canonical posting facts')
            for key in ('metadata','copy','run','managers','publication','followup','readback'):
                require(exact(a.get(key),checked['proof']['firstPublish']['attempts'][-1].get(key)),'Share mutated publication receipt '+key)
            active=stage in ('share-open','share-edited','share-held','share-unavailable')
            require(q.get('active') is active and a.get('closed') is (not active),'Share lifecycle/consumed outcome close')
            require(q.get('text')==('' if stage=='share-open' else MESSAGE) and q.get('selected')==('' if stage=='share-open' else A.DEST),'Share local buffer/destination')
            if active:
                v=native.get('share');require(isinstance(v,dict) and v.get('originalNodes') is True,'Original Share DOM binding')
                require(exact(v.get('options'),expected_options),'Share original combined conversation/person options changed')
                ids=[v['nodeId']]+[v['controls'][k]['nodeId'] for k in ('select','text','refresh','cancel','send')]
                if share_ids is None:share_ids=ids
                require(ids==share_ids,'Share original controls replaced')
                require(v.get('generation')==str(q['generation']) and v.get('epoch')==str(q['epoch']) and v.get('selected')==q['selected'],'Share native/model epoch')
                require(v['controls']['text']['value']==q['text'] and v['controls']['select']['value']==q['selected'],'Share native buffer differs')
                for control in v['controls'].values():require(control.get('hit') is True and control.get('visible') is True,'Share control not exposed');rectangle(control['rect'],viewport)
                require(native['modal']['fields']['title']['displayed'] is False and native['modal']['fields']['description']['displayed'] is False,'Original details not hidden during Share')
                old=checked['proof']['native']['modal'];now=native['modal']
                require(old['nodeId']==now['nodeId'] and old['upload']['nodeId']==now['upload']['nodeId'],'Share remounted original modal/File')
                for key in ('title','description'):
                    require(old['fields'][key]['nodeId']==now['fields'][key]['nodeId'] and old['fields'][key]['value']==now['fields'][key]['value'],'Share altered hidden original detail buffer')
            else:require(native.get('share') is None and native.get('modal') is None,'Closed Share still displayed')
            message_proof(share_case,stage,row,cid,ts)
            require(exact(ex.get('after'),F.observation(row)),'Share action after differs from capture')
            if stage=='share-edited':
                activation(ex.get('selectionActivation'),'select',viewport,rectangle);activation(ex.get('input'),'text',viewport,rectangle)
            elif stage=='late-settled':require(exact(ex.get('release'),dict(serial=1,receiverRetired=False,delivered=True)),'Share late original sender')
            else:activation(ex.get('activation'),'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if 'quiet' in ex:
                quiet=ex['quiet'];require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and exact(quiet.get('after'),F.observation(row)),'Share bounded quiet observation')
            if stage=='destination':
                dest=model['confirmationDestination'];require(dest['channel']==A.DEST and dest['tab']=='Messages','Share actual sidebar destination')
                timeline=native.get('shareTimeline');require(isinstance(timeline,dict) and timeline.get('connected') is True and len(timeline.get('rows',[]))==1,'Actual destination rich message not rendered')
                m=timeline['rows'][0];require(m.get('messageId')==(SERVER_TS if share_case in ('success','held') else ts) and MESSAGE.split('\n')[0] in m.get('text','') and 'Please take a look.' in m.get('text',''),'Share actual rendered content')
                require(any(x.get('href')==URL for x in m.get('links',[])),'Share actual URL link missing')
        if stage=='checked':checked=row
        if stage=='share-open':opened=row
        if stage=='share-edited':edited=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Share capture ownership')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Share final original observation')
    after_cancel=None
    if share_cancel:
        import workflow_management_share_cancel as cancel
        after_cancel=cancel.validate(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    after_resend=None
    if share_resend_case is not None:
        import workflow_management_share_resend as resend
        after_resend=resend.validate(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    after_posting=None
    if share_posting_case is not None:
        after_posting=posting.validate(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    if share_drafts:
        drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png,after_cancel=after_cancel,after_resend=after_resend,after_posting=after_posting)
    return receipt
