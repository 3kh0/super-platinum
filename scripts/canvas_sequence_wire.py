"""Finite ordinary paragraph sequence: independent source, histories and v14 wire."""
import base64
from dataclasses import dataclass
import re

import canvas_repeat_enter_wire as A
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text

CADENCES=('straight','correction-before-enter','erase-before-enter','revert-before-enter','trailing-empty')


@dataclass(frozen=True)
class Config(A.Config):
    append_count:int=1
    cadence:str='straight'
    updates:tuple=()
    reports:tuple=()
    target_index:int=0

    @property
    def base(self):return 0 if self.history=='retained-zero' else 1000
    @property
    def final(self):return tuple(u[-1] if u else '' for u in self.updates)


def config(append_count=1,history='bootstrap',layout='no-following',cadence='straight',corpus='ascii'):
    old=A.config(history,layout,corpus)
    # Finite fixture selection, not a claim about the product's resource cap.
    require(type(append_count) is int and 1<=append_count<=16 and cadence in CADENCES,'Unpinned sequence count/cadence')
    require(cadence not in CADENCES[1:4] or append_count>=2,'Before-Enter cadence needs later Enter')
    require(cadence not in CADENCES[1:4] or append_count<=3 or append_count in (8,16),'Unpinned late cadence cohort')
    target=(5 if append_count==8 else 14 if append_count==16 else 0) if cadence in CADENCES[1:4] else 0
    suffix='' if corpus=='ascii' else ' \u65e5\u672c\U0001f600 &'
    prefixes=[(('New','Next','Last')[i] if i<3 else f'Row {i+1}')+suffix for i in range(append_count)]
    updates=[(prefix+' paragraph',) for prefix in prefixes]
    if cadence=='trailing-empty':updates[-1]=()
    elif cadence in CADENCES[1:4]:
        prefix=prefixes[target]
        updates[target]=((prefix,prefix+' draftx',prefix+' draft',prefix+' paragraph') if cadence=='correction-before-enter'
                         else (prefix,'') if cadence=='erase-before-enter' else (prefix,prefix+' draftx',prefix,''))
    return Config(**vars(old),append_count=append_count,cadence=cadence,updates=tuple(updates),reports=tuple(tuple(map(M.rtml,u)) for u in updates),target_index=target)


def labels(value,e,recovery=False):
    require(value.get('splitMode' if recovery else 'mode')=='sequence' and value.get('sequenceHistory')==e.history
            and value.get('sequenceLayout')==e.layout and value.get('sequenceCadence')==e.cadence
            and type(value.get('sequenceCount')) is int and value['sequenceCount']==e.append_count
            and value.get('corpus')==e.corpus and value.get('typing')=='none','Sequence caller selection changed')
    require(value.get('parentMode')=='bootstrap' and value.get('reuseBase')=='positive' and value.get('reuseContent')=='empty'
            and value.get('reuseChild')==(e.following if e.reused else None),'Sequence acquired unrelated controls')


def ordered_ids(e,selected=None):
    ids=e.ids.copy()
    if selected:
        require(len(selected)==e.append_count and len(set(selected))==len(selected),'Sequence member count/identity merged')
        if e.reused:ids[3:3]=list(selected[1:])
        else:ids[2:2]=list(selected)
    return ids


def later(e,selected):
    # One independent ordinary Save for each selected row, from last to first,
    # followed by the original. Desktop fixture pins these explicit contents.
    return [(identity,f'Appended paragraph {index+1} edited') for index,identity in reversed(list(enumerate(selected)))]+[(e.original,e.edited)]


def contents(e,selected=None,completed=0):
    values={e.original:e.seed,e.following:e.neighbor,'last-text':'Last paragraph'}
    if selected:
        values.update(zip(selected,e.final));values.update(later(e,selected)[:completed])
    return values


