"""Original closed List title inputs and member/policy receipt custody."""
import copy
import json
import re
from pathlib import Path
import profile_user_title as REFERENCE

require=REFERENCE.require
MEMBER='UMISSING01'
TEAM='T1'
SELF='ULIST0001'
CANVAS='FLISTFILE1'
LIST='FLISTFILE2'
CASES=tuple('user-title-'+name for name in ('known','hidden','recovery','draft-recovery','retirement'))
BASE=Path(__file__).resolve().parents[1]/'src/desktop/slack_lists/comments/fixture/completion/files-response.json'


def label(case):
    require(case in CASES,'Unknown List user-title case')
    return REFERENCE.label('hidden' if case=='user-title-hidden' else 'known')


def policy(case):
    return dict(ok=True,prefs=dict(frecency='{}',display_real_names_override=1,
                hidden_users=MEMBER if case=='user-title-hidden' else ''),opaque_policy='list-original-complete-policy')


def member_raw():
    return dict(id=MEMBER,team_id=TEAM,name='username-must-not-win',real_name='Real fallback',
                profile=dict(display_name='  Cafe\u0301 Harbor  ',real_name='Profile fallback'),updated=101)


def member_response():
    return dict(ok=True,results=[member_raw()],can_interact={MEMBER:True})


def member_source(recovered=False):
    # Observations.rows carries hydrated canonical source, not the HTTP row.
    # Same retained name-derivation vector as Profile; List has its own team.
    value=REFERENCE.member_source();value['team_id']=TEAM
    if not recovered:value.pop('is_stranger')
    return value


def original_bytes(value):
    require(isinstance(value,str),'Missing original List file response bytes')
    marker='"title":"Alex Canvas","title_blocks":'
    require(value.count(marker)==1,'Missing unique Canvas title insertion')
    begin=value.index(marker);offset=begin+len(marker)
    blocks,end=json.JSONDecoder().raw_decode(value[offset:])
    require(blocks==REFERENCE.title_blocks(),'Original title user/style/opaque leaves changed')
    restored=value[:begin]+'"title":"Alex Canvas"'+value[offset+end:]
    require(restored==BASE.read_bytes().decode('utf-8'),'Original file bytes/channel-map order rewritten')
    raw=json.loads(value)
    require(raw['files'][0]['id']==CANVAS,'Original Canvas identity changed')
    return raw


def member_ledger(value,model,case,baseline):
    """Validate factories and immutable receipts; caller pins exact stage counts."""
    require(isinstance(value,dict) and value.get('case')==case,'Wrong List title source case')
    original_bytes(value.get('fileResponseBytes'))
    requests=value.get('requests');policies=value.get('policy')
    require(isinstance(requests,list) and isinstance(policies,list),'Missing independent title ledgers')
    previous=baseline.get('memberRequests',[])
    require(len(requests)>=len(previous),'Original member request history truncated')
    for index,row in enumerate(requests,1):
        discovery=index%2==1
        expected=dict(serial=index,lane='known' if case=='user-title-draft-recovery' else 'absent',kind='discovery' if discovery else 'member',
            method='api.getFlannelHttpUrl' if discovery else 'users/info',
            args=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                 dict(**({'ids':[MEMBER]} if case=='user-title-draft-recovery' else {'updated_ids':{MEMBER:0}}),
                      check_interaction=True,include_profile_only_users=True),
            account=model['account'],epoch=model['epoch'],team=TEAM)
        require({k:row.get(k) for k in expected}==expected and type(row.get('released')) is bool,
                'Nested member request factory or original owner changed')
        require(set(row)==set(expected)|({'released','response','disposition'} if row['released'] else {'released'}),
                'Unexpected nested member receipt fields')
        if row['released']:
            response=row['response'];disposition=row['disposition']
            success=dict(raw=dict(ok=True,url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300) if discovery else member_response(),keys=None)
            require(response==success or (not discovery and response==dict(error='offline')),
                    'Original member response/error/ordered keys changed')
            require(set(disposition)=={'serial','receiverRetired','delivered'} and disposition['serial']==index
                    and all(type(disposition[k]) is bool for k in ('receiverRetired','delivered'))
                    and not (disposition['receiverRetired'] and disposition['delivered']),
                    'Missing original callback disposition')
        if index<=len(previous):
            old=previous[index-1]
            require(row==old if old['released'] else all(row.get(k)==v for k,v in old.items() if k!='released'),
                    'Original nested member history rewritten')
    require(value.get('pending')==[r for r in requests if not r['released']],
            'Held member callbacks differ from original requests')
    old_policy=baseline.get('titlePolicies',[])
    require(policies[:len(old_policy)]==old_policy,'Original title policy receipts rewritten')
    for index,row in enumerate(policies,1):
        require(row==dict(serial=index,method='users.prefs.get',raw=policy(case),account=model['account'],
                         epoch=model['epoch'],team=TEAM,user=SELF),'Independent complete title policy/owner changed')
    require(value.get('sourceObserved') is True and type(value.get('sourcePresent')) is bool,
            'Nested canonical source observation missing')
    recovered=case=='user-title-recovery' or (case=='user-title-draft-recovery' and len(requests)==6 and requests[-1]['released'])
    require(type(value.get('sourceReady')) is bool
            and value.get('sourceRaw')==(member_source(recovered) if value['sourcePresent'] else None),
            'Original known/absent source or Ready observation changed')
    for key in ('driver','knownDriver'):
        counts=value.get(key)
        require(isinstance(counts,dict) and set(counts)=={'started','finished','active'}
                and all(type(v) is int and v>=0 for v in counts.values())
                and counts['started']-counts['finished']==counts['active'],
                'Member recovery lane counters invalid')
        old=baseline.get(key)
        if old:require(counts['started']>=old['started'] and counts['finished']>=old['finished'],
                       'Original recovery lane counters regressed')
        if (key=='driver' and case=='user-title-draft-recovery') or (key=='knownDriver' and case!='user-title-draft-recovery'):
            require(counts==dict(started=0,finished=0,active=0),'Member recovery borrowed another reader lane')
        baseline[key]=copy.deepcopy(counts)
    if value['sourcePresent']:
        native=value.get('native')
        require(isinstance(native,dict) and all(native.get(k)==v for k,v in member_raw().items() if k!='profile')
                and all(native.get('profile',{}).get(k)==v for k,v in member_raw()['profile'].items()),
                'Observed native member identity/name changed')
    # Source invalidation retains raw and typed rows. It is not Ready authority;
    # each query/draft/transcript receipt must independently prove its subset.
    baseline.update(memberRequests=copy.deepcopy(requests),titlePolicies=copy.deepcopy(policies))
    return requests,policies


