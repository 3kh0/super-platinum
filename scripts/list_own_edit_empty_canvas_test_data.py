"""Portable corpus built on original own-edit protocol fixtures; never launches UI."""
import copy
import json
import runpy
from pathlib import Path
import slack_list_own_edit_empty_canvas as V

ROOT=Path(__file__).resolve().parents[1]
OLD=runpy.run_path(str(ROOT/'scripts/test_list_own_edit_user_titles.py'))
S=V.S;DIR=OLD['DIR']


def observation(tokens,envelope,plain):
    raw=envelope['file'];effective=S.E.U.title_projection(raw);effective['is_deleted']=False
    return dict(current=True,policy=None,titles=dict(current=True,tokens=copy.deepcopy(tokens),required=[],users=None,
        files=[dict(id=S.CANVAS,source=copy.deepcopy(raw),effective=copy.deepcopy(effective),receipt=copy.deepcopy(envelope)) for _ in tokens],
        labels={json.dumps(t,sort_keys=True):'Untitled' for t in tokens},
        canvasFallback={S.CANVAS:dict(current=True,descriptor=copy.deepcopy(S.DESCRIPTOR),quipTitle=None if plain else
            dict(fileId=S.CANVAS,current=True,value='nullish'))}))


def candidate(envelope,invalid=False):
    if invalid:return dict(rows=[],unavailableIds=[S.CANVAS],excludedIds=[])
    raw=envelope['file'];effective=copy.deepcopy(raw);effective['channels']=['CZMAP001','CAMAP001','CTAIL001']
    return dict(rows=[dict(id=S.CANVAS,source=copy.deepcopy(raw),effective=effective,receipt=copy.deepcopy(envelope))],unavailableIds=[],excludedIds=[])


def unused(case):
    return dict(case=case,fileResponseBytes='unused static legacy fixture',requests=[],policy=[],files=[],pending=[],
        sourceRaw=None,sourceReady=True,sourceObserved=True,sourcePresent=False,native=None,driver=None,knownDriver=None)


def cold(case,index,owner):
    plain=case==V.LIFECYCLE
    texts={fid:S.E.singleton(fid,plain and fid==S.CANVAS) for fid in (S.CANVAS,S.E.LIST)}
    raw={k:json.loads(t) for k,t in texts.items()}
    count=1 if plain else (1,1,1,2,3,3,3,4,4,4)[index]
    completed=1 if plain else (1,1,1,2,2,3,3,3,4,4)[index]
    requests=[];responses=[]
    for n in range(1,count+1):
        mode='success' if n==1 else 'offline' if n==2 else 'held';text='null' if n==2 else texts[S.CANVAS]
        requests.append(dict(serial=n,id=S.CANVAS,method='files.info',httpMethod='POST',path='/api/files.info',
            args=dict(file=S.CANVAS,page='1',count='500',truncate='true'),mode=mode,plannedResponse=json.loads(text),responseText=text))
        responses.append(dict(serial=n,id=S.CANVAS,disposition=mode,raw=json.loads(text),responseText=text))
    held=[];releases=[]
    if not plain and index in (4,5):held=[dict(serial=3,id=S.CANVAS,receiverClosed=index==5)]
    if not plain and index==7:held=[dict(serial=4,id=S.CANVAS,receiverClosed=False)]
    if not plain and index>=6:releases.append(dict(serial=3,id=S.CANVAS,delivered=False,cancelledBeforeReply=True))
    if not plain and index>=8:releases.append(dict(serial=4,id=S.CANVAS,delivered=True,cancelledBeforeReply=False))
    return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,account=owner['account'],epoch=owner['epoch'],team='T1',user=V.SELF,
        networkCalls=0,httpCapability=False,futureResponseTexts=texts,futureResponses=raw,requests=requests,responses=responses,
        completed=completed,cancelled=int(not plain and index>=5),settlements=[dict(serial=n,cancelled=n==3) for n in range(1,completed+1)],
        held=held,releases=releases,invalidations=[],source=candidate(raw[S.CANVAS],not plain and 3<=index<=7))


