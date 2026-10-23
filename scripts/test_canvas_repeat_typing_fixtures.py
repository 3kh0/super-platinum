"""Original-runtime packets bound to synthetic fixture owners; no native acceptance."""
import base64
import html
import test_canvas_repeat_enter_fixtures as A
import test_canvas_empty_typing_fixtures as H
import copy
import json
from pathlib import Path

import canvas_split_oracle as S
import test_canvas_split_oracle as F
import test_canvas_empty_enter_fixtures as E
import test_canvas_split_recovery_oracle as R

REF=Path(__file__).resolve().parent.parent/'tmp/canvas-repeat-typing-20261023/reference'
SESSION='0123456789ab'
HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following','empty-final-following','empty-nonfinal-following')
NAMES={'success':('typed-third','cancelled','saving','confirmed','second-editing','second-saving','second-confirmed','child-editing',
                   'continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
       'stale':('typed-third','cancelled','refused','cancelled-stale'),'partial':('typed-third','cancelled','saving','uncertain')}


def ids(history):return E.ids(history)
def seed(corpus):return 'Keep this' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600'
def rtml(value):return value.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;')
def labels(history,layout,corpus,typing):return dict(mode='repeat',repeatHistory=history,repeatLayout=layout,typing=typing,corpus=corpus,parentMode='bootstrap',
                                           reuseBase='positive',reuseContent='empty',reuseChild=ids(history)[1] if layout=='empty-final-following' else None)


def reports(corpus,typing):return H.reports(corpus,typing)


def source_request(history='bootstrap',layout='no-following',corpus='ascii',typing='correction'):
    initial,value,prior=A.source_request(history,layout,corpus)
    vectors=json.loads((REF/'repeat-typing-result.json').read_text(encoding='utf-8'))['vectors']
    branch='empty-final-following' if layout=='empty-final-following' else 'no-following'
    cadence='exact-revert' if typing=='restore' else typing
    vector=next(v for v in vectors if (v['mode'],v['layout'],v['corpus'],v['cadence'])==(history,branch,corpus,cadence))
    visible,rtml=reports(corpus,typing)
    assert vector['states']==list(rtml) and vector['visible']==seed(corpus) and vector['sourceText']==A.rtml(seed(corpus))
    sent=list(map(S.raw,S.messages(S.decoded(vector['wireBase64']),1)))
    for i,identity in enumerate((value['parent'],value['child'],value['secondChild'])):
        session=F.replace(S.field(S.wire(sent[i]),38,2),1,identity)
        sent[i]=F.replace(F.replace(F.replace(sent[i],1,identity),38,session),33,SESSION+'-0')
        if i and (i==2 or layout!='empty-final-following'):
            position=S.text(S.wire(prior[i]),8);sent[i]=F.replace(F.replace(sent[i],8,position),21,position)
        if i<2:assert sent[i]==prior[i], 'Typing changed an original first-two packet'
    value['documentData']=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in sent))
    return initial,value,sent


def checkpoint(source,value,layout,corpus,typing):
    c=A.checkpoint(source,value,layout,corpus)
    return dict(c,version=13,action={'EndRepeatTyping':dict(c['action']['EndRepeat'],second_child_updates=list(reports(corpus,typing)[1]))})


authored=A.authored
blocks=A.blocks


def witness(generation,corpus,typing):
    value=A.witness(generation,corpus);events=H.witness(generation,typing,corpus)['typing']['events']
    for event in events:
        after=event['afterText'];event['beforeRevision']+=1;event['revision']+=1
        event['after']=blocks([seed(corpus),'',after]);event['rawAfter']=blocks([seed(corpus),'\n',after])
        if not after:event['placeholder']['sectionCount']=3
        third='<span>'+html.escape(after,quote=False)+'</span>' if after else '<br>'
        event['html']='<div class="rich-editor-block"><div class="rich-editor-section">'+rtml(seed(corpus))+'</div><div class="rich-editor-section"><span><br></span></div><div class="rich-editor-section">'+third+'</div></div>'
    value['typing']=dict(revision=len(events)+2,events=events,**{k:copy.deepcopy(events[-1][k]) for k in ('after','rawAfter','html','placeholder')})
    return value


