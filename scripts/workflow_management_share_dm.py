"""Opt-in existing human DM Share proof; never opens or joins a conversation."""
import json
from pathlib import Path
import workflow_management_share as S
from workflow_management_delete import exact, require, integer

A, F = S.A, S.F
CASES = ('success','deleted','mismatched-peer','held-cancel','rejected')
DEST, PEER, LABEL = 'DSHARE001', 'USHARE001', 'Dana Reviewer'
STAGES = {c:A.PREFIX+('share-open',)+(
    ('share-edited','share-settled','destination') if c in ('success','rejected') else
    ('share-held','share-cancelled','late-settled') if c=='held-cancel' else
    ('share-unavailable','share-cancelled')) for c in CASES}


def mode(case, share_case, drafts, cancel=False, resend=None, posting=None):
    if case is None:return
    require(case in CASES and drafts is True and cancel is False and resend is None and posting is None,
            'Existing DM requires isolated explicit Share drafts mode')
    require(share_case==('rejected' if case=='rejected' else 'success'), 'Existing DM base delivery case mismatch')


def identity(receipt, case):
    require(receipt.get('shareDmCheck',False) is (case is not None) and receipt.get('shareDmCase')==case,
            'Requested existing DM mode differs from original receipt')


def peer():
    return dict(F.person(PEER,LABEL),is_app_user=False,is_workflow_bot=False,is_profile_only=False)


def peer_source():
    """Exact Boot-derived DirectoryMembers source, distinct from fresh wire facts.

    member_hydration/profile::{effective,derive} supplies empty first/last names
    and name keys; directory_local::derive_member_names supplies normalized keys.
    This fixture is ASCII, so JS lowercase/deburr preserves these exact values.
    """
    value=peer()
    value['profile'].update(first_name='',last_name='')
    value.update(_name_lc='dana_reviewer',_first_name_lc='',_last_name_lc='',
                 _real_name_lc='dana reviewer',_display_name_lc='dana reviewer',
                 _real_name_normalized_lc='dana reviewer',
                 _display_name_normalized_lc='dana reviewer',_email_normalized_lc='')
    return value


def channel():
    return dict(id=DEST,is_im=True,user=PEER,context_team_id='TWORKFLOW',is_mpim=False,
                is_archived=False,is_frozen=False,is_pending_ext_shared=False,
                is_read_only=False,is_thread_only=False,is_open=True)


def seed_proof(server):
    seed=server.get('seed',{}); boot=seed.get('boot',{})
    A.seed_proof(server,A.channels()+[channel()])
    require(exact(boot.get('users',[])[-1:], [peer()]), 'DM peer was not acquired from original raw Boot')
    rows=[r for r in seed.get('canonicalChannels',[]) if r.get('id')==DEST]
    require(len(rows)==1 and rows[0].get('is_im') is True and rows[0].get('user')==PEER and
            rows[0].get('fromAnotherTeam') is False, 'Canonical DM/peer correlation absent')


def peer_reply(case):
    p=peer()
    if case=='deleted':p['deleted']=True
    if case=='mismatched-peer':p['id']='USHARE002'
    return dict(ok=True,results=[p],can_interact={p['id']:True},next_marker='',opaque_receipt='original-share-dm-peer')