def sample(case=V.LIFECYCLE,narrow=False):
    plain=case==V.LIFECYCLE
    fixture,r=OLD['sample'](narrow=narrow) if plain else OLD['held_sample'](narrow)
    if not plain:r['stages']=[r['stages'][i] for i in (0,1,2,3,4,5,6,8,9,10)]
    r['case']=case;rows=r['stages'];envelope=json.loads(S.E.singleton(S.CANVAS,plain))
    initial_source=unused(case)
    def fix_ready(entries):
        for e in entries:
            e['job']['observed']=observation(e['job']['tokens'],envelope,plain)
            for token in e['native']['tokens']:token['label']='Untitled'
    def fix_trans(trans):
        for row in trans['rows']:
            row['titleUsers']=observation(row['titleUsers']['titles']['tokens'],envelope,plain)
    for i,(row,name) in enumerate(zip(rows,V.STAGES[case])):
        row.update(stage=name,capture=str(DIR/f'{fixture}-{name}.png'))
        m,p,g=row['model'],row['proof'],row['geometry']
        m['titleSource']=copy.deepcopy(initial_source);m['coldFiles']=cold(case,i,m['comments'])
        fix_trans(m['transcriptTitles']);fix_trans(p['initialTranscript'])
        p['initialTitleSource']=copy.deepcopy(initial_source)
        for token in p['initialTranscriptNative']['tokens']:token.update(text='Untitled',renderedText='Untitled')
        fix_ready(p['titleReady'])
        p['emptyCanvas']=dict(initialResponses=dict(initialResponseTexts=m['coldFiles']['futureResponseTexts'],
            initialResponses=m['coldFiles']['futureResponses'],canonicalSeeded=False),plain=plain,label='Untitled')
        p['releases']=copy.deepcopy(m['coldFiles']['releases'])
        if g.get('row'):g['row']['text']=g['row']['text'].replace(OLD['U'].label('own-edit-title-known'),'Untitled')
        a=m['edit']['active']
        if a and a['titleUsers']['observed'] is not None:
            a['titleUsers']['observed']=observation(a['titleUsers']['tokens'],envelope,plain)
        if not plain:
            p.pop('invalidated',None)
            if i==3:
                j=a['titleUsers'];j.update(status='failed',loading=False,error='Title details could not load. Your saved edit and reply are retained.')
                g['status']['text']=j['error'];g['controls'].append(dict(id='slack-list-comment-edit-title-retry',visible=True,hit=True,disabled=False,rect=OLD['BASE']['B']['rect']()))
            p['titleActions']=[OLD['action'](id) for id in
                ((['slack-list-record-comments'] if i>=3 else [])+(['slack-list-comment-edit-title-retry'] if i>=4 else [])+
                 (['slack-list-comment-edit-cancel'] if i>=5 else [])+(['slack-list-comment-edit-cancel'] if i>=9 else []))]
            if i>=3:
                p['fileInvalidation']=dict(id=S.CANVAS,eventRevisionBefore=1,eventRevisionAfter=2,before=candidate(envelope),after=candidate(envelope,True))
                p['restoredOriginal']=copy.deepcopy(rows[3]['model']['edit']['active'])
                p['returnTranscript']=copy.deepcopy(rows[0]['model']['transcriptTitles'])
            if i>=4:
                p['heldOriginal']=copy.deepcopy(rows[4]['model']['edit']['active'])
                p['heldFile']=dict(serial=3,source=candidate(envelope,True))
            if i>=7:p['freshFile']=dict(serial=4,job=copy.deepcopy(rows[7]['model']['edit']['active']['titleUsers']))
            if i>=8:p['freshSeed']=copy.deepcopy(rows[8]['model']['edit']['active'])
    # Retained before/after proofs use the actual synthetic stage models above.
    if not plain:
        for row in rows[2:]:row['proof']['visitReplacement']['before']=copy.deepcopy(rows[1]['model']['edit'])
        propagation(rows,envelope)
    r['final']=copy.deepcopy(rows[-1]['model'])
    return fixture,r


def propagation(rows,envelope):
    """Explicit old Ready/file-stale -> new independently current transcript corpus."""
    token=V.tokens(V.received()['blocks'][:1])[0]
    initial_generation=rows[0]['model']['comments']['comments']['generation']
    generation=rows[7]['model']['comments']['comments']['generation']
    def snapshot(serial,gen,source_current=True,title_current=True):
        obs=observation([token],envelope,False)
        if not title_current:
            obs['current']=obs['titles']['current']=False;obs['titles']['labels']=None
            obs['titles']['canvasFallback'][S.CANVAS]['current']=False
        return dict(generation=gen,running=False,rows=[dict(id=S.CANVAS,serial=serial,ordinal=0,
            phase='ready',state='ready' if title_current else 'unavailable',current=source_current,
            retryable=source_current and not title_current,retryAt=0,notice=None if title_current else 'File unavailable',titleUsers=obs)])
    initial=snapshot(13,initial_generation);returned=snapshot(25,generation)
    before=snapshot(25,generation,title_current=False);after=snapshot(33,generation)
    for i,row in enumerate(rows):
        row['proof']['initialTranscript']=copy.deepcopy(initial)
        row['model']['transcriptTitles']=copy.deepcopy(initial if i<2 else
            snapshot(13,initial_generation,False,False) if i==2 else before if i<8 else after)
        if i>=3:row['proof']['returnTranscript']=copy.deepcopy(returned)
    active=rows[8]['model']['edit']['active'];editor=copy.deepcopy(rows[8]['geometry'])
    editor['edit']['sameNode']=True
    proof=dict(before=copy.deepcopy(before),after=copy.deepcopy(after),
        native=copy.deepcopy(rows[8]['proof']['initialTranscriptNative']),
        readLedgerBefore=copy.deepcopy(rows[7]['model']['coldFiles']['requests']),
        readLedgerAfter=copy.deepcopy(rows[8]['model']['coldFiles']['requests']),
        editorMount=dict(ok=True,scope=active['scope'],generation=str(active['generation']),mount=OLD['UUID'],blocks=copy.deepcopy(active['blocks'])),
        editorBefore=copy.deepcopy(editor),editorAfter=copy.deepcopy(editor))
    rows[8]['proof']['transcriptRecovery']=copy.deepcopy(proof)
    rows[9]['proof']['transcriptRecovery']=copy.deepcopy(proof)
    rows[9]['proof']['transcriptRecoveryFinal']={k:copy.deepcopy(proof[k]) for k in ('after','native','readLedgerAfter')}
