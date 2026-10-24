"""Native interleaved Enter/edit sequence and independent full-operation acceptance."""
import canvas_sequence_wire as W
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_heading import require, text


def stages_for(e):
    success=['sequence','cancelled','saving','confirmed']
    for i in reversed(range(e.append_count)):
        success += [f'member-{i}-{suffix}' for suffix in ('editing','saving','confirmed')]
    success += ['parent-'+suffix for suffix in ('editing','saving','confirmed')]
    return {'success':tuple(success),'stale':('sequence','cancelled','refused','cancelled-stale'),'partial':('sequence','cancelled','saving','uncertain')}


def snapshot(event,parts):
    canonical,raw=event.get('after'),event.get('rawAfter');sections=event.get('sections')
    require(S.plain_sections(canonical)==parts and len(S.plain_sections(raw))==len(parts),'Sequence snapshot member text/count changed')
    root=T.PlainDom(event.get('html')).root
    require(len(root)==1 and root[0][:2]==['div',[('class','rich-editor-block')]] and len(root[0][2])==len(parts)
            and isinstance(sections,list) and len(sections)==len(parts),'Sequence DOM block/section shape changed')
    for index,(section,part,node) in enumerate(zip(sections,parts,root[0][2])):
        require(set(section)=={'html','text','placeholder'} and section['text']==part
                and node[:2]==['div',[('class','rich-editor-section')]] and T.PlainDom(section['html']).root==node[2],
                'Sequence exact section HTML changed')
        nodes=node[2]
        if part:
            def plain(items,depth=0):
                require(depth<=16,'Sequence span depth exceeded');out=''
                for item in items:
                    if isinstance(item,str):out+=item
                    else:
                        require(item[:2]==['span',[]],'Sequence styled/semantic content appeared');out+=plain(item[2],depth+1)
                return out
            require(section['placeholder'] is None and plain(nodes)==part and raw[0]['elements'][index]==canonical[0]['elements'][index],
                    'Sequence nonempty DOM/raw mismatch')
        else:
            depth=0
            while len(nodes)==1 and isinstance(nodes[0],list) and nodes[0][:2]==['span',[]]:nodes=nodes[0][2];depth+=1
            require(depth<=16 and nodes==[['br',[],[]]] and section['placeholder']==dict(html='<span>'*depth+'<br>'+'</span>'*depth,spanDepth=depth)
                    and type(section['placeholder']['spanDepth']) is int and canonical[0]['elements'][index]['elements']==[]
                    and raw[0]['elements'][index]['elements']==([dict(type='text',text='\n')] if depth else []),'Sequence empty placeholder not exact bounded shape')


def expected_events(e):
    # Event chronology is independent of the per-member histories used by wire.
    preenter=e.cadence.startswith('preenter-')
    schedule=[(-1,j) for j in range(len(e.parent_updates))] if preenter else []
    for i,updates in enumerate(e.updates):
        schedule.append((i,None))
        schedule.extend((i,j) for j in range(1 if i==0 and e.cadence.startswith('revisit-') else len(updates)))
        if i==1 and e.cadence.startswith('revisit-'):
            schedule.extend((0,j) for j in range(1,len(e.updates[0])))
        if i==1 and e.parent_updates and not preenter:
            schedule.extend((-1,j) for j in range(len(e.parent_updates)))
    revision=0;parts=[e.seed]
    for i,j in schedule:
        index=i+1;revision+=1
        if j is None:
            before=parts[-1];offset=T.utf16(before);parts.append('')
            yield dict(kind='enter',activeIndex=index,inputType='insertParagraph',data=None,selectionStart=offset,selectionEnd=offset,start=offset,end=offset,
                       beforeText=before,afterText='',beforeRevision=revision-1,revision=revision,caret=0,beforeInputTrusted=True,inputTrusted=True),parts.copy()
            continue
        updates=e.parent_updates if i==-1 else e.updates[i];prior=parts[index];after=updates[j]
        if i==-1:
            n=T.utf16(e.seed)
            if e.cadence.endswith('-erase'):kind,data,start,stop,selection='deleteContentBackward',None,0,n,(0,n)
            elif j==0:kind,data,start,stop,selection='insertText',' draftx',n,n,(n,n)
            elif e.cadence.endswith('-revert'):kind,data,start,stop,selection='deleteContentBackward',None,n,n+7,(n,n+7)
            elif j==1:kind,data,start,stop,selection='deleteContentBackward',None,n+6,n+7,(n+7,n+7)
            else:kind,data,start,stop,selection='insertText','revised',n+1,n+6,(n+1,n+6)
        elif j==0:kind,data,start,stop,selection='insertText',after,0,0,(0,0)
        else:
            n=T.utf16(updates[0])
            if e.cadence in ('erase-before-enter','revisit-erase') or e.cadence=='revert-before-enter' and j==3:
                kind,data,start,stop='deleteContentBackward',None,0,T.utf16(prior);selection=(start,stop)
            elif j==1:kind,data,start,stop,selection='insertText',' draftx',n,n,(n,n)
            elif e.cadence in ('revert-before-enter','revisit-revert'):kind,data,start,stop,selection='deleteContentBackward',None,n,n+7,(n,n+7)
            elif j==2:kind,data,start,stop,selection='deleteContentBackward',None,n+6,n+7,(n+7,n+7)
            else:kind,data,start,stop,selection='insertText','revised' if e.cadence=='revisit-correction' else 'paragraph',n+1,n+6,(n+1,n+6)
        parts[index]=after
        yield dict(kind='edit',activeIndex=index,inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=stop,
                   beforeText=prior,afterText=after,beforeRevision=revision-1,revision=revision,caret=start+T.utf16(data or ''),beforeInputTrusted=True,inputTrusted=True),parts.copy()


