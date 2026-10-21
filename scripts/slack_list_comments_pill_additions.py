"""Closed existing-pill additions: actual acquisition, exact wires and original custody."""
import copy
import re
import runpy
from pathlib import Path
S=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_selection.py')))
X,I,R,C,H,READ=(S[k] for k in ('X','I','R','C','H','READ'))
require,exact,rectangle=S['require'],S['exact'],S['rectangle']
CHANNEL,FIRST,ROOT,USER=S['CHANNEL'],S['FIRST'],S['ROOT'],'U0'
STAGES={'pill-additions':('draft','cold-loading','wave-added','single-added','multi-added','paired-added','raw-added','alias-added'),
        'pill-additions-custody':('draft','acquisition-failed','retry-loading','preferences-retired','catalog-held','catalog-retired','prefs-held','prefs-retired','unknown','readback')}
RAW=['wave','wave::skin-tone-3','handshake','handshake::skin-tone-2-6','thumbsup','+1']
RESOLVED=['wave::skin-tone-4',RAW[1],'handshake::skin-tone-4',RAW[3],'thumbsup::skin-tone-4','+1::skin-tone-4']
GLYPHS=dict(zip(RAW+RESOLVED,['\U0001f44b','\U0001f44b\U0001f3fc','\U0001f91d','\U0001faf1\U0001f3fb\u200d\U0001faf2\U0001f3ff','\U0001f44d','\U0001f44d',
 '\U0001f44b\U0001f3fd','\U0001f44b\U0001f3fc','\U0001f91d\U0001f3fd','\U0001faf1\U0001f3fb\u200d\U0001faf2\U0001f3ff','\U0001f44d\U0001f3fd','\U0001f44d\U0001f3fd']))
PREFS=dict(preferred_skin_tone=4,emoji_mode='native',emoji_use={},frecency='{}')
PREF_RESPONSE=dict(ok=True,prefs=PREFS,fixture_opaque='original cold preference response')
LOADING='Loading emoji and preferences. Your draft is unchanged.'
FAILED='Emoji details could not load. Click the reaction to retry. Your draft is unchanged.'
CHANGED='The comment or emoji details changed. Click the reaction to retry. Your draft is unchanged.'
CHECKING='Checking the current reaction. Your draft is unchanged.'


