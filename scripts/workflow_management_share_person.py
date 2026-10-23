"""Closed known-person selection: quiet open precedes independent rich Send."""
import json
from pathlib import Path
from collections import Counter
from urllib.parse import urlsplit,parse_qsl
import re
import workflow_management_share_dm as D
from workflow_management_delete import exact, require, integer
from workflow_management_managers import boot_member

S,A,F=D.S,D.A,D.F
PEER,DEST,LABEL=D.PEER,D.DEST,D.LABEL
CASES=('success','rejected','missing','mismatched-peer','held-cancel','posting-failure')
STAGES={c:A.PREFIX+('share-open','person-opening')+(
    ('person-ready','share-settled','destination') if c=='success' else
    ('share-cancelled','late-settled') if c=='held-cancel' else
    ('person-failed','share-cancelled')) for c in CASES}
LATEST='1781524808.000001';LAST_READ='1781524802.000001'
FAILURE='This conversation could not be verified. No message was sent. Remove the person and choose again.'


def mode(case,share_case,drafts,cancel=False,resend=None,posting=None,dm=None):
    if case is None:return
    require(case in CASES and share_case=='success' and drafts is True and cancel is False and
            resend is None and posting is None and dm is None,'Person mode requires isolated original Share drafts')


def identity(receipt,case):
    require(receipt.get('sharePersonCheck',False) is (case is not None) and receipt.get('sharePersonCase')==case,
            'Original person mode identity changed')


def seed_proof(server):
    A.seed_proof(server)
    users=server.get('seed',{}).get('boot',{}).get('users',[])
    require(users[-1:]==[D.peer()],'Person original Boot human absent')
    require(not any(r.get('id')==DEST for r in server['seed'].get('canonicalChannels',[])),
            'Person fixture seeded a destination instead of opening it')


def options(q,server):
    """Exact bounded ASCII fixture candidates; channel admission is checked separately.

    Native peer::candidates uses actual canonical source and explicit interaction.
    Self is allowed, bot is excluded, and mapped admitted D suppresses its U option.
    """
    users=server['seed']['boot']['users']
    expected=[];labels={}
    for raw in users:
        if raw.get('is_bot') is True:continue
        require(raw['id'] in ('UWORKFLOW',PEER),'Unpinned known person fixture source')
        source=boot_member(raw)
        expected.append(dict(id=raw['id'],source=source,interaction=not raw['is_stranger']))
        labels[raw['id']]=raw['profile'].get('display_name') or raw.get('real_name') or raw['id']
    expected.sort(key=lambda r:r['id'])
    if q.get('loading') is True and q.get('opening') is None:
        expected=[]  # Original listing has not yet published its candidates.
    require(exact(q.get('people'),expected),'Original canonical people/interaction inventory changed')
    rows=q['rows'];actual=[dict(id=r['id'],label=r['label']) for r in rows]
    mapped={r['raw'].get('user') for r in rows}
    actual += [dict(id=r['id'],label=labels[r['id']]) for r in expected if r['id'] not in mapped]
    require(exact(q.get('options'),actual),'Original person/channel option mapping changed')
    return [dict(value='',text='Choose a conversation',disabled=False)]+[
        dict(value=r['id'],text=r['label'],disabled=False) for r in actual]


def open_raw(case):
    if case=='rejected':return dict(ok=False,error='not_allowed',opaque_receipt='original-person-open-rejected')
    if case=='missing':return dict(ok=True,opaque_receipt='original-person-open-missing')
    channel=dict(D.channel(),latest=dict(ts=LATEST),last_read=LAST_READ)
    if case=='mismatched-peer':channel['user']='USHARE002'
    return dict(ok=True,channel=channel,opaque_receipt='original-person-open')


def read_raw(method):
    if method=='api.getFlannelHttpUrl':return F.DISCOVERY_RAW
    if method=='users/info':return dict(ok=True,results=[D.peer()],can_interact={PEER:True},next_marker='',opaque_receipt='original-person-peer')
    return dict(ok=True,channel=D.channel(),opaque_receipt='original-person-posting')


def phase(case,stage):
    started=stage not in A.PREFIX+('share-open',)
    pending=stage=='person-opening' or case=='held-cancel' and stage=='share-cancelled'
    settled=started and not pending
    reads=0 if not started else 2
    if settled and case in ('success','posting-failure'):reads=3
    if case=='success' and stage in ('share-settled','destination'):reads=6
    return started,pending,settled,reads



