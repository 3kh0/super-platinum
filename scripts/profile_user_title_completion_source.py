"""Original input and callback custody for the opt-in user-title completion fixture."""
import copy
import json
import re
from pathlib import Path
from collections import Counter
import profile_user_title as U

require=U.require
ACCOUNT,TEAM,SELF,MEMBER=U.ACCOUNT,U.TEAM,U.SELF,U.MEMBER
CANVAS='FCOMPLETE1'
BASE_BYTES=Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/completion/files-response.json'
FAMILIES={'users/search','usergroups/search','search.autocomplete.files','channels/search'}


def policy(case):
    return dict(ok=True,prefs=dict(frecency='{}',display_real_names_override=1,
        hidden_users=MEMBER if case=='hidden' else ''),opaque_policy='original-completion-title-policy')


def seed(case):
    user=dict(id=SELF,name='fixture-self',team_id=TEAM,is_admin=True,profile={})
    local=dict(id='UMENTION01',name='local-fixture',team_id=TEAM,profile=dict(display_name='Alex Local',real_name='Local real'))
    users=[user,local]+([U.member_raw()] if case in ('known','hidden') else [])
    team=dict(id=TEAM,name='Title completion')
    return dict(account=ACCOUNT,team=TEAM,user=SELF,
        boot=dict(ok=True,team=team,self=user,users=users,channels=[dict(id='CINIT0001',name='warm-harbor',is_channel=True,is_member=True)],prefs={}),
        init=dict(ok=True,self=user,team=team,workspaces=[dict(id=TEAM)],onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123))


def original_bytes(value):
    require(isinstance(value,str),'Missing original file response bytes')
    marker='"title":"Alex Canvas","title_blocks":'
    require(value.count(marker)==1,'Missing single original user-title insertion')
    offset=value.index(marker)+len(marker)
    blocks,end=json.JSONDecoder().raw_decode(value[offset:])
    require(blocks==U.title_blocks(),'Original nested user/style/opaque blocks changed')
    restored=value[:value.index(marker)]+'"title":"Alex Canvas"'+value[offset+end:]
    require(restored==BASE_BYTES.read_bytes().decode('utf-8'),'Original file bytes/order replaced or reserialized')
    return json.loads(value)


def queries(case,stage):
    if stage=='opened':return []
    if case in ('known','hidden'):return ['alex']
    if case=='recovery':
        return ['alex','al'] if stage in ('retry-member-discovery','retry-member-held','query-ready','inserted','saved') else ['alex']
    return ['alex','project','al'] if stage in ('replacement-member-discovery','replacement-member-held','cancelled') else ['alex','project'] if stage in ('query-replaced','late-settled') else ['alex']


def completion(source,case,stage,raw_bytes,baseline):
    require(isinstance(source,dict) and source.get('networkCalls')==0,'Missing closed completion source')
    requests=source.get('requests');require(isinstance(requests,list),'Missing completion request ledger')
    require(requests[:len(baseline.get('requests',[]))]==baseline.get('requests',[]),'Original completion requests/raw replies changed')
    allowed=FAMILIES|{'files.info','users.prefs.get','experiments.getByUser','client.gantryBoot','client.userBoot',
        'api.getFlannelHttpUrl','drafts.list','drafts.listActive','search.autocomplete.model','search.autocomplete.offlineFeatures'}
    for serial,row in enumerate(requests,1):
        method,args,raw=row.get('method'),row.get('args'),row.get('raw')
        require(row.get('serial')==serial and method in allowed and isinstance(args,dict) and 'raw' in row
            and not {'token','enterprise_token','cookie','cookies'}.intersection(args),'Wrong original completion factory/receipt')
        if method=='users/search':
            require(args==dict(query=args.get('query'),count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace=TEAM),'Wrong people search factory')
        elif method=='usergroups/search':
            require(args==dict(query=args.get('query'),count=25,org_wide=True),'Wrong groups search factory')
        elif method=='channels/search':
            require(args==dict(query='project',count=25,fuzz=1,filter='xws',include_record_channels=True,
                check_membership=True,top_channels=[],default_workspace=TEAM),'Wrong replacement channel factory')
        elif method=='search.autocomplete.files':
            require(args==dict(query=args.get('query'),include_shares='true',_x_reason='texty-autocomplete') and raw==raw_bytes,'Original file query/bytes lost')
        elif method=='files.info':
            require(args==dict(file=CANVAS,page='1',count='500',truncate='true') and raw==dict(ok=True,file=json.loads(raw_bytes)['files'][0]),'Wrong canonical reopen file receipt')
        elif method=='users.prefs.get':require(raw==policy(case),'Actual acquired complete query prefs changed')
        elif method=='experiments.getByUser':require(raw==dict(ok=True,assignments=dict(texty_autocomplete_files=dict(group='on'))),'Actual file completion assignment changed')
        if method!='search.autocomplete.files':require(isinstance(raw,dict) and raw.get('ok') is True,'Missing successful original metadata/remote receipt')
    expected=Counter()
    for q in queries(case,stage):
        if q=='project':families=['channels/search']
        elif q=='alex':families=['users/search','usergroups/search','search.autocomplete.files']
        else:
            # files::remote_threshold rejects ASCII "al" before cache/dispatch.
            # member_run joins local title recovery before dispatching groups;
            # retirement cancels that held dependency, recovery releases it.
            families=['users/search']
            if case=='recovery' and stage in ('query-ready','inserted','saved'):
                families.append('usergroups/search')
        expected.update((method,q) for method in families)
    actual=Counter((r['method'],r['args'].get('query')) for r in requests if r['method'] in FAMILIES)
    require(actual==expected,'Completion repeated/omitted a query family or acquired unrequested query')
    counts={method:sum(r['method']==method for r in requests) for method in FAMILIES}
    require(source.get('searches')==sum(counts.values()) and source.get('completed')==sum(counts.values())
        and source.get('active')==0 and source.get('cancelled')==0 and source.get('held') is False and source.get('heldQueries')==[],
        'Ordinary search callbacks did not settle independently of held nested members')
    for field,method in [('channelSearches','channels/search'),('memberSearches','users/search'),('groupSearches','usergroups/search'),('fileSearches','search.autocomplete.files')]:
        require(source.get(field)==counts[method],'Original search family accounting changed')
    info=[r for r in requests if r['method']=='files.info']
    require(len(info)==int(stage=='saved'),'Unexpected files.info or missing canonical reopen')
    if stage=='opened':require(requests==[],'Query acquisition happened before original input')
    baseline['requests']=copy.deepcopy(requests)
    return requests


