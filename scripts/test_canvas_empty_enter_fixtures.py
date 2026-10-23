"""Synthetic fixtures: original runtime wire/history, independently authored host ACKs."""
import base64
import copy
import json
from pathlib import Path

import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_middle_typing_fixtures as M
import test_canvas_split_recovery_oracle as R

REF=Path(__file__).resolve().parent.parent/'tmp/canvas-empty-enter-20261023/reference'
SESSION='0123456789ab'
HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following-ascii','text-following-unicode','empty-final-following','empty-nonfinal-following')


def ids(history):
    return ('temp:C:Dte0000000000000000000000001','temp:C:Dte0000000000000000000000000') if history=='retained-zero' else ('paragraph','following')


def labels(history,layout):return dict(mode='empty',emptyHistory=history,emptyLayout=layout,typing='none',corpus='ascii',parentMode='bootstrap',
                                     reuseBase='positive',reuseContent='empty',reuseChild=ids(history)[1] if layout=='empty-final-following' else None)


def source_request(history='bootstrap',layout='no-following'):
    original,following=ids(history)
    seeds=json.loads((REF/'source-seeds.json').read_text(encoding='utf-8'))['seeds']
    seed=next(s for s in seeds if s['mode']==history)
    vector=next(v for v in json.loads((REF/'empty-enter-result.json').read_text(encoding='utf-8'))['vectors'] if (v['mode'],v['layout'])==(history,layout))
    neighbor='Another paragraph' if layout=='text-following-ascii' else 'Caf\u00e9 &amp; \u65e5\u672c\U0001f600Second paragraph' if layout=='text-following-unicode' else ''
    assert vector['neighborText']==(None if layout=='no-following' else neighbor)
    def source(identity,position,content,language,empty=False):
        row=F.f(1,identity)+F.f(2,1000)+F.f(6,'thread')+F.f(7,'document')+F.f(8,position)+F.f(12,F.body(content,language))
        if empty and history!='bootstrap':
            row+=F.f(38,F.replace(F.replace(base64.b64decode(seed['canonicalSession']),1,identity),4,8_000_000_000_000_000))
        return row
    rows=[F.source_rows()[0],source(original,'aac','',True,True)]
    if layout!='no-following':rows.append(source(following,'aae',neighbor,False,layout.startswith('empty-')))
    if layout=='empty-nonfinal-following':rows.append(source('last-text','aag','Last paragraph',False))
    sent=list(map(S.raw,S.messages(S.decoded(vector['wireBase64']),1)))
    selected=following if vector['reused'] else vector['selectedId']
    for i,identity in enumerate((original,selected)):
        session=F.replace(S.field(S.wire(sent[i]),38,2),1,identity)
        sent[i]=F.replace(F.replace(F.replace(sent[i],1,identity),38,session),33,SESSION+'-0')
        if i==1 and not vector['reused']:sent[i]=F.replace(F.replace(sent[i],8,'aad:temp'),21,'aad:temp')
    value=dict(kind='empty',parent=original,child=selected,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,clearsChild=False,
               documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in sent)))
    return F.inventory(rows),value,sent


def checkpoint(source,value,layout):
    reused=layout=='empty-final-following'
    return dict(version=10,scope=S.SCOPE.copy(),thread='thread',document='document',target=value['parent'],session=SESSION,sequence=1,draft='',
                action={'EmptyEnter':dict(child=value['child'],position='aae' if reused else 'aad:temp',reused=reused)},
                sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(value['documentData'])))


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


NAMES={'success':('empty','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
       'stale':('empty','cancelled','refused','cancelled-stale'),'partial':('empty','cancelled','saving','uncertain')}


def blocks(count):
    value=F.blocks(*['']*count)
    for section in value[0]['elements']:section['elements']=[]
    return value


def witness(generation):
    before='<div class="rich-editor-block"><div class="rich-editor-section"><br></div></div>'
    after='<div class="rich-editor-block"><div class="rich-editor-section"><br></div><div class="rich-editor-section"><br></div></div>'
    shape=dict(html='<br>',spanDepth=0)
    return dict(kind='empty',offset=0,before=blocks(1),after=blocks(2),rawAfter=blocks(2),revision=1,beforeInputTrusted=True,
                inputTrusted=True,inputType='insertParagraph',caretChildZero=True,generation=str(generation),visit='0',
                emptyProof=dict(beforeHtml=before,afterHtml=after,before=shape,sections=[shape.copy(),shape.copy()],caretSection=1,caretOffset=0))


def reader(source):
    result=[]
    for row in source['sections'][1:]:
        content=S.wire(S.field(S.decoded(row['raw']),12,2));inner=S.wire(S.field(content,1,2))
        result.append(dict(id=row['id'],text=S.text(inner,1).replace('&amp;','&')))
    return result


def receipt(scenario='success',history='bootstrap',layout='no-following'):
    initial,value,sent=source_request(history,layout);old,following=ids(history);selected=value['child'];reused=layout=='empty-final-following'
    source_rows={r['id']:base64.b64decode(r['raw']) for r in initial['sections']}
    parent=authored(sent[0],source_rows[old],'',2000,True)
    child=authored(sent[1],source_rows.get(selected),'',2000 if reused else 1000,False)
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
    cp=checkpoint(initial,value,layout);empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='empty-pair-inert',checkpoint=cp))
    original=dict(**labels(history,layout),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    cancelled,saved=witness(10),witness(11);stages=[]
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
        geo=dict(ok=True,noOverflow=True);isheld=name in ('empty','saving','refused','uncertain')
        parts=['',''] if isheld else [''] if name in ('child-editing','parent-editing') else ['Continued paragraph'] if name=='continued-saving' else ['Original paragraph edited'] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='empty' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None,emptyProof=None)
            if isheld:native.update(blocks=owner['after'],rawBlocks=owner['rawAfter'],emptyProof=owner['emptyProof'])
            else:
                if parts==['']:native['blocks']=blocks(1)
                native['rawBlocks']=copy.deepcopy(native['blocks'])
            geo['native']=native
        else:geo.update(native=None,paragraphs=reader(source['visible']))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(history,layout),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
            fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,history='bootstrap',layout='no-following'):
    stages,terminal,_=R.packets(phase);source,value,_=source_request(history,layout);cp=checkpoint(source,value,layout);old,_=ids(history)
    pinned=labels(history,layout);pinned.pop('mode');pinned['splitMode']='empty'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']==old)
            row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],emptyLabels=[],initialTail=None,originalChild=None,insertion=None,startTyping=None)
        if p['panel']:
            p.update(split=[dict(kind=k,text='') for k in ('original','parent','child')],emptyLabels=[dict(kind=k,text='Empty paragraph') for k in ('original','parent')],
                     current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(**r,deleted=False) for r in reader(source)],missing=[] if layout=='empty-final-following' else ['child'])
            p['warning']=' '.join(['Reuse the existing empty paragraph after this paragraph' if layout=='empty-final-following' else 'Add an empty paragraph after this paragraph',
                    'Either part, both parts, or neither part may have reached Slack. Removing this lock leaves both outcomes unknown.',
                    'Matching text does not establish whether the saved change succeeded.',
                    'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                    'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
