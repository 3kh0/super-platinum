"""Opt-in original recipient tokens -> one exactly correlated group -> one rich Send."""
import copy
import json
from collections import Counter
from pathlib import Path
from urllib.parse import urlsplit, parse_qsl
import workflow_management_share_person as P
from workflow_management_delete import require, exact, integer

S,A,F,D=P.S,P.A,P.F,P.D
CASES=('success','reuse','rejected','missing-members','mismatched-members','held-change-cancel')
PEERS=('USHARE001','USHARE002'); DEST='GSHARE001'; DISPLAY='@dana_reviewer,evan_reviewer'
LABELS=('Dana Reviewer','Evan Reviewer'); MEMBERS=('UWORKFLOW',)+PEERS
FAILURE='This group conversation could not be verified. No message was sent. Remove a person and choose again.'
TOKEN_STAGES=('token-removed','token-selected','tokens-empty','token-restored')
STAGES={c:A.PREFIX+('share-open','first-recipient')+(TOKEN_STAGES if c=='success' else ())+(
    ('group-ready','share-settled','destination') if c=='reuse' else ('group-opening',)+(
        ('group-ready','share-settled','destination') if c=='success' else
        ('recipient-changed','share-cancelled','late-settled') if c=='held-change-cancel' else
        ('group-failed','share-cancelled'))) for c in CASES}

def mode(case,share_case,drafts,cancel=False,resend=None,posting=None,dm=None,person=None,search=None):
    if case is None:return
    require(case in CASES and share_case=='success' and drafts is True and cancel is False and
            all(v is None for v in (resend,posting,dm,person,search)), 'Multiple mode requires isolated original Share drafts')

def identity(receipt,case):
    require(receipt.get('shareMultipleCheck',False) is (case is not None) and receipt.get('shareMultipleCase')==case,
            'Original multiple mode identity changed')

def peer(index):
    return dict(F.person(PEERS[index],LABELS[index]),is_app_user=False,is_workflow_bot=False,is_profile_only=False)

def channel():
    return dict(id=DEST,name='mpdm-dana--evan-1',is_mpim=True,is_im=False,members=list(MEMBERS),
        context_team_id='TWORKFLOW',is_private=True,is_archived=False,is_frozen=False,is_ext_shared=False,
        is_pending_ext_shared=False,is_read_only=False,is_thread_only=False,is_open=True)

def seed_proof(server,case):
    rows=A.channels()+[D.channel()]+([channel()] if case=='reuse' else [])
    A.seed_proof(server,rows)
    require(server['seed']['boot']['users'][-2:]==[peer(0),peer(1)],'Group actual original Boot people changed')
    require(server['seed']['boot'].get('workspaces')==server['seed']['init'].get('workspaces'),
            'Group complete observation membership producer changed')
    actual=[r for r in server['seed']['canonicalChannels'] if r['id']==DEST]
    require(len(actual)==int(case=='reuse'),'Group destination preseeded instead of actual open')
    if actual:require(actual[0].get('members')==list(MEMBERS) and actual[0].get('fromAnotherTeam') is False,'Group reuse lacks exact canonical set')

def open_raw(case):
    if case=='rejected':return dict(ok=False,error='not_allowed',opaque_receipt='original-multiple-open-rejected')
    row=dict(channel(),latest=dict(ts='1781524809.000001'),last_read='1781524803.000001')
    if case=='missing-members':row.pop('members')
    if case=='mismatched-members':row['members']=['UWORKFLOW',PEERS[0],'USHARE003']
    return dict(ok=True,channel=row,opaque_receipt='original-multiple-open')

def open_request(v):
    require(v.get('method')=='conversations.open' and v.get('tokenPresent') is True,'Group typed original open factory')
    require(v.get('fields')==[['quiet','1'],['return_im','1'],['users',','.join(MEMBERS)],['_x_reason','startMpimByMemberIds']],
            'Group open inferred/omitted members or changed quiet wire')
    url=urlsplit(v.get('url',''));fields=parse_qsl(url.query,keep_blank_values=True);query=dict(fields)
    require(url.scheme=='https' and url.netloc=='example.test' and url.path=='/api/conversations.open' and not url.fragment,
            'Group open escaped original account origin')
    fixed={'_x_desktop_ia':'true','_x_foreground':'true','_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0','_x_version_ts':'capture-required','slack_route':'default'}
    import re
    require(len(fields)==len(query) and set(query)==set(fixed)|{'_x_id','_x_csid','fp'} and all(query[k]==v for k,v in fixed.items()) and
            all(re.fullmatch('[0-9a-f]{32}',query[k]) for k in ('_x_id','_x_csid','fp')),'Group open trace/body identity')

