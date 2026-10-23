"""Independent fixed-fixture calendar and custody oracle for Activity clock checks.

Synthetic clock/zone inputs are not observations of an operating-system change.
Existing shared/frozen channel admission and navigation contracts stay separate.
"""
from datetime import datetime, timedelta, timezone
from workflow_activity_shared_frozen import GRACE, NAMES

FLAG = 'SUPER_PLATINUM_ACTIVITY_GRACE_CLOCK_CHECK'
CASE = 'channel-frozen-grace'
FIXTURES = ('workflow-activity', 'workflow-activity-narrow')
START_MS = 1772643600000  # March 4 2026, noon Eastern (17:00 UTC).
UTC_END_MS = 1773075600000
EASTERN_END_MS = 1773072000000
BETWEEN_MS = 1773073800000


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def deadline(eastern):
    """Pinned March 2026 calendar vector, independent of host timezone/tzdata."""
    require(type(eastern) is bool, 'Unobserved fixture calendar variant')
    start_offset = -5 if eastern else 0
    end_offset = -4 if eastern else 0
    start = datetime.fromtimestamp(START_MS / 1000, timezone.utc)
    local = start.replace(tzinfo=None) + timedelta(hours=start_offset)
    end = (local + timedelta(days=5) - timedelta(hours=end_offset)).replace(tzinfo=timezone.utc)
    result = int(end.timestamp() * 1000)
    require(result == (EASTERN_END_MS if eastern else UTC_END_MS), 'Calendar vector changed')
    return result


def policy(channel, now_ms, eastern):
    require(type(now_ms) is int and channel in ('CACTKNOWN01', *GRACE.ids),
            'Unknown original clock/channel input')
    eligible = channel != 'CGRNOGT01'
    end = deadline(eastern) if eligible else None
    live = eligible and now_ms < end
    private = channel in GRACE.private_ids
    archived = channel in GRACE.archived_ids
    icon = ('LockFilled' if private else 'Tag') if live else ('ArchiveFilled' if private else 'Archive')
    name = 'activity-known' if channel == 'CACTKNOWN01' else NAMES[channel]
    return dict(inGrace=live, graceEndMs=end, private=private, archiveIcon=not live,
                archivedSuffix=archived and not live, iconKind=icon,
                text=name + (' (archived)' if archived and not live else ''))


STAGES = ('about','draft','detail','grace-equal','grace-after','zone-held',
          'zone-invalidated','zone-refreshed','leave-held','activity-left','clock-left')
IDS = tuple(sorted(('CACTKNOWN01', *GRACE.ids)))
CHANGES = (
    dict(nowMs=UTC_END_MS,eastern=False,holdNext=False),
    dict(nowMs=UTC_END_MS+1,eastern=False,holdNext=False),
    dict(nowMs=BETWEEN_MS,eastern=False,holdNext=True),
    dict(nowMs=BETWEEN_MS,eastern=True,holdNext=False),
    dict(nowMs=BETWEEN_MS,eastern=True,holdNext=True),
    dict(nowMs=UTC_END_MS-1,eastern=False,holdNext=False),
)
CHANGE_COUNTS = (0,0,0,1,2,3,4,4,5,5,6)


class LabelSpec:
    special = True
    def __init__(self, live): self.live=live
    def vectors(self, fetched=False):
        require(fetched is False,'Unexpected fetched channel')
        values=[]
        for node,channel,text,icon in GRACE.vectors(False):
            if channel in IDS:
                p=policy(channel,UTC_END_MS-1 if self.live else UTC_END_MS,False)
                text,icon=p['text'],p['iconKind']
            values.append((node,channel,text,icon))
        return tuple(values)


