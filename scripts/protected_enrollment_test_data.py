"""Synthetic receipts for protocol tests, never native evidence."""
import copy
import math
from pathlib import Path
import runpy

C=runpy.run_path(str(Path(__file__).with_name('protected_enrollment_receipts.py')))
OLD=runpy.run_path(str(C['SCRIPTS']/'test_protected_drafts_receipts.py'))
uid=OLD['uid'];clone=copy.deepcopy
NONCE='a'*32
STORAGE=Path(__file__).resolve().parent/('protected-enrollment-'+NONCE)


def hit(r,rounded=False):
    if not rounded:return OLD['hit'](r)
    w,h=r['width'],r['height'];d=6-5/math.sqrt(2)
    points=[('center',w/2,h/2),('top',w/2,1),('right',w-1,h/2),('bottom',w/2,h-1),('left',1,h/2),
            ('top-left',d,d),('top-right',w-d,d),('bottom-right',w-d,h-d),('bottom-left',d,h-d)]
    return dict(ok=True,method='rounded-arc',computedRadii=['6px']*4,radius=6,inset=1,points=[dict(name=n,x=r['x']+x,y=r['y']+y,owned=True,element=dict(tag='BUTTON',id='',classes='')) for n,x,y in points])


def button(control,text,disabled=False):
    r=dict(x=230,y=300,width=150,height=28)
    return dict(id='protected-enrollment-'+control if control!='close' else '',text=text,label='Close enrollment' if control=='close' else '',rect=r,disabled=disabled,hit=hit(r,True),visible=True)


def click(control,text,generation=None):
    return dict(ok=True,control=control,generation=generation,clicked=True,proof=button(control,text),bodyScroll=0 if generation else None,focused=True)


def page(fixture,scenario,stage,name,target,generation):
    w,h=C['expected_viewport'](fixture);modal=name!='status';busy=stage in ('prepared','enqueued','interleaved') and modal
    r=dict(x=200,y=50,width=450,height=280)
    bounds=dict(panel=r,selected=dict(x=210,y=120,width=420,height=100)) if modal else dict(panel=r,selected=clone(r))
    if modal:bounds.update(header=dict(x=200,y=50,width=450,height=60),body=dict(x=200,y=110,width=450,height=160),footer=dict(x=200,y=270,width=450,height=60))
    if name=='modal':bounds['selected']=clone(bounds['body'])
    text='';buttons={};initial=C['INITIAL'];newer=C['NEWER']
    channel='Channel draft'+initial+'Saved on this device';thread='Thread draft'+initial+'Saved on this device'
    pending='Input waiting to be saved'+newer+'Recover this input before enabling recovery for another version.'
    if modal:
        text={'channel':channel,'thread':thread,'pending':pending,'modal':'Keep this draft with recovery on this device. Enabling recovery does not send the draft or change its text.'+channel+thread+(pending if scenario=='failed-editor-channel' else '')}[name]
        buttons={'close':button('close',''),'protected-enrollment-cancel':button('cancel','Cancel waiting' if busy else 'Cancel'),'protected-enrollment-submit':button('submit','Enabling recovery...' if busy else 'Enable recovery',busy)}
    else:
        pending_result=scenario=='canceled-waiter-channel' and stage=='interleaved'
        text=C['message'](scenario,stage)+('Check status' if pending_result else '')+'Reload saved draftsReview draft'
        buttons={'protected-enrollment-reload':button('reload','Reload saved drafts'),'protected-enrollment-open':button('open','Review draft',pending_result)}
        if pending_result:buttons['protected-enrollment-check']=button('check','Check status')
    return dict(ok=True,page=name,generation=generation if modal else None,surfaces=bounds,hits={k:hit(v) for k,v in bounds.items()},buttons=buttons,
                scroll=dict(before=dict(document=0,body=0 if modal else None),after=dict(document=0,body=0 if modal else None),bodyHeight=160 if modal else None,bodyScrollHeight=400 if modal else None),viewport=[w,h],documentWidth=w,
                text=text,title='Enable '+target+' draft recovery?' if modal else None,rowKinds=['channel','thread'] if modal else [],pending=modal and scenario=='failed-editor-channel')