def ordinary_reads(stage,server,previous=None,*,search=None):
    rows=server['confirmationAdd']['share'].get('reads')
    require(isinstance(rows,list) and len(rows)<=32,'Group ordinary posting read bound')
    for n,r in enumerate(rows,1):
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Group ordinary posting request shape')
        id=body[0][1];require(id in A.CHANNELS,'Group unobserved ordinary destination')
        require(exact(r,dict(serial=n,method='conversations.info',body=[['channel',id],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,result=dict(raw=dict(ok=True,channel=A.CHANNELS[id],opaque_receipt='original-share-channel')))),'Group original ordinary source/owner changed')
    if stage in A.PREFIX:require(rows==[],'Group premature ordinary acquisition')
    else:
        require(isinstance(search,dict) and isinstance(search.get('history'),list),'Group original search history missing')
        ranked=[v for v in search['history']+[search] if v.get('ranking') is not None]
        revisions=[v.get('revision') for v in ranked]
        require(all(type(v) is int for v in revisions) and len(set(revisions))==len(revisions),'Group duplicated original ranking revision')
        expected=[]
        for v in ranked:
            require(v.get('status') in ('ready','partial') and v.get('query')=='' and v['ranking'].get('source')=='share_dialog_channel_picker','Group ordinary read without completed original rank')
            expected += [r['id'] for r in v['ranking']['results'] if r['id'] in A.CHANNELS]
        require([r['body'][0][1] for r in rows]==expected,'Group ordinary reads differ from exact original ranked posting waves')
    if previous is not None:
        require(exact(rows[:len(previous)],previous),'Group ordinary original receipts overwritten')
    return rows

def acquisition(case,stage,server,previous=None):
    parent=server['confirmationAdd']['share'];v=parent.get('multiple')
    require(isinstance(v,dict) and v.get('case')==case and v.get('peers')==list(PEERS) and v.get('channel')==DEST and
            v.get('networkCalls')==0 and v.get('scheduling')=='closed-fixture-hold-until-original-group-capture','Closed group acquisition identity')
    reads,replies=v.get('reads'),v.get('replies')
    require(isinstance(reads,list) and isinstance(replies,list) and len(reads)==len(replies)<=64,'Group original read inventory')
    if stage in A.PREFIX:require(reads==[],'Group acquisition before original Share')
    for i,(r,response) in enumerate(zip(reads,replies),1):
        method=r.get('method');body=r.get('body');raw=None
        if method=='conversations.info':
            require(isinstance(body,list) and len(body)==2 and body[0][0]=='channel','Group info request malformed')
            dest=body[0][1];require(dest in (DEST,D.DEST) and body==[['channel',dest],['include_num_members','false']],'Group original info exact factory')
            raw=dict(ok=True,channel=channel() if dest==DEST else D.channel(),opaque_receipt='original-multiple-posting')
        elif method=='api.getFlannelHttpUrl':
            require(body==[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']],'Group discovery factory changed')
            raw=F.DISCOVERY_RAW
        elif method=='users/info':
            require(isinstance(body,dict) and body.get('ids') in ([PEERS[0]],[PEERS[1]]) and
                    body==dict(ids=body['ids'],check_interaction=True,include_profile_only_users=True),'Group fresh peer order/flags changed')
            member=body['ids'][0];raw=dict(ok=True,results=[peer(PEERS.index(member))],can_interact={member:True},next_marker='',opaque_receipt='original-multiple-peers')
            require(i>1 and reads[i-2]['method']=='api.getFlannelHttpUrl','Group peer lacks original scoped discovery')
        else:raise RuntimeError('Unexpected group read method')
        endpoint=('https://edgeapi.slack.com/cache/TWORKFLOW/users/info' if method=='users/info' else 'https://example.test/api/'+method)
        require(r.get('serial')==i and type(r.get('serial')) is int and r.get('endpoint')==endpoint and r.get('retrySafe') is (method=='conversations.info') and exact(r.get('owner'),S.OWNER),'Group original read owner/endpoint')
        require(exact(response,dict(serial=i,raw=raw,keys=None)),'Group original raw read/ordered array changed')
    before_open=stage in A.PREFIX+('share-open','first-recipient')+TOKEN_STAGES
    opened=not before_open and case!='reuse'
    if opened or case=='reuse' and stage in ('group-ready','share-settled','destination'):
        require(all(any(r['method']=='users/info' and r['body']['ids']==[id] for r in reads) for id in PEERS),
                'Group resolution omitted a fresh original peer')
    if case in ('success','reuse') and stage in ('group-ready','share-settled','destination'):
        count=sum(r['method']=='conversations.info' and r['body'][0][1]==DEST for r in reads)
        require(count==(2 if stage in ('share-settled','destination') else 1),'Group fresh posting read missing/replayed')
        require(reads[-1]['method']=='conversations.info' and reads[-1]['body'][0][1]==DEST,'Group final admission not from fresh original posting source')
        if stage in ('share-settled','destination'):
            require([r['method'] for r in reads[-5:]]==['api.getFlannelHttpUrl','users/info','api.getFlannelHttpUrl','users/info','conversations.info'] and
                    reads[-4]['body']['ids']==[PEERS[0]] and reads[-2]['body']['ids']==[PEERS[1]],'Group Send lacked new exact ordered peer validation')
    pending=stage=='group-opening' or case=='held-change-cancel' and stage in ('recipient-changed','share-cancelled')
    for k in ('requests','responses'):require(isinstance(v.get(k),list) and len(v[k])==int(opened),'Group extra/premature open '+k)
    require(v.get('held')==([1] if opened and pending else []),'Group original held sender changed')
    if opened:
        r=v['requests'][0];require(r.get('serial')==1 and exact(r.get('owner'),S.OWNER),'Group original open owner')
        open_request(r['request']);require(exact(v['responses'],[dict(serial=1,raw=open_raw(case),mayHaveOpened=case!='rejected')]),'Group exact original open response changed')
    settled=opened and not pending
    require(len(v.get('attempts',[]))==int(settled) and len(v.get('settlements',[]))==int(settled), 'Group original attempt lost/replayed')
    if settled:
        a=v['attempts'][0];outcome='retired' if case=='held-change-cancel' else 'rejected' if case=='rejected' else 'unknown' if case in ('missing-members','mismatched-members') else 'acknowledged'
        require(a.get('number')==1 and a.get('status')==200 and a.get('outcome')==outcome and a.get('retryAfterMillis') is None and a.get('mayHaveApplied') is (case!='rejected') and exact(a.get('raw'),open_raw(case)) and exact(json.loads(a.get('responseText','null')),open_raw(case)),'Group receipt/correlation changed')
        require(v['settlements']==[dict(serial=1,returned=True)] and v.get('releases')==[dict(serial=1,receiverRetired=False,delivered=True)],'Group original release/settlement changed')
    else:require(v.get('releases')==[],'Group released before original held capture')
    if previous:
        for key in ('reads','replies','requests','responses','attempts','settlements','releases'):
            require(exact(v[key][:len(previous[key])],previous[key]),'Group immutable history changed: '+key)
    return v

def recipients(case,stage):
    if stage in A.PREFIX+('share-open','token-removed','tokens-empty'):return []
    if stage in ('first-recipient','token-selected','token-restored','recipient-changed') or case=='held-change-cancel' and stage in ('share-cancelled','late-settled'):return [PEERS[0]]
    return list(PEERS)

def tokens(q,native,expected,viewport,rectangle,previous):
    values=q.get('recipients');require(isinstance(values,list) and [r.get('id') for r in values]==expected,'Original token order/content changed')
    for row in values:
        i=PEERS.index(row['id']);require(row.get('label')==LABELS[i] and exact(row.get('evidence'),dict(id=row['id'],source=P.boot_member(peer(i)),interaction=True)),'Token actual source/label absent')
    proof=native.get('multiple',{});actual=proof.get('recipients')
    require(proof.get('original') is True and isinstance(actual,list) and [r.get('id') for r in actual]==expected and proof.get('recipientRevision')==str(q['recipientRevision']),'Original native token/revision changed')
    require(proof.get('query')==q['search']['query'],'Original native query changed')
    for i,row in enumerate(actual):
        require(row.get('connected') is True and row.get('original') is True and row.get('selected')==str(q.get('selectedRecipient')==i).lower(),'Token selected/original identity changed')
        remove=row.get('remove',{});require(remove.get('connected') is True and remove.get('disabled') is False and remove.get('label')=='Remove '+LABELS[PEERS.index(row['id'])],'Original exact token removal unavailable')
        rectangle(remove.get('rect'),viewport);r=remove['rect'];p=remove.get('hitPoint',{})
        require(r['left']<=p.get('x',-1)<=r['right'] and r['top']<=p.get('y',-1)<=r['bottom'],'Original removal not exposed')
        if row['id'] in previous:require((row['nodeId'],remove['nodeId'])==previous[row['id']],'Retained token identity replaced')
    return {r['id']:(r['nodeId'],r['remove']['nodeId']) for r in actual}

def token_action(v,kind,peer_id,viewport,rectangle):
    require(isinstance(v,dict) and v.get('ok') is True and v.get('kind')==kind and v.get('peer')==peer_id and v.get('defaultPrevented') is True and v.get('queryFocused') is True,'Actual original token event absent')
    integer(v.get('nodeId'),'Token original node',1);rectangle(v.get('rect'),viewport)
    require(v.get('event')==dict(type='mousedown' if kind=='remove' else 'keydown',key=None if kind=='remove' else kind,button=0 if kind=='remove' else None,isTrusted=False,targetNodeId=v['nodeId']),'Original token event provenance changed')
    require(v.get('before',{}).get('query')=='','Token key operated on nonempty query')

def source(case,stage,v):
    success=case in ('success','reuse') and stage in ('group-ready','share-settled','destination')
    if not success and case!='reuse':
        require(v.get('canonical') is None and v.get('raw') is None and v.get('effective') is None,'Uncorrelated/retired group published or granted')
        return
    c=v.get('canonical',{});require(c.get('id')==DEST and c.get('is_mpim') is True and c.get('members')==list(MEMBERS) and c.get('fromAnotherTeam') is False and v.get('canonicalCurrent') is True,'Exact actual canonical group set absent')
    if success:
        require(v.get('raw')==channel() and v.get('rawCurrent') is True and v.get('effective')==dict(readOnly=False,threadOnly=False,permitsRootPost=True),'Group Send lacked independent fresh posting source')
        if case=='success':require(v.get('latest')=='1781524809.000001' and v.get('lastRead')=='1781524803.000001' and v.get('readRevision')==1,'Original open companions were lost')

def options(q,server,source_value):
    users={r['id']:r for r in server['seed']['boot']['users'] if not r.get('is_bot')}
    people=q.get('people');require(isinstance(people,list) and len({p['id'] for p in people})==len(people),'Group candidate inventory missing/duplicated')
    for p in people:require(p.get('id') in users and exact(p,dict(id=p['id'],source=P.boot_member(users[p['id']]),interaction=True)),'Group unobserved person source')
    rows=q.get('rows');require(isinstance(rows,list) and len({r['id'] for r in rows})==len(rows),'Group posting choices missing/duplicated')
    expected=[];mapped=set();results=q['search']['results']
    for r in rows:
        id=r['id'];require(id in (A.DEST,A.PRIVATE,D.DEST,DEST),'Group unadmitted choice')
        if id==DEST:
            require(source_value.get('canonical',{}).get('fromAnotherTeam') is False and exact(r['raw'],dict(channel(),fromAnotherTeam=False)) and r['label']==', '.join(LABELS),'Group choice replaced original posting source/label')
            g=r.get('group');require(isinstance(g,dict) and g.get('selfId')=='UWORKFLOW' and g.get('membership',{}).get('members')==list(MEMBERS),'Group choice inferred membership')
            expected_peers=[dict(id=id,raw=peer(i),effective=peer(i),source=P.boot_member(peer(i))) for i,id in enumerate(PEERS)]
            require(exact(g.get('peers'),expected_peers),'Group fresh peer evidence changed')
        elif id==D.DEST:
            require(r['raw'].get('user')==PEERS[0] and r['raw'].get('is_im') is True and r['raw'].get('fromAnotherTeam') is False and r['label']==LABELS[0],'Group singleton mapping changed')
            require(exact(r.get('peer'),dict(id=PEERS[0],raw=peer(0),effective=peer(0),source=P.boot_member(peer(0)))),'Group singleton fresh peer changed')
            if results is None or id==q['selected'] or id in results:mapped.add(PEERS[0])
        else:require(exact(r['raw'],dict(A.CHANNELS[id],fromAnotherTeam=False)) and r['label']==A.CHANNELS[id]['name'],'Group original channel choice changed')
        expected.append(dict(id=id,label=r['label']))
    expected += [dict(id=p['id'],label=users[p['id']]['profile']['display_name']) for p in people if p['id'] not in mapped]
    if q['selected'] and not any(r['id']==q['selected'] for r in expected):
        require(q['selected'] in PEERS,'Group destination without a proved choice');expected.append(dict(id=q['selected'],label=LABELS[PEERS.index(q['selected'])]))
    if results is not None:
        expected=[r for r in expected if r['id']==q['selected'] or r['id'] in results]
        expected.sort(key=lambda r:results.index(r['id']) if r['id'] in results else 10**6)
    require(exact(q.get('options'),expected),'Group options differ from original ranked candidates/pinned selection')

def validate(fixture,base_case,receipt,captures,viewport,rectangle,validate_png,*,png=True,multiple_case):
    import workflow_management_share_drafts as drafts
    case=multiple_case;mode(case,receipt.get('shareCase'),receipt.get('shareDraftCustody'));identity(receipt,case)
    require(base_case=='first-add-success' and receipt.get('firstPublishCase')==base_case and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and receipt.get('ok') is True and receipt.get('viewport')==viewport and all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'Original group terminal identity')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r['stage'] for r in rows)==STAGES[case] and set(receipt['captures'])==set(STAGES[case]),'Group exact stage/capture inventory')
    checked=None;parent_ids=share_ids=None;previous=None;token_ids={};ordinary=None;observations=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];native=p['native'];model=row['model'];ex=p['exercise']
        parent_ids=P.common(receipt,row,checked,parent_ids,viewport,rectangle,None,multiple_case=case)
        import workflow_management_share_empty as E
        observations=copy.deepcopy(E.observation_ledger('initial',stage,server,observations,ordinary_share=True))
        v=acquisition(case,stage,server,previous);previous=copy.deepcopy(v)
        source(case,stage,model['shareMultipleSource'])
        cid,ts=D.send_ledger('success',stage,server,destination=DEST)
        actual=ordinary_reads(stage,server,ordinary,search=None if stage in A.PREFIX else p['firstPublish']['attempts'][-1]['confirmationShare']['search'])
        if stage not in A.PREFIX:ordinary=copy.deepcopy(actual)
        if stage not in A.PREFIX:
            a=p['firstPublish']['attempts'][-1];q=a['confirmationShare'];active=stage not in ('share-settled','destination','share-cancelled','late-settled')
            expected=recipients(case,stage);pending=stage=='group-opening'
            require(q.get('recipients')==[dict(id=id,label=LABELS[PEERS.index(id)],evidence=dict(id=id,source=P.boot_member(peer(PEERS.index(id))),interaction=True)) for id in expected],'Group original recipients changed after closure')
            options(q,server,model['shareMultipleSource'])
            E.successful_notice(q,native)
            if q['search']['status'] in ('ready','partial'):
                rank=q['search'].get('ranking');require(isinstance(rank,dict) and rank.get('source')=='share_dialog_channel_picker' and rank.get('query')=='','Group omitted actual empty ranking')
                import workflow_management_share_ranking as R
                raw_reads=[dict(method='POST',raw=R.observation_reply(m,'cold-mixed',server['seed']['boot']['self'])) for m in ('client.gantryBoot','users.prefs.get','experiments.getByUser')]
                require(rank.get('observationReads')==raw_reads and rank.get('observationSource')==('fresh' if stage=='share-open' else 'reused') and rank.get('reads')==(raw_reads if stage=='share-open' else []),'Group discarded original complete observations or reacquired on each token')
                E.posting_subset(q['search'],server)
            require(q.get('bound') is True and q.get('loading') is pending and q.get('active') is active and a.get('closed') is (not active),'Original group lifecycle/current binding changed')
            require(q.get('text')==('' if stage=='share-open' else S.MESSAGE) and q['search']['query']=='','Token change lost original message or did not clear query')
            dest='' if not expected or stage=='group-failed' or case in ('rejected','missing-members','mismatched-members') and stage=='share-cancelled' else D.DEST if len(expected)==1 else '' if pending else DEST
            require(q.get('selected')==dest,'Uncorrelated group/retired destination selected')
            require(q.get('selectedRecipient')==(0 if stage=='token-selected' else None),'Two-step Backspace token selection changed')
            S.operation_proof('success',stage,q,server,destination=DEST);S.message_proof('success',stage,row,cid,ts,destination=DEST)
            if active:
                share_ids=D.modal(native,q,checked,share_ids,viewport,rectangle)
                token_ids=tokens(q,native,expected,viewport,rectangle,token_ids)
                controls=native['share']['controls'];require(controls['text'].get('readOnly') is pending and controls['cancel']['disabled'] is False and controls['send']['disabled'] is (not dest or pending),'Group controls granted unresolved/empty Send')
                require(native['share']['options']==[dict(value='',text='Choose a conversation',disabled=False)]+[dict(value=r['id'],text=r['label'],disabled=False) for r in q['options']],'Original grouped native options changed')
            else:require(native.get('share') is None and native.get('modal') is None,'Original group modal remained after close')
            if stage=='group-failed':require(q.get('notice')==FAILURE and native['share'].get('notice')==FAILURE,'Group visible failure guidance absent')
            started=stage not in ('share-open','first-recipient')+TOKEN_STAGES
            g=q.get('groupOpening')
            if case=='held-change-cancel' and stage in ('recipient-changed','share-cancelled','late-settled'):
                require(g is None and len(q.get('groupOpeningHistory',[]))==1,'Retired original group history lost')
                g=q['groupOpeningHistory'][0];require(g['status']==('retired' if stage=='late-settled' else 'opening'),'Original pending/retired group history changed')
            elif not started:require(g is None and q.get('groupOpeningHistory')==[],'Group opened without two tokens')
            if g is not None:
                status=('retired' if stage=='late-settled' else 'opening') if case=='held-change-cancel' else 'opening' if pending else 'ready' if case in ('success','reuse') else 'failed'
                require(g.get('peerIds')==list(PEERS) and g.get('status')==status and g.get('reused') is (case=='reuse'),'Group exact selected identities/status changed')
                require(exact(g.get('request'),None if case=='reuse' else v['requests'][0]['request']) and exact(g.get('attempts'),v['attempts']) and exact(g.get('raw'),None if case=='reuse' or pending or case=='held-change-cancel' and stage!='late-settled' else open_raw(case)),'Group original opening receipt lost/fabricated')
                require(g.get('channelId')==(DEST if case in ('success','reuse') and not pending else None) and g.get('error')==('destination_unavailable' if status=='failed' else None),'Group invented destination/error from uncorrelated open')
            for key in ('metadata','copy','run','managers','publication','followup','readback'):
                require(exact(a.get(key),checked['proof']['firstPublish']['attempts'][-1].get(key)),'Group changed original publication '+key)
            expected_reads=[dict(raw=r['result']['raw']) for r in server['confirmationAdd']['share']['reads']]+[dict(raw=r['raw']) for r in v['replies']]
            enc=lambda x:json.dumps(x,sort_keys=True)
            require(Counter(map(enc,q.get('reads',[])))==Counter(map(enc,expected_reads)),'Group discarded/fabricated source reads')
            require(exact(ex.get('after'),F.observation(row)),'Group actual action/capture differs')
            if stage in ('token-removed','token-selected','tokens-empty','recipient-changed'):
                token_action(ex.get('activation'),'remove' if stage in ('token-removed','recipient-changed') else 'Backspace',PEERS[1] if stage=='recipient-changed' else PEERS[0],viewport,rectangle)
                if stage=='token-selected':S.activation(ex.get('readd'),'select',viewport,rectangle)
            elif stage in ('group-ready','group-failed','late-settled'):
                if case=='reuse':S.activation(ex.get('activation'),'select',viewport,rectangle)
                else:require(ex.get('release')==v['releases'][0],'Group release witness changed')
            else:S.activation(ex.get('activation'),'select' if stage in ('first-recipient','token-restored','group-opening') else 'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if stage=='first-recipient':S.activation(ex.get('input'),'text',viewport,rectangle)
            if 'quiet' in ex:
                z=ex['quiet'];require(z.get('observedMs')==500 and exact(z.get('before'),z.get('after')) and exact(z['after'],F.observation(row)),'Group changed/replayed during quiet interval')
            if stage=='destination':require(model['confirmationDestination']['channel']==DEST and native['destination']['header']['text']==DISPLAY and native['destination']['header']['visible'] is True,'Actual group timeline destination absent')
        if stage=='checked':checked=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','Group capture path ownership')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'Group final observation changed')
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
