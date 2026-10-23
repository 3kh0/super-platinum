"""Original-runtime packets bound to synthetic fixture owners; no native acceptance."""
import base64
import copy
import json
from pathlib import Path

import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_empty_enter_fixtures as E
import test_canvas_split_recovery_oracle as R

REF=Path(__file__).resolve().parent.parent/'tmp/canvas-repeat-enter-20261023/reference'
SESSION='0123456789ab'
HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following','empty-final-following','empty-nonfinal-following')
NAMES={'success':('repeat','cancelled','saving','confirmed','second-editing','second-saving','second-confirmed','child-editing',
                   'continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
       'stale':('repeat','cancelled','refused','cancelled-stale'),'partial':('repeat','cancelled','saving','uncertain')}


def ids(history):return E.ids(history)
def seed(corpus):return 'Keep this' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600'
def rtml(value):return value.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;')
def labels(history,layout,corpus):return dict(mode='repeat',repeatHistory=history,repeatLayout=layout,typing='none',corpus=corpus,parentMode='bootstrap',
                                           reuseBase='positive',reuseContent='empty',reuseChild=ids(history)[1] if layout=='empty-final-following' else None)


def source_request(history='bootstrap',layout='no-following',corpus='ascii'):
    vectors=json.loads((REF/'repeat-enter-result.json').read_text(encoding='utf-8'))['vectors']
    index=next(i for i,v in enumerate(vectors) if (v['mode'],v['layout'],v['corpus'])==(history,layout,corpus))
    vector=vectors[index];assert vector['visible']==seed(corpus) and vector['sourceText']==rtml(seed(corpus))
    bindings=json.loads((REF/'initialization-bindings.json').read_text(encoding='utf-8'))['vectors'][index]['sourceEntities']
    original,following=ids(history);reused=layout=='empty-final-following'
    mapping={'paragraph':original,'following':following,vector['secondId']:'temp:C:xyz'+'2'*25}
    if not reused:mapping[vector['firstId']]='temp:C:xyz'+'1'*25
    def source(identity,position,content,language,session=None):
        row=F.f(1,identity)+F.f(2,1000)+F.f(6,'thread')+F.f(7,'document')+F.f(8,position)+F.f(12,F.body(content,language))
        if session:row+=F.f(38,F.replace(F.replace(base64.b64decode(session),1,identity),4,8_000_000_000_000_000))
        return row
    parent_session=next(s['canonicalField38'] for s in bindings if s['id']=='paragraph')
    empty_session=next((s['canonicalField38'] for s in bindings if s['id']=='following'),None)
    rows=[F.source_rows()[0],source(original,'aac',rtml(seed(corpus)),True,parent_session)]
    if layout!='no-following':rows.append(source(following,'aae','Following paragraph' if layout=='text-following' else '',False,empty_session))
    if layout=='empty-nonfinal-following':rows.append(source('last-text','aag','Last paragraph',False))
    sent=list(map(S.raw,S.messages(S.decoded(vector['wireBase64']),1)))
    first,second=mapping[vector['firstId']],mapping[vector['secondId']]
    positions=('aae','aaf:temp') if reused else ('aad:temp','aae:temp' if layout=='no-following' else 'aadaaaa:temp')
    for i,identity in enumerate((original,first,second)):
        session=F.replace(S.field(S.wire(sent[i]),38,2),1,identity)
        sent[i]=F.replace(F.replace(F.replace(sent[i],1,identity),38,session),33,SESSION+'-0')
        if i and (i==2 or not reused):sent[i]=F.replace(F.replace(sent[i],8,positions[i-1]),21,positions[i-1])
    value=dict(kind='repeat',parent=original,child=first,secondChild=second,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,clearsChild=False,
               documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in sent)))
    return F.inventory(rows),value,sent


def checkpoint(source,value,layout,corpus):
    sent=S.messages(S.decoded(value['documentData']),1);reused=layout=='empty-final-following'
    return dict(version=12,scope=S.SCOPE.copy(),thread='thread',document='document',target=value['parent'],session=SESSION,sequence=1,draft=rtml(seed(corpus)),
                action={'EndRepeat':dict(child=value['child'],position='aae' if reused else S.text(sent[1],8),reused=reused,
                                        second_child=value['secondChild'],second_position=S.text(sent[2],8))},
                sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(value['documentData'])))


def authored(outgoing,source,content,sequence,original):
    row=E.authored(outgoing,source,content,sequence,original)
    if not source:
        position=S.text(S.wire(outgoing),8).removesuffix(':temp');row=F.replace(F.replace(row,8,position),21,position)
    return row


def blocks(parts):
    value=F.blocks(*parts)
    for section,part in zip(value[0]['elements'],parts):
        if part=='':section['elements']=[]
    return value


def witness(generation,corpus):
    value=seed(corpus);before=blocks([value]);after=blocks([value,'']);raw=blocks([value,'\n'])
    return dict(kind='end',offset=9 if corpus=='ascii' else 11,before=before,after=after,rawAfter=raw,revision=1,beforeInputTrusted=True,
                inputTrusted=True,inputType='insertParagraph',caretChildZero=True,generation=str(generation),visit='0',
                placeholder=dict(blockCount=1,sectionCount=2,childHtml='<span><br></span>',spanDepth=1,caretOffset=0),
                repeat=dict(revision=2,after=blocks([value,'','']),rawAfter=blocks([value,'\n','\n']),
                    html='<div class="rich-editor-block"><div class="rich-editor-section">'+rtml(value)+'</div><div class="rich-editor-section"><span><br></span></div><div class="rich-editor-section"><span><br></span></div></div>',
                    beforeInputTrusted=True,inputTrusted=True,inputType='insertParagraph',caretSection=2,caretOffset=0,
                    placeholders=[dict(html='<span><br></span>',spanDepth=1),dict(html='<span><br></span>',spanDepth=1)]))