def witness(value,e):
    preenter=e.cadence.startswith('preenter-')
    first_index=len(e.parent_updates) if preenter else 0
    first_text=e.parent_final if preenter else e.seed
    require(isinstance(value,dict) and value.get('kind')=='sequence' and value.get('offset')==T.utf16(first_text)
            and type(value.get('offset')) is int and S.plain_sections(value.get('before'))==[e.seed]
            and value.get('revision')==first_index+1 and type(value.get('revision')) is int and value.get('beforeInputTrusted') is True
            and value.get('inputTrusted') is True and value.get('inputType')=='insertParagraph'
            and all(k not in value for k in ('placeholder','caretChildZero','typing','repeat','leadingPlaceholder','emptyProof')),
            'Sequence first Enter ownership/proof changed')
    require(all(isinstance(value.get(k),str) and value[k].isascii() and value[k].isdigit() for k in ('generation','visit')),'Sequence owner missing')
    sequence=value.get('sequence');require(isinstance(sequence,dict),'Sequence event ledger missing')
    events=sequence.get('events');expected=list(expected_events(e))
    require(isinstance(events,list) and len(events)==len(expected),'Sequence event loss/duplication')
    previous=None
    for event,(fields,parts) in zip(events,expected):
        require(isinstance(event,dict) and all(event.get(k)==v and type(event.get(k)) is type(v) for k,v in fields.items()),'Sequence trusted event/range/revision changed')
        snapshot(event,parts)
        if previous:
            target=fields['activeIndex'] if fields['kind']=='edit' else None
            for index in range(len(previous['sections'])):
                if index==target:continue
                require(event['sections'][index]==previous['sections'][index]
                        and event['after'][0]['elements'][index]==previous['after'][0]['elements'][index]
                        and event['rawAfter'][0]['elements'][index]==previous['rawAfter'][0]['elements'][index],
                        'Sequence event changed a non-target paragraph')
        previous=event
    require(value.get('after')==events[first_index]['after'] and value.get('rawAfter')==events[first_index]['rawAfter'],'Sequence initial Enter snapshot replaced')
    require(type(sequence.get('revision')) is int and sequence['revision']==len(events)
            and all(sequence.get(k)==events[-1][k] for k in ('after','rawAfter','html')),'Sequence final snapshot detached from event ledger')
    return sequence


def reader(e,selected=None,completed=0):
    expected=W.contents(e,selected,completed)
    return [dict(id=k,text=expected[k]) for k in W.ordered_ids(e,selected)[1:]]


