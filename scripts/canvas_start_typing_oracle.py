"""Typed start: sealed original-second edits, whole-pair ACK and both later saves."""
import canvas_start_typing_wire as W
import canvas_start_oracle as A
import canvas_end_typing_oracle as T
import canvas_split_oracle as S
from canvas_middle_typing_oracle import ack
from canvas_heading import require, text

STAGES={'success':('typed-original','cancelled','saving','confirmed','new-editing','new-saving','new-confirmed','original-editing','original-saving','original-confirmed'),
        'stale':('typed-original','cancelled','refused','cancelled-stale'),
        'partial':('typed-original','cancelled','saving','uncertain')}


def snapshot(value,after):
    """Only the two measured empty sections may project a browser BR to empty."""
    require(S.plain_sections(value.get('after'))==['',after], 'Typed start changed new blank or original report')
    raw=value.get('rawAfter');canonical=value['after'];proof=value.get('placeholder')
    raw_parts=S.plain_sections(raw)
    require(len(raw_parts)==2 and raw_parts[0] in ('','\n') and raw_parts[1] in ((after,) if after else ('','\n')) and canonical[0]['elements'][0]['elements']==[]
            and raw[0]['elements'][0]['elements'] in ([],[dict(type='text',text='\n')]),'Typed-start leading raw projection changed')
    root=T.PlainDom(value.get('html')).root
    require(len(root)==1 and root[0][:2]==['div',[('class','rich-editor-block')]],'Typed-start rich DOM changed')
    sections=root[0][2]
    require(len(sections)==2 and all(isinstance(n,list) and n[:2]==['div',[('class','rich-editor-section')]] for n in sections),
            'Typed-start DOM section structure changed')
    require(sections[0][2]==[['span',[],[['br',[],[]]]]],'Typed-start leading blank DOM changed')
    def plain(nodes,depth=0):
        require(depth<=16,'Typed-start DOM nesting exceeds bound');result=''
        for node in nodes:
            if isinstance(node,str):result+=node
            else:
                require(node[:2]==['span',[]],'Typed-start original contains semantic formatting/break')
                result+=plain(node[2],depth+1)
        return result
    if after:
        require(raw[0]['elements'][1]==canonical[0]['elements'][1] and proof is None and plain(sections[1][2])==after,
                'Typed-start nonempty original projection differs')
    else:
        require(canonical[0]['elements'][1]['elements']==[] and raw[0]['elements'][1]['elements'] in ([],[dict(type='text',text='\n')]),
                'Typed-start erased original hid content')
        nodes=sections[1][2];depth=0
        while len(nodes)==1 and isinstance(nodes[0],list) and nodes[0][:2]==['span',[]]:nodes=nodes[0][2];depth+=1
        require(nodes==[['br',[],[]]] and proof==dict(blockCount=1,sectionCount=2,childHtml='<span>'*depth+'<br>'+'</span>'*depth,spanDepth=depth,caretOffset=0)
                and all(type(proof.get(k)) is int for k in ('blockCount','sectionCount','spanDepth','caretOffset')),
                'Typed-start erased original lacks exact empty proof')


def witness(value,e):
    require(isinstance(value,dict) and isinstance(value.get('typing'),dict),'Typed-start reports missing')
    A.witness({k:v for k,v in value.items() if k!='typing'},e)
    typed=value['typing'];events=typed.get('events');require(isinstance(events,list) and len(events)==len(e.updates),'Typed-start ordered event count changed')
    before=e.seed
    for index,(event,after) in enumerate(zip(events,e.updates)):
        if index==0:kind,data,start,stop='insertText','New ' if e.typing=='correction' else 'x',0,0;selection=(0,0)
        elif e.typing=='erase':kind,data,start,stop='deleteContentBackward',None,0,T.utf16(before);selection=(start,stop)
        elif e.typing=='restore':kind,data,start,stop='deleteContentBackward',None,0,1;selection=(1,1)
        elif index==1:
            kind,data,start,stop='insertText',('' if e.corpus=='ascii' else '\u65e5\u672c\U0001f600 ')+'draftx ',4,4;selection=(4,4)
        elif index==2:
            at=9 if e.corpus=='ascii' else 14;kind,data,start,stop='deleteContentBackward',None,at,at+1;selection=(stop,stop)
        else:
            at=4 if e.corpus=='ascii' else 9;kind,data,start,stop='insertText','paragraph',at,at+5;selection=(start,stop)
        fields=dict(inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=stop,beforeText=before,
                    afterText=after,beforeRevision=index+1,revision=index+2,caret=start+T.utf16(data or ''),beforeInputTrusted=True,inputTrusted=True)
        require(all(event.get(k)==v and type(event.get(k)) is type(v) for k,v in fields.items()),'Typed-start trusted original input/range/revision changed')
        require('leadingPlaceholder' not in event,'Typed-start mutable leading proof injected');snapshot(event,after);before=after
    require(type(typed.get('revision')) is int and typed['revision']==len(events)+1
            and all(typed.get(k)==events[-1].get(k) for k in ('after','rawAfter','html','placeholder'))
            and 'leadingPlaceholder' not in typed,'Typed-start final snapshot not final owned event')
    return typed


