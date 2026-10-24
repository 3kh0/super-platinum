"""Trusted two-boundary chronology, whole-three ACK and independent later row saves."""
import math
import canvas_second_middle_wire as W
from canvas_premiddle_oracle import S,T,snapshot,require,text,ack,continuation
config=W.config

def reader_actions(controls,viewport,lineage_owner=False):
    require(isinstance(viewport,list) and len(viewport)==2 and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in viewport),'Second middle action viewport missing')
    require(isinstance(controls,list) and [c.get('action') for c in controls]==['edit','before','after','delete'],'Second middle reader action identities changed')
    for control in controls:
        absent=lineage_owner and control['action']=='delete'
        require(type(control.get('count')) is int and control['count']==(0 if absent else 1)
                and control.get('enabled') is (not absent) and control.get('visible') is (not absent),'Second middle reader action authority changed')
        if absent:require(control.get('rect') is None,'Second middle forbidden Delete retained geometry')
        else:visible_rect(control.get('rect'),viewport)

def visible_rect(rect,viewport):
    require(isinstance(rect,list) and len(rect)==4 and all(type(n) in (int,float) and math.isfinite(n) for n in rect)
            and rect[2]>0 and rect[3]>0 and rect[0]>=-1 and rect[1]>=-1
            and rect[0]+rect[2]<=viewport[0]+1 and rect[1]+rect[3]<=viewport[1]+1,'Second middle reader target/action clipped')

def reader_target(geometry,e,children,index):
    target=geometry.get('readerTarget',{});viewport=geometry.get('viewport')
    lineage_owner=e.second_middle=='child' and index==0
    require(target.get('ok') is True and target.get('id')==children[index] and target.get('text')==e.edited[index+1]
            and target.get('visible') is True and target.get('canDelete') is (not lineage_owner),'Second middle reader target identity/text/delete authority changed')
    # Only the first created child of a child-target split owns another split.
    # Its root-delete guard remains in force after ordinary edits and readback.
    reader_actions(target.get('controls'),viewport,lineage_owner)
    visible_rect(target.get('rect'),viewport)

def expected_events(e):
    parts=[e.full];order=[0];events=[]
    def split(target,left,right,child,kind):
        index=order.index(target);n=T.utf16(left);before=parts[index]
        parts[index:index+1]=[left,right];order.insert(index+1,child)
        events.append((dict(kind=kind,activeIndex=index+1,inputType='insertParagraph',data=None,selectionStart=n,selectionEnd=n,start=n,end=n,beforeText=before,afterText=right,caret=0),parts.copy()))
    def report(identity,after):
        index=order.index(identity);before=parts[index];n=T.utf16(before)
        if after==before+' draftx' or after==before+' changed':kind,data,start,end,sel='insertText',after[len(before):],n,n,(n,n)
        elif not after:kind,data,start,end,sel='deleteContentBackward',None,0,n,(0,n)
        elif before.endswith(' draftx') and after==before[:-1]:kind,data,start,end,sel='deleteContentBackward',None,n-1,n,(n,n)
        elif before.endswith(' draftx') and after==before[:-7]:kind,data,start,end,sel='deleteContentBackward',None,n-7,n,(n-7,n)
        elif before.endswith(' draft') and after==before[:-5]+'revised':kind,data,start,end,sel='insertText','revised',n-5,n,(n-5,n)
        else:raise RuntimeError('Unpinned second middle browser report')
        parts[index]=after
        events.append((dict(kind='edit',activeIndex=index,inputType=kind,data=data,selectionStart=sel[0],selectionEnd=sel[1],start=start,end=end,beforeText=before,afterText=after,caret=start+T.utf16(data or '')),parts.copy()))
    split(0,e.left,e.tail,1,'middle')
    for identity,after in e.between:report(identity,after)
    split(0 if e.second_middle=='parent' else 1,e.second_left,e.second_right,2,'second-middle')
    for identity,after in e.after:report(identity,after)
    for i,(event,parts) in enumerate(events):yield dict(event,beforeRevision=i,revision=i+1,beforeInputTrusted=True,inputTrusted=True),parts

def witness(value,e):
    require(isinstance(value,dict) and value.get('kind')=='second-middle-sequence' and value.get('inputType')=='insertParagraph'
            and all(value.get(k) is True for k in ('beforeInputTrusted','inputTrusted')) and S.plain_sections(value.get('before'))==[e.full]
            and type(value.get('offset')) is int and value['offset']==e.offset,'Second middle immutable source/authority changed')
    require(all(isinstance(value.get(k),str) and value[k].isascii() and value[k].isdigit() for k in ('generation','visit')),'Second middle owner missing')
    seq=value.get('sequence',{});events=seq.get('events');expected=list(expected_events(e))
    require(isinstance(events,list) and len(events)==len(expected),'Second middle report chronology incomplete')
    for actual,(wanted,parts) in zip(events,expected):
        require(all(actual.get(k)==v and type(actual.get(k)) is type(v) for k,v in wanted.items()),'Second middle trusted event range/identity/text/revision changed')
        snapshot(actual,parts)
    first,last=events[0],events[-1]
    require(all(value.get(k)==first[k] for k in ('revision','after','rawAfter')) and value['revision']==1
            and all(seq.get(k)==last[k] for k in ('revision','after','rawAfter','html')),'Second middle first-boundary/final association changed')
    return seq

