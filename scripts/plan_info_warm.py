"""Independent private-cache PlanInfo evidence checks; cached data is display-only."""
import hashlib
import json
from pathlib import Path
import sqlite3
import uuid
from contextlib import closing

FLAG='SUPER_PLATINUM_PLAN_INFO_WARM_CHECK'
DB_FLAG='SUPER_PLATINUM_PLAN_INFO_WARM_DB'
DB_NAME='plan-info-warm.sqlite'
# Existing fixture workspace metadata and its synthetic request session are
# separate original inputs; the PlanInfo envelope binds the latter exactly.
WORKSPACE_URL='https://echonet.slack.com'
CASE='channel-frozen-grace'
FIXTURES=('workflow-activity','workflow-activity-narrow')


def require(value,message):
    if not value:raise RuntimeError(message)


def envelope(value,scope):
    require(isinstance(value,dict) and set(value)=={'version','account','team','user','url','enterprise','source','raw'},
            'PlanInfo envelope fields changed')
    require(type(value['version']) is int and value['version']==1
            and value['source']=='accepted-plan-info' and isinstance(value['raw'],dict),
            'Unknown or unaccepted PlanInfo envelope')
    try:account=str(uuid.UUID(value['account']))
    except (ValueError,TypeError,AttributeError) as e:raise RuntimeError('Invalid PlanInfo account binding') from e
    require(account==value['account'] and all(value[k]==scope[k] for k in ('account','team','user','url','enterprise')),
            'PlanInfo cache scope differs from original source')
    return value