def validate(receipt,corpus='ascii',typing='correction',parent_mode='bootstrap'):
    e=W.config(corpus,typing,parent_mode);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Start scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading'))
            and original.get('operation') is None and original.get('reservations')==0 and original.get('capacityReason') is None,'Start original source not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Start original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and int(saved['generation'])>int(cancelled['generation'])
            and receipt.get('duplicate')=={'ok':True,'clicks':2},'Start reopen/duplicate proof changed')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES[scenario]),'Start stages incomplete')
    previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};opened={};new=sent=held_journal=None
    for index,(stage,name) in enumerate(zip(stages,STAGES[scenario])):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Start stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        counts=(0,0,0,0) if index<2 else ((1,0,0,0) if scenario=='stale' else (1,1,0,0) if index==2 else (1,1,1,0) if scenario=='partial'
                else (1,1,1,1) if index<5 else (2,2,1,1) if index==5 else (2,2,2,2) if index<8 else (3,3,2,2) if index==8 else (3,3,3,3))
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],
                    'Start immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int
                and source['reservations']==int(scenario!='stale' and index>=2),'Start ownership/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Start preflight not bound to original')
            W.inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Start preflight source drift')
        if source['writes'] and sent is None:
            new,parent,blank=W.write(source['writes'][0],originals['paragraph'],e);sent=[parent,blank];require(new not in originals,'Typed start reused an old ID')
        accepted=scenario=='success' and index>=3
        W.inventory(source.get('fresh'),e,initial,new if accepted else None,index>=6,index>=9,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,new if accepted else None,index>=6,index>=9)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),
                'Start unaccepted canonical changed')
        received=None
        if source['acknowledgements']:
            received=ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,originals, {'paragraph':e.reports[-1],new:''},{'paragraph':2000,new:1000},scenario=='partial')
        if accepted:
            reads=source['readbacks'];first=W.inventory(reads[0],e,initial,new)
            require(all(S.raw(first[text(row,1)])==S.raw(row) for row in received),'Start independent ACK/readback differs')
            require(source['fresh']==source['visible']==reads[-1],'Start current/readback custody lost')
            for write_index,start,identity,content in ((1,5,new,W.M.rtml(e.continued)),(2,8,'paragraph',W.M.rtml(e.edited))):
                if index<start:continue
                prior_value=reads[write_index-1];prior=W.inventory(prior_value,e,initial,new,write_index==2)
                pre=source['preflights'][write_index]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Start later Save preflight drift')
                row=W.continuation(source['writes'][write_index],prior[identity],content,write_index,source['writes'][1]['session'])
                if index>start:
                    confirmed=ack(source['acknowledgements'][write_index],[row],prior,{identity:content},{identity:3000})[0]
                    read=W.inventory(reads[write_index],e,initial,new,True,write_index==2)
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),
                            'Start later Save changed an untouched row')
        pending=name in ('saving','new-saving','original-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Start pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Start operation lock missing')
        else:require(source.get('operation') is None,'Unexpected start lock')
        W.journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':held_journal=stage['journal']
        if uncertain:require(stage['journal']==held_journal,'Start unknown outcome lost exact insertion receipt')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Start layout overflow')
        held=name in ('typed-original','saving','refused','uncertain')
        parts=['',e.updates[-1]] if held else [''] if name=='new-editing' else [e.continued] if name=='new-saving' else [e.updates[-1]] if name=='original-editing' else [e.edited] if name=='original-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='typed-original' else saved
                typed=owner['typing']
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('typing')==typed
                        and editor.get('blocks')==typed['after'] and editor.get('rawBlocks')==typed['rawAfter']
                        and editor.get('leadingPlaceholder')==owner['leadingPlaceholder'] and editor.get('placeholder')==typed['placeholder'],
                        'Typed-start editor lost sealed original reports')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('leadingPlaceholder') is None,'Start ordinary editor retained insertion projection')
                owner=editor.get('generation');target='new' if name.startswith('new-') else 'original'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Start later editor owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Start editor opening recycled owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Start pending Save replaced editor owner')
            if not held:require(editor.get('typing') is None and editor.get('placeholder') is None,'Ordinary start editor kept typed witness')
        else:
            expected=([dict(id=new,text=e.continued if index>=6 else '')] if accepted else [])
            expected.extend([dict(id='paragraph',text=e.edited if index>=9 else e.updates[-1] if accepted else e.seed),dict(id='neighbor',text='Another paragraph')])
            require(geo.get('native') is None and geo.get('paragraphs')==expected,'Start reader identity/content/order changed')
        if name=='cancelled':require(source==original,'Start Cancel dispatched or mutated source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Start terminal differs from last stage')
