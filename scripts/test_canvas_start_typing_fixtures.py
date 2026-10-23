"""Synthetic typed-start native packets; wire/history from frozen original runtime B."""
import base64
import copy
from functools import lru_cache
import json
import html
from pathlib import Path
import subprocess
from types import SimpleNamespace
import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_middle_typing_fixtures as M

ROOT=Path(__file__).resolve().parent.parent
SESSION='0123456789ab'
NAMES={'success':('typed-original','cancelled','saving','confirmed','new-editing','new-saving','new-confirmed','original-editing','original-saving','original-confirmed'),
       'stale':('typed-original','cancelled','refused','cancelled-stale'),'partial':('typed-original','cancelled','saving','uncertain')}


def literals(corpus):
    seed='Keep thisSecond paragraph' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph'
    return SimpleNamespace(seed=seed,continued='Continued paragraph' if corpus=='ascii' else 'Continued \u65e5\u672c\U0001f600 paragraph',edited=seed+' edited')


def labels(corpus,parent_mode,typing='correction'):
    return dict(mode='start',corpus=corpus,typing=typing,parentMode=parent_mode,reuseBase='positive',reuseContent='empty',reuseChild=None)


def source_request(corpus='ascii',parent_mode='bootstrap',typing='correction'):
    ref=ROOT/'tmp/canvas-start-typing-20261023/reference-b'
    seed=next(v for v in json.loads((ref/'source-seeds.json').read_text(encoding='utf-8'))['seeds'] if v['corpus']==corpus and v['sourceMode']==parent_mode)
    rows=F.source_rows(corpus)
    if parent_mode=='retained':rows[1]=F.replace(rows[1],38,F.replace(base64.b64decode(seed['canonicalSessionBase64']),4,8_000_000_000_000_000))
    vector=next(v for v in json.loads((ref/'start-typing-result.json').read_text(encoding='utf-8'))['vectors']
                if v['sourceMode']==parent_mode and v['corpus']==corpus and v['cadence']==('exact-revert' if typing=='restore' else typing))
    expected=literals(corpus).seed.replace('&','&amp;')
    assert seed['rtml']==vector['full']==expected
    parent,new=map(S.raw,S.messages(S.decoded(vector['wireBase64']),1));child=S.text(S.wire(new),1)
    parent=F.replace(F.replace(parent,12,F.body(vector['states'][-1])),33,SESSION+'-0')
    new=F.replace(F.replace(F.replace(new,8,'aab:temp'),21,'aab:temp'),33,SESSION+'-0')
    write=dict(kind='start',parent='paragraph',child=child,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,
               documentData=F.b64(F.f(14,F.TITLE)+F.f(1,parent)+F.f(1,new)))
    return F.inventory(rows),write,parent,new,vector['states']


def checkpoint(source,write,reports):
    return dict(version=9,scope=S.SCOPE.copy(),thread='thread',document='document',target='paragraph',session=SESSION,sequence=1,
            draft=S.body(S.decoded(source['sections'][1]['raw'])),action={'StartTyping':dict(child=write['child'],position='aab:temp',original_updates=reports)},
            sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(write['documentData'])))


def coalesced_original_write(corpus='unicode',parent_mode='retained'):
    """Adversary: genuine original-runtime doc with correct reports but squashed patches."""
    source,value,parent,new,reports=source_request(corpus,parent_mode,'correction')
    ref=ROOT/'tmp/canvas-start-typing-20261023/reference-b'
    seed=next(v for v in json.loads((ref/'source-seeds.json').read_text(encoding='utf-8'))['seeds'] if v['corpus']==corpus and v['sourceMode']==parent_mode)
    before=S.wire(base64.b64decode(seed['initializedSessionBase64']))
    script=r"""
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));let d=A.load(Uint8Array.from(Buffer.from(p.doc,'base64')),{actor:p.actor});
for(const text of p.reports)d=A.change(d,x=>A.updateText(x,['text'],text+'\ufeff'));
console.log(JSON.stringify({head:A.getHeads(d)[0],doc:Buffer.from(A.save(d)).toString('base64')}));
"""
    result=subprocess.run(['node','--input-type=module','-e',script],input=json.dumps(dict(actor=SESSION+'00000000',doc=F.b64(S.field(before,5,2)),reports=reports)),
                          cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=30)
    if result.returncode:raise RuntimeError(result.stderr[:1000])
    h=json.loads(result.stdout);session=S.wire(S.field(S.wire(parent),38,2));heads=[f[2] for f in before if f[0]==6]
    crdt=b''.join(f[3] for f in session if f[0] not in (5,6,12))+F.f(5,base64.b64decode(h['doc']))
    crdt+=b''.join(F.f(6,x) for x in heads+[h['head'].encode()]+heads)+F.f(12,h['head'])
    altered=F.replace(parent,38,crdt)
    return source,dict(value,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,altered)+F.f(1,new)))


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


