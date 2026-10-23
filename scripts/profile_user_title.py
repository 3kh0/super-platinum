"""Independent Canvas nested-user title oracle; constructed vectors are not native evidence."""
import json
from pathlib import Path
import sys
from profile_group_absent_member import member_raw,member_response,member_source

ACCOUNT='00000000-0000-4000-8000-000000000081'
TEAM,SELF,MEMBER='TINIT0001','UINIT0001','UMISSING01'
FILE,URL='FTITLE0001','https://fixture.slack.com/docs/FTITLE0001'
FIXTURES=('profile-sections-mentions','profile-sections-mentions-narrow')
STAGES={
    'known':('cold-loading','opened','confirmed'),
    'hidden':('cold-loading','opened','confirmed'),
    'recovery':('cold-loading','member-discovery','member-held','opened','confirmed'),
    'failure':('cold-loading','member-discovery','member-held','unavailable','retry-member-discovery','retry-member-held','opened','confirmed'),
    'cancel':('cold-loading','member-discovery','member-held','cancelled'),
}
LOADING='Loading document titles. Your description is unchanged.'
UNAVAILABLE='Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'
RICH={'opened','confirmed'}
HELD={'cold-loading','member-discovery','member-held','retry-member-discovery','retry-member-held'}


def require(value,message):
    if not value:raise RuntimeError(message)


def label(case):
    person='Hidden user' if case=='hidden' else '@  Cafe\u0301 Harbor  '
    return f'Review with {person} / {person} &amp; plan'


def title_blocks():
    items=[dict(type='text',text='Review with ',opaque_run=dict(index=0)),
        dict(type='user',user_id=MEMBER,style=dict(italic=True),opaque_run=dict(index=1)),
        dict(type='text',text=' / ',opaque_run=dict(index=2)),
        dict(type='user',user_id=MEMBER,style=dict(bold=True),opaque_run=dict(index=3)),
        dict(type='text',text=' &amp; plan',opaque_run=dict(index=4))]
    return [dict(type='rich_text',block_id='file-title-block',opaque_block=dict(retain=['block',7]),
        elements=[dict(type='rich_text_section',opaque_section=dict(retain=True),elements=items)])]


def blocks(saved=False):
    return [dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[
        dict(type='text',text='Saved with ' if saved else 'Working with '),
        *[dict(type='canvas',file_id=FILE,text=URL,url=URL,fixture_occurrence=i,
               style={style:True},opaque=dict(keep=[i,'original'])) for i,style in ((1,'bold'),(2,'italic'))]])])]


def seed(case):
    user=dict(id=SELF,name='fixture-self',team_id=TEAM,profile={})
    users=[user]+([member_raw()] if case in ('known','hidden') else [])
    team=dict(id=TEAM,name='User title')
    return dict(account=ACCOUNT,team=TEAM,user=SELF,
        boot=dict(ok=True,team=team,self=user,users=users,workspaces=[dict(id=TEAM)],
                  channels=[dict(id='CINIT0001',name='warm-harbor',is_channel=True,is_member=True)],prefs={}),
        init=dict(ok=True,self=user,team=team,workspaces=[dict(id=TEAM)],
                  onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123))


def policy(case):
    return dict(ok=True,prefs=dict(display_real_names_override=1,
        hidden_users=MEMBER if case=='hidden' else ''),opaque_policy='original-complete-policy')


def file_source(stage):
    held=stage=='cold-loading'
    return dict(requests=[dict(serial=1,method='files.info',args=dict(file=FILE,page='1',count='500',truncate='true'))],
                completed=int(not held),cancelled=0,held=held,active=int(held),networkCalls=0)


def member_requests(case,stage,epoch,release=None):
    if case in ('known','hidden') or stage=='cold-loading':return []
    count=1 if stage=='member-discovery' else 3 if stage=='retry-member-discovery' else 4 if case=='failure' and stage in ('retry-member-held','opened','confirmed') else 2
    rows=[]
    for serial in range(1,count+1):
        # The shared driver owns an Endpoint per Dispatch, not a global TTL
        # cache. Explicit Retry starts a fresh Dispatch and discovery receipt.
        discovery=serial in (1,3)
        pending=(stage,serial) in (('member-discovery',1),('member-held',2),('retry-member-discovery',3),('retry-member-held',4))
        row=dict(serial=serial,kind='discovery' if discovery else 'member',
            method='api.getFlannelHttpUrl' if discovery else 'users/info',
            body=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                 dict(updated_ids={MEMBER:0},check_interaction=True,include_profile_only_users=True),
            account=ACCOUNT,team=TEAM,epoch=epoch,released=not pending)
        if not pending:
            failed=case=='failure' and serial==2
            raw=dict(ok=True,url=f'https://edgeapi.slack.com/cache/{TEAM}',ttl_seconds=300) if discovery else member_response()
            row.update(outcome='offline' if failed else 'response',receiverRetired=False,delivered=True,
                       response=dict(error='offline') if failed else dict(raw=raw,disconnected=None))
            if stage=='cancelled' and serial==2:
                require(isinstance(release,dict) and set(release)=={'serial','receiverRetired','delivered'}
                        and release['serial']==2 and type(release['receiverRetired']) is bool
                        and type(release['delivered']) is bool
                        and not (release['receiverRetired'] and release['delivered']),
                        'Missing actual shared callback disposition')
                row.update(receiverRetired=release['receiverRetired'],delivered=release['delivered'])
        rows.append(row)
    return rows