def seed(channel):
    value=channel.get('seed',{})
    require(set(value)=={'boot','init','extras','startSeconds'} and value['startSeconds']==START_MS//1000,
            'Original fixed calendar seed missing')
    plan=dict(is_plan_frozen=True,reconnect_grace_period_start_ts=START_MS//1000)
    require(value['extras'].get('plan_info')==plan,'Original Extras PlanInfo changed')
    require(value['boot'].get('ok') is True and value['init'].get('ok') is True
            and value['boot'].get('team',{}).get('id')=='TWORKFLOW'
            and value['init'].get('team',{}).get('id')=='TWORKFLOW','Foreign Boot/Init owner')
    rows=[]
    for id in ('CACTKNOWN01',*GRACE.ids):
        row=dict(id=id,name='activity-known' if id=='CACTKNOWN01' else NAMES[id],is_channel=True,
                 is_member=True,is_archived=id in GRACE.archived_ids,is_private=id in GRACE.private_ids,
                 is_frozen=True,unlinked=START_MS//1000)
        if id in ('CACTKNOWN01','CGRARCH01'):row['grace_period_end']=1
        rows.append(row)
    raw=value['boot'].get('channels')
    require(isinstance(raw,list) and len(raw)==6 and raw[:5]==rows,'Original canonical channel source changed')
    users=value['boot'].get('users',[])
    require([u.get('id') for u in users]==['UWORKFLOW','UHOME0001']
            and all(isinstance(u.get('profile'),dict) for u in users)
            and raw[5].get('user')=='UHOME0001','Original canonical member seed changed')
    return plan,{r['id']:r for r in rows}


def projections(clock, previous=None):
    rows=clock.get('projections')
    require(isinstance(rows,list) and 0<len(rows)<=256,'Missing/bounded projection journal')
    pending=[]
    for i,r in enumerate(rows):
        require(set(r)=={'id','generation','sample','policies','held','finished','applied'} and r['id']==i
                and type(r['generation']) is int and r['generation']>=0
                and all(type(r[k]) is bool for k in ('held','finished','applied')),
                'Projection identity/disposition malformed')
        require(not r['applied'] or r['finished'],'Unfinished projection claimed applied')
        if not r['finished']:pending.append(r)
        require(i==0 or r['generation']>=rows[i-1]['generation'],'Projection generation reversed')
        sample=r['sample'];require(set(sample)=={'now_ms','eastern'} and type(sample['eastern']) is bool,
                                   'Projection clock/zone missing')
        require(sample in [dict(now_ms=UTC_END_MS-1,eastern=False)]+[
            dict(now_ms=c['nowMs'],eastern=c['eastern']) for c in clock['changes']],
            'Projection used an unobserved clock input')
        expected=[]
        for id in IDS:
            p=policy(id,sample['now_ms'],sample['eastern'])
            expected.append(dict(id=id,nowMs=sample['now_ms'],**{k:p[k] for k in ('graceEndMs','inGrace','archiveIcon','archivedSuffix')}))
        require(r['policies']==expected,'Original computed calendar policy differs')
    require(len(pending)<=1 and type(clock.get('held')) is bool
            and (not clock['held'] or len(pending)==1 and pending[0]['held']), 'Multiple or fabricated pending projections')
    if previous:
        old=previous['projections'];require(len(rows)>=len(old),'Projection history truncated')
        for a,b in zip(old,rows):
            require({k:v for k,v in a.items() if k not in ('finished','applied')}==
                    {k:v for k,v in b.items() if k not in ('finished','applied')},'Original projection rewritten')
            require(not a['finished'] or a==b,'Settled projection changed')
    return rows


def label_observation(labels, plan, raw, records):
    require(isinstance(labels,dict) and labels.get('current') is True and labels.get('plan')==plan,
            'Current original label authority absent')
    require(type(labels.get('clockGeneration')) is int and type(labels.get('observedGeneration')) is int
            and labels['clockGeneration']>=labels['observedGeneration'],'Unobserved clock generation')
    rows=labels.get('rows');require(isinstance(rows,list) and [r.get('id') for r in rows]==list(IDS),'Canonical label rows changed')
    matches=[r for r in records if r['applied'] and r['generation']==labels['observedGeneration']]
    require(matches,'Displayed projection lacks original applied receipt')
    # The product retains captured Policy provenance when recomputing the same
    # calendar result. The observation generation advances independently.
    through=matches[-1]['id'];retained={}
    for record in records[:through+1]:
        if record['applied']:
            for p in record['policies']:
                prior=retained.get(p['id'])
                if prior is None or prior['graceEndMs']!=p['graceEndMs']:
                    retained[p['id']]=p
    for row in rows:
        source=row.get('raw',{});id=row['id']
        canonical=dict(raw[id],is_member=id!='CGRARCH01')
        require(source.get('fromAnotherTeam') is False and 'is_shared' not in source
                and all(source.get(k)==v for k,v in canonical.items()),'Canonical label source changed')
        p=row.get('policy',{});require(p.get('private') is (id in GRACE.private_ids),'Private fact changed')
        require(all(p.get(key)==retained[id][other]
                    for key,other in (('capturedNowMs','nowMs'),('graceEndMs','graceEndMs'),('inGrace','inGrace'),
                                      ('archiveIcon','archiveIcon'),('archivedSuffix','archivedSuffix'))),
                'Retained policy did not preserve original applied calendar provenance')


def validate(fixture, receipt, captures, h, *, platform, png=True):
    from pathlib import Path
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('case')==CASE
            and receipt.get('fixture')==fixture and receipt.get('graceClock') is True,'Wrong native clock receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','actualActivityEntry','noLiveNetwork')),
            'Missing original entry/isolation')
    viewport=h['expected_viewport'](fixture,platform)
    rows=receipt.get('stages');paths=receipt.get('captures')
    require(receipt.get('viewport')==viewport and isinstance(rows,list)
            and tuple(r.get('stage') for r in rows)==STAGES and isinstance(paths,dict) and set(paths)==set(STAGES),
            'Wrong stage/capture inventory')
    original_channel=metadata=detail_model=None;previous=None;held_id=leave_id=None
    for i,row in enumerate(rows):
        name=row['stage'];m=row['model'];g=row['geometry'];proof=row['proof'];channel=proof['channel']
        require(m.get('account')=='synthetic-workflow-activity-channel' and m.get('epoch')==rows[0]['model']['epoch'],
                'Original account changed')
        plan,raw=seed(channel)
        ledger=channel['ledger'];activity=m['ledger'];meta=m['metadata']
        sidebar=channel['sidebar'];native_sidebar=proof.get('sidebarNative',{})
        require(channel['selected']==ledger['original'] and native_sidebar.get('ok') is True
                and native_sidebar.get('rows')==[dict(name=r['name'],active=r['active']) for r in sidebar['rows']]
                and native_sidebar.get('headings')==[s['title'] for s in sidebar['sections']]
                and native_sidebar.get('activeId')==channel['selected']
                and native_sidebar.get('activeIndex')==sidebar['activeIndex']
                and native_sidebar.get('activePosition')==next(n for n,r in enumerate(sidebar['rows']) if r['id']==channel['selected']),
                'Original sidebar/source selection changed')
        require(ledger.get('networkCalls')==activity.get('networkCalls')==meta.get('networkCalls')==meta.get('writes')==0,
                'Clock initiated live request or metadata write')
        require(ledger.get('requests')==ledger.get('aboutRequests')==[] and ledger.get('completed')==ledger.get('cancelled')==0
                and ledger.get('held') is False,'Clock navigated/reacquired conversation')
        h['validate_requests'](activity['requests'])
        require([r['method'] for r in activity['requests']]==(['workflows.activity.list','workflows.activity.get'] if i>=2 else [])
                and activity.get('finished')==len(activity['requests']) and activity.get('cancelled')==0
                and activity.get('held') is None,'Clock replayed or changed Activity request custody')
        require(meta.get('pending') is False and meta.get('busy') is False,'Clock changed draft transaction')
        require(meta.get('reads')==(0 if i==0 else 12 if i==1 else 16),
                'Original entry policy reads changed or clock reacquired policy')
        if i:
            require(meta.get('title')==h['TITLE'] and meta.get('description')==h['DESCRIPTION'] and meta.get('dirty') is True,
                    'Original dirty metadata lost')
            if metadata is None:metadata=meta
            require({k:v for k,v in meta.items() if k!='reads'}=={k:v for k,v in metadata.items() if k!='reads'},
                    'Clock changed original metadata model')
            source={k:v for k,v in channel.items() if k!='labels'}
            if original_channel is None:original_channel=source
            if i==2:
                before=original_channel['documents'];after=source['documents']
                require(len(before)==len(after)==1 and type(after[0].get('revision')) is int
                        and after[0]['revision']>before[0]['revision']
                        and m['activity']['documentRevision']==after[0]['revision']
                        and {k:v for k,v in after[0].items() if k!='revision'}=={k:v for k,v in before[0].items() if k!='revision'}
                        and {k:v for k,v in source.items() if k!='documents'}=={k:v for k,v in original_channel.items() if k!='documents'},
                        'Activity entry did not retain the exact original document under its new revision')
                original_channel=source
            require(source==original_channel,'Clock changed original source/visit/sidebar/document')
            docs=channel['documents'];require(len(docs)==1 and docs[0]['title']==h['TITLE']
                and docs[0]['description']==h['DESCRIPTION'] and docs[0]['dirty'] is True and docs[0]['pending'] is None,
                'Original document missing or duplicated')
        else:require(channel['documents']==[],'Premature document')
        if i>=2:
            if detail_model is None:detail_model=m
            require(detail_model['activity'].get('active') is True
                    and detail_model['activity'].get('detail',{}).get('workflow_event',{}).get('workflow_execution_id')=='E_REVIEW'
                    and detail_model['activity'].get('detailRow',{}).get('workflow_execution_id')=='E_REVIEW',
                    'Original Activity detail was not entered')
            if i<9:require(m==detail_model,'Clock-only step changed unrelated shell model')
            else:
                expected_export={**detail_model['export'],'current':dict(shown=False,disabled=True,notice=None)}
                require(m['activity'].get('active') is False and m.get('export')==expected_export
                        and all(m.get(k)==detail_model.get(k) for k in m if k not in ('activity','export')),
                        'Leaving Activity changed successor/owner/draft')
                require({k:v for k,v in m['activity'].items() if k!='active'}==
                        {k:v for k,v in detail_model['activity'].items() if k!='active'},'Leaving Activity changed retained content')
            evidence=proof['graceClock'] if i==2 else proof['exercise'];clock=evidence['clock'];native=evidence['native']
            require(clock.get('startSeconds')==START_MS//1000 and clock.get('utcEnd')==UTC_END_MS
                    and clock.get('easternEnd')==EASTERN_END_MS and clock.get('changes')==list(CHANGES[:CHANGE_COUNTS[i]]),
                    'Synthetic clock/zone action sequence changed')
            count=CHANGE_COUNTS[i];change=CHANGES[count-1] if count else dict(nowMs=UTC_END_MS-1,eastern=False)
            require(clock.get('sample')==dict(now_ms=change['nowMs'],eastern=change['eastern']), 'Current clock sample differs')
            records=projections(clock,previous)
            if i<9:
                label_observation(clock['labels'],plan,raw,records)
                live=i in (2,5,6)
                known=next(r['policy'] for r in clock['labels']['rows'] if r['id']=='CACTKNOWN01')
                require(known['graceEndMs']==(EASTERN_END_MS if i>=7 else UTC_END_MS)
                        and (clock['sample']['now_ms']<known['graceEndMs']) is live,
                        'DOM deadline is not the currently applied calendar result')
                h['validate_channel_labels'](native,viewport,spec=LabelSpec(live))
                identity=native.get('identity',{})
                require(identity.get('ok') is True and identity.get('sameRoot') is True and identity.get('sameDetail') is True,
                        'Mounted root/detail replaced')
                expected=[dict(id=node,channel=id,sameButton=True,sameName=True,connected=True,text=text)
                          for node,id,text,_ in LabelSpec(live).vectors()]
                require(identity.get('rows')==expected,'Original button/name nodes replaced or reordered')
            else:
                require(clock['labels']=={'current':False},'Retired Activity retained current labels')
                require(native.get('activityPresent') is False and native.get('detailPresent') is False
                        and native.get('originalDetailConnected') is False
                        and type(native.get('originalRootConnected')) is bool,'Retired native detail remains mounted')
                require(native.get('rows')==[dict(id=node,connected=False,nameConnected=False)
                        for node,_,_,_ in GRACE.vectors()], 'Original retired nodes were not detached')
            if i==5:
                held=[r for r in records if r['held'] and not r['finished']]
                require(clock['held'] is True and len(held)==1 and held[0]['sample']==dict(now_ms=BETWEEN_MS,eastern=False),
                        'Old computed UTC projection not actually held');held_id=held[0]['id']
            if i==6:
                old=records[held_id];require(clock['held'] is True and not old['finished']
                    and clock['labels']['clockGeneration']>old['generation'],'Old projection not invalidated by production generation')
            if i>=7:
                old=records[held_id];require(old['finished'] is True and old['applied'] is False,'Stale original projection adopted')
                require(any(r['applied'] and r['sample']==dict(now_ms=BETWEEN_MS,eastern=True) for r in records[held_id+1:]),
                        'New timezone never produced adopted computation')
            if i==8:
                held=[r for r in records if r['held'] and not r['finished']]
                require(clock['held'] is True and len(held)==1 and held[0]['sample']==dict(now_ms=BETWEEN_MS,eastern=True),
                        'Leave did not hold original computed projection');leave_id=held[0]['id']
            if i==9:require(clock['held'] is True and not records[leave_id]['finished'],'Leave reply released prematurely')
            if i==10:
                require(not clock['held'] and records[leave_id]['finished'] is True and records[leave_id]['applied'] is False,
                        'Late reply changed inactive successor')
                quiet=evidence.get('quiet',{})
                require(type(quiet.get('elapsedMillis')) is int and quiet['elapsedMillis']>=1250
                        and quiet.get('before')==quiet.get('after')==records,'Unmounted Activity started more projection work')
                require(m==rows[9]['model'],'Clock after leaving changed successor')
            previous=clock
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,
                'Original native geometry invalid')
        if i==1 or i>=9:
            require(g.get('fields')==dict(title=h['TITLE'],description=h['DESCRIPTION'],titleReadonly=False,descriptionReadonly=False),
                    'Original visible dirty metadata changed')
        require(isinstance(g.get('controls'),list) and g['controls'] and isinstance(g.get('notices'),list),
                'Missing measured original controls')
        for item in g['controls']+g['notices']:
            require(item.get('visible') is True,'Hidden native control');h['rectangle'](item.get('rect'),viewport)
        dimensions=g.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(n) is int and n>0 for n in dimensions),
                'Missing original PNG dimensions')
        path=Path(captures)/f'{fixture}-{name}.png';require(Path(paths[name])==path,'Foreign capture path')
        if png:h['validate_png'](path,dimensions)
    require(receipt.get('final')==rows[-1]['model'] and receipt.get('clock')==previous,'Terminal model/clock changed')
    return receipt
