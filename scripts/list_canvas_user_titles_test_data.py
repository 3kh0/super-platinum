"""Explicit synthetic receipt corpus; never presented as native evidence."""
import copy
import json
import runpy
from pathlib import Path
import slack_list_canvas_user_titles as V
import slack_list_canvas_user_titles_source as S
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_file_completion.py')))
DIRECTORY=Path('/private/list-user-title')


def original():
    return S.BASE.read_bytes().decode().replace('"title":"Alex Canvas",','"title":"Alex Canvas","title_blocks":'+json.dumps(S.REFERENCE.title_blocks(),separators=(',',':'))+',',1)


def user_receipt(generation):
    return dict(current=True,generation=generation,members=[dict(source=S.member_raw(),effective=S.member_raw())])


def files(raw,ids,receipt=False):
    result=[]
    for id in ids:
        source=copy.deepcopy(next(x for x in raw['files'] if x['id']==id));effective=copy.deepcopy(source)
        effective['channels']=['CZMAP001','CAMAP001','CTAIL001'] if id==S.CANVAS else ['CYMAP002','CBMAP002']
        if receipt:effective=S.title_projection(source)
        row=dict(id=id,source=source,effective=effective)
        if receipt:row['receipt']=copy.deepcopy(raw)
        result.append(row)
    return result


def observed(case,raw,generation):
    token=V.F['token'](S.CANVAS)
    return dict(current=True,titles=dict(current=True,tokens=[token],required=[S.MEMBER],
        files=files(raw,[S.CANVAS],True),users=user_receipt(generation),labels={json.dumps(token):S.label(case)}),
        policy=dict(user=S.policy(case),enterprise=None,revision=0))


def query(q,case):
    if not isinstance(q,dict) or not q.get('values'):return
    for c in q['values']:
        id=V.F['identity'](c)
        if id==S.CANVAS:c['label']=S.label(case)
        elif c['value']['type']=='user':c['label']=V.G['LABELS'][id]
    q['labels']=[x['label'] for x in q['values']]
    seq=q['sequence'];u=user_receipt(f'list-comment-member-1-mount-original-{seq}-file-title-users')
    u['policy']=dict(user=S.policy(case),gantry=V.G['observations']()['client.gantryBoot'][1],enterprise=None,revision=0)
    q['titleUsers']={S.CANVAS:u}


def sample(case='user-title-known',narrow=False):
    fixture,receipt=B['sample'](narrow=narrow)
    rows=[receipt['stages'][i] for i in (0,2,3,4,5,6,7,8)]
    receipt.update(case=case,stages=rows)
    rawbytes=original();raw=json.loads(rawbytes);sourceproof=dict(rows=files(raw,[S.CANVAS,S.LIST]),unavailableIds=[],excludedIds=[V.F['HOSTED']])
    names=dict(zip(('draft','refreshed','keyboard-inserted','mouse-inserted','reopened','sending','sent','readback'),V.STAGES[case]))
    names['local-held']='query-ready'
    old_label='Alex Canvas';label=S.label(case)
    def text_rewrite(x):
        if isinstance(x,list):return [text_rewrite(a) for a in x]
        if isinstance(x,dict):return {k:text_rewrite(v) for k,v in x.items()}
        if isinstance(x,str):return x.replace(old_label,label)
        return x
    # Text-derived geometry is synthetic; original wire/source is restored below.
    rows[:]=text_rewrite(rows)
    for i,row in enumerate(rows):
        stage=V.STAGES[case][i];row['stage']=stage;row['capture']=str(DIRECTORY/f'{fixture}-{stage}.png')
        m,p,g=row['model'],row['proof'],row['geometry'];source=m['completionSource'];seed=V.G['seed_source']();seed['boot']['users'].append(S.member_raw())
        g['native']['tokens']=[t for t in g['native']['tokens'] if t['value']['type']!='link']
        source['seed']=seed;source['files']['originalResponse']=rawbytes
        if i:source['files']['source']=copy.deepcopy(sourceproof)
        source.update(completed=int(i>0),held=0,releases=[])
        for q,r in zip(source['requests'],source['responses']):
            if q['method']=='search.autocomplete.files':r.update(disposition='response',raw=rawbytes)
            if q['method']=='users.prefs.get':r['raw']=S.policy(case)
            if q['method']=='client.userBoot':r['raw']=copy.deepcopy(seed['boot'])
        policies=[] if i<4 else [dict(serial=1,method='users.prefs.get',raw=S.policy(case),account=m['account'],epoch=m['epoch'],team=S.TEAM,user=S.SELF)]
        if i>=6:policies.append({**policies[0],'serial':2})
        source['titleUsers']=dict(case=case,fileResponseBytes=rawbytes,requests=[],policy=policies,files=[],pending=[],
             sourcePresent=True,sourceObserved=True,sourceRaw=S.member_source(),sourceReady=True,native=S.member_raw(),
             knownDriver=dict(started=0,finished=0,active=0),driver=dict(started=0,finished=0,active=0))
        p['sourceFiles']=copy.deepcopy(sourceproof) if i else {'unavailable':True};p['releases']=[]
        query(m.get('completion'),case)
        for item in p['inputs']:
            item['stage']=names.get(item['stage'],item['stage'])
            if item['kind']=='choose':
                query(item['publication'],case);item['values']=copy.deepcopy(item['publication']['values'])
                item['value']['label']=item['values'][item['value']['index']]['label']
        if i>=5:p['inputs'].append(dict(kind='click',id='slack-list-comment-send',value=dict(ok=True,id='slack-list-comment-send')))
        if i>=7:p['inputs'].append(dict(kind='click',id='slack-list-comments-refresh',value=dict(ok=True,id='slack-list-comments-refresh')))
        for option,label_ in zip(g['completion']['rows'],(m.get('completion') or {}).get('labels',[])):option['label']=label_
        p.pop('settled',None)
        for d in m['draftStore']['drafts']:d['sequence']=0
        m['titleUsers']=None;m['titleAcquisition']=dict(rows=[])
        if i>=4:
            d=m['draftStore']['drafts'][0];blocks=rows[3]['proof']['expectedBlocks'];scope='independent-list-binding'
            m['titleUsers']=dict(scope=scope,capture=rows[4]['model']['draftStore']['drafts'][0]['capture'],sequence=0,blocks=blocks,
                tokens=[V.F['token'](S.CANVAS)],current=i<6,loading=False,status='ready',retryAt=0,error=None,
                observed=observed(case,raw,'list-draft-title-'+scope+'-file-title-users'))
            p['beforeReopen']=copy.deepcopy(rows[3]['model'])
        if i>=6:
            m['titleAcquisition']['rows']=[dict(id=S.CANVAS,serial=1,current=True,ordinal=0,
                titleUsers=observed(case,raw,'list-transcript-title-1-file-title-users'))]
    receipt['stages']=rows;receipt['final']=copy.deepcopy(rows[-1]['model'])
    return fixture,receipt