def read_database(path,scope,expected):
    """Only read the owned closed database; never load real application state."""
    path=Path(path).resolve();require(path.name==DB_NAME and path.is_file(),'Missing private PlanInfo SQLite file')
    require(0<path.stat().st_size<=16*1024*1024,'Unbounded private PlanInfo SQLite evidence')
    envelope(expected,scope)
    files=[p for p in (path,Path(str(path)+'-wal'),Path(str(path)+'-shm')) if p.exists()]
    before={p:hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    with closing(sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)) as db:
        db.execute('PRAGMA query_only=ON')
        require(db.execute('PRAGMA quick_check').fetchall()==[('ok',)],'Private PlanInfo database failed integrity check')
        tables={r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
        require(tables=={'meta','workspaces','channels','usergroups','users','messages','channel_state'},'Private cache table inventory changed')
        rows=db.execute('SELECT team_id,url,self_user_id,sidebar FROM workspaces').fetchall()
        require(len(rows)==1 and rows[0][:3]==(scope['team'],WORKSPACE_URL,scope['user']),'Private cache contains foreign workspace')
        sidebar=json.loads(rows[0][3]);record=sidebar.get('plan_info')
        require(record==expected,'Closed SQLite PlanInfo differs from original save/reload receipt')
        counts={name:db.execute('SELECT count(*) FROM '+name).fetchone()[0] for name in sorted(tables)}
    require(all(hashlib.sha256(p.read_bytes()).hexdigest()==digest for p,digest in before.items()),'Read-only audit changed private database bytes')
    return dict(path=str(path),sha256=before[path],readOnly=True,scope=scope,workspaceMetadataUrl=WORKSPACE_URL,planInfo=record,rows=counts)

ACCOUNT='67c1fd64-2911-4d9a-aa50-fd73155a98f8'
SCOPE=dict(account=ACCOUNT,team='TWORKFLOW',user='UWORKFLOW',url='https://example.test',enterprise=None)
STAGES=('about','draft','detail','empty-positive','null-positive','foreign-refused','late-refused','returned')


def warm(value,seed,directory,index,previous=None):
    require(isinstance(value,dict) and value.get('account')==ACCOUNT,'Missing original warm account')
    reload=value.get('reload',{});saved=reload.get('saved');envelope(saved,SCOPE)
    raw=seed['extras']['plan_info']
    require(saved['raw']==raw and reload.get('restored')==saved,'Original accepted source/save/reload diverged')
    require(Path(reload.get('path',''))==Path(directory)/DB_NAME
            and reload.get('boot')==seed['boot'] and reload.get('init')==seed['init']
            and reload.get('extras')=={'ok':True},'Reload did not use exact private path/fresh original Boot/Init/missing Extras')
    require(all(reload.get(k) is True for k in ('saveClosedBeforeLoad','startupLoad'))
            and all(reload.get(k) is False for k in ('liveRawRestored','oldLiveCurrent','oldSourceCurrent','foreignAccountMatches','defaultCacheInstalled')),
            'Cached display promoted historical source or installed default cache')
    require(type(reload.get('previousEpoch')) is int and reload['currentEpoch']==reload['previousEpoch']+1
            and value.get('epoch')==reload['currentEpoch'],'Reload epoch/owner changed')
    actions=[dict(kind='empty-positive',proof=dict(raw=dict(ok=True,plan_info={}),originalCurrent=True,accepted=True,error=None)),
        dict(kind='null-positive',proof=dict(raw=dict(ok=True,plan_info={'reconnect_grace_period_start_ts':None}),originalCurrent=True,accepted=True,error=None)),
        dict(kind='foreign-refused',proof=dict(raw=dict(id='TFOREIGN',is_plan_frozen=False,reconnect_grace_period_start_ts=99),ignored=True)),
        dict(kind='late-refused',proof=dict(raw=dict(ok=True,plan_info=dict(is_plan_frozen=False,reconnect_grace_period_start_ts=1,late=True)),originalCurrent=False,accepted=False,error='Slack API returned error: channel_initialization_unavailable'))]
    count=max(0,min(index-2,4));require(value.get('actions')==actions[:count],'Original ordered positive/foreign/late source receipts changed')
    expected=dict(raw)
    if index>=4:expected['reconnect_grace_period_start_ts']=None
    provenance='retained-cache' if index<3 else 'live'
    require(value.get('display')==dict(current=True,provenance=provenance,raw=expected),'Display provenance/raw changed')
    require(value.get('live')==(None if index<3 else dict(current=True,raw=expected)),'Cached plan became live authority or positive action missing')
    require(value.get('oldDisplayCurrent') is (index<3)
            and value.get('oldSourceCurrent') is (False if index<6 else None),'Old display/source was resurrected')
    require(value.get('retained')==dict(saved,raw=expected),'Runtime retained envelope did not preserve exact merge/null semantics')
    if previous:
        require(reload==previous['reload'] and value['actions'][:len(previous['actions'])]==previous['actions'],'Original reload/action history rewritten')
    return expected,provenance


def labels(channel,raw,provenance,live,require_native,h,viewport):
    from workflow_activity_shared_frozen import GRACE,NAMES,validate_observation
    from activity_grace_clock import LabelSpec
    observation=channel['labels'];seed=channel['seed']
    require(observation.get('current') is True and observation.get('plan')==raw
            and observation.get('planProvenance')==provenance,'Canonical label observation has wrong display source')
    if live:
        validate_observation(channel,GRACE,require)
    else:
        ids=sorted(('CACTKNOWN01',*GRACE.ids));rows=observation.get('rows',[])
        require([r.get('id') for r in rows]==ids,'Null replacement lost canonical label inputs')
        sources={r['id']:r for r in seed['boot']['channels'][:5]}
        for row in rows:
            id=row['id'];canonical=dict(sources[id],is_member=id!='CGRARCH01')
            require(row['raw'].get('fromAnotherTeam') is False and 'is_shared' not in row['raw']
                    and all(row['raw'].get(k)==v for k,v in canonical.items()),'Null action replaced canonical channel identity')
            p=row['policy'];require(type(p.get('capturedNowMs')) is int
                and seed['startSeconds']*1000<p['capturedNowMs']<(seed['startSeconds']+7200)*1000
                and p.get('graceEndMs') is None and p.get('inGrace') is False
                and p.get('private') is (id in GRACE.private_ids) and p.get('archiveIcon') is True
                and p.get('archivedSuffix') is (id in GRACE.archived_ids),'Null field did not replace grace display')
    h['validate_channel_labels'](require_native,viewport,spec=LabelSpec(live))
    identity=require_native.get('identity',{})
    require(identity.get('ok') is True and identity.get('sameRoot') is True and identity.get('sameDetail') is True
            and identity.get('rows')==[dict(id=node,channel=id,sameButton=True,sameName=True,connected=True,text=text)
                for node,id,text,_ in LabelSpec(live).vectors()], 'Original mounted label nodes replaced')


def settled_projection(projection,channel,raw,in_grace,native,h,viewport):
    require(isinstance(projection,dict),'Missing installed live label projection')
    # Both passive samples independently satisfy source, deadline and DOM rules.
    labels(dict(channel,labels=projection),raw,'live',in_grace,native,h,viewport)
    def stable(value):
        return dict(value,rows=[dict(row,policy={k:v for k,v in row['policy'].items()
                    if k!='capturedNowMs'}) for row in value['rows']])
    require(stable(projection)==stable(channel['labels']),
            'Installed projection differs from current stage source/deadline')


def validate(fixture,receipt,captures,h,*,platform,png=True):
    from workflow_activity_shared_frozen import GRACE,validate_observation
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('case')==CASE,
            'Wrong PlanInfo native receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')),'Missing original entry/isolation')
    rows=receipt.get('stages');paths=receipt.get('captures');viewport=h['expected_viewport'](fixture,platform)
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES) and isinstance(paths,dict)
            and set(paths)==set(STAGES) and receipt.get('viewport')==viewport,'Wrong PlanInfo stages/capture inventory')
    draft=source_baseline=detail=None;previous=None
    for i,row in enumerate(rows):
        m=row['model'];g=row['geometry'];proof=row['proof'];channel=proof['channel'];meta=m['metadata'];ledger=channel['ledger']
        value=proof.get('planInfoWarm');raw,provenance=warm(value,channel['seed'],captures,i,previous)
        require(m.get('account')==ACCOUNT and m.get('epoch')==value['epoch'],'Foreign model/account epoch')
        # Validate original Boot/Init/Extras independently of later replacement.
        seed_only=dict(channel,labels=None);validate_observation(seed_only,GRACE,require)
        require(meta.get('networkCalls')==meta.get('writes')==m['ledger'].get('networkCalls')==ledger.get('networkCalls')==0,
                'Warm display performed writes/live requests')
        require(meta.get('reads')==(0 if i==0 else 12 if i==1 else 16) and meta.get('pending') is False and meta.get('busy') is False,
                'Original entry reads changed or PlanInfo action reacquired policy')
        require(ledger.get('requests')==ledger.get('aboutRequests')==[] and ledger.get('completed')==ledger.get('cancelled')==0
                and ledger.get('held') is False,'Warm display navigated or fetched a conversation')
        requests=m['ledger']['requests'];h['validate_requests'](requests)
        require([r['method'] for r in requests]==(['workflows.activity.list','workflows.activity.get'] if i>=2 else [])
                and m['ledger'].get('finished')==len(requests) and m['ledger'].get('held') is None and m['ledger'].get('cancelled')==0,
                'Activity request history changed')
        sidebar=channel['sidebar'];native_sidebar=proof['sidebarNative']
        require(channel['selected']==ledger['original'] and native_sidebar.get('ok') is True
                and native_sidebar['rows']==[dict(name=r['name'],active=r['active']) for r in sidebar['rows']]
                and native_sidebar['headings']==[s['title'] for s in sidebar['sections']]
                and native_sidebar['activeId']==channel['selected'] and native_sidebar['activeIndex']==sidebar['activeIndex'],
                'Original sidebar selection changed')
        if i:
            require(meta.get('title')==h['TITLE'] and meta.get('description')==h['DESCRIPTION'] and meta.get('dirty') is True,'Dirty metadata lost')
            if draft is None:draft=meta
            require({k:v for k,v in meta.items() if k!='reads'}=={k:v for k,v in draft.items() if k!='reads'},'Original metadata changed')
            source={k:v for k,v in channel.items() if k!='labels'}
            if source_baseline is None:source_baseline=source
            if i==2:
                a=source_baseline['documents'];b=source['documents']
                require(len(a)==len(b)==1 and type(b[0]['revision']) is int and b[0]['revision']>a[0]['revision']
                        and b[0]['revision']==m['activity']['documentRevision']
                        and {k:v for k,v in a[0].items() if k!='revision'}=={k:v for k,v in b[0].items() if k!='revision'}
                        and {k:v for k,v in source.items() if k!='documents'}=={k:v for k,v in source_baseline.items() if k!='documents'},
                        'Entry changed original document/source')
                source_baseline=source
            require(source==source_baseline,'PlanInfo replacement changed original canonical/source/draft custody')
        else:require(channel['documents']==[],'Unexpected premount document')
        if i>=2:
            if detail is None:detail=m
            require(detail['activity'].get('active') is True and detail['activity']['detail']['workflow_event']['workflow_execution_id']=='E_REVIEW','Actual original detail absent')
            if i<7:
                require(m==detail,'PlanInfo action changed unrelated original Activity model')
                native=proof['warmNative'] if i==2 else proof['exercise']['native']
                labels(channel,raw,provenance,i<4,native,h,viewport)
                if i>=3:
                    settled_projection(proof['exercise'].get('projection'),channel,raw,i<4,native,h,viewport)
            else:
                require(m['activity']==dict(detail['activity'],active=False)
                        and m['export']==dict(detail['export'],current=dict(shown=False,disabled=True,notice=None))
                        and all(m.get(k)==detail.get(k) for k in m if k not in ('activity','export')),'Returning changed original content/draft/owner')
                native=proof['exercise']['native'];require(native.get('ok') is True and native.get('originalDetailConnected') is False
                        and type(native.get('originalRootConnected')) is bool and native.get('title')==h['TITLE']
                        and native.get('description')==h['DESCRIPTION'],'Original detail not detached or visible draft changed')
        if i in (1,7):require(g.get('fields')==dict(title=h['TITLE'],description=h['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False),'Visible dirty draft changed')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,'Native geometry invalid')
        require(isinstance(g.get('controls'),list) and g['controls'] and isinstance(g.get('notices'),list),'Missing native controls')
        for control in g['controls']+g['notices']:
            require(control.get('visible') is True,'Original control hidden');h['rectangle'](control.get('rect'),viewport)
        dimensions=g.get('pngDimensions');require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(n) is int and n>0 for n in dimensions),'Missing native PNG dimensions')
        path=Path(captures)/f'{fixture}-{row["stage"]}.png';require(Path(paths[row['stage']])==path,'Foreign capture path')
        if png:h['validate_png'](path,dimensions)
        previous=value
    require(receipt.get('final')==rows[-1]['model'] and receipt.get('planInfoWarm')==previous,'Terminal warm/model receipt changed')
    return receipt


def validate_database(path,receipt):
    record=receipt['planInfoWarm']['reload']['saved']
    evidence=read_database(path,SCOPE,record)
    output=Path(path).with_suffix('.closed.json')
    require(not output.exists(),'Preserve prior closed database audit')
    output.write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
    return evidence