def send_ledger(case,stage,server,*,destination=DEST):
    v=server.get('confirmationAdd',{}).get('share',{})
    delivery='rejected' if case=='rejected' else 'success'
    require(v.get('case')==delivery and v.get('networkCalls')==0,'Closed DM Share delivery identity')
    require(exact(v.get('retrySchedule'),dict(kind='closed-fixture-source-minimum',delayMillis=1000,maxAttempts=3)),
            'DM changed original generic retry schedule')
    sent=case in ('success','rejected') and stage in ('share-settled','destination')
    require(v.get('started') is sent,'DM queue started before original valid selection')
    for key in ('requests','responses','attempts','settlements'):
        require(isinstance(v.get(key),list) and len(v[key])==int(sent),'DM duplicate or premature dispatch '+key)
    require(v.get('held')==[] and v.get('releases')==[],'DM acquisition confused with held mutation')
    if not sent:
        require(v.get('applied')==[],'Refused DM applied a server message')
        return None,None
    request=v['requests'][0]
    require(type(request.get('serial')) is int and request['serial']==1 and exact(request.get('owner'),S.OWNER),
            'DM mutation owner/serial changed')
    cid,ts=S.request_proof(request.get('request',{}),destination=destination)
    raw=S.raw_reply(delivery,cid,destination=destination)
    require(exact(v['responses'],[dict(serial=1,raw=raw,applied=case=='success')]),'DM original raw ACK/rejection changed')
    a=v['attempts'][0]
    require(type(a.get('number')) is int and a['number']==1 and type(a.get('status')) is int and a['status']==200 and
            a.get('outcome')==('acknowledged' if case=='success' else 'rejected') and
            a.get('mayHaveApplied') is (case=='success') and a.get('retryAfterMillis') is None,
            'DM consumed outcome mistaken for ACK')
    require(exact(a.get('raw'),raw) and exact(json.loads(a.get('responseText','null')),raw),'DM immutable attempt response')
    require(exact(v['settlements'],[dict(serial=1,returned=True)]),'DM dispatch settlement missing')
    require(exact(v.get('applied'),[raw['message']] if case=='success' else []),'DM independent server application changed')
    return cid,ts


def common_stage(receipt,row,checked,parent_ids,viewport,rectangle,dm_case):
    import workflow_management_share_drafts as drafts
    stage=row['stage'];p=row['proof'];server=p['server'];product=p['firstPublish'];native=p['native'];model=row['model']
    phase=stage if stage in A.PREFIX else 'checked'
    seed_proof(server);A.add_ledger('first-add-success',phase,server)
    spec=F.server_proof('first-success',phase,server,dm_case=dm_case)
    require(model.get('reads')==len(spec['reads']) and model.get('writes')==spec['writes'] and model.get('networkCalls')==0,
            'DM altered original publication IO')
    if stage in A.PREFIX:
        F.attempt_proof('first-success',stage,product,server,spec)
        F.native_proof('first-success',stage,native,model,viewport,rectangle,
                       passive=lambda item:drafts.control(receipt,row,item,viewport,rectangle))
        F.confirmation_proof('first-success',stage,product,native,model);A.embedded_settings(stage,product)
    else:drafts.stage(receipt,row,checked)
    require(exact(row['geometry'],dict(native,pngDimensions=row['geometry'].get('pngDimensions'))),'DM geometry not actual sample')
    require(native.get('ok') is True and native.get('noOverflow') is True and native.get('composing') is False and
            native.get('viewport')==viewport,'DM native window state changed')
    for item in native.get('controls',[])+native.get('feedback',[]):drafts.control(receipt,row,item,viewport,rectangle)
    if native.get('fields'):
        ids=[native['fields'][k]['nodeId'] for k in ('title','description')]
        if stage=='editor':parent_ids=ids
        require(ids==parent_ids and native.get('originalNodes') is True,'DM replaced original metadata nodes')
    if stage not in ('about','editor'):
        docs=model.get('confirmationDestination',{}).get('documents',[])
        require(len([d for d in docs if d.get('title')==F.TITLE and d.get('description')==F.DESCRIPTION])==1,
                'DM lost original Unicode metadata draft')
    return parent_ids


def modal(native,q,checked,ids,viewport,rectangle):
    v=native.get('share');require(isinstance(v,dict) and v.get('originalNodes') is True,'Original DM Share DOM absent')
    current=[v['nodeId']]+[v['controls'][k]['nodeId'] for k in ('select','text','refresh','cancel','send')]
    if ids is not None:require(current==ids,'DM Share controls remounted')
    require(v.get('generation')==str(q['generation']) and v.get('epoch')==str(q['epoch']) and
            v.get('selected')==q['selected'],'DM native/model epoch mismatch')
    require(v['controls']['text']['value']==q['text'] and v['controls']['select']['value']==q['selected'],
            'DM native buffer differs')
    for control in v['controls'].values():
        require(control.get('hit') is True and control.get('visible') is True,'DM control not exposed')
        rectangle(control['rect'],viewport)
    require(native['modal']['fields']['title']['displayed'] is False and native['modal']['fields']['description']['displayed'] is False,
            'Underlying details not hidden during DM Share')
    old=checked['proof']['native']['modal'];now=native['modal']
    require(old['nodeId']==now['nodeId'] and old['upload']['nodeId']==now['upload']['nodeId'],'DM remounted original modal/File')
    for key in ('title','description'):
        require(old['fields'][key]['nodeId']==now['fields'][key]['nodeId'] and old['fields'][key]['value']==now['fields'][key]['value'],
                'DM altered hidden original detail buffer')
    return current