def completion(source,model,case,inputs,baseline,G):
    require(all(source.get(k) is True for k in ('enabled','named','protectedFixture','identityCurrent'))
            and source.get('httpCapability') is False and source.get('networkCalls')==0
            and source.get('account')==model['account'] and source.get('epoch')==model['epoch']
            and source.get('team')==TEAM and source.get('user')==SELF,'Original closed completion owner changed')
    raw=original_bytes(source['titleUsers']['fileResponseBytes'])
    require(source.get('files',{}).get('enabled') is True and source['files'].get('gate')=='on'
            and source['files'].get('originalResponse')==source['titleUsers']['fileResponseBytes'],
            'File gate or original response bytes were not observed')
    seed=G['seed_source']()
    if case in ('user-title-known','user-title-hidden','user-title-draft-recovery'):
        seed['boot']['users'].append(member_raw())
    require(source.get('seed')==seed,'Actual Boot/Init title member source changed')
    obs=G['observations']();obs['users.prefs.get']=({},policy(case))
    obs['experiments.getByUser'][1]['assignments']['texty_autocomplete_files']={'group':'on'}
    obs['client.userBoot']=(obs['client.userBoot'][0],seed['boot'])
    requests,responses=source.get('requests'),source.get('responses')
    require(isinstance(requests,list) and isinstance(responses,list) and len(requests)==len(responses),'Missing paired original completion receipts')
    prior=baseline.get('completionRequests',[]);prior_r=baseline.get('completionResponses',[])
    require(requests[:len(prior)]==prior and responses[:len(prior_r)]==prior_r,'Completion receipt history rewritten')
    for n,(q,r) in enumerate(zip(requests,responses),1):
        require(set(q)=={'serial','method','args'} and q['serial']==n and set(r)=={'serial','disposition','raw'}
                and r['serial']==n and r['disposition']=='response','Unexpected completion factory/disposition')
        method,a=q['method'],q['args']
        if method=='search.autocomplete.files':
            require(a==dict(query=a.get('query'),include_shares='true',_x_reason='texty-autocomplete')
                    and r['raw']==source['titleUsers']['fileResponseBytes'],'Original file search bytes/arguments changed')
        elif method=='users/search':
            require(a==dict(query=a.get('query'),count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace=TEAM)
                    and r['raw']==dict(ok=True,results=G['source_rows']()),'Original people search changed')
        elif method=='usergroups/search':
            require(a==dict(query=a.get('query'),count=25,org_wide=True)
                    and r['raw']==dict(ok=True,results=G['group_rows']()),'Original groups search changed')
        elif method=='users/info':require(r['raw']==G['interaction_observation'](a),'Original first30 interaction reply changed')
        else:require(method in obs and (a,r['raw'])==obs[method],'Unrequested/changed original context receipt')
    queries=[x['value']['query'][1:] for x in inputs if x['kind']=='query']
    people=[q['args']['query'] for q in requests if q['method']=='users/search']
    require(people==queries,'Missing/replayed people query')
    # ASCII @al never dispatches file IO. Exact repeated alex uses its accepted cache.
    files=list(dict.fromkeys(q for q in queries if len(q)>3))
    actual_files=[q['args']['query'] for q in requests if q['method']=='search.autocomplete.files']
    require(actual_files==files,'File threshold/exact-query source reuse changed')
    groups=[q['args']['query'] for q in requests if q['method']=='usergroups/search']
    active=source['titleUsers']['driver']['active']
    expected_groups=queries[:-1] if queries and queries[-1]=='al' and active else queries
    if case=='user-title-retirement' and queries and queries[-1]=='al':expected_groups=queries[:-1]
    require(groups==expected_groups,'Group dispatch crossed unresolved/retired title dependency')
    require(source.get('completed')==len(files) and source.get('cancelled')==0 and source.get('held')==0
            and source.get('releases')==[],'Unexpected file search hold/replay/cancellation')
    baseline.update(completionRequests=copy.deepcopy(requests),completionResponses=copy.deepcopy(responses))
    return raw