def nested(source,model,case,stage,exercise,baseline):
    require(source.get('case')==case and source.get('seed')==seed(case) and source.get('policy')==policy(case)
        and source.get('networkCalls')==0,'Original title completion seed/policy/owner changed')
    original_bytes(source.get('fileResponseBytes'))
    member=source.get('member',{});known=case in ('known','hidden')
    phase={'opened':'cold-loading','member-discovery':'member-discovery','member-held':'member-held','unavailable':'unavailable',
        'retry-member-discovery':'retry-member-discovery','retry-member-held':'retry-member-held',
        'query-ready':'opened','inserted':'opened','saved':'confirmed'}.get(stage)
    if case!='retirement':
        expected=U.member_requests('known' if known else 'failure',phase,model['epoch'])
    else:
        phase={'opened':'cold-loading','member-discovery':'member-discovery','member-held':'member-held',
            'query-replaced':'member-held','late-settled':'opened','replacement-member-discovery':'retry-member-discovery',
            'replacement-member-held':'retry-member-held','cancelled':'confirmed'}[stage]
        expected=U.member_requests('recovery' if stage=='late-settled' else 'failure',phase,model['epoch'])
        for r in expected:
            if r['serial'] in (2,4) and r['released']:
                r.update(outcome='response',response=dict(raw=U.member_response(),disconnected=None))
                release=exercise.get('release') if stage in ('late-settled','cancelled') and exercise.get('release',{}).get('serial')==r['serial'] else baseline.get('releases',{}).get(r['serial'])
                require(isinstance(release,dict) and set(release)=={'serial','receiverRetired','delivered'}
                    and type(release['receiverRetired']) is bool and type(release['delivered']) is bool
                    and not (release['receiverRetired'] and release['delivered']),'Missing actual original shared callback disposition')
                r.update(receiverRetired=release['receiverRetired'],delivered=release['delivered'])
                baseline.setdefault('releases',{})[r['serial']]=copy.deepcopy(release)
    require(member.get('account')==ACCOUNT and member.get('epoch')==model['epoch'] and member.get('profile')==model
        and member.get('sourceObserved') is True,'Nested acquisition did not retain original Profile owner')
    require(member.get('requests')==expected and member.get('pending')==[r for r in expected if not r['released']],
        'Original nested discovery/member request/reply timeline changed')
    present=known or (case=='recovery' and stage in ('query-ready','inserted','saved'))
    require(member.get('sourcePresent') is present,'Unavailable/retired nested member became authority')
    native=member.get('native')
    if present:
        require(isinstance(native,dict) and all(native.get(k)==v for k,v in U.member_raw().items() if k!='profile')
            and all(native.get('profile',{}).get(k)==v for k,v in U.member_raw()['profile'].items()),'Original nested member identity/name changed')
        if 'nativeMember' in baseline:require(native==baseline['nativeMember'],'Nested native member changed during query/save')
        baseline['nativeMember']=copy.deepcopy(native)
    else:require(native is None,'Absent member projected native placeholder')
    driver=member.get('driver',{});started=0 if not expected else 1 if len(expected)<=2 else 2
    active=int(stage in ('member-discovery','member-held','retry-member-discovery','retry-member-held','replacement-member-discovery','replacement-member-held'))
    if stage=='query-replaced':
        require(driver.get('active') in (0,1),'Retired query started another member acquisition')
        active=driver['active']
    require(driver==dict(started=started,finished=started-active,active=active),'Shared nested-member acquisition did not settle or replayed')


def title_proof(value,case,sequence,requests):
    require(isinstance(value,dict) and value.get('current') is True,'Private title member subset is stale')
    generation=value.get('generation')
    require(isinstance(generation,str) and re.fullmatch(r'profile-member-search-\d+-\d+-'+str(sequence)+r'-file-title-users',generation),
        'Title members borrowed a different query or existing-token generation')
    canonical=U.member_source();native=dict(U.member_raw(),is_stranger=False)
    if case in ('known','hidden'):canonical.pop('is_stranger');native.pop('is_stranger')
    require(value.get('members')==[dict(source=canonical,effective=native)],'Original deduplicated title member source/effective changed')
    observed=value.get('policy',{})
    require(observed.get('user')==policy(case) and observed.get('enterprise') is None
        and type(observed.get('revision')) is int and observed['revision']>=0,'Wrong original complete query name policy')
    require(any(r['method']=='users.prefs.get' and r['raw']==observed['user'] for r in requests)
        and any(r['method']=='client.gantryBoot' and r['raw']==observed.get('gantry') for r in requests),
        'Private title policy did not come from actual original query reads')