def acquisition(case,stage,server,opened=None):
    parent=server['confirmationAdd']['share'];v=parent.get('dm')
    require(isinstance(v,dict) and v.get('case')==case and v.get('channel')==DEST and v.get('peer')==PEER and
            v.get('networkCalls')==0,'Original closed DM acquisition missing')
    prefix=stage in A.PREFIX
    n=0 if prefix else 6 if case in ('success','rejected') and stage in ('share-settled','destination') else 3
    req,res,settle,release=(v.get(k) for k in ('requests','responses','settlements','releases'))
    require(all(isinstance(x,list) for x in (req,res,settle,release)) and len(req)==len(res)==n,'DM acquisition count/replay')
    methods=['conversations.info','api.getFlannelHttpUrl','users/info']
    bodies=[[['channel',DEST],['include_num_members','false']],
            [['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']],
            dict(ids=[PEER],check_interaction=True,include_profile_only_users=True)]
    raws=[dict(ok=True,channel=channel(),opaque_receipt='original-share-dm-channel'),F.DISCOVERY_RAW,peer_reply(case)]
    for i,(r,response) in enumerate(zip(req,res),1):
        slot=(i-1)%3;method=methods[slot]
        endpoint='https://edgeapi.slack.com/cache/TWORKFLOW/users/info' if slot==2 else 'https://example.test/api/'+method
        require(type(r.get('serial')) is int and r['serial']==i and r.get('method')==method and
                exact(r.get('body'),bodies[slot]) and r.get('endpoint')==endpoint and r.get('retrySafe') is (slot==0) and
                exact(r.get('owner'),S.OWNER),'DM exact ordered request/owner changed')
        require(exact(response,dict(serial=i,raw=raws[slot],keys=None)),'Original DM raw response/order evidence changed')
    held=case=='held-cancel' and not prefix and stage!='late-settled'
    require(v.get('held')==([3] if held else []),'DM held receiver identity changed')
    serials=[r.get('serial') for r in settle]
    require(all(type(i) is int for i in serials) and len(serials)==len(set(serials)), 'DM duplicate settlement')
    expected=set(range(1,n+1))-({3} if held else set())
    # Cancel may retire the awaiting future before the original sender is released.
    if held and stage=='share-cancelled':
        require(set(serials) in (expected,expected|{3}),'DM Cancel settled foreign read')
    else:require(set(serials)==expected,'DM original read settlement inventory')
    for r in settle:
        if case=='held-cancel' and r['serial']==3:
            require(type(r.get('returned')) is bool,'DM original member settlement disposition missing')
            if held:require(r['returned'] is False,'Cancelled held member already returned a reply')
        else:require(r.get('returned') is True,'DM acquisition failed before original response')
    if case=='held-cancel' and stage=='late-settled':
        require(len(release)==1 and release[0].get('serial')==3 and
                type(release[0].get('receiverRetired')) is bool and type(release[0].get('delivered')) is bool and
                release[0]['receiverRetired'] is not release[0]['delivered'],'DM original release disposition missing')
        member=next(r for r in settle if r['serial']==3)
        require(member['returned'] is release[0]['delivered'],'DM held response settlement conflicts with original receiver')
    else:require(release==[],'DM unrequested response release')
    reads=parent.get('reads');require(isinstance(reads,list),'Original ordinary acquisition ledger absent')
    ids=[]
    for i,r in enumerate(reads,1):
        body=r.get('body');require(isinstance(body,list) and len(body)==2,'Ordinary read shape changed during DM Share')
        id=body[0][1];require(id in A.CHANNELS,'Unexpected ordinary destination/open/join')
        require(exact(r,dict(serial=i,method='conversations.info',body=[['channel',id],['include_num_members','false']],
                endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=S.OWNER,
                result=dict(raw=dict(ok=True,channel=A.CHANNELS[id],opaque_receipt='original-share-channel')))),
                'Original ordinary channel acquisition changed')
        ids.append(id)
    require(len(ids)==len(set(ids)) and len(ids)<=5 and (not prefix or reads==[]),'Ordinary acquisition replay/premature read')
    if opened is not None:
        old=opened['proof']['server']['confirmationAdd']['share']
        require(exact(reads,old['reads']),'DM Send repeated ordinary listing')
        require(exact(req[:3],old['dm']['requests']) and exact(res[:3],old['dm']['responses']),
                'Original listing request/response changed during Send')
    return v


