"""One canonical-empty Enter, direct pair acceptance and two later ordinary saves."""
import canvas_empty_enter_wire as W
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_heading import require, text

STAGES={'success':('empty','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
        'stale':('empty','cancelled','refused','cancelled-stale'),'partial':('empty','cancelled','saving','uncertain')}


def empty_dom(markup,count):
    root=T.PlainDom(markup).root
    require(len(root)==1 and root[0][:2]==['div',[('class','rich-editor-block')]],'Empty Enter rich-block DOM changed')
    sections=root[0][2]
    require(len(sections)==count and all(isinstance(n,list) and n[:2]==['div',[('class','rich-editor-section')]] for n in sections),
            'Empty Enter section DOM changed')
    result=[]
    for section in sections:
        nodes=section[2];depth=0
        while len(nodes)==1 and isinstance(nodes[0],list) and nodes[0][:2]==['span',[]]:
            nodes=nodes[0][2];depth+=1;require(depth<=16,'Empty placeholder exceeds bound')
        require(nodes==[['br',[],[]]],'Empty Enter contains text/style/semantic wrapper or multiple breaks')
        result.append(dict(html='<span>'*depth+'<br>'+'</span>'*depth,spanDepth=depth))
    return result


def witness(value):
    require(isinstance(value,dict) and value.get('kind')=='empty' and value.get('caretChildZero') is True
            and value.get('beforeInputTrusted') is True and value.get('inputTrusted') is True and value.get('inputType')=='insertParagraph'
            and type(value.get('revision')) is int and value['revision']==1 and type(value.get('offset')) is int and value['offset']==0,
            'Empty trusted single Enter seal changed')
    require(S.plain_sections(value.get('before'))==[''] and S.plain_sections(value.get('after'))==['','']
            and all(section['elements']==[] for blocks in (value['before'],value['after']) for section in blocks[0]['elements'])
            and not any(k in value for k in ('typing','placeholder','leadingPlaceholder','caretOriginalZero')),
            'Empty Enter admitted typing or another branch/content')
    proof=value.get('emptyProof');require(isinstance(proof,dict) and set(proof)=={'beforeHtml','afterHtml','before','sections','caretSection','caretOffset'},
                                          'Empty Enter DOM/caret proof missing')
    require(proof['before']==empty_dom(proof['beforeHtml'],1)[0] and proof['sections']==empty_dom(proof['afterHtml'],2)
            and type(proof['caretSection']) is int and proof['caretSection']==1 and type(proof['caretOffset']) is int and proof['caretOffset']==0,
            'Empty Enter placeholder/caret evidence changed')
    require(all(type(section.get('spanDepth')) is int for section in [proof['before'],*proof['sections']]),'Empty placeholder depth is not an integer')
    def markup(leaves):return '<div class="rich-editor-block">'+''.join('<div class="rich-editor-section">'+leaf['html']+'</div>' for leaf in leaves)+'</div>'
    require(proof['beforeHtml']==markup([proof['before']]) and proof['afterHtml']==markup(proof['sections']),
            'Empty placeholder HTML differs from sealed exact structure')
    raw=value.get('rawAfter');parts=S.plain_sections(raw)
    require(len(parts)==2 and all(part in ('','\n') for part in parts)
            and all(section['elements'] in ([],[dict(type='text',text='\n')]) for section in raw[0]['elements']),
            'Empty projection hid content beyond measured unstyled BR')
    require(parts==['\n' if section['spanDepth'] else '' for section in proof['sections']], 'Empty raw shared-reader result differs from exact DOM shape')
    require(all(isinstance(value.get(k),str) and value[k].isascii() and value[k].isdigit() for k in ('generation','visit')),'Empty event owner missing')


def reader(e,selected=None,selected_saved=False,original_saved=False):
    ids=e.ids[1:]
    if selected and not e.reused:ids.insert(1,selected)
    return [dict(id=k,text=e.edited if k==e.original and original_saved else e.continued if k==selected and selected_saved
                 else e.neighbor if k==e.following else 'Last paragraph' if k=='last-text' else '') for k in ids]


def validate(receipt,empty_history='bootstrap',empty_layout='no-following'):
    e=W.config(empty_history,empty_layout);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Empty scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading'))
            and original.get('operation') is None and original.get('reservations')==0 and original.get('capacityReason') is None,'Empty original not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Empty original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled);witness(saved)
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
            received=W.ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,originals,{e.original:'',selected:''},
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
        held=name in ('empty','saving','refused','uncertain')
        parts=['',''] if held else [''] if name in ('child-editing','parent-editing') else [e.continued] if name=='continued-saving' else [e.edited] if name=='parent-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='empty' else saved
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('blocks')==owner['after']
                        and editor.get('rawBlocks')==owner['rawAfter'] and editor.get('emptyProof')==owner['emptyProof'],'Empty editor lost sealed event evidence')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('emptyProof') is None,'Ordinary editor retained Empty projection')
                owner=editor.get('generation');target='selected' if index in (4,5) else 'original'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Empty later owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Empty editor opening recycled owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Empty pending Save changed editor owner')
            require(all(editor.get(k) is None for k in ('typing','placeholder','leadingPlaceholder')),'Empty acquired typed/start/end projection')
        else:require(geo.get('native') is None and geo.get('paragraphs')==reader(e,selected if accepted else None,index>=6,index>=9),'Empty reader identity/content/order changed')
        if name=='cancelled':require(source==original,'Empty Cancel dispatched or changed source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Empty terminal differs from original final stage')