def observed(g):
    n=g['native']
    return dict(ok=True,target=g['target'],before={k:n[k] for k in ('text','start','end','direction')},native=clone(n),readOnly=False,disabled=False,admission=None,fieldFocused=True,activeId=g['target']+'-composer',visibilityEvidence='not-measured')


def enrolled(target):
    inv=C['empty_inventory'](target);inv['clock']=5
    inv['row']=dict(scope=C['scope'](target),revision=1,lineage=dict(enrollment=uid(30),verifiedEpoch=0,basis='Unverified'),state=dict(scope=C['scope'](target),client_msg_id=uid(31),desired=C['encoded'](C['INITIAL'],target,target=='thread'),local_revision=1,acknowledged_local_revision=0,acknowledged=None,workflow='Idle',tombstone=False,retired_ids=[],consumption=None,check_deadline=None,check_ticket=None,observed_events={},blocked_observation=None))
    return inv


def good(scenario='enroll-channel',fixture='protected-enrollment'):
    target='thread' if scenario.endswith('thread') else 'channel';failure=scenario=='failed-editor-channel';late=scenario=='late-typing-channel';canceled=scenario=='canceled-waiter-channel'
    replaced=scenario in ('owner-change-channel','canceled-waiter-channel');success=scenario.startswith('enroll-') or canceled;generation='protected-20'
    initial={k:C['empty_inventory'](k,6 if failure and k=='channel' else 4) for k in ('channel','thread')};final=clone(initial)
    if success:final[target]=enrolled(target)
    if late:final[target]=C['empty_inventory'](target,7,C['NEWER'])
    result=dict(kind='enrolled',current=not canceled,account=C['ACCOUNT'],expected=initial[target],review=C['review'](final[target])) if success else dict(kind='refused',phase='capture' if failure else 'prepare-or-admit',error='Stale' if scenario=='owner-change-channel' else 'ReviewRequired')
    details=dict(open=click('open','Review draft'),submit=click('submit','Enable recovery',generation),reload=click('reload','Reload saved drafts'),result=clone(result))
    if canceled:details.update(cancel=click('cancel','Cancel waiting',generation),check=click('check','Check status'),canceled=dict(kind='canceled',pending=True))
    records=[];actions=[dict(kind='current-load',epoch=10,viewId=3)];trace=[]
    for stage in C['stages'](scenario):
        after_input=failure and stage!='initial' or late and stage in ('interleaved','result','reloaded');after_replace=replaced and stage in ('interleaved','result','reloaded')
        text=C['NEWER'] if after_input else C['INITIAL'];epoch=11 if after_replace else 10;view=4 if after_replace else 3
        p=dict(account=C['ACCOUNT'],epoch=epoch,viewId=view,team='T1',channel='C2',thread=C['THREAD'] if target=='thread' else None,channelGeneration=1,threadGeneration=1,fieldScopes={},sessionPresent=False,transportPresent=False,cachePresent=False,isolated=True,book={},channelComposer=None,threadComposer=None,discovery=None,retained=[],localWriters=[],routed=int(after_input),refused=0,fixtureActions=[],nativeAdmission=[],result=None)
        p['fieldScopes']={k:C['field_scope'](p,k) for k in ('channel','thread')}
        p['book']={k:C['local'](text if k==target else C['INITIAL'],k=='thread') for k in ('channel','thread')}
        p['channelComposer']=clone(p['book']['channel']['composer']);p['threadComposer']=clone(p['book']['thread']['composer'] if target=='thread' else C['local']('')['composer'])
        p['localWriters']=[dict(key=C['scope'](k)['key'],serial=2 if after_input and not failure and k==target else 1,pending=None,running=False,error=None,failedInputs=[]) for k in ('channel','thread') if not(failure and k=='channel')]
        if failure:p['retained']=[dict(retentionId='protected-1',scope=C['scope']('channel'),writerId=uid(40),enrollment=uid(41),serial=int(after_input),pending=C['local'](C['NEWER']) if after_input else None,failedInputs=[C['local'](C['NEWER'])] if after_input else [],running=False,parked=bool(after_input),error='Storage' if after_input else None)]
        inv=final if late and after_input or success and stage in ('enqueued','interleaved','result','reloaded') else initial
        pending=canceled and stage in ('enqueued','interleaved');status='NeedsLoad' if success and stage in ('enqueued','interleaved','result') or after_replace and stage!='reloaded' else 'Ready'
        p['discovery']=C['displayed'](final if stage=='reloaded' else initial,status,pending)
        p['result']=clone(result if stage in ('result','reloaded') else details['canceled'] if canceled and stage=='interleaved' else None)
        g=OLD['measured'](fixture,target,text,p,stage,int(after_input));n=observed(g)
        if stage=='initial':
            details['nativeInitial']=clone(g['native']);anchor=dict(expected=dict(scope='timeline',latestId='message',windowEnd=3),connected=True,scope='timeline',windowEnd=3,latestMounted=True,latestHeight=32,inputGeneration=0,scrollTop=100,scrollHeight=300,clientHeight=200,bottomGap=0,ready=True,atBottom=True)
            details['nativeInitial']['homeAnchor']=dict(landed=True,before=anchor,after=clone(anchor))
        if stage=='input' or late and stage=='interleaved':
            actions.append(dict(kind='input',key=C['scope'](target)['key'],serial=1 if failure else 2,local=C['local'](C['NEWER'],target=='thread')));details['typed']=clone(n)
            details['typed']['before']={k:records[0]['native']['native'][k] for k in ('text','start','end','direction')}
        if stage=='dialog':
            actions.extend([dict(kind='open-click'),dict(kind='current-load',epoch=10,viewId=3),dict(kind='dialog',id=generation,displayed=C['displayed'](initial),pending=[C['local'](C['NEWER'])] if failure else [])]);trace=[dict(kind='capture',evidence=dict(before=clone(g['proof']['current'])))]
        if stage in ('prepared','enqueued') or failure and stage=='result':
            actions.append(dict(kind='consent-click',id=generation))
            if not failure:actions.append(dict(kind='prepared',expected=initial[target]))
        if stage=='enqueued' or scenario.startswith('enroll-') and stage=='result':
            actions.append(dict(kind='enqueue',expected=initial[target]));trace.extend([dict(kind='lock',evidence=dict(token='7',before=clone(trace[0]['evidence']['before']))),dict(kind='release-requested',evidence=dict(token='7'))])
        if after_replace and stage=='interleaved':
            before=clone(records[-1]['projection'])
            if canceled:
                actions.append(dict(kind='cancel-click',pending=True));before['fixtureActions']=clone(actions);before['result']=clone(details['canceled'])
            actions.append(dict(kind='shell-replaced',before=dict(epoch=10,viewId=3),after=dict(epoch=11,viewId=4)));details['replacement']=dict(before=before,after=None)
        if canceled and stage=='result':actions.append(dict(kind='check-click'))
        if stage=='reloaded':actions.extend([dict(kind='reload-click'),dict(kind='current-load',epoch=epoch,viewId=view)])
        p['fixtureActions']=clone(actions);p['nativeAdmission']=clone(trace)
        if after_replace and stage=='interleaved':details['replacement']['after']=clone(p)
        pages=[page(fixture,scenario,stage,name,target,generation) for name in C['pages'](scenario,stage)]
        records.append(dict(stage=stage,pages=pages,native=n,composer=g if C['pages'](scenario,stage)==('status',) else None,durable=clone(inv),projection=p))
    return dict(ok=True,fixture=fixture,scenario=scenario,phase='exercise',nonce=NONCE,pid=123,isolatedRoot=str(STORAGE),target=target,viewport=C['expected_viewport'](fixture),captures={},snapshots=records,details=details,finalInventory=clone(final),finalProjection=clone(records[-1]['projection']),evidenceScope='protected local discovery/enrollment consent; fixture construction has no HTTP capability')


def validate(receipt):return C['validate'](receipt['fixture'],receipt['scenario'],'exercise',NONCE,123,STORAGE,receipt)