def choices(case,stage,q):
    held=case=='held-cancel'
    expected=[] if held else [A.DEST,A.PRIVATE]+([DEST] if case in ('success','rejected') else [])
    rows=q.get('rows');require(isinstance(rows,list) and sorted(r.get('id') for r in rows)==sorted(expected),
                              'DM eligibility admitted wrong/late/missing choice')
    for r in rows:
        if r['id']==DEST:
            require(r.get('label')==LABEL and exact(r.get('raw'),dict(channel(),fromAnotherTeam=False)) and
                    exact(r.get('peer'),dict(id=PEER,raw=peer(),effective=peer(),source=peer_source())),
                    'DM original peer, source, effective facts or label changed')
        else:
            require(r.get('label')==A.CHANNELS[r['id']]['name'] and
                    exact(r.get('raw'),dict(A.CHANNELS[r['id']],fromAnotherTeam=False)) and r.get('peer') is None,
                    'Existing ordinary choice facts changed')
    require(q.get('incomplete') is True,'Excluded/unverified destinations falsely claimed complete')
    if not held:
        require(q.get('loading') is False and q.get('notice')==('Some destinations could not be verified. Refresh to try again.' if stage=='share-open' or case in ('deleted','mismatched-peer') else None),
                'Unverified destination feedback missing')
    else:
        require(q.get('loading') is True,'Cancelled held acquisition changed retained loading state')
    return expected


