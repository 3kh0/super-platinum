"""Independent known-member factories and title display receipts for own edits."""
import copy
import json
import slack_list_canvas_user_titles_source as T
require=T.require
MEMBER,TEAM,SELF,CANVAS=T.MEMBER,T.TEAM,T.SELF,T.CANVAS
original_bytes,member_response,member_raw,member_source=T.original_bytes,T.member_response,T.member_raw,T.member_source

def label(case):
    return T.label('user-title-hidden' if case=='own-edit-title-hidden' else 'user-title-known')
def policy(case):
    return T.policy('user-title-hidden' if case=='own-edit-title-hidden' else 'user-title-known')
def tokens(blocks):
    out=[]
    def walk(value):
        if isinstance(value,list):
            for v in value:walk(v)
        elif isinstance(value,dict):
            if value.get('type')=='canvas':out.append(copy.deepcopy(value))
            for key in ('elements','blocks'):walk(value.get(key))
    walk(blocks)
    return out

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
        expected=dict(serial=index,lane='known',kind='discovery' if discovery else 'member',
            method='api.getFlannelHttpUrl' if discovery else 'users/info',
            args=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                 dict(**{'ids':[MEMBER]},
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
    require(value.get('sourceObserved') is True and value.get('sourcePresent') is True,
            'Nested canonical source observation missing')
    recovered=bool(requests and len(requests)==4 and requests[-1]['released'])
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
        if key=='driver':
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



def metadata(source,previous):
    raw=original_bytes(source['fileResponseBytes']);original=raw['files'][0]
    rows=source.get('files')
    require(isinstance(rows,list) and rows,'Missing actual singleton title metadata')
    require(rows[:len(previous)]==previous,'Original singleton metadata receipt rewritten')
    for i,row in enumerate(rows,1):
        line=next(line.strip().rstrip(',') for line in source['fileResponseBytes'].splitlines() if '"id":"'+CANVAS+'"' in line)
        text='{"ok":true,"file":'+line+'}'
        require(row==dict(serial=i,method='files.info',id=CANVAS,raw=dict(ok=True,file=original),responseText=text),
                'Wrong singleton file factory/raw bytes')
    require(len(rows)==1,'Nested retry refreshed already-current original file metadata')
    return raw


def observed(value,case,source,expected_tokens,generation):
    require(isinstance(value,dict) and value.get('current') is True,'Independent title display is not current')
    titles=value.get('titles',{})
    require(titles.get('current') is True and titles.get('tokens')==expected_tokens and titles.get('required')==[MEMBER],
            'Title display borrowed another token/dependency')
    raw=original_bytes(source['fileResponseBytes']);original=raw['files'][0]
    rows=titles.get('files')
    require(isinstance(rows,list) and len(rows)==len(expected_tokens),'Duplicate occurrence provenance lost')
    for row in rows:
        require(row==dict(id=CANVAS,source=original,effective=T.title_projection(original),receipt=dict(ok=True,file=original)),
                'Title display borrowed query metadata or changed canonical projection')
    T.users(titles.get('users'),generation)
    p=value.get('policy',{})
    require(p.get('user')==policy(case) and p.get('enterprise') is None and type(p.get('revision')) is int
            and p['revision']>=0 and any(v['raw']==p['user'] for v in source['policy']),
            'Title display lacks independent complete policy')
    labels=titles.get('labels')
    require(isinstance(labels,dict) and len(labels)==len(expected_tokens),'Missing original title labels')
    parsed=[(json.loads(k),v) for k,v in labels.items()]
    require(all((t,label(case)) in parsed for t in expected_tokens),'Title label changed original wire token')


def native_tokens(native,case,expected_tokens,blocks):
    require(isinstance(native,dict) and native.get('snapshot',{}).get('blocks')==blocks,
            'Ready original editor snapshot changed')
    values=native.get('tokens')
    require(isinstance(values,list),'Missing actual resolved native tokens')
    found=[v for v in values if v.get('value',{}).get('file_id')==CANVAS]
    require([v.get('value') for v in found]==expected_tokens and all(v.get('label')==label(case) for v in found),
            'Original title token occurrence/style/label changed')


def ready(entry,case,source,expected_blocks,*,recovered):
    job=entry.get('job',{});expected_tokens=tokens(expected_blocks)
    require(job.get('current') is True and job.get('status')=='ready' and job.get('loading') is False
            and job.get('error') is None and job.get('retryAt')==0 and job.get('blocks')==expected_blocks
            and job.get('tokens')==expected_tokens and job.get('ts')=='1781524801.000001'
            and isinstance(job.get('scope'),str) and job['scope'].startswith('list-edit:')
            and type(job.get('generation')) is int and job['generation']>0
            and type(job.get('revision')) is int and job['revision']>=0,
            'Original pre-input edit title job not ready/current')
    generation='list-own-edit-title-'+job['scope']+('' if recovered else '-file-title-users')
    observed(job.get('observed'),case,source,expected_tokens,generation)
    native_tokens(entry.get('native'),case,expected_tokens,expected_blocks)
    return job