def validator(case):
    require(case in STAGES,'Unknown pill addition case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)


def reactions(name=None):
    rows=[dict(name=n,count=1,users=['U1']) for n in RAW]
    if name:
        row=next((r for r in rows if r['name']==name),None)
        if row:row.update(count=2,users=['U1',USER])
        else:rows.append(dict(name=name,count=1,users=[USER]))
    return rows


def page(rx):
    raw=X['original_page']();raw['messages'][1].update(reactions=rx,pill_additions_opaque={'keep':'original raw names and independent membership'})
    return raw


def prefs(case,i):
    if case=='pill-additions':return dict(loaded=i>=2,revision=int(i>=2),raw=copy.deepcopy(PREFS) if i>=2 else {})
    if i<3:return dict(loaded=False,revision=0,raw={})
    if i==3:return dict(loaded=False,revision=1,raw={'preferred_skin_tone':5})
    raw=copy.deepcopy(PREFS)
    if i>=7:raw['preferred_skin_tone']=5
    return dict(loaded=True,revision=2+int(i>=7),raw=raw)


def spec(case,i):
    if case=='pill-additions':
        n=max(0,i-1);return dict(reads=4+4*n,posts=n,phases=['confirmed']*n,preflights=n,
            pref_count=int(i>0),pref_done=int(i>1),pref_held=i==1,addition='loading' if i==1 else None,
            target=RESOLVED[i-2] if i>1 else RAW[0],owned=RESOLVED[i-2] if i>1 else None)
    return dict(reads=[4,4,4,4,8,8,12,12,16,20][i],posts=int(i>=8),
        phases=[[],[],[],[],['preparing'],['not-sent'],['not-sent','preparing'],['not-sent']*2,['not-sent','not-sent','unknown'],['not-sent','not-sent','observed']][i],
        preflights=[0,0,0,0,1,1,2,2,3,3][i],pref_count=[0,1,2,2,3,3,3,3,3,3][i],pref_done=[0,1,1,2,3,3,3,3,3,3][i],pref_held=i==2,
        addition='failed' if i in (1,3) else 'loading' if i==2 else None,target=RAW[1],owned=RAW[1] if i==9 else None)


def source(rows,case):
    first=rows[0]['model']['additionSource'];revision=first.get('customRevision');require(R['integer'](revision),'Custom revision missing')
    token=None;successor=None
    for i,row in enumerate(rows):
        s=row['model']['additionSource'];e=spec(case,i);custody=case!='pill-additions'
        require(set(s)=={'preferences','customRevision','catalog','requests','outcomes','held','completed','cancelled'},'Preference observation shape changed')
        require(exact(s['preferences'],prefs(case,i)) and exact(s['customRevision'],revision+int(custody and i>=5)), 'Original full/partial preference adoption changed')
        modes=(['Failure','Held','Success'] if custody else ['Held'])[:e['pref_count']]
        requests=[dict(method='users.prefs.get',args={'prefs':'preferred_skin_tone,emoji_mode,emoji_use,frecency'},mode=m) for m in modes]
        outcomes=[]
        for j in range(e['pref_done']):
            failed=custody and j==0
            outcomes += [dict(kind='response',raw=None if failed else PREF_RESPONSE,failed=failed),
                         dict(kind='adoption',failed=failed,preferences=prefs(case,[1,3,4][j] if custody else 2))]
        require(exact(s['requests'],requests) and exact(s['outcomes'],outcomes) and exact(s['completed'],e['pref_done'])
                and s['held'] is e['pref_held'] and exact(s['cancelled'],0),'Original preference request/reply/adoption custody changed')
        c=s['catalog']
        if i==0 or custody and i==5:require(c is None,'Cold or invalidated catalog retained')
        elif c is None:
            require((not custody and i==1) or (custody and i in (1,2,3)), 'Required acquired catalog missing')
        else:
            require(set(c)=={'token','current'} and c['current'] is True and isinstance(c['token'],str)
                    and re.fullmatch(r'[1-9][0-9]*/local',c['token']) and int(c['token'].split('/')[0])<=2**64-1,'Wrong immutable local catalog token')
            if custody and i>=6:
                if successor is None:successor=c['token'];require(token is not None and int(successor.split('/')[0])>int(token.split('/')[0]),'Catalog replacement reused ticket')
                require(c['token']==successor,'Successor catalog changed')
            else:
                if token is None:token=c['token']
                require(c['token']==token,'Original catalog changed')


def attempt_rows(case,i,ids,target):
    e=spec(case,i);out=[]
    for j,phase in enumerate(e['phases']):
        observed=phase=='observed';confirmed=phase=='confirmed'
        out.append(dict(id=ids[j],target=target,ts=FIRST,name=RESOLVED[j] if case=='pill-additions' else RAW[1],user=USER,adding=True,
            phase=phase,raw={'ok':True} if confirmed else None,socketObserved=False,socketRevision=0,canonicalObserved=observed,
            readbackResolved=observed,published=confirmed,retryAt=0,fullReadback=None))
    return dict(attempts=out,pending=sum(p in ('preparing','unknown') for p in e['phases']))


def wire(case,i,ids):
    e=spec(case,i);indices=range(e['posts']) if case=='pill-additions' else ([2] if e['posts'] else [])
    entries=[dict(id=ids[j],user=USER,request=dict(method='reactions.add',args=dict(channel=CHANNEL,timestamp=FIRST,name=RESOLVED[j] if case=='pill-additions' else RAW[1]))) for j in indices]
    outcomes=[dict(id=r['id'],disposition='response' if case=='pill-additions' else 'unconfirmed',raw={'ok':True} if case=='pill-additions' else None) for r in entries]
    return dict(prepared=entries,issued=copy.deepcopy(entries),outcomes=outcomes,completed=e['posts'],held=False,networkCalls=0,fullReads=[])


def status(case,i):
    e=spec(case,i)
    if e['addition']:return LOADING if e['addition']=='loading' else FAILED if i==1 else CHANGED
    if not e['phases']:return None
    return CHECKING if e['phases'][-1]=='preparing' else X['NOTICES'][e['phases'][-1]]


def geometry(row,case,i):
    m,g,p=row['model'],row['geometry'],row['proof'];e=spec(case,i);rx=reactions(e['owned']);v=g['viewport'];a=g.get('additions',{})
    blocked_all=e['addition']=='loading' or 'preparing' in e['phases'];unknown=e['phases'] and e['phases'][-1]=='unknown'
    expected=[dict(name=r['name'],count=r['count'],own=USER in r['users'],glyph=GLYPHS[r['name']],available=not(blocked_all or unknown and r['name'].split('::')[0]=='wave')) for r in rx]
    require(exact(m.get('additionProbe'),expected) and exact(m.get('actionProbe'),{'status':status(case,i)}),'Addition projection/admission/status changed')
    require(a.get('target')==e['target'] and a.get('picker') is False and exact(a.get('tools'),0) and m.get('picker') is None,'Pill click opened unrelated picker/tools')
    pills=a.get('pills',[]);require(len(pills)==len(expected),'Named pill count changed')
    for pill,exp in zip(pills,expected):
        require(all(exact(pill.get(k),exp[k]) for k in ('name','count','own')) and pill.get('tag')=='BUTTON' and pill.get('type')=='button'
            and pill.get('pressed')==str(exp['own']).lower() and pill.get('disabled') is (not exp['available'])
            and pill.get('ariaDisabled')==str(not exp['available']).lower() and type(pill.get('hit')) is bool and type(pill.get('visible')) is bool,'Pill semantics changed')
        if exp['name']==e['target']:
            require(pill['visible'] and pill['hit'],'Selected pill not readable/hit-testable');rectangle(pill['rect'],v)
    views=p.get('pillViews',[]);require(len(views)==len(expected),'Per-pill native scroll witnesses missing')
    for view,exp in zip(views,expected):
        require(view.get('name')==exp['name'] and view.get('ok') is True and view.get('hit') is True,'Pill native scroll/hit proof changed');rectangle(view.get('rect'),v)
    feedback=a.get('status');text=status(case,i)
    if text is None:require(feedback is None,'Unexpected status')
    else:
        require(feedback and feedback.get('text')==text and feedback.get('role')=='status' and feedback.get('visible') is True and text in g.get('notices',[]),'Addition guidance missing')
        rectangle(feedback.get('rect'),v)
    for control in g['controls']:
        blocked=case=='pill-additions-custody' and i in (4,6) and control['id'] in ('slack-list-comments-next','slack-list-comments-refresh')
        require(control.get('disabled') is blocked,'Read/comment controls changed')


def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
    case=receipt.get('case');require(case in STAGES and (expected_case is None or case==expected_case),'Wrong pill addition case')
    rows,initial,_=S['common'](fixture,receipt,directory,case,dimensions,png,stages=STAGES[case],open_stages=(),compose_held_stages=('catalog-held','prefs-held'))
    source(rows,case);ids=[a['id'] for a in rows[-1]['model']['reactionActions']['attempts']]
    require(len(ids)==(6 if case=='pill-additions' else 3) and all(R['integer'](n,1) for n in ids) and ids==sorted(set(ids)),'Attempt IDs missing/reused')
    target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=CHANNEL,root=ROOT)
    original=page(reactions());inputs=[];releases=[];updates=[];events=[];changes=[];addition_tokens=[];oldread=None
    for i,row in enumerate(rows):
        m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments'];e=spec(case,i);rx=reactions(e['owned']);custody=case!='pill-additions'
        require(exact(m.get('reactionActions'),attempt_rows(case,i,ids,target)),'Addition outcome/original response changed')
        require(exact(m.get('reactionLedger'),wire(case,i,ids)),'Exact resolved once-only addition wire changed')
        raw=page(rx) if custody and i==9 else original;typed=[I['typed'](v) for v in raw['messages']];typed[1]['reactions']=rx
        require(exact(c.get('page'),raw) and exact(c.get('pageMessages'),typed) and exact(p.get('initialResponse'),original),'Original raw Page/typed projection changed')
        short=[dict(name=r['name'],count=r['count']) for r in rx]
        require(exact(c.get('reactions'),[dict(ts=FIRST,reactions=short)]) and c.get('visibleTs')==[FIRST]
                and len(g.get('rows',[]))==1 and g['rows'][0].get('ts')==FIRST and exact(g['rows'][0].get('reactions'),short),'DOM/model reaction rows differ')
        X['reads'](row,initial['ledger']['requests'],e['reads'],e['preflights'])
        require(c.get('loading') is (custody and i in (4,6)) and c.get('error') is None and exact(c.get('retryAt'),0) and exact(c.get('retryAfter'),0),'Unexpected comment read/error/cooldown')
        require(R['integer'](c.get('read'),1),'Read serial missing')
        if oldread is not None:
            changed=(i>=2 if not custody else i in (4,6,8,9))
            require(c['read']>oldread if changed else c['read']==oldread,'Automatic or missing read/preflight')
        oldread=c['read'];addition=m.get('addition')
        if e['addition']:
            require(isinstance(addition,dict) and R['integer'](addition.get('token'),1),'Acquisition invocation token missing')
            expected=dict(token=addition['token'],generation=c['generation'],ts=FIRST,requested=RAW[1] if custody else RAW[0],phase=e['addition'],notice=status(case,i))
            require(exact(addition,expected),'Acquisition ownership/notice changed')
            if not custody or i in (1,2):
                require(addition['token'] not in addition_tokens,'Acquisition retry reused invocation');addition_tokens.append(addition['token'])
            else:require(addition['token']==addition_tokens[-1],'Retired acquisition borrowed successor')
        else:require(addition is None,'Finished acquisition retained pending state')
        geometry(row,case,i)
        clicked=RAW[0] if not custody and i==1 else RAW[i-2] if not custody and i>=3 else RAW[1] if custody and i in (1,2,4,6,8) else None
        if clicked:inputs.append(dict(stage=row['stage'],value=dict(ok=True,name=clicked,picker=False)))
        if not custody and i==2 or custody and i==3:
            releases.append(dict(stage=row['stage'],kind='preferences',value=dict(delivered=True,cancelledBeforeReply=False,raw=PREF_RESPONSE)))
        if custody and i in (5,7):releases.append(dict(stage=row['stage'],kind='preflight',value=dict(delivered=True,cancelledBeforeReply=False)))
        if not custody and i>=3:
            updates.append(dict(stage=row['stage'],raw=original))
            before={k:copy.deepcopy(rows[i-1]['model']['comments'][k]) for k in S['EVENT_KEYS']};after=copy.deepcopy(before)
            after['pageMessages'][1]['reactions']=reactions();after['reactions']=[dict(ts=FIRST,reactions=[dict(name=n,count=1) for n in RAW])]
            events.append(dict(stage=row['stage'],team='T1',generation=7,parsed=True,frame=dict(type='reaction_removed',user=USER,reaction=RESOLVED[i-3],item=dict(type='message',channel=CHANNEL,ts=FIRST)),before=before,after=after))
        if custody and i==9:updates.append(dict(stage=row['stage'],raw=page(rx)))
        if custody and i in (3,5,7):
            before=copy.deepcopy(rows[i-1]['model']['additionSource']);after=copy.deepcopy(before)
            if i==5:
                after['customRevision']+=1;after['catalog']=None
                changes.append(dict(stage=row['stage'],kind='accepted-custom-metadata',name='fixture_addition_changed',value='alias:eyes',before=before,after=after))
            else:
                after['preferences']=prefs(case,i)
                changes.append(dict(stage=row['stage'],kind='pref_change',team='T1',generation=7,frame=dict(type='pref_change',name='preferred_skin_tone',value=5),before=before,after=after))
        require(exact(p.get('inputs'),inputs) and exact(p.get('releases'),releases) and exact(p.get('serverUpdates'),updates)
            and exact(p.get('socket'),events) and exact(p.get('sourceChanges'),changes),'Native input/source/socket/release provenance changed')
        if custody and i>=2:
            wait=p.get('throttleWait',{});require(set(wait)=={'ready','elapsedMillis'} and wait['ready'] is True and R['integer'](wait['elapsedMillis']) and 0<=wait['elapsedMillis']<=36000,'Original acquisition throttle expiry proof missing')
            require(wait==rows[2]['proof']['throttleWait'],'Throttle wait proof rewritten')
        else:require('throttleWait' not in p,'Unexpected throttle reset/wait')
        require(all(p.get(k) is None for k in ('heldResponse','release','originalMessage','serverResponse')),'Unrelated response substituted')
    return receipt
