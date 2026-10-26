"""Exact finite shared missing-usergroup fixture source contract, never a native producer."""
from profile_channel_title import ACCOUNT, FOREIGN, TEAM, SELF, FILE, blocks, require, same
from profile_usergroup_title_source import raw, effective, native_group, group_rows, raw_file, file_ledger, reset
GROUP='SGROUP001'
UNKNOWN='Notes for @unknown-group'
FOUND='Notes for @design'
PRESENTATION=dict(locale='en-US',namespace='usergroup',value='unknown-group',origin='application-presentation')
STAGES={c:('cold-loading','opened','settled','used','confirmed') for c in ('positive','omission','failure')}
STAGES.update(cancel=('cold-loading','opened','settled','reopened','used','confirmed'),
 restricted=('cold-loading','opened','existing-restricted','used','confirmed'),
 retirement=('cold-loading','opened','pre-mount-retired','socket-held','socket-retired','account-held','account-replaced'))
COUNTS={'positive':(0,2,2,2,2),'omission':(0,2,2,2,2),'failure':(0,2,3,3,3),
 'cancel':(0,2,2,2,2,2),'restricted':(0,0,0,0,0),'retirement':(0,2,2,4,4,6,6)}


def self_raw(restricted=False):
    return dict(id=SELF,name='fixture-self',team_id=TEAM,is_restricted=restricted,profile={})


def seed(case):
    user=self_raw(case=='restricted');team=dict(id=TEAM,name='Missing group title')
    return dict(boot=dict(ok=True,team=team,self=user,users=[user],channels=[dict(id='CINIT0001',name='warm-harbor',is_channel=True,is_member=True)],prefs={}),
        init=dict(ok=True,self=user,team=team,workspaces=[dict(id=TEAM)],onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123),rows=[],actualSourceSink=True)


def terminal_raw(): return dict(id=GROUP,isNonExistent=True)
def terminal_effective(): return dict(terminal_raw(),user_count=0,is_usergroup=True,users=[])
def transient(): return dict(id=GROUP,name=GROUP,handle=GROUP,isUnknown=True)
def terminal_native():
    return dict(id=GROUP,team_id='',name='',handle='',description='',users=[],members_loaded=False,prefs=dict(channels=[],groups=[]),date_delete=0,label_source=terminal_raw())


def state_for(case,stage):
    i=STAGES[case].index(stage);retired=stage=='account-replaced';cold=i==0
    after=i>=2 and case!='retirement'
    known=after and case in ('positive','cancel','restricted')
    terminal=after and case in ('omission','failure')
    held=not cold and not retired and case!='restricted' and (i==1 or stage in ('socket-held','account-held'))
    stale=stage in ('pre-mount-retired','socket-retired')
    resets=0 if case!='retirement' else (1 if i in (2,3) else 2 if i>=4 else 0)
    socket=resets+1
    files=1  # Same socket and file receipt survive Use/Save and fresh editor opening.
    if case=='retirement': files=1+(i>=3)+(i>=5)
    editor=stage not in ('cold-loading','used','pre-mount-retired','account-replaced') and not(case=='cancel' and stage=='settled')
    model_prefix='Saved with ' if stage in ('used','confirmed') else 'Working with '
    dom_prefix='Retained draft with ' if stage=='settled' and case in ('positive','omission','failure') else model_prefix
    label=FOUND if (case=='positive' and stage=='confirmed') or (case=='cancel' and stage in ('reopened','used','confirmed')) else UNKNOWN
    return dict(count=COUNTS[case][i],cold=cold,retired=retired,after=after,known=known,terminal=terminal,
        held=held,stale=stale,resets=resets,socket=socket,files=int(files),editor=editor,saved=stage=='confirmed',
        model_prefix=model_prefix,dom_prefix=dom_prefix,label=label,can_save=stage=='used',restricted=case=='restricted')


def requests(case,count=None,held=False):
    kinds=[] if case=='restricted' else ['discovery','directory']*(3 if case=='retirement' else 1)+(['direct'] if case=='failure' else [])
    if count is not None: kinds=kinds[:count]
    out=[]
    for i,kind in enumerate(kinds,1):
        if kind=='discovery':
            method='api.getFlannelHttpUrl';body=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url')
            response=dict(raw=dict(ok=True,url=f'https://edgeapi.slack.com/cache/{TEAM}',ttl_seconds=300))
        elif kind=='direct':
            method='subteams.info';body=dict(usergroups=GROUP,include_users='false',include_count='true',include_disabled='true',_x_reason='fallback:fetchRawUserGroupsById');response=dict(error='offline')
        else:
            method='usergroups/info';body=dict(ids=[GROUP]);response=dict(error='offline') if case=='failure' else dict(raw=dict(ok=True,results=[] if case=='omission' else [raw()]))
        pending=held and i==len(kinds)
        row=dict(serial=i,kind=kind,method=method,body=body,account=ACCOUNT,team=TEAM,epoch=0,released=not pending)
        if not pending:row.update(response=response,outcome='offline' if 'error' in response else 'response',receiverRetired=False,delivered=True)
        out.append(row)
    return out


def rows(e):
    return group_rows() if e['known'] else [dict(source=terminal_raw(),effective=terminal_effective())] if e['terminal'] else []


def selector(e):
    if e['retired'] or e['stale'] or e['cold'] or (e['restricted'] and not e['known']):return None
    kind='Known' if e['known'] else 'NonExistent' if e['terminal'] else 'Unknown'
    return dict(kind=kind,transient=not(e['known'] or e['terminal']),current=True,
        effective=effective() if e['known'] else terminal_effective() if e['terminal'] else transient(),provenance='shared-group-selector')