def receipt(scenario='success',history='bootstrap',layout='no-following',corpus='ascii',typing='correction'):
    initial,value,sent=source_request(history,layout,corpus,typing);old,following=ids(history);one,two=value['child'],value['secondChild'];reused=layout=='empty-final-following'
    sources={r['id']:base64.b64decode(r['raw']) for r in initial['sections']}
    accepted={identity:authored(row,sources.get(identity),rtml(seed(corpus)) if identity==old else reports(corpus,typing)[1][-1] if identity==two else '',2000 if identity==old or reused and identity==one else 1000,identity==old)
              for identity,row in zip((old,one,two),sent)}
    def readback():return F.inventory(sorted([accepted.get(k,v) for k,v in sources.items()]+[v for k,v in accepted.items() if k not in sources],key=lambda raw:S.text(S.wire(raw),8)))
    reads=[readback()];acks=[F.acknowledgement(list(accepted.values()))];writes=[value]
    for sequence,identity,content in ((1,two,'Second paragraph edited'),(2,one,'Continued paragraph'),(3,old,seed(corpus)+' edited')):
        v,row=E.M.continued(accepted[identity],content,'c'*12,sequence)
        row=F.replace(row,12,F.body(rtml(content),identity==old));v['documentData']=F.b64(F.f(14,F.TITLE)+F.f(1,row));writes.append(v)
        accepted[identity]=authored(row,accepted[identity],rtml(content),3000,identity==old);reads.append(readback());acks.append(F.acknowledgement([accepted[identity]],continued=True))
    cp=checkpoint(initial,value,layout,corpus,typing);empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='repeat-triple-inert',checkpoint=cp))
    original=dict(**labels(history,layout,corpus,typing),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    cancelled,saved=witness(10,corpus,typing),witness(11,corpus,typing);stages=[]
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
        geo=dict(ok=True,noOverflow=True);isheld=name in ('typed-third','saving','refused','uncertain')
        parts=[seed(corpus),'',reports(corpus,typing)[0][-1]] if isheld else [reports(corpus,typing)[0][-1]] if name=='second-editing' else [''] if name=='child-editing' else [seed(corpus)] if name=='parent-editing' else ['Second paragraph edited'] if name=='second-saving' else ['Continued paragraph'] if name=='continued-saving' else [seed(corpus)+' edited'] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='typed-third' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107 if i in (7,8) else 110,not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None,emptyProof=None,repeatProof=None)
            if isheld:native.update(blocks=owner['typing']['after'],rawBlocks=owner['typing']['rawAfter'],typing=owner['typing'],repeatProof=owner['repeat'],placeholder=owner['typing']['placeholder'])
            else:native.update(blocks=blocks(parts),rawBlocks=blocks(parts))
            geo['native']=native
        else:geo.update(native=None,paragraphs=E.reader(source['visible']))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(history,layout,corpus,typing),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
                original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
                fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


def recovery(phase,history='bootstrap',layout='no-following',corpus='ascii',typing='correction'):
    stages,terminal,_=R.packets(phase);source,value,_=source_request(history,layout,corpus,typing);cp=checkpoint(source,value,layout,corpus,typing);old,_=ids(history)
    pinned=labels(history,layout,corpus,typing);pinned.pop('mode');pinned['splitMode']='repeat'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']==old);row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],emptyLabels=[],initialTail=None,originalChild=None,insertion=None,startTyping=None)
        if p['panel']:
            p.update(split=[dict(kind='original',text=seed(corpus)),dict(kind='parent',text=seed(corpus)),dict(kind='child',text=''),dict(kind='secondChild',text=reports(corpus,typing)[0][-1])],
                     current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(**r,deleted=False) for r in E.reader(source)],missing=['secondChild'] if layout=='empty-final-following' else ['child','secondChild'])
            p['warning']=' '.join(['Reuse the following empty paragraph and add another after it' if layout=='empty-final-following' else 'Add two empty paragraphs after this paragraph',
                'Any, all, or none of these three paragraph changes may have reached Slack. Removing this lock leaves all three outcomes unknown.',
                'The third paragraph was edited after the second Enter. Those edits remain part of this unresolved change, even if the paragraph is empty again.',
                'Matching text does not establish whether the saved change succeeded.',
                'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