def inventory(value,e,original=None,selected=None,completed=0,stale=False):
    if selected is None:return A.inventory(value,e,original,None,0,stale)
    require(isinstance(value,dict) and all(value.get(k)==original[k] for k in original if k!='sections'),'Sequence raw document changed')
    items=value.get('sections');require(isinstance(items,list) and [r.get('id') for r in items]==ordered_ids(e,selected),'Sequence inventory order/count changed')
    before={r['id']:r for r in original['sections']};values=contents(e,selected,completed);edited={k for k,_ in later(e,selected)[:completed]};rows={};positions=[]
    for item in items:
        identity=item['id'];row=decoded(item.get('raw'));rows[identity]=row;positions.append(text(row,8))
        if identity not in (*selected,e.original):require(item==before[identity],'Sequence untouched original row changed');continue
        sequence=3000 if identity in edited else 2000 if identity==e.original or e.reused and identity==selected[0] else 1000
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document'
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0)==sequence
                and S.body(row,identity==e.original)==M.rtml(values[identity]),'Sequence canonical body/owner/revision changed')
        require(re.fullmatch('[A-Za-z]{1,256}',text(row,8)),'Sequence canonical position malformed')
        if identity in before:require(text(row,8)==text(decoded(before[identity]['raw']),8),'Sequence existing position changed')
        for tag in (3,9,10,35):
            if any(f[0]==tag for f in row):require(field(row,tag,0)==0,'Sequence member became nonordinary')
        if any(f[0]==13 for f in row):require(field(row,13,2)==b'','Sequence member acquired structural parent')
    require(all(a<b for a,b in zip(positions,positions[1:])),'Sequence canonical ordering changed')
    return rows


def patch_groups(e,index):
    reports=e.reports[index]
    if not reports:return []
    if index==e.target_index and e.cadence in CADENCES[1:4]:
        # Original live-patch order; index units are serialized RTML UTF-16.
        # Expected prefix comes from caller-pinned literals, never sent content.
        p=M.patch;n=T.utf16(reports[0]);result=[[p('splice',0,reports[0])]]
        if e.cadence=='erase-before-enter':return result+[[p('del',0,length=n)]]
        result.append([p('splice',n,' draftx')])
        if e.cadence=='revert-before-enter':return result+[[p('del',n,length=7)],[p('del',0,length=n)]]
        return result+[[p('del',n+6)],[p('splice',n+1,'parag'),p('del',n+6),p('del',n+8,length=2),p('splice',n+8,'ph')]]
    return [[M.patch('splice',0,reports[0])]]


def member_history(row,source,e,index,session):
    now=wire(field(row,38,2));identity=text(row,1);actor=session+'00000000';reused=e.reused and index==0
    require(text(now,1)==identity and field(now,2,0)==(e.base if reused else 0) and field(now,3,0)==(1000 if reused else 0)
            and all(f[0] in (1,2,3,5,6,8,11,12) for f in now)
            and all(sum(f[0]==k for f in now)==1 for k in (1,2,3,5,8,12)),'Sequence typed history metadata changed')
    before=S.messages(source,38) if source else []
    if before:
        A.source_session(source,e)
        require(S.stable(now,(5,6,12))==S.stable(before[0],(4,5,6,7,12)),'Sequence retained metadata reset')
        current,prior=T.decode_history(field(now,5,2),field(before[0],5,2));prefix=prior['changes'];heads=[f[2] for f in before[0] if f[0]==6]
    else:
        require(T.tags(now)==([1,2,3,5,6,6,6,8,12] if reused else [1,2,3,5,6,8,12]),'Sequence new/bootstrap session shape changed')
        current=T.decode_history(field(now,5,2))[0];prefix=current['changes'][:1];M.chain(prefix,[],actor)
        require(prefix[0]['text']=='\ufeff' and prefix[0]['hash']==text(now,8),'Sequence bootstrap not genuine empty')
        heads=[prefix[0]['hash'].encode()] if reused else []
    M.chain(current['changes'],prefix,actor);changes=current['changes'][len(prefix):]
    groups=patch_groups(e,index);patches=[p for g in groups for p in g];state='\ufeff';states=[]
    for p in patches:state=M.apply_patch(state,p);states.append(state)
    require([c['text'] for c in changes]==states and [c['patches'] for c in changes]==[[p] for p in patches],
            'Sequence lost, squashed, reordered or fabricated intermediate mutations')
    cursor=0
    for group,report in zip(groups,e.reports[index]):
        cursor+=len(group);require(changes[cursor-1]['text']==report+'\ufeff','Sequence report endpoint changed')
    final=field(now,12,2)
    require(current['heads']==[text(now,12)] and [f[2] for f in now if f[0]==6]==(heads+[final]+heads if reused else [final]),'Sequence final processed-head order changed')


