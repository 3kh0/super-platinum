"""Pre-middle trusted input, complete history/readback and independent later saves."""
import re
import canvas_premiddle_wire as W
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_sequence_oracle import snapshot
from canvas_heading import require, text, wire, decoded, field
config=W.config
inventory=W.inventory
journal=W.journal
split_write=W.write

def ack(value,sent,source,contents,sequences,partial=False,parent_id="paragraph"):
    require(value.get('partial') is partial and value.get('accepted') is (not partial),'Middle ACK admission flag changed')
    outer=decoded(value.get('raw'));require(T.tags(outer)==[1,2] and field(outer,1,0)==1,'Middle ACK failed/ambiguous')
    data=wire(field(outer,2,2));require(T.tags(data)==[1]*len(sent)+[15] and text(data,15)=='document','Middle ACK incomplete/misdirected')
    rows=S.messages(data,1)
    for row,outgoing in zip(rows,sent):
        identity=text(outgoing,1);parent=identity==parent_id
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and S.body(row,parent)==contents[identity]
                and field(row,2,0)==sequences[identity],'Middle ACK content/language/revision changed')
        now,prior=wire(field(row,38,2)),wire(field(outgoing,38,2))
        require(field(now,3,0)==sequences[identity] and field(now,4,0)>0 and not any(f[0]==12 for f in now)
                and S.stable(now,(3,4,12))==S.stable(prior,(3,4,12)),'Middle ACK lost exact history/lineage')
        if identity in source:
            require(S.stable(row,(2,9,12,13,19,20,33,38))==S.stable(source[identity],(2,9,12,13,19,20,33,38)),'Middle ACK replaced opaque original row')
    return rows


def continuation(value,identity,source,content,sequence,session,parent_id="paragraph"):
    data=decoded(value.get('documentData'));S.rich_title(data)
    require(value.get('kind')=='text' and value.get('parent')==identity and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==sequence and value.get('session')==session
            and re.fullmatch('[0-9a-f]{12}',session) and T.tags(data)==[1,14],'Middle continuation not ordinary Save')
    row=wire(field(data,1,2));before=wire(field(source,38,2));now=wire(field(row,38,2))
    require(T.tags(row)==[1,2,9,12,13,19,20,33,38] and text(row,1)==identity and field(row,2,0)==field(source,2,0)
            and S.body(row,identity==parent_id)==content and field(row,9,0)==0 and field(row,13,2)==b''
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==session+'-'+str(sequence-1),'Middle continuation body/structure changed')
    require(S.stable(now,(4,5,6,12))==S.stable(before,(4,5,6,12)) and not any(f[0]==4 for f in now)
            and [f[2] for f in now if f[0]==6]==[f[2] for f in before if f[0]==6]+[field(now,12,2)],'Middle continuation lost lineage/metadata custody')
    current,prior=T.decode_history(field(now,5,2),field(before,5,2))
    W.chain(current['changes'],prior['changes'],value['session']+'00000000')
    require(current['heads']==[text(now,12)] and current['changes'][-1]['text']==content+'\ufeff','Middle continuation final historical text differs')
    return row