def title_projection(original):
    # file_title_source::FIELDS: title/status only, deliberately no share/rank facts.
    fields=('id','mode','filetype','subtype','name','title','title_blocks','isUnknown','isNonExistent',
            'is_deleted','is_tombstoned','restriction_type','is_locked','is_hidden_by_limit','file_access',
            'permalink','updated','created','timestamp','is_external','external_type','is_skill',
            'canvas_template_mode','list_template_mode','is_huddle_canvas','is_huddle_transcript_canvas',
            'huddle_summary_id','huddle_transcript_file_id')
    require(any(original.get(k) for k in ('updated','created','timestamp')) and not original.get('is_tombstoned'),
            'Unexpected fixture title projection branch')
    return {**{k:copy.deepcopy(original[k]) for k in fields if k in original},
            'is_tombstoned':False,'isUnknown':False,'isNonExistent':False}


def file_rows(rows,raw,ids,*,receipt=False,title_only=False):
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids,'Original file projection identities changed')
    by_id={r['id']:r for r in raw['files']}
    for row in rows:
        id=row['id'];original=by_id[id];effective=row.get('effective',{})
        require(row.get('source')==original and effective.get('id')==id
                and effective.get('title')==original['title'] and effective.get('permalink')==original['permalink'],
                'Original raw-to-canonical file projection/order changed')
        if title_only:require(effective==title_projection(original),'Title-only observation borrowed candidate/share authority')
        else:require(effective.get('channels')==({CANVAS:['CZMAP001','CAMAP001','CTAIL001'],LIST:['CYMAP002','CBMAP002']}[id]),
                     'Original candidate channel-map order changed')
        if id==CANVAS:require(effective.get('title_blocks')==REFERENCE.title_blocks(),'Canonical title leaves changed')
        if receipt:require(row.get('receipt')==raw,'Independent title borrowed a different original file receipt')


def users(value,generation):
    require(isinstance(value,dict) and value.get('current') is True and value.get('generation')==generation,
            'Wrong/stale independent nested-member generation')
    rows=value.get('members');require(isinstance(rows,list) and len(rows)==1,'Duplicate/omitted nested user dependency')
    for key in ('source','effective'):
        row=rows[0].get(key,{})
        require(all(row.get(k)==v for k,v in member_raw().items() if k!='profile')
                and all(row.get('profile',{}).get(k)==v for k,v in member_raw()['profile'].items()),
                'Nested member source/native identity or name changed')


def observed(value,case,raw,tokens,generation,policies):
    require(isinstance(value,dict) and value.get('current') is True,'Independent existing-token title receipt stale')
    titles=value.get('titles',{})
    require(titles.get('current') is True and titles.get('tokens')==tokens and titles.get('required')==[MEMBER],
            'Existing-token title receipt borrowed another occurrence/dependency')
    file_rows(titles.get('files'),raw,[t['file_id'] for t in tokens],receipt=True,title_only=True)
    users(titles.get('users'),generation)
    p=value.get('policy',{})
    require(p.get('user')==policy(case) and p.get('enterprise') is None and type(p.get('revision')) is int
            and p['revision']>=0 and any(r['raw']==p['user'] for r in policies),
            'Existing-token title did not acquire original complete policy')
    labels=titles.get('labels');require(isinstance(labels,dict),'Missing independently witnessed display labels')
    parsed=[(json.loads(k),v) for k,v in labels.items()]
    require(len(parsed)==len(tokens) and all((t,label(case) if t['file_id']==CANVAS else 'Alex List') in parsed for t in tokens),
            'Derived label changed original token wire/occurrence')


def query_users(value,case,sequence,requests,responses):
    require(isinstance(value,dict) and set(value)=={CANVAS},'Query Users borrowed by non-user List/file')
    v=value[CANVAS];generation=v.get('generation')
    require(isinstance(generation,str) and re.fullmatch(r'list-comment-member-[0-9]+-.+-'+str(sequence)+r'-file-title-users',generation) is not None,
            'Query member generation is not List-owned')
    users(v,generation)
    p=v.get('policy',{})
    original={q['method']:r['raw'] for q,r in zip(requests,responses) if q['method'] in ('users.prefs.get','client.gantryBoot')}
    require(p.get('user')==policy(case) and p.get('enterprise') is None and type(p.get('revision')) is int
            and original.get('users.prefs.get')==p['user'] and 'client.gantryBoot' in original
            and p.get('gantry')==original['client.gantryBoot'],'Query title borrowed display policy')