def ordinary_reads(stage,server,initial=None):
    reads=server['confirmationAdd']['share'].get('reads')
    require(isinstance(reads,list),'Person original ordinary read ledger absent')
    ids=[]
    for i,r in enumerate(reads,1):
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Person ordinary read shape')
        id=body[0][1];require(id in A.CHANNELS,'Person unrequested ordinary destination/open/join')
        require(exact(r,dict(serial=i,method='conversations.info',body=[['channel',id],['include_num_members','false']],
            endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,
            result=dict(raw=dict(ok=True,channel=A.CHANNELS[id],opaque_receipt='original-share-channel')))),
            'Person original ordinary request/source changed')
        ids.append(id)
    require(len(ids)==len(set(ids)) and len(ids)<=5 and (stage not in A.PREFIX or reads==[]),'Person ordinary listing replay')
    if initial is not None:require(exact(reads,initial),'Person selection/Send replayed ordinary acquisition')
    return reads


def ledger(case,stage,server):
    parent=server['confirmationAdd']['share'];v=parent.get('person',{})
    require(v.get('case')==case and v.get('peer')==PEER and v.get('channel')==DEST and v.get('networkCalls')==0,
            'Closed original person server absent')
    require(v.get('scheduling')=='closed-fixture-hold-until-original-selection-capture','Person original hold provenance')
    started,pending,settled,count=phase(case,stage)
    methods=['api.getFlannelHttpUrl','users/info','conversations.info','conversations.info','api.getFlannelHttpUrl','users/info'][:count]
    require(len(v.get('reads',[]))==len(v.get('replies',[]))==count,'Person read count/replay')
    for i,(r,reply,method) in enumerate(zip(v['reads'],v['replies'],methods),1):
        body=([['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']] if method=='api.getFlannelHttpUrl' else
              dict(ids=[PEER],check_interaction=True,include_profile_only_users=True) if method=='users/info' else
              [['channel',DEST],['include_num_members','false']])
        endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/users/info' if method=='users/info' else 'https://example.test/api/'+method
        require(exact(r,dict(serial=i,method=method,body=body,endpoint=endpoint,retrySafe=method=='conversations.info',owner=S.OWNER)),
                'Person exact original read factory/order/owner changed')
        expected=dict(serial=i,error='offline') if case=='posting-failure' and i==3 else dict(serial=i,raw=read_raw(method),keys=None)
        require(exact(reply,expected),'Person immutable read response changed')
    for key in ('requests','responses'):require(len(v.get(key,[]))==int(started),'Person duplicate/premature open '+key)
    require(v.get('held')==([1] if pending else []),'Person held original response changed')
    require(len(v.get('attempts',[]))==len(v.get('settlements',[]))==len(v.get('releases',[]))==int(settled),
            'Person disposition appeared before original response or replayed')
    if not started:return v
    req=v['requests'][0]
    u=urlsplit(req.get('request',{}).get('url',''));pairs=parse_qsl(u.query,keep_blank_values=True);query=dict(pairs)
    fixed={'_x_desktop_ia':'true','_x_foreground':'true','_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0','_x_version_ts':'capture-required','slack_route':'default'}
    require(u.scheme=='https' and u.netloc=='example.test' and u.path=='/api/conversations.open' and not u.fragment and
        len(pairs)==len(query) and set(query)==set(fixed)|{'_x_id','_x_csid','fp'} and all(query[k]==v for k,v in fixed.items()) and
        all(re.fullmatch('[0-9a-f]{32}',query[k]) for k in ('_x_id','_x_csid','fp')),'Person original REST identity changed')
    wire=dict(method='conversations.open',url=req['request']['url'],tokenPresent=True,
              fields=[['quiet','1'],['return_im','1'],['users',PEER],['_x_reason','startMpimByMemberIds']])
    require(exact(req,dict(serial=1,request=wire,owner=S.OWNER)),'Original quiet selection open wire changed')
    raw=open_raw(case)
    require(exact(v['responses'],[dict(serial=1,raw=raw,mayHaveOpened=case!='rejected')]),'Original open response overwritten')
    if settled:
        a=v['attempts'][0]
        outcome='retired' if case=='held-cancel' else 'rejected' if case=='rejected' else 'unknown' if case in ('missing','mismatched-peer') else 'acknowledged'
        require(exact({k:a.get(k) for k in ('number','status','outcome','raw','retryAfterMillis','mayHaveApplied')},
            dict(number=1,status=200,outcome=outcome,raw=raw,retryAfterMillis=None,mayHaveApplied=case!='rejected')) and
            exact(json.loads(a.get('responseText','null')),raw),'Original open attempt correlation/disposition changed')
        require(exact(v['settlements'],[dict(serial=1,returned=True)]) and
                exact(v['releases'],[dict(serial=1,receiverRetired=False,delivered=True)]),'Person original response release changed')
    return v