def expected_events(e):
    n=e.source_offset;full=e.full;events=[]
    def edit(active,before,after,kind,data,start,end,selection):
        events.append(dict(kind='edit',activeIndex=active,inputType=kind,data=data,selectionStart=selection[0],selectionEnd=selection[1],start=start,end=end,beforeText=before,afterText=after,caret=start+T.utf16(data or '')))
    if e.pre_enter=='retype':
        edit(0,full,'','deleteContentBackward',None,0,T.utf16(full),(0,T.utf16(full)))
        edit(0,'',full,'insertText',full,0,0,(0,0))
    else:
        edit(0,full,e.pre_updates[0],'insertText',' draftx',n,n,(n,n))
        if e.pre_enter=='revert':edit(0,e.pre_updates[0],full,'deleteContentBackward',None,n,n+7,(n,n+7))
        else:
            edit(0,e.pre_updates[0],e.pre_updates[1],'deleteContentBackward',None,n+6,n+7,(n+7,n+7))
            edit(0,e.pre_updates[1],e.live,'insertText','revised',n+1,n+6,(n+1,n+6))
    events.append(dict(kind='middle',activeIndex=1,inputType='insertParagraph',data=None,selectionStart=e.offset,selectionEnd=e.offset,start=e.offset,end=e.offset,beforeText=e.live,afterText=e.tail,caret=0))
    before=e.tail
    for index,after in enumerate(e.updates if e.typing!='none' else ()):
        if index==0:
            kind='insertText';data='New' if e.typing=='correction' else 'x'
            start,stop=(0,T.utf16(before)) if e.typing=='correction' else (T.utf16(before),T.utf16(before));selection=(start,stop)
        elif e.typing=='erase':kind,data,start,stop='deleteContentBackward',None,0,T.utf16(before);selection=(start,stop)
        elif e.typing=='restore' or index==2:kind,data,start,stop='deleteContentBackward',None,T.utf16(before)-1,T.utf16(before);selection=(stop,stop)
        elif index==1:kind,data,start,stop='insertText',after[len(before):],T.utf16(before),T.utf16(before);selection=(start,stop)
        else:kind,data,start,stop='insertText','paragraph',T.utf16(before)-5,T.utf16(before);selection=(start,stop)
        edit(1,before,after,kind,data,start,stop,selection);before=after
    parts=[full]
    for index,event in enumerate(events):
        event.update(beforeRevision=index,revision=index+1,beforeInputTrusted=True,inputTrusted=True)
        if event['kind']=='middle':parts=[e.seed,e.tail]
        else:parts[event['activeIndex']]=event['afterText']
        yield event,parts.copy()

def witness(value,e):
    require(isinstance(value,dict) and value.get('kind')=='middle-sequence'
            and all(value.get(k) is True for k in ('beforeInputTrusted','inputTrusted')) and value.get('inputType')=='insertParagraph'
            and S.plain_sections(value.get('before'))==[e.full] and type(value.get('offset')) is int and value['offset']==e.offset,
            'Pre-middle immutable source/Enter authority changed')
    require(all(isinstance(value.get(k),str) and value[k].isascii() and value[k].isdigit() for k in ('generation','visit')),'Pre-middle owner missing')
    seq=value.get('sequence',{});events=seq.get('events');expected=list(expected_events(e))
    require(isinstance(events,list) and len(events)==len(expected),'Pre-middle event history missing')
    for actual,(wanted,parts) in zip(events,expected):
        require(all(actual.get(k)==v and type(actual.get(k)) is type(v) for k,v in wanted.items()),'Pre-middle trusted range/revision/text chronology changed')
        snapshot(actual,parts)
    first=next(event for event in events if event['kind']=='middle');last=events[-1]
    require(all(value.get(k)==first[k] for k in ('revision','after','rawAfter')) and value['revision']>1
            and all(seq.get(k)==last[k] for k in ('revision','after','rawAfter','html')),'Pre-middle first Enter/final snapshot association changed')
    return seq

