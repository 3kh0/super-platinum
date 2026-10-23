"""Independent original-source and dual-pane custody checks for selected emoji facts."""
import copy
import sys
from pathlib import Path
from urllib.parse import urlsplit, parse_qs
from native_check import expected_viewport

PREFIX='channel-thread-emoji'
FLAG='SUPER_PLATINUM_CHANNEL_THREAD_EMOJI_CHECK'
FIXTURES=(PREFIX,PREFIX+'-narrow')
STAGES=('unobserved','assigned-pending','assigned-ready','socket-retired','fresh-assigned-ready')
ACCOUNT='af304cc3-6681-4acd-bf35-3c27c68c555c'
TEAM,USER,CHANNEL='T1','UCHANEMOJI','CCHANEMOJI'
ROOT,REPLY='1781524800.000001','1781524801.000001'
URL='https://fixture.invalid/channel-thread-emoji.png'
RAW={'ok':True,'assignments':{'emoji_v17':{'group':'on'}}}
ISOLATION=dict(syntheticContext=True,noHttp=True,networkDispatched=False,account=ACCOUNT,team=TEAM,user=USER)
CHANNEL_DRAFT='Channel draft café 中'
THREAD_DRAFT='Thread draft kept 🙂 edited'

def require(value,message):
    if not value:raise RuntimeError(message)

def integer(value,minimum=0):return type(value) is int and value>=minimum

def blocks():
    return [dict(type='rich_text',block_id='channel-thread-source',opaque={'keep':True},elements=[dict(type='rich_text_section',elements=[
        dict(type='text',text='Known '),dict(type='emoji',name='conflicting_original',unicode='1f44d-1f3fd',skin_tone=6,display_alias='wrong_alias',display_url=URL),
        dict(type='text',text=' Unknown '),dict(type='emoji',name='unknown_original',unicode='0041',display_url='https://fixture.invalid/unknown-never.png'),
        dict(type='text',text=' Invalid '),dict(type='emoji',name='invalid_original',unicode='1F44D',display_url='https://fixture.invalid/invalid-never.png')])])]

def boot():
    user=dict(id=USER,name='emoji-reviewer',team_id=TEAM,profile={'display_name':'Emoji reviewer'})
    return dict(ok=True,team=dict(id=TEAM,name='Emoji projection'),self=user,users=[user],channels=[dict(id=CHANNEL,name='emoji-projection',is_channel=True,is_member=True,updated=100,previous_names=[])],prefs={})

def context(receipt):
    return receipt.get('noSession') is False and receipt.get('noTransport') is False and receipt.get('isolation')==ISOLATION

def custody(m):
    changing={'requests','witnesses','selected','socket','channelProjection','threadProjection','mediaEpoch','registered','channelWindow','threadWindow'}
    return {k:v for k,v in m.items() if k not in changing}

def opaque(src):
    p=urlsplit(src or '')
    require(not p.username and not p.password and not p.fragment,'Unexpected media URL credentials/fragment')
    require((p.scheme=='super-platinum-media' and p.netloc=='emoji') or (p.scheme=='http' and p.netloc=='super-platinum-media.emoji'),'Original opaque protocol/emoji kind missing')
    q=parse_qs(p.query)
    require(set(q)=={'v'} and len(q['v'])==1 and q['v'][0].isdigit() and p.path.startswith('/') and len(p.path)>1,'Original media epoch/id missing')
    return p.path,int(q['v'][0])

def rectangle(r):
    return isinstance(r,dict) and all(type(r.get(k)) in (int,float) for k in ('x','y','width','height')) and r['width']>0 and r['height']>0

