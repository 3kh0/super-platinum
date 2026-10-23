"""Portable start fixtures with original bootstrap bytes and independent literals."""
import base64
import copy
from functools import lru_cache
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_middle_typing_fixtures as M

ROOT=Path(__file__).resolve().parent.parent
SESSION='0123456789ab'
NAMES={'success':('start','cancelled','saving','confirmed','new-editing','new-saving','new-confirmed','original-editing','original-saving','original-confirmed'),
       'stale':('start','cancelled','refused','cancelled-stale'),'partial':('start','cancelled','saving','uncertain')}


def literals(corpus):
    seed='Keep thisSecond paragraph' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph'
    return SimpleNamespace(seed=seed,continued='Continued paragraph' if corpus=='ascii' else 'Continued \u65e5\u672c\U0001f600 paragraph',edited=seed+' edited')


def labels(corpus,parent_mode):
    return dict(mode='start',corpus=corpus,typing='none',parentMode=parent_mode,reuseBase='positive',reuseContent='empty',reuseChild=None)


def source_request(corpus='ascii',parent_mode='bootstrap'):
    rows=F.source_rows(corpus)
    if parent_mode=='retained':
        ref=ROOT/'tmp/canvas-middle-typing-20261023/reference'
        if corpus=='unicode':seed=json.loads((ref/'legacy-unicode-seed.json').read_text(encoding='utf-8'))
        else:seed=next(v for v in json.loads((ref/'source-seeds.json').read_text(encoding='utf-8'))['seeds'] if v['corpus']=='ascii' and v['parentMode']=='retained')
        rows[1]=F.replace(rows[1],38,F.replace(base64.b64decode(seed['initializedSessionBase64']),4,8_000_000_000_000_000))
    vector=json.loads((ROOT/'tmp/canvas-start-split-20261023/reference/start-result.json').read_text(encoding='utf-8'))['vectors'][0]
    row=S.raw(S.messages(S.decoded(vector['wireBase64']),1)[0]);child=S.text(S.wire(row),1)
    row=F.replace(F.replace(F.replace(row,8,'aab:temp'),21,'aab:temp'),33,SESSION+'-0')
    write=dict(kind='start',parent='paragraph',child=child,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,
               documentData=F.b64(F.f(14,F.TITLE)+F.f(1,row)))
    return F.inventory(rows),write,row


def checkpoint(source,write):
    return dict(version=1,scope=S.SCOPE.copy(),thread='thread',document='document',target=write['child'],session=SESSION,sequence=1,draft='',
            action={'Insert':dict(anchor='paragraph',before=True,position='aab:temp')},
            sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(write['documentData'])))


def authored(outgoing,source,content,sequence):
    row=M.authored(outgoing,source,content,sequence)
    if S.text(S.wire(row),1)!='paragraph':row=F.replace(F.replace(row,8,'aab'),21,'aab')
    return row


@lru_cache(maxsize=12)
def continued(source,content,sequence):
    if S.messages(S.wire(source),38):return M.continued(source,content,'c'*12,sequence)
    script=r"""
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));let d=A.init({actor:p.actor});
d=A.change(d,x=>{x.text=p.before+'\ufeff';x.deleted=false});const base=A.getHeads(d)[0];
d=A.change(d,x=>A.updateText(x,['text'],p.after+'\ufeff'));
console.log(JSON.stringify({base,head:A.getHeads(d)[0],doc:Buffer.from(A.save(d)).toString('base64')}));
"""
    result=subprocess.run(['node','--input-type=module','-e',script],input=json.dumps(dict(actor='c'*12+'00000000',before=S.body(S.wire(source)),after=M.wiretext(content))),
                          cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=30)
    if result.returncode:raise RuntimeError(result.stderr[:1000])
    h=json.loads(result.stdout);identity='paragraph';session='c'*12
    crdt=F.f(1,identity)+F.f(2,1000)+F.f(3,1000)+F.f(5,base64.b64decode(h['doc']))+F.f(6,h['base'])+F.f(6,h['head'])+F.f(8,h['base'])+F.f(12,h['head'])
    row=F.f(1,identity)+F.f(2,1000)+F.f(9,0)+F.f(12,F.body(M.wiretext(content)))+F.f(13,b'')+F.f(19,1)+F.f(20,1)+F.f(33,session+'-'+str(sequence-1))+F.f(38,crdt)
    return dict(kind='text',parent=identity,session=session,sequence=sequence,formTitle='Fixture title',httpFactoryExecuted=False,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,row))),row


