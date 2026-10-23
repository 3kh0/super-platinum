"""Synthetic fixtures: original runtime wire/history, independently authored host ACKs."""
import base64
import copy
import json
from pathlib import Path

import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_middle_typing_fixtures as M
import test_canvas_split_recovery_oracle as R
import test_canvas_empty_enter_fixtures as A
import html

REF=Path(__file__).resolve().parent.parent/'tmp/canvas-empty-typing-20261023/reference'
SESSION='0123456789ab'
HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following-ascii','text-following-unicode','empty-final-following','empty-nonfinal-following')


def ids(history):
    return ('temp:C:Dte0000000000000000000000001','temp:C:Dte0000000000000000000000000') if history=='retained-zero' else ('paragraph','following')


def reports(corpus,typing):
    prefix='New' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 &'
    visible=(prefix,prefix+' draftx',prefix+' draft',prefix+' paragraph') if typing=='correction' else (prefix,'') if typing=='erase' else (prefix,prefix+' draftx',prefix,'')
    return visible,tuple(x.replace('&','&amp;') for x in visible)


def labels(history,layout,typing,corpus):return dict(A.labels(history,layout),typing=typing,corpus=corpus)


def source_request(history='bootstrap',layout='no-following',typing='correction',corpus='ascii'):
    initial,old,_=A.source_request(history,layout)
    vectors=json.loads((REF/'empty-typing-result.json').read_text(encoding='utf-8'))['vectors']
    cadence='exact-revert' if typing=='restore' else typing
    vector=next(v for v in vectors if (v['mode'],v['layout'],v['corpus'],v['cadence'])==(history,layout,corpus,cadence))
    visible,rtml=reports(corpus,typing)
    assert vector['states']==list(rtml)
    sent=list(map(S.raw,S.messages(S.decoded(vector['wireBase64']),1)))
    selected=old['child'];original=old['parent']
    for i,identity in enumerate((original,selected)):
        session=F.replace(S.field(S.wire(sent[i]),38,2),1,identity)
        sent[i]=F.replace(F.replace(F.replace(sent[i],1,identity),38,session),33,SESSION+'-0')
        if i==1 and layout!='empty-final-following':sent[i]=F.replace(F.replace(sent[i],8,'aad:temp'),21,'aad:temp')
    value=dict(old,documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in sent)))
    return initial,value,sent


def checkpoint(source,value,layout,typing,corpus):
    c=A.checkpoint(source,value,layout);action=c['action']['EmptyEnter']
    return dict(c,version=11,action={'EmptyTyping':dict(action,child_updates=list(reports(corpus,typing)[1]))})


def authored(outgoing,source,content,sequence,original):
    row=F.merge(source,outgoing) if source else outgoing
    crdt=S.field(S.wire(outgoing),38,2);crdt=F.replace(F.replace(F.remove(crdt,12),3,sequence),4,8_000_000_000_000_000)
    for tag,value in [(2,sequence),(6,'thread'),(7,'document'),(12,F.body(content,original)),(38,crdt)]:row=F.replace(row,tag,value)
    if not source:row=F.replace(F.replace(row,8,'aad'),21,'aad')
    return row


def continued(source,content,sequence,original):
    value,row=M.continued(source,content,'c'*12,sequence)
    row=F.replace(row,12,F.body(content,original));value['documentData']=F.b64(F.f(14,F.TITLE)+F.f(1,row))
    return value,row


