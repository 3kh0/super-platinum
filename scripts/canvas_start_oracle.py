"""Start Enter native acceptance: new blank before unchanged original, then both saves."""
import canvas_start_wire as W
import canvas_split_oracle as S
from canvas_middle_typing_oracle import ack
from canvas_heading import require, text

STAGES={'success':('start','cancelled','saving','confirmed','new-editing','new-saving','new-confirmed','original-editing','original-saving','original-confirmed'),
        'stale':('start','cancelled','refused','cancelled-stale'),
        'partial':('start','cancelled','saving','uncertain')}


def leading(raw,canonical,proof,seed):
    require(proof==dict(blockCount=1,sectionCount=2,firstHtml='<span><br></span>',spanDepth=1,caretSection=1,caretOffset=0)
            and all(type(proof.get(k)) is int for k in ('blockCount','sectionCount','spanDepth','caretSection','caretOffset')),
            'Start lacks exact leading placeholder/original-caret proof')
    require(S.plain_sections(raw)==['\n',seed] and S.plain_sections(canonical)==['',seed]
            and raw[0]['elements'][0]['elements']==[dict(type='text',text='\n')]
            and canonical[0]['elements'][0]['elements']==[] and raw[0]['elements'][1]==canonical[0]['elements'][1],
            'Start rewrote more than the measured leading placeholder')


def witness(value,e):
    require(isinstance(value,dict) and value.get('kind')=='start' and value.get('caretOriginalZero') is True
            and value.get('beforeInputTrusted') is True and value.get('inputTrusted') is True and value.get('inputType')=='insertParagraph'
            and type(value.get('revision')) is int and value['revision']==1 and type(value.get('offset')) is int and value['offset']==0,
            'Start trusted original-start Enter seal changed')
    require(S.plain_sections(value.get('before'))==[e.seed] and not any(k in value for k in ('typing','placeholder','caretChildZero')),
            'Start admits post-Enter typing or different branch')
    leading(value.get('rawAfter'),value.get('after'),value.get('leadingPlaceholder'),e.seed)
    require(all(isinstance(value.get(k),str) and value[k].isascii() and value[k].isdigit() for k in ('generation','visit')),
            'Start witness owner missing')


def validate(receipt,corpus='ascii',parent_mode='bootstrap'):
    e=W.config(corpus,parent_mode);W.labels(receipt,e);scenario=receipt.get('scenario')
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Start scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');originals=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading'))
            and original.get('operation') is None and original.get('reservations')==0 and original.get('capacityReason') is None,'Start original source not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Start original already dispatched')
    W.journal(receipt.get('initialJournal'))
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
        if source['writes'] and sent is None:new,sent=W.write(source['writes'][0]);require(new not in originals,'Start reused an old ID')
        accepted=scenario=='success' and index>=3
        W.inventory(source.get('fresh'),e,initial,new if accepted else None,index>=6,index>=9,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,new if accepted else None,index>=6,index>=9)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),
                'Start unaccepted canonical changed')
        received=None
        if source['acknowledgements']:
            received=ack(source['acknowledgements'][0],[] if scenario=='partial' else [sent],{}, {new:''},{new:1000},scenario=='partial')
        if accepted:
            reads=source['readbacks'];first=W.inventory(reads[0],e,initial,new)
            require(S.raw(first[new])==S.raw(received[0]),'Start independent ACK/readback differs')
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
        W.journal(stage.get('journal'),source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':held_journal=stage['journal']
        if uncertain:require(stage['journal']==held_journal,'Start unknown outcome lost exact insertion receipt')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Start layout overflow')
        held=name in ('start','saving','refused','uncertain')
        parts=['',e.seed] if held else [''] if name=='new-editing' else [e.continued] if name=='new-saving' else [e.seed] if name=='original-editing' else [e.edited] if name=='original-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='start' else saved
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('blocks')==owner['after']
                        and editor.get('rawBlocks')==owner['rawAfter'] and editor.get('leadingPlaceholder')==owner['leadingPlaceholder'],
                        'Start editor lost sealed leading projection')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('leadingPlaceholder') is None,'Start ordinary editor retained insertion projection')
                owner=editor.get('generation');target='new' if name.startswith('new-') else 'original'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Start later editor owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Start editor opening recycled owner');owners.add(owner);opened[target]=owner
                else:require(owner==opened.get(target),'Start pending Save replaced editor owner')
            require(editor.get('typing') is None and editor.get('placeholder') is None,'Start admits a typed/end witness')
        else:
            expected=([dict(id=new,text=e.continued if index>=6 else '')] if accepted else [])
            expected.extend([dict(id='paragraph',text=e.edited if index>=9 else e.seed),dict(id='neighbor',text='Another paragraph')])
            require(geo.get('native') is None and geo.get('paragraphs')==expected,'Start reader identity/content/order changed')
        if name=='cancelled':require(source==original,'Start Cancel dispatched or mutated source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Start terminal differs from last stage')
