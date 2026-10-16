"""Synthetic receipt construction for pure validator regressions only."""
import copy,json,runpy
from pathlib import Path

C=runpy.run_path(str(Path(__file__).with_name('protected_owned_send_receipts.py')))
OLD=runpy.run_path(str(C['SCRIPTS']/'test_protected_drafts_receipts.py'))
uid=OLD['uid'];hit=OLD['hit'];local=C['local'];encoded=C['encoded'];review=C['review']
ACCOUNT=uid(301);NONCE='a'*32;STORAGE=Path(__file__).resolve().parent/('protected-owned-send-'+NONCE)


def good(scenario='newer-before-prepare-channel',fixture='protected-owned-send'):
    target='thread' if scenario.endswith('thread') else 'channel'
    scope=lambda t:C['scope'](ACCOUNT,t)
    initial={}
    for t,n in [('channel',10),('thread',11)]:
        state=dict(scope=scope(t),client_msg_id=uid(n),desired=encoded(C['INITIAL'],t,t=='thread'),local_revision=1,
                   acknowledged_local_revision=0,acknowledged=None,workflow='Idle',tombstone=False,retired_ids=[],consumption=None,
                   check_deadline=None,check_ticket=None,observed_events={},blocked_observation=None)
        initial[t]=dict(scope=scope(t),clock=3,row=dict(scope=scope(t),revision=3,state=state,lineage=dict(enrollment=uid(n+10),verifiedEpoch=2,basis='Local')),
                        local=local(C['INITIAL'],t=='thread'),blocked=None,alternates=[],origins=[],uncommitted=None)
    typed=C['edited'](initial[target],C['REQUESTED'],target=='thread')
    intent=dict(id=uid(100),scope=scope(target),client_msg_id=typed['row']['state']['client_msg_id'],local_revision=2,kind='Send',snapshot=encoded(C['REQUESTED'],target,target=='thread'),base=None)
    cleared=copy.deepcopy(typed);cleared['clock']+=2;cleared['row']['revision']+=1;cleared['local']=local('')
    cleared['row']['state'].update(desired=None,local_revision=3,consumption=intent)
    newer=C['edited'](cleared,C['NEWER'],False)
    op={**intent,'id':uid(101),'consume_id':intent['id']}
    committed=copy.deepcopy(newer);committed['row']['revision']+=2
    committed['row']['state'].update(client_msg_id=uid(102),consumption=None,acknowledged_local_revision=3,workflow='Idle',local_completion=dict(intent=intent,successor_client_msg_id=uid(102)))
    committed['row']['lineage'].update(verifiedEpoch=0,basis='Unverified')
    final=copy.deepcopy(newer);final['row']=None;final['clock']+=1
    inventories=dict(initial=initial[target],input=typed,cleared=cleared,newer=newer,result=final)
    message=dict(channel='C2',ts=C['ACK_TS'],user='U0',client_msg_id=op['client_msg_id'])
    if target=='thread':message['thread_ts']=C['THREAD']
    raw=dict(ok=True,channel='C2',ts=C['ACK_TS'],message=message,fixtureEvidence=dict(retained=True))
    snapshots=[];before=None
    for stage in C['stages'](scenario):
        counter=0 if stage=='initial' else 1 if stage in ('input','cleared') else 2
        text=dict(initial=C['INITIAL'],input=C['REQUESTED'],cleared='',newer=C['NEWER'],result=C['NEWER'])[stage]
        inv=copy.deepcopy(initial);inv[target]=inventories[stage]
        p=dict(account=ACCOUNT,epoch=10,viewId=3,current=True,sourceCurrent=True,bindingCurrent=True,workerCurrent=True,syntheticSession=True,cachePresent=False,
               book={},editors={},operation=None,retainedJobs=[dict(family='protected-inventory',generation=1)],routed=counter,refused=0)
        jobs={'protected-enroll-local':2,'protected-stream-state':1,'protected-open-stream':1,
              'protected-verify-local':2,'protected-register-writer':2,
              'protected-checkpoint':2 if stage=='initial' else 3 if stage in ('input','cleared') else 4}
        if stage in ('cleared','newer','result'):
            jobs.update({name:1 for name in ('protected-consume-send','protected-prepare','protected-claim-dispatch','protected-record')})
        if stage=='result':jobs.update({'protected-release':1,'protected-start-local':1})
        p['retainedJobs'] += [dict(family=family,generation=1) for family,count in jobs.items() for _ in range(count)]
        for t in inv:
            v=inv[t]
            p[t+'Composer']=local('')['composer'] if t=='thread' and target=='channel' else v['local']['composer']
            p['book'][t]=v['local'] if v['local']['composer']['text'] else None
            p['editors'][t]=dict(retentionId='protected-1' if t=='channel' else 'protected-2',writerId=uid(40 if t=='channel' else 41),enrollment=initial[t]['row']['lineage']['enrollment'],
                                serial=1 if t!=target or stage=='initial' else 2 if stage in ('input','cleared') else 3,
                                local=v['local'],managed=None if v['row'] is None else review(v),uncommitted=None,pending=None,running=False,parked=False,error=None,failedInputs=[])
        field_p=dict(account=ACCOUNT,epoch=10,viewId=3,thread=C['THREAD'] if target=='thread' else None,channelGeneration=1,threadGeneration=1)
        g=OLD['measured'](fixture,target,text,field_p,stage,counter);g['proof']=None
        if stage=='input':
            n=g['native'];before=dict(id=target+'-composer',scope=n['scope'],visit=n['visit'],wire=text,start=n['start'],end=n['end'],direction=n['direction'],input=1,composition=0,composing=False,
                                     display_start=n['start'],display_end=n['end'],display_len=n['utf16Length'])
        if stage in ('cleared','newer','result'):
            end=stage=='result'
            p['operation']=dict(id=1,scope=scope(target),epoch=10,serial=2,local=local(C['REQUESTED'],target=='thread'),book=local(C['REQUESTED'],target=='thread'),live=True,
                phase='StartLocal' if end else 'NativeClear',status='Finished' if end else 'Running',native=copy.deepcopy(before),review=review(typed),
                consumed=dict(before=review(typed),after=review(cleared),serial=2,nativeClear='Cleared'),operation=op if end else None,recorded=review(committed) if end else None,
                attempt=dict(dispatched=True,raw=raw,outcome=f'Sent {{ client_msg_id: "{op["client_msg_id"]}", draft_id: None, ts: "{C["ACK_TS"]}" }}',issue=None) if end else None,
                publication=dict(initialInventory=True,pendingSend=True,sendHelperInvoked=True,inventory=True,queue=False,afterReleaseInventory=True) if end else dict.fromkeys(('initialInventory','pendingSend','sendHelperInvoked','inventory','queue','afterReleaseInventory')),
                releasePresent=end,localLeasePresent=end,handoff={'Ok':None} if end else None)
        snapshots.append(dict(stage=stage,geometry=g,durable=dict(managedCount=1 if stage=='result' else 2,localCount=1 if stage=='result' else 0,uncommittedCount=0,scopes=copy.deepcopy(inv)),projection=p))
    native_initial=copy.deepcopy(snapshots[0]['geometry']['native'])
    anchor=dict(expected=dict(scope='timeline',latestId='message',windowEnd=3),connected=True,scope='timeline',windowEnd=3,latestMounted=True,latestHeight=32,inputGeneration=0,scrollTop=100,scrollHeight=300,clientHeight=200,bottomGap=0,ready=True,atBottom=True)
    native_initial['homeAnchor']=dict(landed=True,before=anchor,after=copy.deepcopy(anchor))
    g=snapshots[1]['geometry'];button=dict(x=g['viewport'][0]-50,y=g['viewport'][1]-37,width=28,height=28)
    click=dict(ok=True,kind='composer',target=target,clickRequested=True,clicked=True,disabled=False,fieldUnobscured=True,composerUnobscured=True,buttonUnobscured=True,targetMatches=True,
               scope=g['native']['scope'],visit=g['native']['visit'],fieldFocused=False,buttonFocused=True,viewport=g['viewport'],documentWidth=g['documentWidth'],
               **{k:g[k] for k in ('field','composer','pane','header')},button=button,hitSamples=dict(field=g['hitSamples']['field'],composer=g['hitSamples']['composer'],button=hit(button,True)))
    after={**before,'wire':'','start':0,'end':0,'direction':'forward','display_start':0,'display_end':0,'display_len':0}
    events=[dict(kind='capture',evidence=dict(before=before)),dict(kind='lock',evidence=dict(token='7',before=before)),dict(kind='clear-ack',evidence=dict(token='7',before=before,after=after)),dict(kind='release-requested',evidence=dict(token='7'))]
    fields=[['blocks',json.dumps(op['snapshot']['blocks'],ensure_ascii=False,sort_keys=True,separators=(',',':'))],['channel','C2'],['client_msg_id',op['client_msg_id']]]
    if target=='thread':fields += [['thread_ts',C['THREAD']],['reply_broadcast','true']]
    headers=[['sec-ch-ua-platform','"macOS"'],['Referer','https://app.slack.com/client/'],['User-Agent','Mozilla/5.0 (Macintosh; Intel Mac OS X 26_6_0) AppleWebKit/537.36 (KHTML, like Gecko) Slack/4.50.143 Chrome/148.0.7778.265 Electron/42.4.1 Safari/537.36 AppleSilicon Sonic Slack_SSB/4.50.143'],['sec-ch-ua','"Chromium";v="148", "Slack";v="4", "Not=A?Brand";v="99"'],['Content-Type','application/x-www-form-urlencoded'],['sec-ch-ua-mobile','?0']]
    query='_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required&fp=2123456789ab4def8123456789abcdef&slack_route=default'
    attempts=[dict(method='POST',url='https://owned-send.slack.com/api/chat.postMessage?'+query,headers=headers,fields=fields,retrySafe=False,operation=op,rawResponse=raw,delivered=True)]
    return dict(ok=True,fixture=fixture,scenario=scenario,phase='exercise',nonce=NONCE,pid=123,account=ACCOUNT,
        ownedTicket=json.dumps(dict(version=1,nonce=uid(302),account=ACCOUNT,marker=[1,2],database=[1,3])),isolatedRoot=str(STORAGE),target=target,viewport=C['expected_viewport'](fixture),
        captures={},snapshots=snapshots,details=dict(nativeInitial=native_initial,input=copy.deepcopy(snapshots[1]['geometry']),send=click,newer=copy.deepcopy(snapshots[3]['geometry'])),attempts=attempts,
        actions=[dict(kind='input',key=scope(target)['key'],serial=2,local=local(C['REQUESTED'],target=='thread')),dict(kind='input',key=scope(target)['key'],serial=3,local=local(C['NEWER']))],
        nativeAdmission=events,preparedRequests=1,realHttpRequests=0,syntheticConnectedEvent=True,syntheticSession=True,actualOwnedRoute=True,evidenceScope=C['EVIDENCE_SCOPE'])


def validate(value):C['validate'](value['fixture'],value['scenario'],'exercise',NONCE,123,STORAGE,value)