NAMES={'success':('typed-child','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
       'stale':('typed-child','cancelled','refused','cancelled-stale'),'partial':('typed-child','cancelled','saving','uncertain')}


def blocks(count):
    value=F.blocks(*['']*count)
    for section in value[0]['elements']:section['elements']=[]
    return value


def witness(generation,typing,corpus):
    value=A.witness(generation);updates,_=reports(corpus,typing);before='';events=[]
    for index,after in enumerate(updates):
        n=3 if corpus=='ascii' else 10
        if index==0:kind,data,start,end,selection='insertText',updates[0],0,0,(0,0)
        elif typing=='erase' or typing=='restore' and index==3:
            kind,data,start,end,selection='deleteContentBackward',None,0,len(before.encode('utf-16-le'))//2,(0,len(before.encode('utf-16-le'))//2)
        elif index==1:kind,data,start,end,selection='insertText',' draftx',n,n,(n,n)
        elif typing=='restore':kind,data,start,end,selection='deleteContentBackward',None,n,n+7,(n,n+7)
        elif index==2:kind,data,start,end,selection='deleteContentBackward',None,n+6,n+7,(n+7,n+7)
        else:kind,data,start,end,selection='insertText','paragraph',n+1,n+6,(n+1,n+6)
        current=blocks(2)
        if after:current[0]['elements'][1]['elements']=[dict(type='text',text=after)]
        placeholder=None if after else dict(blockCount=1,sectionCount=2,childHtml='<br>',spanDepth=0,caretOffset=0)
        markup='<div class="rich-editor-block"><div class="rich-editor-section"><br></div><div class="rich-editor-section">'+('<span>'+html.escape(after,quote=False)+'</span>' if after else '<br>')+'</div></div>'
        event=dict(inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=end,beforeText=before,
                   afterText=after,beforeRevision=index+1,revision=index+2,caret=start+len((data or '').encode('utf-16-le'))//2,
                   beforeInputTrusted=True,inputTrusted=True,after=current,rawAfter=copy.deepcopy(current),html=markup,placeholder=placeholder)
        events.append(event);before=after
    value['typing']=dict(revision=len(events)+1,events=events,**{k:copy.deepcopy(events[-1][k]) for k in ('after','rawAfter','html','placeholder')})
    return value


def reader(source):
    result=[]
    for row in source['sections'][1:]:
        content=S.wire(S.field(S.decoded(row['raw']),12,2));inner=S.wire(S.field(content,1,2))
        result.append(dict(id=row['id'],text=S.text(inner,1).replace('&amp;','&')))
    return result


def receipt(scenario='success',history='bootstrap',layout='no-following',typing='correction',corpus='ascii'):
    initial,value,sent=source_request(history,layout,typing,corpus);old,following=ids(history);selected=value['child'];reused=layout=='empty-final-following'
    source_rows={r['id']:base64.b64decode(r['raw']) for r in initial['sections']}
    parent=authored(sent[0],source_rows[old],'',2000,True)
    child=authored(sent[1],source_rows.get(selected),reports(corpus,typing)[1][-1],2000 if reused else 1000,False)
    def readback(new_parent,new_child):
        rows=[]
        for row in initial['sections']:
            identity=row['id'];rows.append(new_parent if identity==old else new_child if identity==selected else source_rows[identity])
            if identity==old and not reused:rows.append(new_child)
        return F.inventory(rows)
    read0=readback(parent,child)
    wchild,crow=continued(child,'Continued paragraph',1,False);child_edited=authored(crow,child,'Continued paragraph',3000,False)
    read1=readback(parent,child_edited)
    wparent,prow=continued(parent,'Original paragraph edited',2,True);parent_edited=authored(prow,parent,'Original paragraph edited',3000,True)
    read2=readback(parent_edited,child_edited)
    cp=checkpoint(initial,value,layout,typing,corpus);empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='empty-pair-inert',checkpoint=cp))
    original=dict(**labels(history,layout,typing,corpus),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    cancelled,saved=witness(10,typing,corpus),witness(11,typing,corpus);stages=[]
    for i,name in enumerate(NAMES[scenario]):
        source=copy.deepcopy(original);pending=name in ('saving','continued-saving','parent-saving');uncertain=name=='uncertain';accepted=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':
                changed=next(r for r in fresh['sections'] if r['id']==(following if reused else old))
                changed.update(sequence=3000,raw=F.b64(F.replace(base64.b64decode(changed['raw']),2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[value],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[F.acknowledgement([parent,child],partial=scenario=='partial')]
        if accepted:
            source.update(fresh=read0,visible=read0,readbacks=[read0])
            for start,write,prior,next_read,row in [(5,wchild,read0,read1,child_edited),(8,wparent,read1,read2,parent_edited)]:
                if i>=start:
                    source['writes'].append(write);source['preflights'].append(dict(source=prior,fresh=prior,outcome='prepared'))
                    if i>start:source['acknowledgements'].append(F.acknowledgement([row],continued=True));source['readbacks'].append(next_read);source.update(fresh=next_read,visible=next_read)
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True);isheld=name in ('typed-child','saving','refused','uncertain')
        parts=['',reports(corpus,typing)[0][-1]] if isheld else [reports(corpus,typing)[0][-1]] if name=='child-editing' else [''] if name=='parent-editing' else ['Continued paragraph'] if name=='continued-saving' else ['Original paragraph edited'] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='typed-child' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None,emptyProof=None)
            if isheld:native.update(blocks=owner['typing']['after'],rawBlocks=owner['typing']['rawAfter'],emptyProof=owner['emptyProof'],typing=owner['typing'],placeholder=owner['typing']['placeholder'])
            else:
                if parts==['']:native['blocks']=blocks(1)
                native['rawBlocks']=copy.deepcopy(native['blocks'])
            geo['native']=native
        else:geo.update(native=None,paragraphs=reader(source['visible']))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(history,layout,typing,corpus),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
            fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,history='bootstrap',layout='no-following',typing='correction',corpus='ascii'):
    stages,terminal,_=R.packets(phase);source,value,_=source_request(history,layout,typing,corpus);cp=checkpoint(source,value,layout,typing,corpus);old,_=ids(history)
    pinned=labels(history,layout,typing,corpus);pinned.pop('mode');pinned['splitMode']='empty'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']==old)
            row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],emptyLabels=[],initialTail=None,originalChild=None,insertion=None,startTyping=None)
        if p['panel']:
            p.update(split=[dict(kind='original',text=''),dict(kind='parent',text=''),dict(kind='child',text=reports(corpus,typing)[0][-1])],emptyLabels=[dict(kind=k,text='Empty paragraph') for k in ('original','parent')],
                     current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(**r,deleted=False) for r in reader(source)],missing=[] if layout=='empty-final-following' else ['child'])
            p['warning']=' '.join(['Reuse and edit the existing empty paragraph after this paragraph' if layout=='empty-final-following' else 'Add and edit a paragraph after this paragraph',
                    'The second paragraph was edited after Enter. Those edits remain part of this unresolved change, even if the paragraph is empty again.',
                    'Either part, both parts, or neither part may have reached Slack. Removing this lock leaves both outcomes unknown.',
                    'Matching text does not establish whether the saved change succeeded.',
                    'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                    'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
