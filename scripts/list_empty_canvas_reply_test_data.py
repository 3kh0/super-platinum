"""Synthetic portable corpus only; no native evidence or acceptance."""
import copy
import json
from pathlib import Path
import list_canvas_user_titles_test_data as OLD
import slack_list_empty_canvas_reply as O
S=O.S
DIR=Path('/private/list-empty-reply')

def fallback(plain=False):
    return dict(current=True,descriptor=copy.deepcopy(S.DESCRIPTOR),quipTitle=None if plain else dict(fileId=S.CANVAS,current=True,value='nullish'))

def observed(envelope=None,plain=False):
    envelope=envelope or json.loads(S.search_text());token=O.F['token'](S.CANVAS)
    raw=envelope['file'] if 'file' in envelope else envelope['files'][0]
    effective=S.U.title_projection(raw)
    if 'file' in envelope:effective['is_deleted']=False
    return dict(current=True,policy=None,titles=dict(current=True,tokens=[copy.deepcopy(token)],required=[],users=None,
        files=[dict(id=S.CANVAS,source=copy.deepcopy(raw),effective=effective,receipt=copy.deepcopy(envelope))],
        labels={json.dumps(token):'Untitled'},canvasFallback={S.CANVAS:fallback(plain)}))

def projection():
    raw=json.loads(S.search_text());rows=[]
    for r in raw['files'][:2]:
        e=copy.deepcopy(r);e['channels']=['CZMAP001','CAMAP001','CTAIL001'] if r['id']==S.CANVAS else ['CYMAP002','CBMAP002']
        rows.append(dict(id=r['id'],source=copy.deepcopy(r),effective=e,receipt=copy.deepcopy(raw)))
    return dict(rows=rows,unavailableIds=[],excludedIds=[O.F['HOSTED']])

def cold(model,source):
    texts={id:S.singleton(id) for id in (S.CANVAS,S.LIST)}
    return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,account=model['account'],epoch=model['epoch'],
        team='T1',user='ULIST0001',networkCalls=0,httpCapability=False,requests=[],responses=[],releases=[],settlements=[],invalidations=[],
        completed=0,cancelled=0,held=[],futureResponses={k:json.loads(t) for k,t in texts.items()},futureResponseTexts=texts,source=source)

def sample(narrow=False):
    fixture,r=OLD.sample(narrow=narrow)
    old_label=OLD.S.label('user-title-known')
    def display(v):
        if isinstance(v,list):return [display(x) for x in v]
        if isinstance(v,dict):return {k:display(x) for k,x in v.items()}
        return v.replace(old_label,'Untitled') if isinstance(v,str) else v
    r=display(r);r['case']=O.LIFECYCLE
    sourceproof=projection()
    for i,row in enumerate(r['stages']):
        m,p,g=row['model'],row['proof'],row['geometry'];source=m['completionSource']
        source.pop('titleUsers',None);source['seed']=O.G['seed_source']();source['files']['originalResponse']=S.search_text()
        source['files']['source']=copy.deepcopy(sourceproof) if i else {'unavailable':True}
        obs=O.F['observations'](False)
        for q,reply in zip(source['requests'],source['responses']):
            if q['method']=='search.autocomplete.files':reply['raw']=S.search_text()
            elif q['method'] in obs:reply['raw']=copy.deepcopy(obs[q['method']][1])
        def query(q):
            if isinstance(q,dict) and q.get('values'):
                q.pop('titleUsers',None);q['canvasFallback']={S.CANVAS:fallback()}
        query(m.get('completion'))
        for item in p['inputs']:
            if item['kind']=='choose':query(item['publication'])
        p['sourceFiles']=copy.deepcopy(source['files']['source']);m['coldFiles']=cold(m,copy.deepcopy(p['sourceFiles']))
        texts={id:S.singleton(id) for id in (S.CANVAS,S.LIST)}
        p['emptyCanvas']=dict(case=O.LIFECYCLE,label='Untitled',fileSeed=dict(initialResponseTexts=texts,
            initialResponses={k:json.loads(t) for k,t in texts.items()},canonicalSeeded=False))
        if m.get('titleUsers'):m['titleUsers']['observed']=observed()
        for attempt in m['titleAcquisition']['rows']:attempt['titleUsers']=observed()
        row['capture']=str(DIR/f'{fixture}-{row["stage"]}.png')
    r['final']=copy.deepcopy(r['stages'][-1]['model'])
    return fixture,r