def validate(receipt,corpus='ascii',parent_mode='bootstrap',second_middle='parent',second_middle_cadence='straight'):
    e=config(corpus,parent_mode,second_middle,second_middle_cadence);W.labels(receipt,e);scenario=receipt.get('scenario')
    names=(('second-split','cancelled','saving','confirmed')+tuple(f'member-{i}-{s}' for i in range(2) for s in ('editing','saving','confirmed'))+('parent-editing','parent-saving','parent-confirmed') if scenario=='success' else
           ('second-split','cancelled','refused','cancelled-stale') if scenario=='stale' else ('second-split','cancelled','saving','uncertain'))
    require(scenario in ('success','stale','partial') and all(receipt.get(k) is True for k in ('ok','noSession','noTransport','trustedNativeEnter'))
            and receipt.get('httpFactoryExecuted') is False,'Second middle scenario/isolation failed')
    original=receipt.get('original',{});W.labels(original,e);initial=original.get('fresh');initial_rows=W.inventory(initial,e)
    require(original.get('visible')==initial and all(original.get(k) is False for k in ('pending','stale','loading')) and original.get('operation') is None
            and original.get('reservations')==0 and original.get('capacityReason') is None,'Second middle source not ready')
    keys=('preflights','writes','acknowledgements','readbacks');require(all(original.get(k)==[] for k in keys),'Second middle original already dispatched')
    W.journal(receipt.get('initialJournal'),e)
    cancelled,saved=receipt.get('cancelledWitness'),receipt.get('savedWitness');witness(cancelled,e);witness(saved,e)
    require(cancelled['visit']==saved['visit'] and cancelled['generation']!=saved['generation'] and receipt.get('duplicate')=={'ok':True,'clicks':2},'Second middle reopen/duplicate proof missing')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(names),'Second middle stages incomplete')
    previous={k:[] for k in keys};owners={cancelled['generation'],saved['generation']};later={};children=sent=first_journal=None
    for index,(stage,name) in enumerate(zip(stages,names)):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Second middle stage association changed')
        source,geo=stage.get('fixture',{}),stage.get('geometry',{});W.labels(source,e)
        starts=(5,8,11)
        counts=(0,0,0,0) if index<2 else (1,0,0,0) if scenario=='stale' else (1,1,0,0) if index==2 else (1,1,1,0) if scenario=='partial' else (1+sum(index>=s for s in starts),)*2+(1+sum(index>s for s in starts),)*2
        for key,count in zip(keys,counts):
            values=source.get(key);require(isinstance(values,list) and len(values)==count and values[:len(previous[key])]==previous[key],'Second middle immutable ledger changed: '+key);previous[key]=values
        require(source.get('scenario')==scenario and source.get('visit')==original.get('visit') and type(source.get('read')) is int and source['read']>0
                and source.get('loading') is False and type(source.get('reservations')) is int and source['reservations']==int(scenario!='stale' and index>=2),'Second middle ownership/reservation changed')
        if source['preflights']:
            first=source['preflights'][0]
            require(first.get('source')==initial and first.get('outcome')==('Changed' if scenario=='stale' else 'prepared'),'Second middle preflight not original')
            W.inventory(first.get('fresh'),e,initial,stale=scenario=='stale')
            if scenario!='stale':require(first['fresh']==initial,'Second middle preflight changed source')
        if source['writes'] and sent is None:children,sent,_=W.write(source['writes'][0],initial_rows[e.parent],e)
        ids=[e.parent,*(children or [])];accepted=scenario=='success' and index>=3;saved_rows=tuple(i for i,s in ((1,5),(2,8),(0,11)) if index>s)
        W.inventory(source.get('fresh'),e,initial,children if accepted else None,saved_rows,scenario=='stale' and index>=2)
        W.inventory(source.get('visible'),e,initial,children if accepted else None,saved_rows)
        if not accepted:require(source['visible']==initial and source['fresh']==(source['preflights'][0]['fresh'] if scenario=='stale' and index>=2 else initial),'Second middle unaccepted canonical changed')
        ack_rows=None
        if source['acknowledgements']:
            ack_rows=ack(source['acknowledgements'][0],sent[:1] if scenario=='partial' else sent,initial_rows,{ids[i]:W.rtml(e.final[i]) for i in range(3)},
                         {ids[i]:2000 if i==0 else 1000 for i in range(3)},scenario=='partial',e.parent)
        if accepted:
            reads=source['readbacks'];first=W.inventory(reads[0],e,initial,children)
            require(all(S.raw(first[text(row,1)])==S.raw(row) for row in ack_rows) and source['fresh']==source['visible']==reads[-1],'Second middle whole independent readback differs')
            for j,(identity_index,start) in enumerate(((1,5),(2,8),(0,11)),1):
                if index<start:continue
                prior_saved=tuple([1,2,0][:j-1]);prior_value=reads[j-1];prior=W.inventory(prior_value,e,initial,children,prior_saved);pre=source['preflights'][j]
                require(pre.get('source')==pre.get('fresh')==prior_value and pre.get('outcome')=='prepared','Second middle later preflight changed')
                identity=ids[identity_index];content=W.rtml(e.edited[identity_index]);row=continuation(source['writes'][j],identity,prior[identity],content,j,source['writes'][1]['session'],e.parent)
                if index>start:
                    confirmed=ack(source['acknowledgements'][j],[row],prior,{identity:content},{identity:3000},parent_id=e.parent)[0]
                    read=W.inventory(reads[j],e,initial,children,tuple([1,2,0][:j]))
                    require(S.raw(read[identity])==S.raw(confirmed) and all(S.raw(read[k])==S.raw(v) for k,v in prior.items() if k!=identity),'Second middle later Save changed another row')
        pending=name=='saving' or name.endswith('-saving');uncertain=name=='uncertain'
        require(source.get('pending') is pending and source.get('stale') is (pending or uncertain),'Second middle pending state changed')
        if pending or uncertain:require((source.get('operation') or {}).get('stage')==('Uncertain' if uncertain else 'Saving'),'Second middle whole lock missing')
        else:require(source.get('operation') is None,'Unexpected second middle lock')
        W.journal(stage.get('journal'),e,source['writes'][0] if name=='saving' or uncertain else None,initial)
        if name=='saving':first_journal=stage['journal']
        if uncertain:require(stage['journal']==first_journal,'Second middle incomplete ACK lost whole journal')
        require(geo.get('ok') is True and geo.get('noOverflow') is True,'Second middle native layout overflow')
        held=name in ('second-split','saving','refused','uncertain');editing=name.endswith('-editing') or name.endswith('-saving')
        if held or editing:
            row_index=0 if name.startswith('parent-') else int(name.split('-')[1])+1 if name.startswith('member-') else None
            parts=[e.final[i] for i in e.visual] if held else [e.edited[row_index] if pending else e.final[row_index]]
            editor=geo.get('native');S.native(editor,parts,not(pending or uncertain),S.ERROR if name=='refused' else None)
            if held:
                owner=cancelled if name=='second-split' else saved;seq=owner['sequence']
                require(all(editor.get(k)==owner[k] for k in ('generation','visit')) and editor.get('sequenceProof')==seq and editor.get('typing') is None
                        and editor.get('blocks')==seq['after'] and editor.get('rawBlocks')==seq['rawAfter'] and editor.get('placeholder') is None,'Second middle held editor lost sealed sequence')
            else:
                require(editor.get('rawBlocks')==editor.get('blocks') and all(editor.get(k) is None for k in ('placeholder','typing','sequenceProof')),'Second middle ordinary editor retained split proof')
                owner=editor.get('generation');require(isinstance(owner,str) and owner.isascii() and owner.isdigit() and editor.get('visit')==saved['visit'],'Second middle later owner missing')
                if name.endswith('-editing'):
                    require(owner not in owners and int(owner)>max(map(int,owners)),'Second middle editor reused owner');owners.add(owner);later[row_index]=owner
                else:require(owner==later.get(row_index),'Second middle saving changed editor owner')
        else:
            expected=([dict(id=ids[i],text=e.edited[i] if i in saved_rows else e.final[i]) for i in e.visual] if accepted else [dict(id=e.parent,text=e.full)])+[dict(id='neighbor',text='Another paragraph')]
            require(geo.get('native') is None and geo.get('paragraphs')==expected,'Second middle reader identity/visual order changed')
            if name in ('member-0-confirmed','member-1-confirmed'):reader_target(geo,e,children,int(name.split('-')[1]))
        if name=='cancelled':require(source==original,'Second middle Cancel dispatched or mutated source')
    require(receipt.get('fixture')==stages[-1]['fixture'] and receipt.get('layout')==stages[-1]['geometry'] and receipt.get('finalJournal')==stages[-1]['journal'],'Second middle terminal differs from final stage')
