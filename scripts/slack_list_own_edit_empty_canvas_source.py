"""Independent retained no-user title receipts for original own-comment edits."""
import json
import runpy
from pathlib import Path
import slack_list_empty_canvas_reply_source as E
import slack_list_own_edit_titles_source as OLD
require=E.require
CANVAS=E.CANVAS
DESCRIPTOR=E.DESCRIPTOR
tokens=OLD.tokens

def label(case):return 'Untitled'

def observed(value,envelope,expected_tokens,plain,*,current=True):
    require(isinstance(value,dict) and value.get('current') is current and value.get('policy') is None,
        'Own-edit title receipt stale or borrowed policy authority')
    titles=value.get('titles',{})
    require(titles.get('current') is current and titles.get('tokens')==expected_tokens and titles.get('required')==[]
        and titles.get('users') is None and 'excludedTokens' not in titles,
        'Original supported duplicate Canvas title occurrences/dependencies changed')
    raw=envelope['file'];effective=E.U.title_projection(raw);effective['is_deleted']=False
    wanted=dict(id=CANVAS,source=raw,effective=effective,receipt=envelope)
    require(titles.get('files')==[wanted for _ in expected_tokens],
        'Own-edit title lost original per-occurrence singleton/title projection')
    fallback=titles.get('canvasFallback',{})
    require(set(fallback)=={CANVAS},'Unrelated owner/file borrowed fallback capability')
    E.fallback(fallback[CANVAS],plain,current)
    labels=titles.get('labels')
    if not current:
        require(labels is None,'Stale retained title still grants labels')
        return
    require(isinstance(labels,dict) and len(labels)==len(expected_tokens),
        'Original duplicate Canvas title labels missing')
    parsed=[(json.loads(k),v) for k,v in labels.items()]
    require(all((token,'Untitled') in parsed for token in expected_tokens),'Title label replaced original raw/style/opaque token')

def ready(entry,envelope,expected_blocks,plain):
    job=entry.get('job',{});expected=tokens(expected_blocks)
    require(job.get('current') is True and job.get('status')=='ready' and job.get('loading') is False
        and job.get('error') is None and job.get('retryAt')==0 and job.get('blocks')==expected_blocks
        and job.get('tokens')==expected and job.get('ts')=='1781524801.000001'
        and isinstance(job.get('scope'),str) and job['scope'].startswith('list-edit:')
        and type(job.get('generation')) is int and job['generation']>0 and type(job.get('revision')) is int and job['revision']>=0,
        'Original pre-input own-edit title job not ready/current')
    observed(job.get('observed'),envelope,expected,plain)
    native=entry.get('native',{})
    require(native.get('snapshot',{}).get('blocks')==expected_blocks and native.get('tokens')==[
        dict(value=t,label='Untitled') for t in expected],'Original native edit tokens/wire changed')
    return job


def candidate(value,envelope,invalid=False):
    require(isinstance(value,dict) and value.get('excludedIds')==[]
        and value.get('unavailableIds')==([CANVAS] if invalid else []),
        'Original singleton availability changed')
    rows=value.get('rows')
    require(isinstance(rows,list) and len(rows)==(0 if invalid else 1),
        'Original singleton collection gained/lost unrelated rows')
    if invalid:return
    row=rows[0];raw=envelope['file'];effective=row.get('effective',{})
    require(row.get('id')==CANVAS and row.get('source')==raw and row.get('receipt')==envelope,
        'Original singleton raw/envelope lost')
    require(all(effective.get(k)==raw.get(k) for k in ('id','title','name','mode','permalink','title_blocks'))
        and effective.get('channels')==['CZMAP001','CAMAP001','CTAIL001'],
        'Original canonical candidate projection/channel order changed')


def unused_member_source(value,case):
    # Snapshot exists even though the named member fixture is NOT admitted.
    # Its fileResponseBytes is an unused static fixture literal, not an HTTP receipt.
    require(isinstance(value,dict) and value.get('case')==case
        and all(value.get(k)==[] for k in ('requests','policy','files','pending'))
        and value.get('driver') is None and value.get('knownDriver') is None
        and value.get('sourcePresent') is False and value.get('sourceRaw') is None
        and value.get('native') is None,'No-user edit dispatched or borrowed member/policy work')