def validate(receipt,append_count=1,history='bootstrap',layout='no-following',cadence='straight',corpus='ascii'):
    e=W.config(append_count,history,layout,cadence,corpus);STAGES=stages_for(e);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Sequence isolation/scenario failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading')) and original.get('operation') is None
            and original.get('reservations')==0 and original.get('capacityReason') is None,'Sequence original not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Sequence original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and int(saved['generation'])>int(cancelled['generation'])
            and receipt.get('duplicate')=={'ok':True,'clicks':2},'Sequence reopen/duplicate protection missing')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES[scenario]),'Sequence stage set incomplete')
    starts=tuple(5+3*i for i in range(e.append_count+1));previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};opened={};selected=sent=held_journal=None
    for index,(stage,name) in enumerate(zip(stages,STAGES[scenario])):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Sequence stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        accepted=scenario=='success' and index>=3
        completed=sum(index>start for start in starts) if accepted else 0
        if index<2:counts=(0,0,0,0)
        elif scenario=='stale':counts=(1,0,0,0)
        elif not accepted:counts=(1,1,int(index>=3),0)
        else:count=1+sum(index>=start for start in starts);counts=(count,count,1+completed,1+completed)
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],
                    'Sequence immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int
                and source['reservations']==int(scenario!='stale' and index>=2),'Sequence owner/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Sequence preflight not original-bound')
            W.inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Sequence preflight source drift')
        if source['writes'] and sent is None:selected,sent=W.write(source['writes'][0],originals,e)
        W.inventory(source.get('fresh'),e,initial,selected if accepted else None,completed,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,selected if accepted else None,completed)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),
                                'Unaccepted sequence changed visible/canonical inventory')
        received=None
        if source['acknowledgements']:
            bodies={e.original:e.parent_wire,**{k:W.M.rtml(v) for k,v in zip(selected,e.final)}};revisions={e.original:2000,**{k:2000 if e.reused and i==0 else 1000 for i,k in enumerate(selected)}}
            received=W.ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,originals,bodies,revisions,e,scenario=='partial')
        if accepted:
            reads=source['readbacks'];first_rows=W.inventory(reads[0],e,initial,selected)
            require(all(S.raw(first_rows[text(row,1)])==S.raw(row) for row in received),'Sequence independent ACK/readback disagrees')
            require(source['fresh']==source['visible']==reads[-1],'Sequence accepted readback custody lost')
            for write_index,(start,(identity,content)) in enumerate(zip(starts,W.later(e,selected)),1):
                if index<start:continue
                prior_value=reads[write_index-1];prior=W.inventory(prior_value,e,initial,selected,write_index-1)
                pre=source['preflights'][write_index]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Sequence later preflight drift')
                row=W.continuation(source['writes'][write_index],prior[identity],W.M.rtml(content),write_index,source['writes'][1]['session'],e)
                if index>start:
                    confirmed=W.ack(source['acknowledgements'][write_index],[row],prior,{identity:W.M.rtml(content)},{identity:3000},e)[0]
                    read=W.inventory(reads[write_index],e,initial,selected,write_index)
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),
                            'Sequence later Save changed untouched member')
        pending=name=='saving' or name.endswith('-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Sequence pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Sequence operation lock missing')
        else:require(source.get('operation') is None,'Unexpected sequence operation lock')
        W.journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':held_journal=stage['journal']
        if uncertain:require(stage['journal']==held_journal,'Unknown outcome lost durable third member')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Sequence layout overflow')
        held=name in ('sequence','saving','refused','uncertain')
        if held:parts=[e.parent_final,*e.final]
        elif name.endswith('-editing') or name.endswith('-saving'):
            identity,content=W.later(e,selected)[(index-4)//3]
            parts=[content] if name.endswith('-saving') else [e.parent_final if identity==e.original else e.final[selected.index(identity)]]
        else:parts=None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='sequence' else saved
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('blocks')==owner['sequence']['after']
                        and editor.get('rawBlocks')==owner['sequence']['rawAfter'] and editor.get('sequenceProof')==owner['sequence'] and editor.get('repeatProof') is None
                        and editor.get('placeholder') is None and editor.get('typing') is None,'Sequence native lost sealed two-event evidence')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('repeatProof') is None and editor.get('placeholder') is None and editor.get('typing') is None and editor.get('sequenceProof') is None,'Ordinary editor retained sequence projection')
                owner=editor.get('generation');target=(index-4)//3
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Sequence later owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Sequence editor recycled opening owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Sequence pending Save changed owner')
            require(all(editor.get(k) is None for k in ('leadingPlaceholder','emptyProof')),'Sequence acquired unrelated projection')
        else:require(geo.get('native') is None and geo.get('paragraphs')==reader(e,selected if accepted else None,completed),'Sequence reader content/order changed')
        if name=='cancelled':require(source==original,'Sequence Cancel dispatched or changed source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Sequence terminal differs from original final stage')