def validate_sources(proof,model,case,stage,baseline):
    source=proof.get('source',{});known=case in ('known','hidden');ready=stage in RICH
    require(source.get('case')==case and source.get('seed')==seed(case) and source.get('networkCalls')==0,
            'Original Boot/member absence or owner changed')
    require(source.get('files')==file_source(stage),'Original files.info/reuse/settlement changed')
    member=source.get('member',{})
    release=proof['exercise'].get('release') if stage=='cancelled' else None
    requests=member_requests(case,stage,model['epoch'],release)
    require(member.get('account')==ACCOUNT and member.get('epoch')==model['epoch']
            and member.get('profile')==model and member.get('sourceObserved') is True,
            'Nested member observation lost original Profile owner')
    require(member.get('requests')==requests and member.get('pending')==[r for r in requests if not r['released']],
            'Original ordered member factory/raw/callback history changed')
    started=0 if not requests else 2 if len(requests)>=3 else 1
    active=int(stage in ('member-discovery','member-held','retry-member-discovery','retry-member-held'))
    require(member.get('driver')==dict(started=started,finished=started-active,active=active),
            'Shared member acquisition loop/replay or unfinished callback')
    present=known or ready
    require(member.get('sourcePresent') is present,'Missing member promoted before owned completion or after Cancel')
    native=member.get('native')
    if not present:require(native is None,'Unavailable or retired member became native authority')
    else:
        require(isinstance(native,dict) and all(native.get(k)==v for k,v in member_raw().items() if k!='profile')
                and all(native.get('profile',{}).get(k)==v for k,v in member_raw()['profile'].items()),
                'Native member identity/name history changed')
        if baseline.get('member') is None:baseline['member']=native
        require(native==baseline['member'],'Actual native member changed during title edit')
    expected_policy=[dict(serial=1,method='users.prefs.get',body={},account=ACCOUNT,team=TEAM,user=SELF,
                          epoch=model['epoch'],raw=policy(case))] if ready else []
    require(source.get('policy')==expected_policy,'Missing complete hidden policy or unexpected acquisition/replay')
    cache=proof.get('cache')
    if stage=='cold-loading':require(cache is None,'Cold file/title cache was preseeded');return
    require(isinstance(cache,dict) and cache.get('current') is ready,'File metadata incorrectly granted nested member readiness')
    raw=dict(id=FILE,mode='quip',filetype='quip',created=101,updated=101,title='Misleading Canvas fallback',
             name='wrong-name-fallback',fixture_opaque=dict(preserve=[FILE,1,2]),title_blocks=title_blocks())
    effective={k:v for k,v in raw.items() if k!='fixture_opaque'}
    effective.update(is_deleted=False,is_tombstoned=False,isUnknown=False,isNonExistent=False)
    require(cache.get('source')==raw and cache.get('effective')==effective,'Original title blocks/style/opaque file source changed')
    require(cache.get('quipTitle')==dict(current=True,value='nullish',fileId=FILE),
            'Passive Quip observation changed; no nonempty native override is claimed')
    title=cache.get('titleUsers')
    if not ready:require(title is None,'Unavailable nested member/policy granted title');return
    expected_member=member_source()
    expected_native=dict(member_raw(),is_stranger=False)
    if known:expected_member.pop('is_stranger');expected_native.pop('is_stranger')
    require(isinstance(title,dict) and title.get('members')==[dict(source=expected_member,effective=expected_native)]
            and title.get('generation')=='profile-canvas-title-users','Wrong deduplicated original nested member source/effective projection')
    observed=title.get('policy',{})
    require(observed.get('user')==policy(case) and observed.get('enterprise') is None
            and type(observed.get('revision')) is int and observed['revision']>=0,'Missing original complete policy custody')
    if baseline.get('title') is None:baseline['title']=title
    require(title==baseline['title'],'Section Save/readback changed nested label custody')