def source(case,stage,value):
    started,pending,settled,_=phase(case,stage)
    adopted=settled and case in ('success','posting-failure')
    if not adopted:
        require(value.get('canonical') is None and value.get('raw') is None and value.get('effective') is None and
                value.get('rawCurrent') is False and value.get('latest') is None and value.get('lastRead') is None and value.get('readRevision') is None,
                'Unresolved/retired person invented canonical destination or companions')
        return
    require(value.get('latest')==LATEST and value.get('lastRead')==LAST_READ and value.get('readRevision')==1,
            'Actual open latest/last_read companions were lost or fabricated')
    c=value.get('canonical',{})
    require(c.get('id')==DEST and c.get('user')==PEER and c.get('is_im') is True and c.get('fromAnotherTeam') is False,
            'Canonical open destination identity changed')
    if case=='success':
        require(exact(value.get('raw'),D.channel()) and value.get('rawCurrent') is True and
                exact(value.get('effective'),dict(readOnly=False,threadOnly=False,permitsRootPost=True)),
                'Fresh paired posting source/permission absent')
    else:require(value.get('effective') is None and value.get('raw') is None and value.get('rawCurrent') is False,
                 'Failed posting acquisition granted permission or raw authority')


def common(receipt,row,checked,parent_ids,viewport,rectangle,case,*,search_case=None,multiple_case=None):
    import workflow_management_share_drafts as drafts
    stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];model=row['model']
    phase_name=stage if stage in A.PREFIX else 'checked'
    if multiple_case is not None:
        from workflow_management_share_multiple import seed_proof as multiple_seed
        multiple_seed(server,multiple_case)
    elif search_case is None:seed_proof(server)
    else:
        from workflow_management_share_search import seed_proof as search_seed
        search_seed(server)
    A.add_ledger('first-add-success',phase_name,server)
    spec=F.server_proof('first-success',phase_name,server,person_case=case if search_case is None and multiple_case is None else None,search_case=search_case,multiple_case=multiple_case)
    require(model.get('reads')==len(spec['reads']) and model.get('writes')==spec['writes'] and model.get('networkCalls')==0,'Person changed publication IO')
    if stage in A.PREFIX:
        F.attempt_proof('first-success',stage,p['firstPublish'],server,spec)
        F.native_proof('first-success',stage,native,model,viewport,rectangle,passive=lambda item:drafts.control(receipt,row,item,viewport,rectangle))
        F.confirmation_proof('first-success',stage,p['firstPublish'],native,model);A.embedded_settings(stage,p['firstPublish'])
    else:drafts.stage(receipt,row,checked)
    require(exact(row['geometry'],dict(native,pngDimensions=row['geometry'].get('pngDimensions'))) and native.get('ok') is True and
            native.get('noOverflow') is True and native.get('composing') is False and native.get('viewport')==viewport,'Person actual native window changed')
    for item in native.get('controls',[])+native.get('feedback',[]):drafts.control(receipt,row,item,viewport,rectangle)
    if native.get('fields'):
        ids=[native['fields'][k]['nodeId'] for k in ('title','description')]
        if stage=='editor':parent_ids=ids
        require(ids==parent_ids and native.get('originalNodes') is True,'Person replaced original metadata editor')
    if stage not in ('about','editor'):
        require(len([d for d in model['confirmationDestination']['documents'] if d.get('title')==F.TITLE and d.get('description')==F.DESCRIPTION])==1,'Person lost original metadata draft')
    return parent_ids


