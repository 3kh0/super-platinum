"""Portable fixture adapter: original histories with independent host/UI literals."""
import base64
import copy
from functools import lru_cache
import html
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_split_recovery_oracle as R

ROOT=Path(__file__).resolve().parent.parent
REF=ROOT/'tmp/canvas-middle-typing-20261023/reference'
SESSION='0123456789ab'


def literals(corpus,typing):
    left='Keep this' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600'
    tail='Second paragraph' if corpus=='ascii' else 'Second \u65e5\u672c\U0001f600 & paragraph'
    prefix='New ' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 '
    updates=['New',prefix+'draftx',prefix+'draft',prefix+'paragraph'] if typing=='correction' else [tail+'x','' if typing=='erase' else tail]
    return SimpleNamespace(left=left,tail=tail,full=left+tail,updates=updates,offset=9 if corpus=='ascii' else 11,
          continued='Continued paragraph' if corpus=='ascii' else 'Continued \u65e5\u672c\U0001f600 paragraph',edited=left+' edited')

def wiretext(value):return value.replace('&','&amp;')
def utf16(value):return len(value.encode('utf-16-le'))//2

def vector(corpus,typing,parent_mode):
    return next(v for v in json.loads((REF/'middle-typing-result.json').read_text(encoding='utf-8'))['vectors']
                if (v['corpus'],v['cadence'],v['parentMode'])==(corpus,'restore-tail' if typing=='restore' else typing,parent_mode))


def source_request(corpus='ascii',typing='correction',parent_mode='bootstrap'):
    e=literals(corpus,typing);v=vector(corpus,typing,parent_mode);rows=F.source_rows(corpus)
    rows[1]=F.replace(rows[1],12,F.body(wiretext(e.full)))
    if parent_mode=='retained':rows[1]=F.replace(rows[1],38,F.replace(base64.b64decode(v['initializedSession']),4,8_000_000_000_000_000))
    source=F.inventory(rows);parent,child=S.messages(S.decoded(v['wireBase64']),1)
    child_id=S.text(child,1)
    parent=F.replace(F.replace(S.raw(parent),12,F.body(wiretext(e.left))),33,SESSION+'-0')
    child=F.replace(F.replace(F.replace(S.raw(child),8,'aad:temp'),21,'aad:temp'),33,SESSION+'-0')
    child=F.replace(child,12,F.body(wiretext(e.updates[-1]),False))
    data=F.f(14,F.TITLE)+F.f(1,parent)+F.f(1,child)
    write=dict(kind='split',parent='paragraph',child=child_id,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,documentData=F.b64(data))
    return source,write,[parent,child]


def checkpoint(source,write,corpus,typing):
    e=literals(corpus,typing)
    return dict(version=8,scope=S.SCOPE.copy(),thread='thread',document='document',target='paragraph',session=SESSION,sequence=1,draft=wiretext(e.full),
            action={'SplitTyping':dict(child=write['child'],position='aad:temp',offset_utf16=e.offset,parent_draft=wiretext(e.left),child_draft=wiretext(e.tail),child_updates=[wiretext(v) for v in e.updates])},
            sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(write['documentData'])))


def authored(outgoing,source,content,sequence):
    row=F.merge(source,outgoing) if source else outgoing;identity=S.text(S.wire(outgoing),1);parent=identity=='paragraph'
    crdt=S.field(S.wire(outgoing),38,2);crdt=F.replace(F.replace(F.remove(crdt,12),3,sequence),4,8_000_000_000_000_000)
    for tag,value in [(2,sequence),(6,'thread'),(7,'document'),(12,F.body(wiretext(content),parent)),(38,crdt)]:row=F.replace(row,tag,value)
    if not parent:row=F.replace(F.replace(row,8,'aad'),21,'aad')
    return row


@lru_cache(maxsize=24)
def continued(source,content,session,sequence=1):
    crdt=S.field(S.wire(source),38,2)
    script=r"""
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));let d=A.load(new Uint8Array(Buffer.from(p.doc,'base64')),{actor:p.actor});
d=A.change(d,x=>A.updateText(x,['text'],p.content+'\ufeff'));
console.log(JSON.stringify({doc:Buffer.from(A.save(d)).toString('base64'),head:A.getHeads(d)[0]}));
"""
    r=subprocess.run(['node','--input-type=module','-e',script],input=json.dumps(dict(doc=F.b64(S.field(S.wire(crdt),5,2)),actor=session+'00000000',content=wiretext(content))),cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=30)
    if r.returncode:raise RuntimeError(r.stderr[:1000])
    result=json.loads(r.stdout);current=F.replace(F.replace(F.remove(crdt,4),5,base64.b64decode(result['doc'])),12,result['head'])+F.f(6,result['head'])
    identity=S.text(S.wire(source),1)
    row=F.f(1,identity)+F.f(2,S.field(S.wire(source),2,0))+F.f(9,0)+F.f(12,F.body(wiretext(content),identity=='paragraph'))+F.f(13,b'')+F.f(19,1)+F.f(20,1)+F.f(33,session+'-'+str(sequence-1))+F.f(38,current)
    write=dict(kind='text',parent=identity,session=session,sequence=sequence,formTitle='Fixture title',httpFactoryExecuted=False,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,row)))
    return write,row