def geometry(g,viewport,case,stage,h):
    rich=stage in RICH;waiting=stage in HELD or stage=='unavailable'
    require(isinstance(g,dict) and g.get('ok') is True and g.get('noOverflow') is True
            and g.get('viewport')==viewport and g.get('dialog') is True,'Original title dialog/layout missing')
    h['rectangle'](g.get('dialogRect'),viewport)
    for key in ('controls','notices','tokens'):
        require(isinstance(g.get(key),list),'Missing native geometry inventory')
        for item in g[key]:
            require(item.get('visible') is True,'Original title control/token/feedback hidden')
            h['rectangle'](item.get('rect'),viewport)
    controls=['Back','Save section']+(['Use description','Cancel description edit'] if rich else
              ['Retry description','Cancel description edit'] if waiting else [])
    require(sorted(c.get('text') for c in g['controls'])==sorted(controls),'Wrong original title action inventory')
    require(all(c.get('disabled') is (c['text']=='Save section' or (c['text']=='Retry description' and stage in HELD))
                for c in g['controls']),'Wrong title recovery/Save locks')
    wanted=LOADING if stage in HELD else UNAVAILABLE if stage=='unavailable' else 'Profile fields saved.' if stage=='confirmed' else None
    if wanted:require(wanted in [n.get('text') for n in g['notices']],'Exact title recovery/readback notice absent')
    require(g.get('richEditable') is (True if rich else None),'Editor mounted without complete nested title sources')
    if not rich:
        require(g['tokens']==[] and g.get('captured') is None,'Retired/incomplete editor retained native tokens');return
    require([t.get('text') for t in g['tokens']]==[label(case)]*2
            and all(t.get('editable') is False for t in g['tokens']),'Nested user title/name priority/literal entity changed')
    require([(t.get('fontWeight'),t.get('fontStyle')) for t in g['tokens']]==[('700','normal'),('400','italic')],
            'Original outer Canvas token painted style changed')
    captured=g.get('captured',{});snap=captured.get('snapshot',{})
    require(snap.get('blocks')==blocks(stage=='confirmed') and snap.get('composing') is False
            and type(snap.get('revision')) is int and snap['revision']>=0
            and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Native raw editor capture/custody changed')
    require(captured.get('tokens')==[dict(value=t,label=label(case)) for t in blocks()[0]['elements'][0]['elements'][1:]],
            'Native title substituted wire URL/style/opaque occurrence')


def validate(fixture,receipt,captures,h,*,case,platform=sys.platform,png=True):
    require(case in STAGES and fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture
            and receipt.get('case')==case and receipt.get('userTitle') is True,'Wrong original user-title case/owner')
    require(receipt.get('mentionKind')=='canvas' and receipt.get('titleBlocks') is True
            and receipt.get('captureChecksPassed') is True
            and receipt.get('visualReview')=='Original PNG inspection required separately','Wrong title source/visual boundary')
    viewport=h['expected_viewport'](fixture,platform);rows=receipt.get('stages');paths=receipt.get('captures')
    require(receipt.get('viewport')==viewport and isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case])
            and isinstance(paths,dict) and set(paths)==set(STAGES[case]),'Missing/reordered original title stages/captures')
    previous=None;baseline={}
    for row in rows:
        stage=row['stage'];m=row['model'];proof=row['proof'];g=row['geometry'];saved=stage=='confirmed'
        require(m.get('enabled') is True and m.get('account')==ACCOUNT and type(m.get('epoch')) is int
                and all(type(m.get(k)) is int for k in ('visit','serial')),'Original Profile owner missing')
        if previous:
            require((m['epoch'],m['visit'])==(previous['epoch'],previous['visit'])
                    and ((m['serial']!=previous['serial']) if saved else m['serial']==previous['serial']),
                    'Only original Save/readback may replace section serial')
        require(m.get('blocks')==blocks(saved) and m.get('busy') is False and m.get('uncertain') is False
                and m.get('canSave') is False,'Raw original Canvas draft or write state changed')
        require(m.get('managedValue')=='Preserved organization value' and m.get('choiceValue')=='Morning'
                and m.get('countryValue')=='US','Original unrelated Profile values changed')
        require(m.get('reads')==int(saved) and isinstance(m.get('attempts'),list) and len(m['attempts'])==int(saved),
                'Unexpected Profile Save/readback/replay')
        if saved:
            attempt=m['attempts'][0];values={v.get('element_id'):v for v in attempt}
            require(len(values)==len(attempt) and {'long','locked'}<=set(values)
                    and json.loads(values['long']['long_text']['blocks'])==blocks(True)
                    and values['locked'].get('text',{}).get('text')=='Preserved organization value','Exact original rich Save wire changed')
        exercise=proof.get('exercise')
        expected={'cold-loading':dict(cold=True),'member-discovery':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
            'member-held':dict(serial=2),'unavailable':None,'retry-member-discovery':dict(serial=3,fileReused=True),
            'retry-member-held':dict(serial=4,fileReused=True),
            'opened':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
            'confirmed':dict(confirmedSave=True,fileReused=True)}
        if stage!='cancelled':require(exercise==expected[stage],'Original source/action proof changed')
        else:require(isinstance(exercise,dict) and set(exercise)=={'release'},'Cancelled original callback receipt missing')
        validate_sources(proof,m,case,stage,baseline);geometry(g,viewport,case,stage,h)
        dimensions=g.get('pngDimensions');require(isinstance(dimensions,list) and len(dimensions)==2
            and all(type(n) is int and n>0 for n in dimensions),'Missing native PNG dimensions')
        path=Path(captures).resolve()/f'{fixture}-{stage}.png'
        require(isinstance(paths[stage],str) and Path(paths[stage])==path,'Foreign original title capture path')
        if png:
            require(path.is_file() and not path.is_symlink(),'Missing original title PNG')
            h['validate_png'](path,dimensions)
        previous=m
    return receipt