def cold(row,plain,index,prior):
    m,p=row['model'],row['proof'];owner=m['comments'];v=m.get('coldFiles',{})
    require(all(v.get(k) is True for k in ('enabled','named','protectedFixture','identityCurrent'))
        and v.get('account')==owner['account'] and v.get('epoch')==owner['epoch']
        and v.get('team')=='T1' and v.get('user')=='ULIST0001'
        and v.get('networkCalls')==0 and v.get('httpCapability') is False,
        'Original files.info owner/capability changed')
    texts={fid:E.singleton(fid,plain and fid==CANVAS) for fid in (CANVAS,E.LIST)}
    raws={fid:json.loads(text) for fid,text in texts.items()};envelope=raws[CANVAS]
    require(v.get('futureResponseTexts')==texts and v.get('futureResponses')==raws
        and p.get('emptyCanvas')==dict(initialResponses=dict(initialResponseTexts=texts,
            initialResponses=raws,canonicalSeeded=False),plain=plain,label='Untitled'),
        'Original raw singleton bytes/seed changed')
    count=1 if plain else (1,1,1,2,3,3,3,4,4,4)[index]
    qs=v.get('requests');rs=v.get('responses')
    require(isinstance(qs,list) and isinstance(rs,list) and len(qs)==len(rs)==count,
        'Original files.info duplicated/omitted')
    for serial,(q,r) in enumerate(zip(qs,rs),1):
        mode='success' if serial==1 else 'offline' if serial==2 else 'held'
        text='null' if serial==2 else texts[CANVAS];raw=json.loads(text)
        require(q==dict(serial=serial,id=CANVAS,method='files.info',httpMethod='POST',path='/api/files.info',
            args=dict(file=CANVAS,page='1',count='500',truncate='true'),mode=mode,plannedResponse=raw,responseText=text)
            and r==dict(serial=serial,id=CANVAS,disposition=mode,raw=raw,responseText=text),
            'Original files.info factory/raw envelope/bytes changed')
    completed=1 if plain else (1,1,1,2,2,3,3,3,4,4)[index]
    require(v.get('completed')==completed and v.get('cancelled')==int(not plain and index>=5)
        and v.get('settlements')==[dict(serial=i,cancelled=i==3) for i in range(1,completed+1)],
        'Original file completion/cancellation disposition changed')
    held=[]
    if not plain and index in (4,5):held=[dict(serial=3,id=CANVAS,receiverClosed=index==5)]
    if not plain and index==7:held=[dict(serial=4,id=CANVAS,receiverClosed=False)]
    releases=[]
    if not plain and index>=6:releases.append(dict(serial=3,id=CANVAS,delivered=False,cancelledBeforeReply=True))
    if not plain and index>=8:releases.append(dict(serial=4,id=CANVAS,delivered=True,cancelledBeforeReply=False))
    require(v.get('held')==held and v.get('releases')==p.get('releases')==releases
        and v.get('invalidations')==[],'Original held file/release history changed')
    invalid=not plain and 3<=index<=7
    candidate(v.get('source'),envelope,invalid)
    if not plain and index>=3:
        event=p.get('fileInvalidation',{})
        require(event.get('id')==CANVAS and type(event.get('eventRevisionBefore')) is int
            and type(event.get('eventRevisionAfter')) is int and event['eventRevisionAfter']>event['eventRevisionBefore'],
            'Actual file source invalidation missing')
        candidate(event.get('before'),envelope);candidate(event.get('after'),envelope,True)
        if index>=4:
            require(p.get('heldFile')==dict(serial=3,source=event['after']),
                'Held original file source/serial changed')
        if index>=7:
            job=p.get('freshFile',{}).get('job',{})
            require(p.get('freshFile',{}).get('serial')==4 and job.get('scope')!=p['heldOriginal']['scope']
                and job.get('current') is True and job.get('observed') is None,
                'Fresh file acquisition reused retired edit owner')
    else:require('fileInvalidation' not in p,'Unexpected early file invalidation')
    if prior:
        old=prior['model']['coldFiles']
        for key in ('requests','responses','settlements','releases'):
            require(v[key][:len(old[key])]==old[key],'Original file protocol history rewritten')
    return envelope