def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,person_case):
    import workflow_management_share_drafts as drafts
    case=person_case;mode(case,receipt.get('shareCase'),receipt.get('shareDraftCustody'));identity(receipt,case)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and
            receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and
            all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Person original terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==STAGES[case] and set(receipt['captures'])==set(STAGES[case]),'Person stage/capture inventory')
    checked=None;parent_ids=share_ids=None;initial_reads=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];model=row['model'];ex=p['exercise']
        parent_ids=common(receipt,row,checked,parent_ids,viewport,rectangle,case)
        ordinary=ordinary_reads(stage,server,initial_reads)
        if stage=='share-open':initial_reads=ordinary
        v=ledger(case,stage,server);source(case,stage,model['sharePersonSource'])
        cid,ts=D.send_ledger('success',stage if case=='success' else 'share-open',server)
        if stage not in A.PREFIX:
            a=p['firstPublish']['attempts'][-1];q=a['confirmationShare'];started,pending,settled,_=phase(case,stage)
            S.operation_proof('success',stage,q,server,destination=DEST);S.message_proof('success',stage,row,cid,ts,destination=DEST)
            sent=case=='success' and stage in ('share-settled','destination')
            active=stage in ('share-open','person-opening','person-ready','person-failed')
            require(q.get('bound') is True and q.get('loading') is (pending or case=='held-cancel' and started),'Person selection loading/binding changed')
            require(q.get('active') is active and a.get('closed') is (not active),'Person original modal lifecycle changed')
            require(q.get('text')==('' if stage=='share-open' else S.MESSAGE),'Person consumed/changed optional message')
            selected='' if stage=='share-open' or case not in ('success','held-cancel') and settled else PEER if pending or case=='held-cancel' else DEST
            require(q.get('selected')==selected,'Person selected uncorrelated or stale destination')
            require(q.get('openingHistory')==[],'Person replaced prior selection history')
            expected_options=options(q,server)
            require(sorted(r['id'] for r in q['rows'])==sorted([A.DEST,A.PRIVATE]+([DEST] if case=='success' and settled else [])),'Person channel choices changed')
            for r in q['rows']:
                if r['id']==DEST:require(r['raw']['user']==PEER and exact(r.get('peer'),dict(id=PEER,raw=D.peer(),effective=D.peer(),source=D.peer_source())),'Person fresh peer custody changed')
                else:require(exact(r['raw'],dict(A.CHANNELS[r['id']],fromAnotherTeam=False)),'Person changed original channel source')
            o=q.get('opening')
            if not started:require(o is None,'Person opened on modal mount')
            else:
                status='opening' if pending else 'retired' if case=='held-cancel' else 'ready' if case=='success' else 'failed'
                require(o.get('peerId')==PEER and o.get('status')==status and o.get('reused') is False and
                        exact(o.get('request'),v['requests'][0]['request']) and exact(o.get('attempts'),v['attempts']) and
                        exact(o.get('raw'),open_raw(case) if settled else None),'Person original opening receipt changed')
                require(o.get('error')==('destination_unavailable' if status=='failed' else None),'Person opening failure classification changed')
                require(o.get('channelId')==(DEST if settled and case in ('success','posting-failure') else None),'Person invented destination from failed open')
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle)
                controls=native['share']['controls']
                require(exact(native['share']['options'],expected_options),'Person original native options changed')
                require(controls['text'].get('readOnly') is pending and controls['cancel'].get('disabled') is False and
                        controls['send'].get('disabled') is (stage!='person-ready'),'Person pending editor/Send/Cancel state changed')
                require(native['share'].get('notice')==q.get('notice'),'Person visible/model notice differs')
            else:require(native.get('share') is None and native.get('modal') is None,'Person modal remained after Cancel/consumption')
            if stage=='person-failed':require(q.get('notice')==FAILURE,'Person failure feedback absent')
            for key in ('metadata','copy','run','managers','publication','followup','readback'):
                require(exact(a.get(key),checked['proof']['firstPublish']['attempts'][-1].get(key)),'Person overwrote original publication '+key)
            expected_reads=[dict(raw=r['result']['raw']) for r in server['confirmationAdd']['share']['reads']]
            # Query::read_guarded retains an admitted failed read as a failure
            # witness; the independent fixture ledger retains its original error.
            expected_reads += [dict(raw=r['raw']) if 'raw' in r else dict(failure='read failed') for r in v['replies']]
            encode=lambda x:json.dumps(x,sort_keys=True)
            require(Counter(map(encode,q.get('reads',[])))==Counter(map(encode,expected_reads)),'Person discarded/fabricated original read receipts')
            require(exact(ex.get('after'),F.observation(row)),'Person action/capture changed')
            if stage=='person-opening':
                S.activation(ex.get('input'),'text',viewport,rectangle);S.activation(ex.get('selectionActivation'),'select',viewport,rectangle)
                require(ex['selectionActivation'].get('value')==PEER,'Person selected different source identity')
            elif stage in ('person-ready','person-failed','late-settled'):require(exact(ex.get('release'),v['releases'][0]),'Person release differs')
            else:S.activation(ex.get('activation'),'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if 'quiet' in ex:
                z=ex['quiet'];require(z.get('observedMs')==500 and exact(z.get('before'),z.get('after')) and exact(z['after'],F.observation(row)),'Person replayed/changed during quiet interval')
            if stage=='destination':
                require(model['confirmationDestination']['channel']==DEST and native['destination']['header']['text']==LABEL and native['destination']['header']['visible'] is True,'Actual opened human DM destination absent')
                rendered=native['shareTimeline']['rows'];require(len(rendered)==1 and rendered[0]['messageId']==S.SERVER_TS and all(t in rendered[0]['text'] for t in S.MESSAGE.split('\n')) and any(l['href']==S.URL for l in rendered[0]['links']),'Person rich delivered row changed')
        if stage=='checked':checked=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Person capture ownership')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Person final observation changed')
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