def native(n,fixture,index,m,initial):
    require(n.get('ok') is True and n.get('viewport')==expected_viewport(fixture) and n.get('sameOriginalNodes') is True,'Original dual pane native identity/viewport absent')
    narrow=fixture.endswith('-narrow')
    for key,text,start,end in [('channelComposer',CHANNEL_DRAFT+('' if narrow else ' edited'),2,7),('threadComposer',THREAD_DRAFT,1,6)]:
        c=n.get(key,{})
        require(c.get('value')==text and c.get('start')==start and c.get('end')==end and c.get('connected') is True
            and isinstance(c.get('scope'),str) and bool(c['scope']) and isinstance(c.get('mount'),str) and bool(c['mount']) and rectangle(c.get('rect')),'Original independent composer text/node/selection changed')
        if key=='threadComposer' or not narrow:
            require(c.get('exposed') is True and isinstance(c.get('hitPoint'),dict),'Exposed original composer point missing')
        if initial is not None:
            for field in ('value','start','end','direction','scope','mount','connected'):
                require(c.get(field)==initial[key].get(field),'Original independent composer binding/caret retired')
    ready=index in (2,4);selected=index in (1,2,4)
    glyphs=[dict(text='A',title=':unknown_original:')]
    if not selected:glyphs.insert(0,dict(text='👍🏽',title=':conflicting_original:'))
    elif not ready:glyphs.insert(0,dict(text=':+1::skin-tone-4:',title=':+1::skin-tone-4:'))
    for surface,ids,stamp,scope in [('channel',[ROOT],'channelProjection','channelScope'),('thread',[ROOT,REPLY],'threadProjection','threadScope')]:
        s=n.get('surfaces',{}).get(surface,{})
        require(s.get('generation')==str(m[stamp]) and s.get('scope')==m[scope] and rectangle(s.get('rect')),'Original surface presentation stamp/scope missing')
        rows=s.get('rows',[])
        require([r.get('id') for r in rows]==ids and [r.get('index') for r in rows]==list(range(len(ids))),'Canonical original row identity/indices changed')
        for row in rows:
            require(type(row.get('height')) in (int,float) and row['height']>0 and rectangle(row.get('rect')) and rectangle(row.get('bodyExposure',{}).get('rect')),'Original row/body measurements absent')
            require(row.get('glyphs')==glyphs,'Unknown/invalid/absent-map glyph projection changed')
            images=row.get('images')
            require(isinstance(images,list) and len(images)==int(ready),'Unobserved/unknown/invalid URL painted an image')
            if ready:
                image=images[0]
                require(image.get('complete') is True and image.get('width')==16 and image.get('height')==16 and image.get('alt')==':+1::skin-tone-4:' and image.get('title')==':+1::skin-tone-4:','Original supplied image canonical label/decode changed')
                opaque(image.get('src'))
            require(row.get('body')=='Known '+('' if ready else ':+1::skin-tone-4:' if selected else '👍🏽')+' Unknown A Invalid ','Original message body changed')

def source(m,index,initial):
    require(m.get('contextCurrent') is True and m.get('canonical') is True and m.get('account')==ACCOUNT and m.get('team')==TEAM and m.get('user')==USER and m.get('channel')==CHANNEL and m.get('root')==ROOT and integer(m.get('epoch')),'Original canonical source/context missing')
    require(m.get('boot')==boot() and m.get('socket')==(7 if index<3 else 8),'Original Boot/socket source changed')
    require(m.get('channelRows')==[ROOT] and m.get('threadRows')==[ROOT,REPLY] and m.get('channelPending')==[] and m.get('threadPending')==[] and m.get('pendingScroll') is None,'Canonical row/pending target changed')
    require(m.get('channelWindow')==[0,1] and m.get('threadWindow')==[0,2],'Bounded canonical window changed')
    for name,ids in [('channelRaw',[ROOT]),('threadRaw',[ROOT,REPLY])]:
        rows=m.get(name,[]);require([r.get('ts') for r in rows]==ids,'Original raw bag changed')
        for row in rows:
            require(row.get('user')==USER and row.get('thread_ts')==ROOT and row.get('text')=='Known unknown invalid' and row.get('blocks')==blocks() and row.get('reply_count')==(1 if row['ts']==ROOT else 0),'Original semantic Unicode/URL/alias raw changed')
    require(m.get('registered')==dict(known=int(index>0),unknown=0,invalid=0),'Unobserved/unknown/invalid media registration changed')
    require(m.get('customEmoji')=={} and m.get('attachments')==[] and m.get('threadAttachments')==[],'Unexpected custom catalog/attachments')
    platform='windows' if sys.platform=='win32' else 'darwin' if sys.platform=='darwin' else 'linux'
    count=(0,1,1,1,2)[index]
    expected=[dict(serial=i+1,socket=7+i,method='POST',path='/api/experiments.getByUser',args=dict(ssb_platform=platform,ssb_release_channel='prod'),tokenPresent=True,factoryMatched=True,response=RAW) for i in range(count)]
    require(m.get('requests')==expected,'Original factory/request count/owner changed')
    witnesses=m.get('witnesses',[]);require(len(witnesses)==count,'Original selected receipt omitted')
    for i,w in enumerate(witnesses):
        o=w.get('owner',{})
        require(w.get('current') is (i==count-1 and index!=3 and (i>0 or index<3)) and w.get('version')=='v17' and w.get('response')==RAW
            and set(o)=={'account','epoch','team','user','socket','slotSerial'} and o.get('account')==ACCOUNT and o.get('epoch')==m['epoch'] and o.get('team')==TEAM and o.get('user')==USER and o.get('socket')==7+i and integer(o.get('slotSerial'),1),'Original assignment handle laundered or stale selected map retained')
    require(m.get('selected')==(witnesses[-1] if index in (1,2,4) else None),'Missing observation received fabricated default/current map')
    if initial is not None:require(custody(m)==custody(initial),'Source/draft/navigation changed during map/media transition')

