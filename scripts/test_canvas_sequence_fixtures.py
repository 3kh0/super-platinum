"""Synthetic protocol fixtures from frozen short/long vectors; not native proof."""
import base64
import copy
import json
import html
from pathlib import Path
import test_canvas_repeat_enter_fixtures as A
import test_canvas_split_oracle as F
import test_canvas_empty_enter_fixtures as E
import test_canvas_split_recovery_oracle as R
import canvas_split_oracle as S

REF=Path(__file__).resolve().parent.parent/'tmp/canvas-paragraph-sequence-20261023/reference'
SESSION='0123456789ab'
SUMMARIES=json.loads((REF/'vector-summary.json').read_text(encoding='utf-8'))
BINDINGS=json.loads((REF/'vector-bindings.json').read_text(encoding='utf-8'))['vectors']
PACKETS=json.loads((REF/'collector-packets.json').read_text(encoding='utf-8'))['vectors']
LONG_REF=REF.parent.parent/'canvas-long-sequence-20261023/reference'
SHORT_COUNT=len(SUMMARIES)
SUMMARIES+=json.loads((LONG_REF/'vector-summary.json').read_text(encoding='utf-8'))
BINDINGS+=json.loads((LONG_REF/'vector-bindings.json').read_text(encoding='utf-8'))['vectors']
PACKETS+=json.loads((LONG_REF/'collector-packets.json').read_text(encoding='utf-8'))['vectors']


def source_request(vector=0):
    v=SUMMARIES[vector];binding=BINDINGS[vector];old,following=E.ids(v['mode']);reused=v['layout']=='empty-final-following'
    visible=A.seed(v['corpus']);assert v['sourceText']==A.rtml(visible)
    selected=[following if reused and i==0 else 'temp:C:xyz'+(str(i+1)*25 if i<9 else f'{i+1:025x}') for i in range(v['appendCount'])]
    def source(identity,position,content,language,session=None):
        row=F.f(1,identity)+F.f(2,1000)+F.f(6,'thread')+F.f(7,'document')+F.f(8,position)+F.f(12,F.body(content,language))
        if session:row+=F.f(38,F.replace(F.replace(base64.b64decode(session),1,identity),4,8_000_000_000_000_000))
        return row
    bound={s['id']:s['canonicalField38'] for s in binding['sourceEntities']}
    originals=[F.source_rows()[0],source(old,'aac',A.rtml(visible),True,bound['paragraph'])]
    if v['layout']!='no-following':originals.append(source(following,'aae','Following paragraph' if v['layout']=='text-following' else '',False,bound.get('following') if reused else None))
    if v['layout']=='empty-nonfinal-following':originals.append(source('last-text','aag','Last paragraph',False))
    sent=list(map(S.raw,S.messages(S.decoded(PACKETS[vector]['wireBase64']),1)));members=[]
    for i,identity in enumerate([old]+selected):
        session=F.replace(S.field(S.wire(sent[i]),38,2),1,identity)
        sent[i]=F.replace(F.replace(F.replace(sent[i],1,identity),38,session),33,SESSION+'-0')
        if i:
            position=(('aae','aaf:temp','aah:temp')[i-1] if i<=3 else 'aah'+'a'*(i-3)+':temp') if reused else ('aad'+('a'*(i-1))+':temp')
            if not (reused and i==1):sent[i]=F.replace(F.replace(sent[i],8,position),21,position)
            updates=[t['rtml'] for t in v['transitions'] if t['kind']=='report' and t['appendIndex']==i-1]
            members.append(dict(id=identity,position=position,reused=reused and i==1,updates=updates,finalRtml=updates[-1] if updates else ''))
    value=dict(kind='sequence',parent=old,members=members,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,clearsChild=False,
               documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in sent)))
    return F.inventory(originals),value,sent


def checkpoint(source,value):
    return dict(version=14,scope=S.SCOPE.copy(),thread='thread',document='document',target=value['parent'],session=SESSION,sequence=1,
        draft=S.body(S.decoded(next(r['raw'] for r in source['sections'] if r['id']==value['parent'])),True),
        action={'EndSequence':{'members':[{k:m[k] for k in ('id','position','reused','updates')} for m in value['members']]}},
        sections=[list(base64.b64decode(r['raw'])) for r in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(value['documentData'])))


def kwargs(vector):
    v=SUMMARIES[vector]
    return dict(append_count=v['appendCount'],history=v['mode'],layout=v['layout'],cadence=v['cadence'],corpus=v['corpus'])


authored=A.authored
blocks=A.blocks