def initial_witness(generation,corpus):
    seed=literals(corpus).seed;after=F.blocks('',seed);after[0]['elements'][0]['elements']=[]
    return dict(kind='start',caretOriginalZero=True,generation=str(generation),visit='0',offset=0,before=F.blocks(seed),after=after,
                rawAfter=F.blocks('\n',seed),revision=1,beforeInputTrusted=True,inputTrusted=True,inputType='insertParagraph',
                leadingPlaceholder=dict(blockCount=1,sectionCount=2,firstHtml='<span><br></span>',spanDepth=1,caretSection=1,caretOffset=0))


def witness(generation,corpus,typing):
    value=initial_witness(generation,corpus);seed=literals(corpus).seed;u=lambda x:len(x.encode('utf-16-le'))//2
    prefix='New ' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 '
    reports=['New '+seed,prefix+'draftx '+seed,prefix+'draft '+seed,prefix+'paragraph '+seed] if typing=='correction' else ['x'+seed,'' if typing=='erase' else seed]
    events=[];before=seed
    for i,after in enumerate(reports):
        if i==0:kind,data,start,end,lo,hi='insertText','New ' if typing=='correction' else 'x',0,0,0,0
        elif typing=='erase':kind,data,start,end,lo,hi='deleteContentBackward',None,0,u(before),0,u(before)
        elif typing=='restore':kind,data,start,end,lo,hi='deleteContentBackward',None,0,1,1,1
        elif i==1:kind,data,start,end,lo,hi='insertText',('' if corpus=='ascii' else '\u65e5\u672c\U0001f600 ')+'draftx ',4,4,4,4
        elif i==2:
            at=9 if corpus=='ascii' else 14;kind,data,start,end,lo,hi='deleteContentBackward',None,at,at+1,at+1,at+1
        else:
            at=4 if corpus=='ascii' else 9;kind,data,start,end,lo,hi='insertText','paragraph',at,at+5,at,at+5
        canonical=F.blocks('',after);canonical[0]['elements'][0]['elements']=[]
        raw=F.blocks('\n',after)
        if not after:canonical[0]['elements'][1]['elements']=[];raw[0]['elements'][1]['elements']=[]
        markup='<div class="rich-editor-block"><div class="rich-editor-section"><span><br></span></div><div class="rich-editor-section">'+('<span>'+html.escape(after)+'</span>' if after else '<br>')+'</div></div>'
        event=dict(inputType=kind,data=data,selectionStart=lo,selectionEnd=hi,start=start,end=end,beforeText=before,afterText=after,
                   beforeRevision=i+1,revision=i+2,caret=start+u(data or ''),beforeInputTrusted=True,inputTrusted=True,
                   after=canonical,rawAfter=raw,html=markup,placeholder=None if after else dict(blockCount=1,sectionCount=2,childHtml='<br>',spanDepth=0,caretOffset=0))
        events.append(event);before=after
    value['typing']={k:copy.deepcopy(events[-1][k]) for k in ('revision','after','rawAfter','html','placeholder')};value['typing']['events']=events
    return value