def member(row,identity,source,value,e,index):
    reused=e.reused and index==0;reports=e.reports[index]
    if not reports:
        if reused:A.status(row,source,value,e)
        else:A.creation(row,identity,value)
        return
    expected=[1,2,9,12,13,19,20,33,38] if reused else [1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38]
    require(T.tags(row)==expected and text(row,1)==identity and field(row,2,0)==(1000 if reused else 0)
            and S.body(row,False)==reports[-1] and field(row,9,0)==0 and field(row,13,2)==b''
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0','Sequence member CONTENT/trace changed')
    if not reused:require(text(row,8)==text(row,21) and re.fullmatch('[A-Za-z]{1,256}:temp',text(row,8))
            and field(row,16,2)==b'\x20\x00' and all(field(row,k,0)==0 for k in (3,10,11,29,35)),'Sequence full creation changed')
    member_history(row,source,e,index,value['session'])


def write(value,original,e):
    require(value.get('kind')=='sequence' and value.get('parent')==e.original and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and value.get('clearsChild') is False and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Sequence request owner/session changed')
    members=value.get('members');require(isinstance(members,list) and len(members)==e.append_count and all(isinstance(m,dict) for m in members),'Sequence member count/shape')
    selected=[m.get('id') for m in members];require(all(isinstance(k,str) for k in selected) and len(set(selected))==len(selected),'Sequence duplicate identity')
    for i,identity in enumerate(selected):
        require(identity==e.following if e.reused and i==0 else isinstance(identity,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',identity) and identity not in original,'Sequence selected identity changed')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1]*(e.append_count+1)+[14],'Sequence Save lost complete ordered operation');S.rich_title(data)
    rows=S.messages(data,1);require([text(r,1) for r in rows]==[e.original]+selected,'Sequence request member order changed')
    A.status(rows[0],original[e.original],value,e)
    for i,(identity,row) in enumerate(zip(selected,rows[1:])):
        reused=e.reused and i==0
        require(members[i]==dict(id=identity,position='aae' if reused else text(row,8),reused=reused,updates=list(e.reports[i]),finalRtml=M.rtml(e.final[i]))
                and type(members[i]['reused']) is bool,'Sequence ledger member differs from exact intended reports')
        member(row,identity,original.get(identity),value,e,i)
    positions={k:text(r,8) for k,r in original.items()}
    positions.update({identity:text(row,8).removesuffix(':temp') for identity,row in zip(selected,rows[1:]) if identity not in original})
    ids=ordered_ids(e,selected);require(all(positions[a]<positions[b] for a,b in zip(ids,ids[1:])),'Sequence allocated relative ordering changed')
    return tuple(selected),rows


def journal(value,e,write_value=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Sequence journal scope changed')
    if write_value is None:require(value['receipt'] is None,'Unexpected retained sequence');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and set(receipt)=={'operation','checkpoint'} and isinstance(receipt['operation'],str) and receipt['operation'],'Sequence operation missing')
    c=receipt['checkpoint'];require(isinstance(c,dict) and set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'},'Sequence checkpoint fields changed')
    rows=S.messages(decoded(write_value['documentData']),1)[1:]
    members=[dict(id=m['id'],position=m['position'],reused=m['reused'],updates=m['updates']) for m in write_value['members']]
    require(type(c['version']) is int and c['version']==14 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.original and c['session']==write_value['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']==e.wire_seed and c['action']=={'EndSequence':{'members':members}}
            and all(type(m.get('reused')) is bool for m in c['action']['EndSequence']['members']),'Sequence durable ordered members/reports changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write_value['documentData'],validate=True),'Sequence retained request changed')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};sections=c['sections']
    require(isinstance(sections,list) and len(sections)==len(expected) and {text(wire(S.byte_vector(r)),1):S.byte_vector(r) for r in sections}==expected,'Sequence whole original inventory lost')


ack=A.ack
continuation=A.continuation