def validate(receipt,fixture):
    require(fixture in FIXTURES and receipt.get('ok') is True and context(receipt),'Unknown/nonclosed channel-thread fixture')
    stages=receipt.get('stages',[])
    require([s.get('stage') for s in stages]==list(STAGES),'Missing/reordered original native stages')
    initial=stages[0].get('model',{});first_native=stages[0].get('native',{})
    for index,s in enumerate(stages):
        require(s.get('ok') is True and s.get('ordinal')==index and type(s.get('ordinal')) is int and context(s),'Original stage order/isolation changed')
        m=s.get('model',{});n=s.get('native',{});p=s.get('proof',{})
        source(m,index,initial if index else None);native(n,fixture,index,m,first_native if index else None)
        require(integer(m.get('channelProjection'),1) and integer(m.get('threadProjection'),1),'Original content generations absent')
        current=s.get('measurementGuard',{}).get('current',{})
        require(current==dict(kind='retained-original-DOM-measurement',asyncCallbackHeld=False,sample=n['surfaces'],channelAccepted=True,threadAccepted=True),'Current original measurement not accepted by exact guard')
        old=s['measurementGuard'].get('old')
        if index==0:require(old is None,'Unexpected old measurement')
        else:
            held=stages[0 if index<3 else index-1]['native']['surfaces']
            require(old==dict(kind='retained-original-DOM-measurement',asyncCallbackHeld=False,sample=held,channelAccepted=False,threadAccepted=False),'Retained old original measurement was not refused')
        setup=p.get('setup',{});inputs=setup.get('inputs',[])
        require(setup.get('ok') is True and len(inputs)==2 and [a.get('target') for a in inputs]==['channel','thread'],'Original input setup absent')
        for i,action in enumerate(inputs):
            if i==0 and fixture.endswith('-narrow'):
                require(action.get('kind')=='preseeded-occluded' and action.get('before',{}).get('value')==CHANNEL_DRAFT,'Narrow occluded input was fabricated')
            else:require(action.get('kind')=='synthetic-native-input' and action.get('isTrusted') is False and action.get('before',{}).get('exposed') is True and action.get('value')==n['channelComposer' if i==0 else 'threadComposer']['value'],'Original visible input event missing')
        require(p.get('arm',{}).get('ok') is True,'Original caret arming absent')
        if index:
            require(p.get('setup')==stages[0]['proof']['setup'] and p.get('arm')==stages[0]['proof']['arm'],'Original setup/caret proof replaced')
            require(p.get('assignment1')==stages[1]['model']['selected'],'First actual assignment receipt changed')
        if index>=2:
            media=p.get('media',{});path,epoch=opaque(media.get('uriBefore'))
            require(media.get('url')==URL and media.get('mime')=='image/png' and media.get('readyBefore') is False and media.get('readyAfter') is True and media.get('bytes')==len((Path(__file__).resolve().parent.parent/'assets/icons/png/icon_16.png').read_bytes()) and media.get('epochBefore')==epoch,'Original registry PNG delivery changed')
            require(media.get('id')=='super-platinum-media://emoji'+path,'Original delivered opaque identity changed')
            if index in (2,4):
                for surface in n['surfaces'].values():
                    for row in surface['rows']:
                        actual,generation=opaque(row['images'][0]['src']);require(actual==path and epoch<=generation<=m['mediaEpoch'],'Current native image not original supplied asset/epoch')
        if index>=3:
            retirement=p.get('retirement',{})
            require(retirement==dict(kind='synthetic-socket-connected',before=stages[1]['model']['selected'],after=dict(stages[1]['model']['selected'],current=False),socket=8,accessorAvailable=False,networkDispatched=False,workResumed=False),'Actual socket retirement refused/old map adopted')
        if index==4:
            require(p.get('assignment2')==m['selected'] and m['selected']['owner']['slotSerial']!=stages[1]['model']['selected']['owner']['slotSerial'],'Fresh socket source reused original slot')
    expected_channel=CHANNEL_DRAFT+('' if fixture.endswith('-narrow') else ' edited')
    require(initial.get('composer')==expected_channel and initial.get('threadComposer')==THREAD_DRAFT,'Independent dirty drafts missing')
    keys=[dict(team=TEAM,channel=CHANNEL,thread=None),dict(team=TEAM,channel=CHANNEL,thread=ROOT)]
    drafts=initial.get('drafts',{});items=drafts.get('items',[])
    require(drafts.get('channel')==keys[0] and drafts.get('thread')==keys[1] and [x.get('key') for x in items]==keys,'Independent original draft keys missing')
    require([x.get('value',{}).get('composer',{}).get('text') for x in items]==[expected_channel,THREAD_DRAFT] and all(x.get('value',{}).get('attachments')==[] for x in items),'Original captured DraftBook differs from native input')
    require(receipt.get('layout')==stages[-1]['native'] and receipt.get('proof')==stages[-1]['proof'] and custody(receipt.get('model',{}))==custody(initial),'Terminal original proof/custody changed')
    source(receipt['model'],4,initial)