def validate(fixture,case,receipt,captures,viewport,rectangle,validate_png,*,png=True,dm_case,share_case):
    import workflow_management_share_drafts as drafts
    mode(dm_case,share_case,True);identity(receipt,dm_case)
    require(case=='first-add-success' and receipt.get('shareCase')==share_case and
            receipt.get('shareDraftCustody') is True,'DM explicit base/draft identity')
    require(receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor' and
            receipt.get('firstPublishCase')==case and receipt.get('viewport')==viewport and
            all(receipt.get(k) is True for k in ('syntheticSession','noLiveNetwork','aboutConfiguration')),'DM closed terminal identity')
    for flag,key in [('shareCancelCheck','shareCancel'),('shareResendCheck','shareResend'),('sharePostingCheck','sharePosting')]:
        require(receipt.get(flag,False) is False and receipt.get(key) is None,'Conflicting additive DM mode')
    drafts.validate_setup(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    rows=receipt.get('stages');require(isinstance(rows,list) and tuple(r.get('stage') for r in rows)==STAGES[dm_case], 'Exact DM capture stages')
    require(set(receipt.get('captures',{}))==set(STAGES[dm_case]),'DM PNG inventory')
    checked=opened=None;parent_ids=share_ids=None
    for row in rows:
        stage=row['stage'];p=row['proof'];server=p['server'];product=p['firstPublish'];native=p['native'];model=row['model'];ex=p['exercise']
        parent_ids=common_stage(receipt,row,checked,parent_ids,viewport,rectangle,dm_case)
        ledger=acquisition(dm_case,stage,server,opened)
        cid,ts=send_ledger(dm_case,stage,server)
        if stage not in A.PREFIX:
            a=product['attempts'][-1];q=a.get('confirmationShare');require(isinstance(q,dict),'Actual DM Share observer absent')
            S.operation_proof(share_case,stage,q,server,destination=DEST)
            S.message_proof(share_case,stage,row,cid,ts,destination=DEST)
            require(q.get('bound') is True,'Original DM Share not bound')
            choices(dm_case,stage,q)
            from workflow_management_share_person import options as person_options
            expected_options=person_options(q,server)
            from collections import Counter
            observed=q.get('reads')
            require(isinstance(observed,list),'DM original query read receipts absent')
            expected_reads=[dict(raw=r['result']['raw']) for r in server['confirmationAdd']['share']['reads']]
            expected_reads += [dict(raw=r['raw']) for r in ledger['responses'] if not (dm_case=='held-cancel' and r['serial']==3)]
            encode=lambda v:json.dumps(v,sort_keys=True,ensure_ascii=False)
            require(Counter(map(encode,observed))==Counter(map(encode,expected_reads)),'DM query discarded/fabricated/adopted late original read receipt')
            for key in ('metadata','copy','run','managers','publication','followup','readback'):
                require(exact(a.get(key),checked['proof']['firstPublish']['attempts'][-1].get(key)),'DM changed publication receipt '+key)
            active=stage in ('share-open','share-edited','share-unavailable','share-held')
            require(q.get('active') is active and a.get('closed') is (not active),'DM Cancel/queue lifecycle wrong')
            selected=dm_case in ('success','rejected') and stage!='share-open'
            require(q.get('text')==(S.MESSAGE if selected else '') and q.get('selected')==(DEST if selected else ''),'DM independent text/destination changed')
            if active:
                share_ids=modal(native,q,checked,share_ids,viewport,rectangle)
                controls=native['share']['controls']
                require(exact(native['share'].get('options'),expected_options),'DM original combined conversation/person options changed')
                require(native['share'].get('notice')==q.get('notice'),'DM visible notice differs from original model')
                require(controls['send'].get('disabled') is (not selected),'DM Send permission or loading control wrong')
                require(controls['cancel'].get('disabled') is False,'DM Cancel unavailable during acquisition')
            else:require(native.get('share') is None and native.get('modal') is None,'DM modal remained after closing')
            require(exact(ex.get('after'),F.observation(row)),'DM original action after/capture differs')
            if stage=='share-edited':
                S.activation(ex.get('selectionActivation'),'select',viewport,rectangle)
                S.activation(ex.get('input'),'text',viewport,rectangle)
                require(ex['selectionActivation'].get('value')==DEST,'DM chose different original conversation')
            elif stage=='late-settled':require(exact(ex.get('release'),ledger['releases'][0]),'DM release witness changed')
            elif stage not in ('share-unavailable','share-held'):
                S.activation(ex.get('activation'),'sidebar' if stage=='destination' else 'click',viewport,rectangle)
            if stage in ('share-unavailable','share-held','late-settled') or 'quiet' in ex:
                quiet=ex.get('quiet',{});require(quiet.get('observedMs')==500 and exact(quiet.get('before'),quiet.get('after')) and
                    exact(quiet.get('after'),F.observation(row)),'DM quiet interval changed source/draft or replayed')
            if stage=='destination':
                dest=model['confirmationDestination'];require(dest.get('channel')==DEST and dest.get('team')=='TWORKFLOW' and dest.get('tab')=='Messages','Existing DM actual route missing')
                header=native.get('destination',{}).get('header',{})
                # Existing confirmation/native.js paint() folds original center-hit into visible.
                require(set(header)=={'id','nodeId','text','rect','visible'} and header.get('id')=='' and
                    integer(header.get('nodeId'),'Original IM header node identity missing',1) and header.get('text')==LABEL and header.get('visible') is True,
                    'Actual original peer-labelled IM header paint missing or malformed')
                rectangle(header.get('rect'),viewport)
                timeline=native.get('shareTimeline',{});require(timeline.get('connected') is True and len(timeline.get('rows',[]))==1,'DM rich row not rendered exactly once')
                message=timeline['rows'][0]
                require(message.get('messageId')==(S.SERVER_TS if dm_case=='success' else ts) and
                        all(t in message.get('text','') for t in S.MESSAGE.split('\n')) and
                        any(v.get('href')==S.URL for v in message.get('links',[])),'DM rendered rich content/link changed')
        if stage=='checked':checked=row
        if stage=='share-open':opened=row
        path=Path(receipt['captures'][stage]);require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png','DM capture ownership')
        if png:validate_png(path,row['geometry']['pngDimensions'])
    require(exact(receipt.get('final'),F.observation(rows[-1])) and receipt.get('replacement') is None,'DM final observation changed')
    drafts.validate_return(receipt,fixture,captures,viewport,rectangle,validate_png,png=png)
    return receipt