def receipt(scenario='success',history='bootstrap',layout='no-following',corpus='ascii'):
    initial,value,sent=source_request(history,layout,corpus);old,following=ids(history);one,two=value['child'],value['secondChild'];reused=layout=='empty-final-following'
    sources={r['id']:base64.b64decode(r['raw']) for r in initial['sections']}
    accepted={identity:authored(row,sources.get(identity),rtml(seed(corpus)) if identity==old else '',2000 if identity==old or reused and identity==one else 1000,identity==old)
              for identity,row in zip((old,one,two),sent)}
    def readback():return F.inventory(sorted([accepted.get(k,v) for k,v in sources.items()]+[v for k,v in accepted.items() if k not in sources],key=lambda raw:S.text(S.wire(raw),8)))
    reads=[readback()];acks=[F.acknowledgement(list(accepted.values()))];writes=[value]
    for sequence,identity,content in ((1,two,'Second paragraph edited'),(2,one,'Continued paragraph'),(3,old,seed(corpus)+' edited')):
        v,row=E.M.continued(accepted[identity],content,'c'*12,sequence)
        row=F.replace(row,12,F.body(rtml(content),identity==old));v['documentData']=F.b64(F.f(14,F.TITLE)+F.f(1,row));writes.append(v)
        accepted[identity]=authored(row,accepted[identity],rtml(content),3000,identity==old);reads.append(readback());acks.append(F.acknowledgement([accepted[identity]],continued=True))
    cp=checkpoint(initial,value,layout,corpus);empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='repeat-triple-inert',checkpoint=cp))
    original=dict(**labels(history,layout,corpus),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    cancelled,saved=witness(10,corpus),witness(11,corpus);stages=[]
    for i,name in enumerate(NAMES[scenario]):
        source=copy.deepcopy(original);pending=name in ('saving','second-saving','continued-saving','parent-saving');uncertain=name=='uncertain';complete=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':
                changed=next(r for r in fresh['sections'] if r['id']==(following if reused else old));changed.update(sequence=3000,raw=F.b64(F.replace(base64.b64decode(changed['raw']),2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[value],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[F.acknowledgement([base64.b64decode(reads[0]['sections'][1]['raw'])],partial=True) if scenario=='partial' else acks[0]]
        if complete:
            source.update(fresh=reads[0],visible=reads[0],readbacks=[reads[0]])
            for j,start in enumerate((5,8,11),1):
                if i>=start:
                    source['writes'].append(writes[j]);source['preflights'].append(dict(source=reads[j-1],fresh=reads[j-1],outcome='prepared'))
                    if i>start:source['acknowledgements'].append(acks[j]);source['readbacks'].append(reads[j]);source.update(fresh=reads[j],visible=reads[j])
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True);isheld=name in ('repeat','saving','refused','uncertain')
        parts=[seed(corpus),'',''] if isheld else [''] if name in ('second-editing','child-editing') else [seed(corpus)] if name=='parent-editing' else ['Second paragraph edited'] if name=='second-saving' else ['Continued paragraph'] if name=='continued-saving' else [seed(corpus)+' edited'] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='repeat' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107 if i in (7,8) else 110,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None,emptyProof=None,repeatProof=None)
            if isheld:native.update(blocks=owner['repeat']['after'],rawBlocks=owner['repeat']['rawAfter'],repeatProof=owner['repeat'],placeholder=owner['placeholder'])
            else:native.update(blocks=blocks(parts),rawBlocks=blocks(parts))
            geo['native']=native
        else:geo.update(native=None,paragraphs=E.reader(source['visible']))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(history,layout,corpus),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
                original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
                fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,history='bootstrap',layout='no-following',corpus='ascii'):
    stages,terminal,_=R.packets(phase);source,value,_=source_request(history,layout,corpus);cp=checkpoint(source,value,layout,corpus);old,_=ids(history)
    pinned=labels(history,layout,corpus);pinned.pop('mode');pinned['splitMode']='repeat'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']==old);row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],emptyLabels=[],initialTail=None,originalChild=None,insertion=None,startTyping=None)
        if p['panel']:
            p.update(split=[dict(kind='original',text=seed(corpus)),dict(kind='parent',text=seed(corpus)),dict(kind='child',text=''),dict(kind='secondChild',text='')],
                     current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(**r,deleted=False) for r in E.reader(source)],missing=['secondChild'] if layout=='empty-final-following' else ['child','secondChild'])
            p['warning']=' '.join(['Reuse the following empty paragraph and add another after it' if layout=='empty-final-following' else 'Add two empty paragraphs after this paragraph',
                'Any, all, or none of these three paragraph changes may have reached Slack. Removing this lock leaves all three outcomes unknown.',
                'Matching text does not establish whether the saved change succeeded.',
                'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