def validate(receipt,corpus='ascii',typing='none',parent_mode='bootstrap',pre_enter='correction'):
    e=config(corpus,typing,parent_mode,pre_enter);W.labels(receipt,e);scenario=receipt.get('scenario')
    first_stage='split' if typing=='none' else 'typed-child'
    STAGES={'success':(first_stage,'cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed'),'stale':(first_stage,'cancelled','refused','cancelled-stale'),'partial':(first_stage,'cancelled','saving','uncertain')}
    require(scenario in STAGES and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Middle scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');initial_rows=inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading'))
            and original.get('operation') is None and original.get('reservations')==0 and original.get('capacityReason') is None,'Middle initial source not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Middle original already dispatched')
    journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and cancelled['generation']!=saved['generation'] and receipt.get('duplicate')=={'ok':True,'clicks':2},'Middle reopen/duplicate Save proof missing')
    names=STAGES[scenario];stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(names),'Middle stages incomplete')
    previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};later_owners={};child=sent=first_journal=None
    for index,(stage,name) in enumerate(zip(stages,names)):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Middle stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        counts=(0,0,0,0) if index<2 else ((1,0,0,0) if scenario=='stale' else (1,1,0,0) if index==2 else (1,1,1,0) if scenario=='partial'
                else (1,1,1,1) if index<5 else (2,2,1,1) if index==5 else (2,2,2,2) if index<8 else (3,3,2,2) if index==8 else (3,3,3,3))
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],'Middle immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int and source['reservations']==int(scenario!='stale' and index>=2),'Middle ownership/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Middle preflight not original')
            inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Middle preflight changed source')
        if source['writes'] and sent is None:
            child,parent,tail,_=split_write(source['writes'][0],initial_rows[e.parent],e);sent=[parent,tail]
        accepted=scenario=='success' and index>=3
        current=inventory(source.get('fresh'),e,initial,child if accepted else None,index>=6,index>=9,scenario=='stale' and index>=2)
        inventory(source.get('visible'),e,initial,child if accepted else None,index>=6,index>=9)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),'Middle unaccepted canonical changed')
        pair_rows=None
        if source['acknowledgements']:
            pair_rows=ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,initial_rows,
                          {e.parent:e.left_wire,child:e.reports[-1]},{e.parent:2000,child:1000},scenario=='partial',e.parent)
        if accepted:
            reads=source['readbacks'];first=inventory(reads[0],e,initial,child)
            require(all(S.raw(first[text(row,1)])==S.raw(row) for row in pair_rows),'Middle independent first readback differs from ACK')
            require(source['fresh']==source['visible']==reads[-1],'Middle current/readback custody lost')
            for write_index,start,identity,content in ((1,5,child,W.rtml(e.continued)),(2,8,e.parent,W.rtml(e.parent_edited))):
                if index<start:continue
                prior_value=reads[write_index-1];prior=inventory(prior_value,e,initial,child,write_index==2)
                pre=source['preflights'][write_index]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Middle subsequent Save preflight changed')
                row=continuation(source['writes'][write_index],identity,prior[identity],content,write_index,source['writes'][1]['session'],e.parent)
                if index>start:
                    confirmed=ack(source['acknowledgements'][write_index],[row],prior,{identity:content},{identity:3000},parent_id=e.parent)[0]
                    read=inventory(reads[write_index],e,initial,child,True,write_index==2)
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),'Middle continuation readback changed other member')
        pending=name in ('saving','continued-saving','parent-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Middle pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Middle whole operation lock missing')
        else:require(source.get('operation') is None,'Unexpected middle lock')
        journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':first_journal=stage['journal']
        if uncertain:require(stage['journal']==first_journal,'Middle partial ACK lost durable whole pair')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Middle native layout overflow')
        held=name in (first_stage,'saving','refused','uncertain')
        parts=[e.seed,e.updates[-1]] if held else [e.updates[-1]] if name=='child-editing' else [e.continued] if name=='continued-saving' else [e.seed] if name=='parent-editing' else [e.parent_edited] if name=='parent-saving' else None
        if parts is not None:
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name==first_stage else saved;typed=owner['sequence']
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('sequenceProof')==typed and editor.get('typing') is None
                        and editor.get('blocks')==typed['after'] and editor.get('rawBlocks')==typed['rawAfter']
                        and editor.get('placeholder') is None,'Middle held editor lost sealed event association')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and editor.get('placeholder') is None and editor.get('typing') is None and editor.get('sequenceProof') is None,'Middle ordinary editor kept split witness')
                owner=editor.get('generation');target='child' if name in ('child-editing','continued-saving') else 'parent'
                require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Middle later editor owner/visit missing')
                if name in ('child-editing','parent-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Middle editor opening reused/nonmonotonic owner')
                    owners.add(owner);later_owners[target]=owner
                else:
                    require(owner==later_owners.get(target),'Middle saving changed its open editor owner')
        else:
            expected=[dict(id=e.parent,text=e.parent_edited if index>=9 else e.seed if accepted else e.full)]
            if accepted:expected.append(dict(id=child,text=e.continued if index>=6 else e.updates[-1]))
            expected.append(dict(id='neighbor',text='Another paragraph'))
            require(geo.get('native') is None and geo.get('paragraphs')==expected,'Middle reader content/order changed')
        if name=='cancelled':require(source==original,'Middle Cancel dispatched or mutated source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry']
            and receipt.get('finalJournal')==stages[-1]['journal'],'Middle terminal differs from final original stage')