def witness(generation,corpus,typing):
    e=literals(corpus,typing);result=dict(kind='middle',caretChildZero=True,generation=str(generation),visit='0',offset=e.offset,before=F.blocks(e.full),after=F.blocks(e.left,e.tail),
            revision=1,beforeInputTrusted=True,inputTrusted=True,inputType='insertParagraph',placeholder=None)
    result['rawAfter']=copy.deepcopy(result['after']);before=e.tail;events=[]
    for i,after in enumerate(e.updates):
        if i==0:
            kind,data='insertText','New' if typing=='correction' else 'x';start,stop=(0,utf16(before)) if typing=='correction' else (utf16(before),utf16(before));selection=(start,stop)
        elif typing=='erase':kind,data,start,stop='deleteContentBackward',None,0,utf16(before);selection=(start,stop)
        elif typing=='restore' or i==2:kind,data,start,stop='deleteContentBackward',None,utf16(before)-1,utf16(before);selection=(stop,stop)
        elif i==1:kind,data,start,stop='insertText',after[len(before):],utf16(before),utf16(before);selection=(start,stop)
        else:kind,data,start,stop='insertText','paragraph',utf16(before)-5,utf16(before);selection=(start,stop)
        canonical=F.blocks(e.left,after)
        if not after:canonical[0]['elements'][1]['elements']=[]
        child_html='<span>'+html.escape(after)+'</span>' if after else '<span><br></span>'
        rendered='<div class="rich-editor-block"><div class="rich-editor-section"><span>'+html.escape(e.left)+'</span></div><div class="rich-editor-section">'+child_html+'</div></div>'
        events.append(dict(inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=stop,beforeText=before,afterText=after,beforeRevision=i+1,revision=i+2,caret=start+utf16(data or ''),
                beforeInputTrusted=True,inputTrusted=True,after=canonical,rawAfter=copy.deepcopy(canonical) if after else F.blocks(e.left,'\n'),html=rendered,
                placeholder=None if after else dict(blockCount=1,sectionCount=2,childHtml=child_html,spanDepth=1,caretOffset=0)))
        before=after
    result['typing']={k:copy.deepcopy(events[-1][k]) for k in ('revision','after','rawAfter','html','placeholder')};result['typing']['events']=events
    return result


NAMES={'success':('typed-child','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
       'stale':('typed-child','cancelled','refused','cancelled-stale'),'partial':('typed-child','cancelled','saving','uncertain')}

def receipt(scenario='success',corpus='ascii',typing='correction',parent_mode='bootstrap'):
    e=literals(corpus,typing);initial,write,sent=source_request(corpus,typing,parent_mode);cp=checkpoint(initial,write,corpus,typing);child=write['child']
    original=dict(mode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    raws=[base64.b64decode(r['raw']) for r in initial['sections']]
    pair=[authored(sent[0],raws[1],e.left,2000),authored(sent[1],None,e.updates[-1],1000)]
    read0=F.inventory([raws[0],*pair,raws[2]])
    wchild,crow=continued(pair[1],e.continued,'c'*12);childrow=authored(crow,pair[1],e.continued,3000);read1=F.inventory([raws[0],pair[0],childrow,raws[2]])
    wparent,prow=continued(pair[0],e.edited,'c'*12,2);parentrow=authored(prow,pair[0],e.edited,3000);read2=F.inventory([raws[0],parentrow,childrow,raws[2]])
    empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='owned-operation',checkpoint=cp))
    cancelled,saved=witness(10,corpus,typing),witness(11,corpus,typing);stages=[]
    for i,name in enumerate(NAMES[scenario]):
        source=copy.deepcopy(original);pending=name in ('saving','continued-saving','parent-saving');uncertain=name=='uncertain';accepted=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':fresh['sections'][1].update(sequence=3000,raw=F.b64(F.replace(raws[1],2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[write],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[F.acknowledgement(pair,partial=scenario=='partial')]
        if accepted:
            source.update(fresh=read0,visible=read0,readbacks=[read0])
            for j,start,w,prior,next_read,row in [(1,5,wchild,read0,read1,childrow),(2,8,wparent,read1,read2,parentrow)]:
                if i>=start:
                    source['writes'].append(w);source['preflights'].append(dict(source=prior,fresh=prior,outcome='prepared'))
                    if i>start:source['acknowledgements'].append(F.acknowledgement([row],continued=True));source['readbacks'].append(next_read);source.update(fresh=next_read,visible=next_read)
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True)
        isheld=name in ('typed-child','saving','refused','uncertain')
        parts=[e.left,e.updates[-1]] if isheld else [e.updates[-1]] if name=='child-editing' else [e.continued] if name=='continued-saving' else [e.left] if name=='parent-editing' else [e.edited] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='typed-child' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if isheld:
                t=owner['typing'];native.update(blocks=t['after'],rawBlocks=t['rawAfter'],typing=t,placeholder=t['placeholder'])
            else:native.update(rawBlocks=copy.deepcopy(native['blocks']),typing=None,placeholder=None)
            geo['native']=native
        else:
            paragraphs=[dict(id='paragraph',text=e.edited if i>=9 else e.left if accepted else e.full)]
            if accepted:paragraphs.append(dict(id=child,text=e.continued if i>=6 else e.updates[-1]))
            paragraphs.append(dict(id='neighbor',text='Another paragraph'));geo.update(native=None,paragraphs=paragraphs)
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,mode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,corpus='ascii',typing='correction',parent_mode='bootstrap'):
    stages,terminal,_=R.packets(phase,corpus);source,write,_=source_request(corpus,typing,parent_mode);cp=checkpoint(source,write,corpus,typing);e=literals(corpus,typing)
    for stage in stages:
        stage.update(splitMode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=stage['canonical']['sections'][-1];row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p['originalChild']=None;p['initialTail']=dict(text=e.tail) if p['panel'] else None
        if p['panel']:
            p['split']=[dict(kind='original',text=e.full),dict(kind='parent',text=e.left),dict(kind='child',text=e.updates[-1])];p['current'][1]['text']=e.full
    terminal.update(splitMode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