def recovery(narrow=False):
    fixture,lifecycle=sample(narrow);original=lifecycle['stages'];r=copy.deepcopy(lifecycle);r['case']=O.RECOVERY
    base=original[2];wire=copy.deepcopy(base['proof']['expectedBlocks']);journal=copy.deepcopy(base['proof']['journal'])
    before_source=projection();invalid=copy.deepcopy(before_source);invalid['rows']=invalid['rows'][1:];invalid['unavailableIds']=[S.CANVAS]
    final_source=projection();raw=json.loads(S.singleton(S.CANVAS,True))
    final_source['rows'][0].update(source=raw['file'],receipt=raw)
    final_source['rows'][0]['effective'].update(title=' \t ',title_blocks=[])
    inv=dict(kind='future-plain-response-and-actual-local-invalidation',id=S.CANVAS,
        previousResponseText=S.singleton(S.CANVAS),futureResponseText=S.singleton(S.CANVAS,True),futureResponse=raw,
        invalidation=dict(id=S.CANVAS,before=before_source,after=invalid,eventRevisionBefore=1,eventRevisionAfter=2))
    tokens=copy.deepcopy(base['geometry']['native']['tokens'])
    arm=dict(ok=True,blocks=wire,tokens=tokens,selection=dict(anchor=2,focus=2),focused=True)
    mounted=dict(before=copy.deepcopy(arm),after=dict(arm,sameNode=True,sameSelection=True))
    old=dict(scope='independent-old',capture=journal['capture'],sequence=0,blocks=wire,tokens=[O.F['token'](S.CANVAS)],
        current=True,loading=True,status='loading',retryAt=0,error=None,observed=None)
    fresh={**copy.deepcopy(old),'scope':'independent-fresh'}
    retired=dict(job=copy.deepcopy(old),journal=copy.deepcopy(journal),fileSerial=2,fileSource=copy.deepcopy(invalid))
    plan,counts=O.action_plan(O.RECOVERY);events=copy.deepcopy(base['proof']['inputs'])
    events.extend(dict(kind='click',id=ident,value=dict(ok=True,id=ident)) for _,_,ident in plan[2:])
    rows=[]
    for index,name in enumerate(O.STAGES[O.RECOVERY]):
        row=copy.deepcopy(original[index] if index<3 else original[4] if index==9 else base)
        row['stage']=name;row['capture']=str(DIR/f'{fixture}-{name}.png');m,p,g=row['model'],row['proof'],row['geometry']
        p['emptyCanvas']['case']=O.RECOVERY
        if index>=2:
            p['plainInvalidation']=copy.deepcopy(inv);p['mountedCustody']=copy.deepcopy(mounted)
        if index>=3:
            p['beforeReopen']=copy.deepcopy(base['model']);p['runtimeForgotten']=True
            p['expectedBlocks']=copy.deepcopy(wire);p['journal']=copy.deepcopy(journal);p['inputs']=copy.deepcopy(events[:counts[index]])
            m['completionSource']=copy.deepcopy(base['model']['completionSource']);m['completion']=None
            m['titleAcquisition']=dict(rows=[]);m['composeLedger']=copy.deepcopy(base['model']['composeLedger'])
            m['comments']=copy.deepcopy(base['model']['comments'])
            m['ledger']=copy.deepcopy(original[4 if index<5 else 5 if index<8 else 7]['model']['ledger'])
        src=copy.deepcopy(before_source if index<2 else final_source if index==9 else invalid)
        if index>0:
            p['sourceFiles']=copy.deepcopy(src);m['completionSource']['files']['source']=copy.deepcopy(src)
        m['coldFiles']=cold(m,copy.deepcopy(p['sourceFiles']))
        c=m['coldFiles']
        if index>=2:
            c['futureResponseTexts'][S.CANVAS]=S.singleton(S.CANVAS,True);c['futureResponses'][S.CANVAS]=raw;c['invalidations']=[copy.deepcopy(inv)]
        count=[0,0,0,1,2,2,2,2,3,3][index]
        for serial in range(1,count+1):
            mode='offline' if serial==1 else 'held';text='null' if serial==1 else S.singleton(S.CANVAS,True)
            c['requests'].append(dict(serial=serial,id=S.CANVAS,method='files.info',httpMethod='POST',path='/api/files.info',
                args=dict(file=S.CANVAS,page='1',count='500',truncate='true'),mode=mode,plannedResponse=json.loads(text),responseText=text))
            c['responses'].append(dict(serial=serial,id=S.CANVAS,disposition=mode,raw=json.loads(text),responseText=text))
        done=[0,0,0,1,1,2,2,2,2,3][index];c['completed']=done;c['cancelled']=int(done>=2)
        c['settlements']=[dict(serial=n,cancelled=n==2) for n in range(1,done+1)]
        if 4<=index<=6:c['held']=[dict(serial=2,id=S.CANVAS,receiverClosed=index>=5)]
        if index==8:c['held']=[dict(serial=3,id=S.CANVAS,receiverClosed=False)]
        if index>=7:c['releases'].append(dict(serial=2,id=S.CANVAS,delivered=False,cancelledBeforeReply=True))
        if index>=9:c['releases'].append(dict(serial=3,id=S.CANVAS,delivered=True,cancelledBeforeReply=False))
        p['releases']=copy.deepcopy(c['releases'])
        if 3<=index<=8:
            job=copy.deepcopy(fresh if index==8 else old)
            if index==3:job.update(loading=False,status='failed',error='Title details could not load. Your saved draft is retained.')
            if index in (5,6,7):job.update(current=False)
            m['titleUsers']=job
            g.update(editorAbsent=True,editor=None,dialogPresent=index not in (6,7),record='R_LIST_001' if index==5 else None if index in (6,7) else O.READ['KNOWN'])
            g.pop('completion',None);g['titleStatus']=g['titleRetry']=None
            if index in (3,4,8):
                rect=dict(left=10,top=10,right=310,bottom=40,width=300,height=30)
                g['titleStatus']=dict(id='slack-list-comment-title-status',text=job['error'] or 'Loading title details...',visible=True,hit=True,rect=rect)
                if index==3:g['titleRetry']=dict(id='slack-list-comment-title-retry',text='Retry',visible=True,hit=True,disabled=False,rect=rect)
            if index in (5,6,7):m['comments']['active']=False
            if index in (6,7):
                for key in ('document','records','unrelated','detail'):m[key]=None
        if index>=5:p['retiredDraft']=copy.deepcopy(retired)
        if index>=8:p['returnedDraft']=dict(job=copy.deepcopy(fresh),fileSerial=3)
        if index==9:
            job=copy.deepcopy(fresh);job.update(loading=False,status='ready',observed=observed(raw,True));m['titleUsers']=job
            d=next(d for d in m['draftStore']['drafts'] if d['scope']==journal['scope'])
            d.update(blocks=copy.deepcopy(wire),capture=journal['capture'],savedRevision=journal['revision'],sequence=0)
            g['editor'].update(blocks=copy.deepcopy(wire),text=O.display(wire,O.RECOVERY))
            g['native']['snapshot']['blocks']=copy.deepcopy(wire);g['native']['tokens']=copy.deepcopy(tokens)
        rows.append(row)
    r['stages']=rows;r['final']=copy.deepcopy(rows[-1]['model']);return fixture,r