def receipt(scenario='success',corpus='ascii',parent_mode='bootstrap',typing='correction'):
    e=literals(corpus);initial,write,parent,sent,reports=source_request(corpus,parent_mode,typing);cp=checkpoint(initial,write,reports);new=write['child'];final=reports[-1].replace('&amp;','&')
    original=dict(**labels(corpus,parent_mode,typing),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    raws=[base64.b64decode(r['raw']) for r in initial['sections']];inserted=authored(sent,None,'',1000);updated=authored(parent,raws[1],final,2000)
    read0=F.inventory([raws[0],inserted,updated,raws[2]])
    wnew,nrow=continued(inserted,e.continued,1);newrow=authored(nrow,inserted,e.continued,3000);read1=F.inventory([raws[0],newrow,updated,raws[2]])
    wold,orow=continued(updated,e.edited,2);oldrow=authored(orow,updated,e.edited,3000);read2=F.inventory([raws[0],newrow,oldrow,raws[2]])
    empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='owned-start',checkpoint=cp))
    cancelled,saved=witness(10,corpus,typing),witness(11,corpus,typing);stages=[]
    for i,name in enumerate(NAMES[scenario]):
        source=copy.deepcopy(original);pending=name in ('saving','new-saving','original-saving');uncertain=name=='uncertain';accepted=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':fresh['sections'][1].update(sequence=3000,raw=F.b64(F.replace(raws[1],2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[write],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[dict(partial=scenario=='partial',accepted=scenario!='partial',raw=F.b64(F.f(1,1)+F.f(2,F.f(15,'document')+F.f(1,updated)+(b'' if scenario=='partial' else F.f(1,inserted)))))]
        if accepted:
            source.update(fresh=read0,visible=read0,readbacks=[read0])
            for start,w,prior,next_read,row in [(5,wnew,read0,read1,newrow),(8,wold,read1,read2,oldrow)]:
                if i>=start:
                    source['writes'].append(w);source['preflights'].append(dict(source=prior,fresh=prior,outcome='prepared'))
                    if i>start:source['acknowledgements'].append(F.acknowledgement([row],continued=True));source['readbacks'].append(next_read);source.update(fresh=next_read,visible=next_read)
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True);isheld=name in ('typed-original','saving','refused','uncertain')
        parts=['',final] if isheld else [''] if name=='new-editing' else [e.continued] if name=='new-saving' else [final] if name=='original-editing' else [e.edited] if name=='original-saving' else None
        if parts is not None:
            owner=cancelled if name=='typed-original' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None)
            if isheld:native.update(blocks=owner['typing']['after'],rawBlocks=owner['typing']['rawAfter'],leadingPlaceholder=owner['leadingPlaceholder'],typing=owner['typing'],placeholder=owner['typing']['placeholder'])
            else:
                if parts==['']:native['blocks'][0]['elements'][0]['elements']=[]
                native['rawBlocks']=copy.deepcopy(native['blocks'])
            geo['native']=native
        else:
            paragraphs=([dict(id=new,text=e.continued if i>=6 else '')] if accepted else [])
            paragraphs.extend([dict(id='paragraph',text=e.edited if i>=9 else final if accepted else e.seed),dict(id='neighbor',text='Another paragraph')]);geo.update(native=None,paragraphs=paragraphs)
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(corpus,parent_mode,typing),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
            fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,corpus='ascii',parent_mode='bootstrap',typing='correction'):
    stages,terminal,_=M.R.packets(phase,corpus);source,write,_,_,reports=source_request(corpus,parent_mode,typing)
    cp=checkpoint(source,write,reports);e=literals(corpus);final=reports[-1].replace('&amp;','&')
    pinned=labels(corpus,parent_mode,typing);pinned.pop('mode');pinned['splitMode']='start'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=stage['canonical']['sections'][-1];row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],initialTail=None,originalChild=None,insertion=None,startTyping=None)
        if p['panel']:
            p['startTyping']=dict(originalBeforeText=e.seed,originalAfterText=final,newText='',emptyLabel='Empty paragraph',currentText=e.seed)
            p['warning']=' '.join(['Add an empty paragraph before and edit this paragraph','Original paragraph before the edit','Intended original paragraph',
                    'Intended new paragraph','Current original paragraph','The new empty paragraph belongs before the original paragraph.',
                    'This is a current comparison, not confirmation that the saved changes succeeded.',
                    'Either part, both parts, or neither part may have reached Slack. Removing this lock leaves both outcomes unknown.',
                    'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                    'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp


