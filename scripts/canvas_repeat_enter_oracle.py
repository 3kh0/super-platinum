"""Two trusted End events, one triple Save, and three ordinary later saves."""
import canvas_repeat_enter_wire as W
import canvas_empty_enter_oracle as E
import canvas_end_split_oracle as B
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_heading import require, text

STAGES={'success':('repeat','cancelled','saving','confirmed','second-editing','second-saving','second-confirmed','child-editing',
                   'continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),
        'stale':('repeat','cancelled','refused','cancelled-stale'),'partial':('repeat','cancelled','saving','uncertain')}


def witness(value,e):
    B.witness(value,e.corpus)
    require('typing' not in value and 'leadingPlaceholder' not in value and 'emptyProof' not in value,'Repeat includes another edit branch')
    proof=value.get('repeat')
    require(isinstance(proof,dict) and set(proof)=={'revision','after','rawAfter','html','beforeInputTrusted','inputTrusted','inputType','caretSection','caretOffset','placeholders'},
            'Second trusted Enter evidence missing')
    require(type(proof['revision']) is int and proof['revision']==2 and proof['beforeInputTrusted'] is True and proof['inputTrusted'] is True
            and proof['inputType']=='insertParagraph' and type(proof['caretSection']) is int and proof['caretSection']==2
            and type(proof['caretOffset']) is int and proof['caretOffset']==0,'Second Enter trust/revision/caret changed')
    after,raw=proof['after'],proof['rawAfter']
    require(S.plain_sections(after)==[e.seed,'',''] and after[0]['elements'][0]==value['after'][0]['elements'][0]
            and all(section['elements']==[] for section in after[0]['elements'][1:]),'Second Enter changed original/first paragraph')
    root=T.PlainDom(proof['html']).root
    require(len(root)==1 and root[0][:2]==['div',[('class','rich-editor-block')]] and len(root[0][2])==3
            and all(isinstance(n,list) and n[:2]==['div',[('class','rich-editor-section')]] for n in root[0][2]),'Repeat DOM wrapper shape changed')
    first=root[0][2][0][2]
    require(first==[e.seed] or first==[['span',[],[e.seed]]],'Repeat original DOM content/style changed')
    leaves=proof['placeholders'];require(isinstance(leaves,list) and len(leaves)==2,'Repeat empty DOM evidence incomplete')
    require(leaves[0]==dict(html=value['placeholder']['childHtml'],spanDepth=value['placeholder']['spanDepth']),
            'Second Enter replaced the first empty placeholder')
    for index,leaf in enumerate(leaves):
        require(isinstance(leaf,dict) and set(leaf)=={'html','spanDepth'} and type(leaf['spanDepth']) is int,'Repeat placeholder malformed')
        markup='<div class="rich-editor-block"><div class="rich-editor-section">'+leaf['html']+'</div></div>'
        require(E.empty_dom(markup,1)==[leaf] and T.PlainDom(markup).root[0][2][0]==root[0][2][index+1],'Repeat placeholder not exact DOM')
    expected=[e.seed]+['\n' if leaf['spanDepth'] else '' for leaf in leaves]
    require(S.plain_sections(raw)==expected and raw[0]['elements'][0]==after[0]['elements'][0]
            and all(section['elements']==([] if leaf['spanDepth']==0 else [dict(type='text',text='\n')])
                    for section,leaf in zip(raw[0]['elements'][1:],leaves)),'Repeat projection hid text beyond exact BR placeholders')


def reader(e,selected=None,completed=0):
    expected=W.contents(e,selected,completed)
    return [dict(id=k,text=expected[k]) for k in W.ordered_ids(e,selected)[1:]]


def validate(receipt,repeat_history='bootstrap',repeat_layout='no-following',corpus='ascii'):
    e=W.config(repeat_history,repeat_layout,corpus);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Repeat isolation/scenario failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading')) and original.get('operation') is None
            and original.get('reservations')==0 and original.get('capacityReason') is None,'Repeat original not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Repeat original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and int(saved['generation'])>int(cancelled['generation'])
            and receipt.get('duplicate')=={'ok':True,'clicks':2},'Repeat reopen/duplicate protection missing')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES[scenario]),'Repeat stage set incomplete')
    previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};opened={};selected=sent=held_journal=None
    for index,(stage,name) in enumerate(zip(stages,STAGES[scenario])):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Repeat stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        accepted=scenario=='success' and index>=3
        completed=sum(index>start for start in (5,8,11)) if accepted else 0
        if index<2:counts=(0,0,0,0)
        elif scenario=='stale':counts=(1,0,0,0)
        elif not accepted:counts=(1,1,int(index>=3),0)
        else:count=1+sum(index>=start for start in (5,8,11));counts=(count,count,1+completed,1+completed)
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],
                    'Repeat immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int
                and source['reservations']==int(scenario!='stale' and index>=2),'Repeat owner/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Repeat preflight not original-bound')
            W.inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Repeat preflight source drift')
        if source['writes'] and sent is None:selected,*sent=W.write(source['writes'][0],originals,e)
        W.inventory(source.get('fresh'),e,initial,selected if accepted else None,completed,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,selected if accepted else None,completed)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),
                                'Unaccepted repeat changed visible/canonical inventory')
        received=None
        if source['acknowledgements']:
            first,second=selected
            received=W.ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,originals,{e.original:e.wire_seed,first:'',second:''},
                           {e.original:2000,first:2000 if e.reused else 1000,second:1000},e,scenario=='partial')
        if accepted:
            reads=source['readbacks'];first_rows=W.inventory(reads[0],e,initial,selected)
            require(all(S.raw(first_rows[text(row,1)])==S.raw(row) for row in received),'Repeat independent ACK/readback disagrees')
            require(source['fresh']==source['visible']==reads[-1],'Repeat accepted readback custody lost')
            for write_index,start,identity,content in ((1,5,selected[1],e.second_edited),(2,8,selected[0],e.continued),(3,11,e.original,e.edited)):
                if index<start:continue
                prior_value=reads[write_index-1];prior=W.inventory(prior_value,e,initial,selected,write_index-1)
                pre=source['preflights'][write_index]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Repeat later preflight drift')
                row=W.continuation(source['writes'][write_index],prior[identity],W.M.rtml(content),write_index,source['writes'][1]['session'],e)
                if index>start:
                    confirmed=W.ack(source['acknowledgements'][write_index],[row],prior,{identity:W.M.rtml(content)},{identity:3000},e)[0]
                    read=W.inventory(reads[write_index],e,initial,selected,write_index)
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),
                            'Repeat later Save changed untouched member')
        pending=name in ('saving','second-saving','continued-saving','parent-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Repeat pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Repeat operation lock missing')
        else:require(source.get('operation') is None,'Unexpected repeat operation lock')
        W.journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':held_journal=stage['journal']
        if uncertain:require(stage['journal']==held_journal,'Unknown outcome lost durable third member')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Repeat layout overflow')
        held=name in ('repeat','saving','refused','uncertain')
        parts=[e.seed,'',''] if held else [''] if name in ('second-editing','child-editing') else [e.seed] if name=='parent-editing' else [e.second_edited] if name=='second-saving' else [e.continued] if name=='continued-saving' else [e.edited] if name=='parent-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='repeat' else saved
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('blocks')==owner['repeat']['after']
                        and editor.get('rawBlocks')==owner['repeat']['rawAfter'] and editor.get('repeatProof')==owner['repeat']
                        and editor.get('placeholder')==owner['placeholder'],'Repeat native lost sealed two-event evidence')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('repeatProof') is None and editor.get('placeholder') is None,'Ordinary editor retained repeat projection')
                owner=editor.get('generation');target='second' if index in (4,5) else 'first' if index in (7,8) else 'original'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Repeat later owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Repeat editor recycled opening owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Repeat pending Save changed owner')
            require(all(editor.get(k) is None for k in ('typing','leadingPlaceholder','emptyProof')),'Repeat acquired unrelated projection')
        else:require(geo.get('native') is None and geo.get('paragraphs')==reader(e,selected if accepted else None,completed),'Repeat reader content/order changed')
        if name=='cancelled':require(source==original,'Repeat Cancel dispatched or changed source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Repeat terminal differs from original final stage')