def unknown_status(case,stage,e):
    if e['cold'] or e['restricted'] or e['retired']: return dict(managers=[],quiescent=True)
    old=e['stale'];socket=e['socket']-int(old)
    complete=e['after']
    last=None if not complete else dict(requested=[GROUP],retainedOrReturned=[GROUP] if e['known'] else [],missing=[GROUP] if e['terminal'] else [],errors=2 if case=='failure' else 0,attempt=1)
    return dict(managers=[dict(account=ACCOUNT,team=TEAM,epoch=0,socketGeneration=socket,current=not old,
        worker=e['held'],active=e['held'],pendingIds=[] if complete else [GROUP],started=1,finished=int(complete),lastSettlement=last)],quiescent=not e['held'])


def freshness(e):
    if not e['after'] or e['retired']:return None
    return dict(scope=dict(account=ACCOUNT,team=TEAM,user=SELF,epoch=0),freshIds=[GROUP],pending=[],inFlight=None,worker=False,started=0,finished=0,
        observations=dict(slowConnectionCount=1,major=1),lastSettlement=None)


def validate_source(s,case,stage,model):
    e=state_for(case,stage)
    require(isinstance(s,dict) and s.get('missingGroupTitle') is True and s.get('case')==case and same(s.get('seed'),seed(case)) and s.get('networkCalls')==0,'Wrong missing group source/mode/seed')
    require(s.get('account')==(FOREIGN if e['retired'] else ACCOUNT) and s.get('epoch')==int(e['retired']) and s.get('socketGeneration')==(0 if e['retired'] else e['socket']),'Wrong account/socket custody')
    wanted=requests(case,e['count'],e['held'])
    require(same(s.get('requests'),wanted) and same(s.get('pending'),[wanted[-1]] if e['held'] else []),'Unexpected acquisition/extra retry/body/held response')
    require(same(s.get('files'),file_ledger(e,stage)),'Wrong exact cold/readback file ledger')
    source=None if e['retired'] else dict(rows=rows(e),membership=None)
    require(same(s.get('source'),source),'Wrong canonical raw/effective group source')
    require(same(s.get('group'),native_group() if e['known'] else terminal_native() if e['terminal'] else None),'Wrong typed group or fabricated source')
    require(s.get('selfRestricted') is e['restricted'],'Wrong actual current-self restriction')
    require(same(s.get('selector'),selector(e)),'Wrong exact sealed group selector')
    require(same(s.get('unknown'),unknown_status(case,stage,e)),'Missing exact shared manager settlement/retirement')
    shared=s.get('shared',{})
    require(shared.get('seed') is None and shared.get('networkCalls')==0 and same(shared.get('resets'),[reset(i) for i in range(1,e['resets']+1)]),'Wrong genuine reconnect history')
    require(same(shared.get('source'),None if e['retired'] else dict(source,current=True)) and same(shared.get('scheduler'),freshness(e)),'Wrong shared source/freshness/duplicate acquisition')
    if e['retired']:require(shared.get('opening') is None,'Retired opening survived')
    else:
        opening=shared.get('opening',{})
        require(opening.get('visit')==model.get('visit') and opening.get('serial')==model.get('serial') and type(opening.get('key')) is int and opening['key']>=0
            and opening.get('mounted') is e['editor'] and opening.get('reading') is e['cold'] and opening.get('lease') is e['cold'],'Wrong opening or cold-file lease')


def self_rows(restricted):
    source=self_raw(restricted)
    source=dict(source,profile=dict(first_name='',last_name='',real_name_normalized='',display_name_normalized=''),is_self=True,_real_name_normalized_lc='',_display_name_normalized_lc='',_email_normalized_lc='',_name_lc='fixture-self',_first_name_lc='',_last_name_lc='',_real_name_lc='',_display_name_lc='')
    return [dict(native=self_raw(restricted),source=source,interaction=None)]


def title_groups(e):
    title_rows=[] if e['restricted'] else rows(e)
    label='@design' if e['known'] and not e['restricted'] else '@unknown-group'
    value=dict(ids=[GROUP],rows=title_rows,labels={GROUP:label},provenance='shared-group-history-snapshot')
    if e['restricted'] or not e['known']:
        s=selector(e)
        value['selector']=dict(restricted=e['restricted'],selfRows=self_rows(e['restricted']),presentation=dict(PRESENTATION),
            observed=[] if e['restricted'] else [dict(id=GROUP,**{k:s[k] for k in ('kind','transient','effective')})])
    return value


def validate_cache(value,case,stage):
    e=state_for(case,stage)
    if e['cold'] or e['retired']:
        require(value is None,'Cold/retired file cache granted authority');return
    require(isinstance(value,dict),'Missing file observation')
    source=raw_file()['file'];projection={k:v for k,v in source.items() if k!='fixture_opaque'}
    projection.update(is_deleted=False,is_tombstoned=False,isUnknown=False,isNonExistent=False)
    require(same(value.get('source'),source) and same(value.get('effective'),projection),'Raw file title/styles/opaque changed')
    current=not e['stale']
    require(value.get('current') is current and value.get('titleUsers') is None and value.get('titleChannels') is None,'Wrong file authority/dependency kind')
    require(value.get('quipTitle')==dict(current=True,value='nullish',fileId=FILE),'Missing independent Quip observation')
    require(same(value.get('titleGroups'),title_groups(e) if current else None),'Wrong source-bound placeholder/group policy proof')
    from profile_emoji_title import DESCRIPTOR,FALLBACK
    require(value.get('canvasFallback')==dict(current=current,descriptor=FALLBACK) and value.get('emojiText')==dict(current=current,descriptor=DESCRIPTOR),'Independent presentation context changed')