def witness(generation,corpus):
    seed=literals(corpus).seed;after=F.blocks('',seed);after[0]['elements'][0]['elements']=[]
    return dict(kind='start',caretOriginalZero=True,generation=str(generation),visit='0',offset=0,before=F.blocks(seed),after=after,
                rawAfter=F.blocks('\n',seed),revision=1,beforeInputTrusted=True,inputTrusted=True,inputType='insertParagraph',
                leadingPlaceholder=dict(blockCount=1,sectionCount=2,firstHtml='<span><br></span>',spanDepth=1,caretSection=1,caretOffset=0))


def receipt(scenario='success',corpus='ascii',parent_mode='bootstrap'):
    e=literals(corpus);initial,write,sent=source_request(corpus,parent_mode);cp=checkpoint(initial,write);new=write['child']
    original=dict(**labels(corpus,parent_mode),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    raws=[base64.b64decode(r['raw']) for r in initial['sections']];inserted=authored(sent,None,'',1000)
    read0=F.inventory([raws[0],inserted,*raws[1:]])
    wnew,nrow=continued(inserted,e.continued,1);newrow=authored(nrow,inserted,e.continued,3000);read1=F.inventory([raws[0],newrow,*raws[1:]])
    wold,orow=continued(raws[1],e.edited,2);oldrow=authored(orow,raws[1],e.edited,3000);read2=F.inventory([raws[0],newrow,oldrow,raws[2]])
    empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='owned-start',checkpoint=cp))
    cancelled,saved=witness(10,corpus),witness(11,corpus);stages=[]
    for i,name in enumerate(NAMES[scenario]):
        source=copy.deepcopy(original);pending=name in ('saving','new-saving','original-saving');uncertain=name=='uncertain';accepted=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':fresh['sections'][1].update(sequence=3000,raw=F.b64(F.replace(raws[1],2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[write],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[dict(partial=scenario=='partial',accepted=scenario!='partial',raw=F.b64(F.f(1,1)+F.f(2,F.f(15,'document')+(b'' if scenario=='partial' else F.f(1,inserted)))))]
        if accepted:
            source.update(fresh=read0,visible=read0,readbacks=[read0])
            for start,w,prior,next_read,row in [(5,wnew,read0,read1,newrow),(8,wold,read1,read2,oldrow)]:
                if i>=start:
                    source['writes'].append(w);source['preflights'].append(dict(source=prior,fresh=prior,outcome='prepared'))
                    if i>start:source['acknowledgements'].append(F.acknowledgement([row],continued=True));source['readbacks'].append(next_read);source.update(fresh=next_read,visible=next_read)
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True);isheld=name in ('start','saving','refused','uncertain')
        parts=['',e.seed] if isheld else [''] if name=='new-editing' else [e.continued] if name=='new-saving' else [e.seed] if name=='original-editing' else [e.edited] if name=='original-saving' else None
        if parts is not None:
            owner=cancelled if name=='start' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None)
            if isheld:native.update(blocks=owner['after'],rawBlocks=owner['rawAfter'],leadingPlaceholder=owner['leadingPlaceholder'])
            else:
                if parts==['']:native['blocks'][0]['elements'][0]['elements']=[]
                native['rawBlocks']=copy.deepcopy(native['blocks'])
            geo['native']=native
        else:
            paragraphs=([dict(id=new,text=e.continued if i>=6 else '')] if accepted else [])
            paragraphs.extend([dict(id='paragraph',text=e.edited if i>=9 else e.seed),dict(id='neighbor',text='Another paragraph')]);geo.update(native=None,paragraphs=paragraphs)
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(corpus,parent_mode),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
            fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,corpus='ascii',parent_mode='bootstrap'):
    stages,terminal,_=M.R.packets(phase,corpus);source,write,_=source_request(corpus,parent_mode);cp=checkpoint(source,write);e=literals(corpus)
    pinned=labels(corpus,parent_mode);pinned.pop('mode');pinned['splitMode']='start'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=stage['canonical']['sections'][-1];row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],initialTail=None,originalChild=None,insertion=None)
        if p['panel']:
            p['insertion']=dict(anchor='paragraph',side='before',draftText='',emptyLabel='Empty paragraph',currentText=e.seed)
            p['warning']=' '.join(['Slack may have applied this change.','Add an empty paragraph before this paragraph','Intended new paragraph','Empty paragraph',
                    'The saved request inserts this paragraph before the paragraph shown below.','Current anchor paragraph',e.seed,
                    'This is a current comparison, not confirmation that the saved insertion succeeded.',
                    'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                    'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