def labels(vector):
    v=SUMMARIES[vector]
    return dict(mode='sequence',sequenceCount=v['appendCount'],sequenceHistory=v['mode'],sequenceLayout=v['layout'],sequenceCadence=v['cadence'],
                corpus=v['corpus'],typing='none',parentMode='bootstrap',reuseBase='positive',reuseContent='empty',
                reuseChild=E.ids(v['mode'])[1] if v['layout']=='empty-final-following' else None)


def witness(vector,generation):
    # This fixture ledger is derived independently from the original runtime
    # transition list; protocol DOM is synthetic, never claimed browser evidence.
    v=SUMMARIES[vector];seed=A.seed(v['corpus']);parts=[seed];events=[];revision=0
    utf16=lambda s:len(s.encode('utf-16-le'))//2
    for t in v['transitions']:
        index=t['appendIndex']+1;before=parts[-1]
        if t['kind']=='enter':
            kind,data,start,stop='insertParagraph',None,utf16(before),utf16(before);selection=(start,stop);after='';parts.append('');caret=0
        else:
            after=html.unescape(t['rtml'])
            if not before:kind,data,start,stop,selection='insertText',after,0,0,(0,0)
            elif after.startswith(before):kind,data,start,stop='insertText',after[len(before):],utf16(before),utf16(before);selection=(start,stop)
            elif not after:kind,data,start,stop='deleteContentBackward',None,0,utf16(before);selection=(start,stop)
            elif before==after+'x':kind,data,start,stop='deleteContentBackward',None,utf16(after),utf16(before);selection=(stop,stop)
            elif before==after+' draftx':kind,data,start,stop='deleteContentBackward',None,utf16(after),utf16(before);selection=(start,stop)
            else:
                assert before.endswith(' draft') and after.endswith(' paragraph')
                start=utf16(before)-5;stop=utf16(before);kind,data,selection='insertText','paragraph',(start,stop)
            caret=start+utf16(data or '');parts[-1]=after
        revision+=1;sections=[dict(html=html.escape(p,quote=False) if p else '<br>',text=p,placeholder=None if p else dict(html='<br>',spanDepth=0)) for p in parts]
        event=dict(kind='enter' if t['kind']=='enter' else 'edit',activeIndex=index,beforeRevision=revision-1,revision=revision,beforeText=before,afterText=after,
                   inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=stop,caret=caret,beforeInputTrusted=True,inputTrusted=True,
                   after=blocks(parts),rawAfter=blocks(parts),sections=sections,
                   html='<div class="rich-editor-block">'+''.join('<div class="rich-editor-section">'+s['html']+'</div>' for s in sections)+'</div>')
        events.append(event)
    return dict(kind='sequence',generation=str(generation),visit='0',before=blocks([seed]),offset=utf16(seed),after=events[0]['after'],rawAfter=events[0]['rawAfter'],
                revision=1,beforeInputTrusted=True,inputTrusted=True,inputType='insertParagraph',
                sequence=dict(revision=revision,events=events,**{k:events[-1][k] for k in ('after','rawAfter','html')}))


