"""Typed canonical-empty Enter, ordered trusted reports and complete acceptance."""
import canvas_empty_typing_wire as W
import canvas_empty_enter_oracle as A
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_heading import require, text

STAGES={'success':('typed-child','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
        'stale':('typed-child','cancelled','refused','cancelled-stale'),'partial':('typed-child','cancelled','saving','uncertain')}


def snapshot(value,after,proof):
    """Exact unchanged first empty DOM and plain or measured-empty second DOM."""
    require(S.plain_sections(value.get('after'))==['',after], 'Typed-empty report changed original or selected text')
    raw=value.get('rawAfter');canonical=value['after'];placeholder=value.get('placeholder')
    raw_parts=S.plain_sections(raw);first=proof['sections'][0]
    require(len(raw_parts)==2 and raw_parts[0]==('\n' if first['spanDepth'] else '')
            and raw_parts[1] in ((after,) if after else ('','\n')) and canonical[0]['elements'][0]['elements']==[]
            and raw[0]['elements'][0]['elements']==([dict(type='text',text='\n')] if first['spanDepth'] else []),
            'Typed-empty first raw projection differs from sealed original DOM')
    root=T.PlainDom(value.get('html')).root
    require(len(root)==1 and root[0][:2]==['div',[('class','rich-editor-block')]],'Typed-empty rich DOM changed')
    sections=root[0][2]
    require(len(sections)==2 and all(isinstance(n,list) and n[:2]==['div',[('class','rich-editor-section')]] for n in sections),
            'Typed-empty DOM sections changed')
    require(sections[0][2]==T.PlainDom(first['html']).root,'Typed-empty original blank DOM changed')
    def plain(nodes,depth=0):
        require(depth<=16,'Typed-empty nested DOM exceeds bound');result=''
        for node in nodes:
            if isinstance(node,str):result+=node
            else:
                require(node[:2]==['span',[]],'Typed-empty selected contains semantic formatting or break')
                result+=plain(node[2],depth+1)
        return result
    if after:
        require(raw[0]['elements'][1]==canonical[0]['elements'][1] and placeholder is None and plain(sections[1][2])==after,
                'Typed-empty nonempty selected projection differs')
    else:
        require(canonical[0]['elements'][1]['elements']==[],'Typed-empty erased selected has nonempty canonical runs')
        nodes=sections[1][2];depth=0
        while len(nodes)==1 and isinstance(nodes[0],list) and nodes[0][:2]==['span',[]]:nodes=nodes[0][2];depth+=1
        require(depth<=16 and nodes==[['br',[],[]]] and placeholder==dict(blockCount=1,sectionCount=2,
                childHtml='<span>'*depth+'<br>'+'</span>'*depth,spanDepth=depth,caretOffset=0)
                and all(type(placeholder.get(k)) is int for k in ('blockCount','sectionCount','spanDepth','caretOffset'))
                and raw[0]['elements'][1]['elements']==([dict(type='text',text='\n')] if depth else []),
                'Typed-empty erased selected lacks exact bounded DOM/raw proof')


def witness(value,e):
    require(isinstance(value,dict) and isinstance(value.get('typing'),dict),'Typed-empty ordered input ledger missing')
    A.witness({k:v for k,v in value.items() if k!='typing'})
    typed=value['typing'];events=typed.get('events')
    require(isinstance(events,list) and len(events)==len(e.updates),'Typed-empty event count changed')
    before='';n=3 if e.corpus=='ascii' else 10
    for index,(event,after) in enumerate(zip(events,e.updates)):
        if index==0:kind,data,start,stop='insertText',e.updates[0],0,0;selection=(0,0)
        elif e.typing=='erase' or e.typing=='restore' and index==3:
            kind,data,start,stop='deleteContentBackward',None,0,T.utf16(before);selection=(start,stop)
        elif index==1:kind,data,start,stop='insertText',' draftx',n,n;selection=(n,n)
        elif e.typing=='restore':kind,data,start,stop='deleteContentBackward',None,n,n+7;selection=(start,stop)
        elif index==2:kind,data,start,stop='deleteContentBackward',None,n+6,n+7;selection=(stop,stop)
        else:kind,data,start,stop='insertText','paragraph',n+1,n+6;selection=(start,stop)
        fields=dict(inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=stop,
                    beforeText=before,afterText=after,beforeRevision=index+1,revision=index+2,caret=start+T.utf16(data or ''),
                    beforeInputTrusted=True,inputTrusted=True)
        require(all(event.get(k)==v and type(event.get(k)) is type(v) for k,v in fields.items()),
                'Typed-empty trusted input/range/revision changed')
        require(not any(k in event for k in ('emptyProof','leadingPlaceholder')),'Typed-empty mutable original proof injected')
        snapshot(event,after,value['emptyProof']);before=after
    require(type(typed.get('revision')) is int and typed['revision']==len(events)+1
            and all(typed.get(k)==events[-1].get(k) for k in ('after','rawAfter','html','placeholder'))
            and not any(k in typed for k in ('emptyProof','leadingPlaceholder')),'Typed-empty final snapshot is not last owned event')
    return typed


def reader(e,selected=None,selected_saved=False,original_saved=False):
    ids=e.ids[1:]
    if selected and not e.reused:ids.insert(1,selected)
    return [dict(id=k,text=e.edited if k==e.original and original_saved else e.continued if k==selected and selected_saved
                 else e.updates[-1] if k==selected else e.neighbor if k==e.following else 'Last paragraph' if k=='last-text' else '') for k in ids]


def validate(receipt,typing='correction',empty_history='bootstrap',empty_layout='no-following',corpus='ascii'):
    e=W.config(typing,empty_history,empty_layout,corpus);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Empty scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading'))
            and original.get('operation') is None and original.get('reservations')==0 and original.get('capacityReason') is None,'Empty original not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Empty original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and int(saved['generation'])>int(cancelled['generation'])
            and receipt.get('duplicate')=={'ok':True,'clicks':2},'Empty reopen/duplicate protection missing')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES[scenario]),'Empty stages incomplete')
    previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};opened={};selected=sent=held_journal=None
    for index,(stage,name) in enumerate(zip(stages,STAGES[scenario])):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Empty stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        counts=(0,0,0,0) if index<2 else ((1,0,0,0) if scenario=='stale' else (1,1,0,0) if index==2 else (1,1,1,0) if scenario=='partial'
                else (1,1,1,1) if index<5 else (2,2,1,1) if index==5 else (2,2,2,2) if index<8 else (3,3,2,2) if index==8 else (3,3,3,3))
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],
                    'Empty immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int
                and source['reservations']==int(scenario!='stale' and index>=2),'Empty ownership/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Empty preflight not original-bound')
            W.inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Empty preflight source drift')
        if source['writes'] and sent is None:selected,*sent=W.write(source['writes'][0],originals,e)
        accepted=scenario=='success' and index>=3
        W.inventory(source.get('fresh'),e,initial,selected if accepted else None,index>=6,index>=9,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,selected if accepted else None,index>=6,index>=9)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),
                'Empty unaccepted change altered visible/canonical source')
        received=None
        if source['acknowledgements']:
            received=W.ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,originals,{e.original:'',selected:e.reports[-1]},
                           {e.original:2000,selected:2000 if e.reused else 1000},e,scenario=='partial')
        if accepted:
            reads=source['readbacks'];first=W.inventory(reads[0],e,initial,selected)
            require(all(S.raw(first[text(row,1)])==S.raw(row) for row in received),'Empty independent ACK/readback differs')
            require(source['fresh']==source['visible']==reads[-1],'Empty accepted source/readback custody lost')
            for write_index,start,identity,content in ((1,5,selected,e.continued),(2,8,e.original,e.edited)):
                if index<start:continue
                prior_value=reads[write_index-1];prior=W.inventory(prior_value,e,initial,selected,write_index==2)
                pre=source['preflights'][write_index]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Empty later preflight drift')
                row=W.continuation(source['writes'][write_index],prior[identity],content,write_index,source['writes'][1]['session'],e)
                if index>start:
                    confirmed=W.ack(source['acknowledgements'][write_index],[row],prior,{identity:content},{identity:3000},e)[0]
                    read=W.inventory(reads[write_index],e,initial,selected,True,write_index==2)
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),
                            'Empty later Save changed an untouched row')
        pending=name in ('saving','continued-saving','parent-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Empty pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Empty operation lock missing')
        else:require(source.get('operation') is None,'Unexpected Empty operation lock')
        W.journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':held_journal=stage['journal']
        if uncertain:require(stage['journal']==held_journal,'Empty unknown outcome lost durable whole pair')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Empty layout overflow')
        held=name in ('typed-child','saving','refused','uncertain')
        parts=['',e.updates[-1]] if held else [e.updates[-1]] if name=='child-editing' else [''] if name=='parent-editing' else [e.continued] if name=='continued-saving' else [e.edited] if name=='parent-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='typed-child' else saved
                typed=owner['typing']
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('blocks')==typed['after'] and editor.get('typing')==typed and editor.get('placeholder')==typed['placeholder']
                        and editor.get('rawBlocks')==typed['rawAfter'] and editor.get('emptyProof')==owner['emptyProof'],'Empty editor lost sealed event evidence')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('emptyProof') is None,'Ordinary editor retained Empty projection')
                owner=editor.get('generation');target='selected' if index in (4,5) else 'original'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Empty later owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Empty editor opening recycled owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Empty pending Save changed editor owner')
            require(editor.get('leadingPlaceholder') is None,'Typed-empty acquired start projection')
            if not held:require(editor.get('typing') is None and editor.get('placeholder') is None,'Ordinary editor retained typed-empty projection')
        else:require(geo.get('native') is None and geo.get('paragraphs')==reader(e,selected if accepted else None,index>=6,index>=9),'Empty reader identity/content/order changed')
        if name=='cancelled':require(source==original,'Empty Cancel dispatched or changed source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Empty terminal differs from original final stage')