def receipt(vector=0,scenario='success'):
    v=SUMMARIES[vector];history,layout,corpus=v['mode'],v['layout'],v['corpus'];count=v['appendCount']
    initial,value,sent=source_request(vector);old,following=E.ids(history);selected=[m['id'] for m in value['members']];reused=layout=='empty-final-following';final=[m['finalRtml'] for m in value['members']];visible=[html.unescape(t) for t in final];seed=A.seed(corpus)
    later=[(identity,f'Appended paragraph {i+1} edited') for i,identity in reversed(list(enumerate(selected)))]+[(old,seed+' edited')]
    starts=tuple(5+3*i for i in range(count+1));names=['sequence','cancelled','saving','confirmed']
    for i in reversed(range(count)):names.extend(f'member-{i}-{s}' for s in ('editing','saving','confirmed'))
    names.extend('parent-'+s for s in ('editing','saving','confirmed'))
    names=names if scenario=='success' else ['sequence','cancelled','refused','cancelled-stale'] if scenario=='stale' else ['sequence','cancelled','saving','uncertain']
    sources={r['id']:base64.b64decode(r['raw']) for r in initial['sections']}
    accepted={identity:authored(row,sources.get(identity),A.rtml(seed) if identity==old else final[selected.index(identity)],2000 if identity==old or reused and identity==selected[0] else 1000,identity==old)
              for identity,row in zip([old]+selected,sent)}
    def readback():return F.inventory(sorted([accepted.get(k,v) for k,v in sources.items()]+[v for k,v in accepted.items() if k not in sources],key=lambda raw:S.text(S.wire(raw),8)))
    reads=[readback()];acks=[F.acknowledgement(list(accepted.values()))];writes=[value]
    for sequence,(identity,content) in enumerate(later,1):
        v,row=E.M.continued(accepted[identity],content,'c'*12,sequence)
        row=F.replace(row,12,F.body(A.rtml(content),identity==old));v['documentData']=F.b64(F.f(14,F.TITLE)+F.f(1,row));writes.append(v)
        accepted[identity]=authored(row,accepted[identity],A.rtml(content),3000,identity==old);reads.append(readback());acks.append(F.acknowledgement([accepted[identity]],continued=True))
    cp=checkpoint(initial,value);empty=dict(scope=S.SCOPE.copy(),receipt=None);held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='repeat-triple-inert',checkpoint=cp))
    original=dict(**labels(vector),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    cancelled,saved=witness(vector,10),witness(vector,11);stages=[]
    for i,name in enumerate(names):
        source=copy.deepcopy(original);pending=name=='saving' or name.endswith('-saving');uncertain=name=='uncertain';complete=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':
                changed=next(r for r in fresh['sections'] if r['id']==(following if reused else old));changed.update(sequence=3000,raw=F.b64(F.replace(base64.b64decode(changed['raw']),2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[value],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[F.acknowledgement([base64.b64decode(reads[0]['sections'][1]['raw'])],partial=True) if scenario=='partial' else acks[0]]
        if complete:
            source.update(fresh=reads[0],visible=reads[0],readbacks=[reads[0]])
            for j,start in enumerate(starts,1):
                if i>=start:
                    source['writes'].append(writes[j]);source['preflights'].append(dict(source=reads[j-1],fresh=reads[j-1],outcome='prepared'))
                    if i>start:source['acknowledgements'].append(acks[j]);source['readbacks'].append(reads[j]);source.update(fresh=reads[j],visible=reads[j])
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True);isheld=name in ('sequence','saving','refused','uncertain')
        parts=[seed,*visible] if isheld else None
        if name.endswith('-editing') or name.endswith('-saving'):
            identity,content=later[(i-4)//3]
            parts=[content] if name.endswith('-saving') else [seed if identity==old else visible[selected.index(identity)]]
        if parts is not None:
            owner=cancelled if name=='sequence' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104+3*((i-4)//3),not(pending or uncertain),S.ERROR if name=='refused' else None)
            native.update(typing=None,placeholder=None,leadingPlaceholder=None,emptyProof=None,repeatProof=None,sequenceProof=None)
            if isheld:native.update(blocks=owner['sequence']['after'],rawBlocks=owner['sequence']['rawAfter'],sequenceProof=owner['sequence'])
            else:native.update(blocks=blocks(parts),rawBlocks=blocks(parts))
            geo['native']=native
        else:geo.update(native=None,paragraphs=E.reader(source['visible']))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(**labels(vector),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,
                original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,
                fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))



def recovery(phase,vector=0):
    v=SUMMARIES[vector];history,layout,corpus=v['mode'],v['layout'],v['corpus']
    stages,terminal,_=R.packets(phase);source,value,_=source_request(vector);cp=checkpoint(source,value);old,_=E.ids(history)
    pinned=labels(vector);pinned.pop('mode');pinned['splitMode']='sequence'
    for stage in stages:
        stage.update(**pinned,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=next(r for r in stage['canonical']['sections'] if r['id']==old);row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(split=[],current=[],missing=[],emptyLabels=[],initialTail=None,originalChild=None,insertion=None,startTyping=None,sequence=None)
        if p['panel']:
            members=[dict(id=m['id'],index=i,text=html.unescape(m['finalRtml']),reused=m['reused'],sourceText='' if m['reused'] else None,
                          reportCount=len(m['updates']),emptyLabel=None if m['finalRtml'] else 'Empty paragraph') for i,m in enumerate(value['members'])]
            p.update(sequence=dict(originalText=A.seed(corpus),parentText=A.seed(corpus),members=members),
                     current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(**r,deleted=False) for r in E.reader(source)],
                     missing=[m['id'] for m in value['members'] if not m['reused']])
            p['warning']=' '.join(['Reuse the following empty paragraph and continue adding paragraphs' if layout=='empty-final-following' else 'Add and edit paragraphs after this paragraph',
                'Any, all, or none of these paragraph changes may have reached Slack. Removing this lock leaves every outcome unknown.',
                'Edits remain part of this unresolved change, including paragraphs that became empty again.',
                'Matching text does not establish whether the saved change succeeded.',
                'Removing this local edit lock does not confirm success, undo a change, or send it again.',
                'The saved record will remain on this computer with an unknown outcome.'])
    terminal.update(**pinned,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp
