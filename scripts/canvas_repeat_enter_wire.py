"""Two owned End events: exact three-row STATUS/creation wire and v12 custody."""
import base64
from dataclasses import dataclass
import hashlib
import re

import canvas_empty_enter_wire as E
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text

HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following','empty-final-following','empty-nonfinal-following')
DIGESTS={
    ('retained-positive','ascii'):'1e05597231c882d2bca70244da0c6d019e521bbc770c260efcd92d816d7eeb2c',
    ('retained-positive','unicode'):'26f22178e92ed9370ef542abcba9877615cc8411f26ae8365dc3779b272079d3',
    ('retained-zero','ascii'):'272deef0a85fdc897c3cffbf17f7b5d9e3f7dcb63e6d8163c3085a725b2ce747',
    ('retained-zero','unicode'):'e26bb85ced8b5e829e1de0d96d08b1d10a5486f071c8c46f0a810ad79c4040b7',
    ('retained-positive','empty'):'081547cf2f0764cd2fca39c4a30b2a7dd85531f97ec8a607cc276cd2b032cdf7',
    ('retained-zero','empty'):'44ad2f4407d16fd5fda38b06dee5e8ea1df5a0e627ef662fdf6e76d2c61901ea',
}
# Digests exclude only host identity and the independently supplied offline lease.


@dataclass(frozen=True)
class Config:
    history:str
    layout:str
    corpus:str
    original:str
    following:str
    reused:bool
    seed:str
    second_edited:str='Second paragraph edited'
    continued:str='Continued paragraph'

    @property
    def wire_seed(self):return M.rtml(self.seed)
    @property
    def edited(self):return self.seed+' edited'
    @property
    def stale_id(self):return self.following if self.reused else self.original
    @property
    def neighbor(self):return 'Following paragraph' if self.layout=='text-following' else ''
    @property
    def ids(self):return ['title',self.original]+([] if self.layout=='no-following' else [self.following])+(['last-text'] if self.layout=='empty-nonfinal-following' else [])


def config(repeat_history='bootstrap',repeat_layout='no-following',corpus='ascii'):
    require(repeat_history in HISTORIES and repeat_layout in LAYOUTS and corpus in ('ascii','unicode'),'Unpinned repeat cohort')
    zero=repeat_history=='retained-zero'
    return Config(repeat_history,repeat_layout,corpus,'temp:C:Dte0000000000000000000000001' if zero else 'paragraph',
                  'temp:C:Dte0000000000000000000000000' if zero else 'following',repeat_layout=='empty-final-following',
                  'Keep this' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600')


def labels(value,e,recovery=False):
    require(value.get('splitMode' if recovery else 'mode')=='repeat' and value.get('repeatHistory')==e.history
            and value.get('repeatLayout')==e.layout and value.get('corpus')==e.corpus and value.get('typing')=='none',
            'Repeat caller selection changed')
    require(value.get('parentMode')=='bootstrap' and value.get('reuseBase')=='positive' and value.get('reuseContent')=='empty'
            and value.get('reuseChild')==(e.following if e.reused else None),'Repeat acquired unrelated controls')


def source_session(row,e):
    sessions=S.messages(row,38)
    if e.history=='bootstrap':require(not sessions,'Repeat bootstrap source acquired retained history');return
    require(len(sessions)==1,'Repeat source Session missing');session=sessions[0]
    require(text(session,1)==text(row,1) and field(session,4,0)==8_000_000_000_000_000 and sum(f[0]==4 for f in session)==1,
            'Repeat source identity/lease changed')
    raw=b''.join(f[3] for f in sorted(session,key=lambda f:f[0]) if f[0] not in (1,4))
    key=e.corpus if text(row,1)==e.original else 'empty'
    require(hashlib.sha256(raw).hexdigest()==DIGESTS[e.history,key],'Repeat exact source history/base/heads changed')


def ordered_ids(e,selected=None):
    ids=e.ids.copy()
    if selected:
        first,second=selected
        if e.reused:ids.insert(ids.index(first)+1,second)
        else:ids[2:2]=[first,second]
    return ids


def contents(e,selected=None,completed=0):
    values={e.original:e.seed,e.following:e.neighbor,'last-text':'Last paragraph'}
    if selected:
        first,second=selected;values[first]=e.continued if completed>=2 else '';values[second]=e.second_edited if completed>=1 else ''
        if completed>=3:values[e.original]=e.edited
    return values


def inventory(value,e,original=None,selected=None,completed=0,stale=False):
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document'))==('Ffixture','thread','document'),'Repeat inventory owner changed')
    tr,doc=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(tr,1)=='thread' and text(tr,10)=='document' and text(doc,1)=='document' and text(doc,6)=='thread'
            and text(doc,42)=='xyz' and text(wire(field(doc,69,2)),1)=='Fixture title','Repeat document/title changed')
    items=value.get('sections');require(isinstance(items,list) and [r.get('id') for r in items]==ordered_ids(e,selected),'Repeat count/identity/order changed')
    expected=contents(e,selected,completed);rows={};positions=[]
    for item in items:
        row=decoded(item.get('raw'));identity=item['id'];rows[identity]=row;position=text(row,8);positions.append(position)
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and re.fullmatch('[A-Za-z]{1,256}',position)
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0),'Repeat raw row association changed')
        fixed={'title':'aaa',e.original:'aac',e.following:'aae','last-text':'aag'}
        if identity in fixed:require(position==fixed[identity],'Repeat original canonical position changed')
        if identity=='title':
            require(field(row,9,0)==64 and field(row,10,0)==48 and field(row,2,0)==1000
                    and text(wire(field(wire(field(row,12,2)),58,2)),1)=='Fixture title','Repeat title changed');continue
        sequence=1000
        if selected:
            first,second=selected
            if identity==e.original or e.reused and identity==first:sequence=2000
            if (completed>=1 and identity==second) or (completed>=2 and identity==first) or (completed>=3 and identity==e.original):sequence=3000
        if stale and identity==e.stale_id:sequence=3000
        require(S.body(row,identity==e.original)==M.rtml(expected[identity]) and field(row,2,0)==sequence,'Repeat body/language/revision changed')
        for tag in (3,9,10,35):
            if any(f[0]==tag for f in row):require(field(row,tag,0)==0,'Repeat nonordinary member')
        if any(f[0]==13 for f in row):require(field(row,13,2)==b'','Repeat structural parent appeared')
        if selected is None and (identity==e.original or identity==e.following and e.reused):source_session(row,e)
        if identity=='last-text' or identity==e.following and not e.reused:
            require(not S.messages(row,38),'Repeat untouched neighbor acquired history')
    require(all(a<b for a,b in zip(positions,positions[1:])),'Repeat canonical positions not strictly ordered')
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Repeat raw document changed')
        before={r['id']:r for r in original['sections']}
        for item in items:
            identity=item['id']
            if selected and identity in (*selected,e.original):continue
            if stale and identity==e.stale_id:require(S.stable(decoded(item['raw']),(2,))==S.stable(decoded(before[identity]['raw']),(2,)),'Repeat stale change not revision-only')
            else:require(item==before[identity],'Repeat untouched raw inventory changed')
    return rows


def status(row,source,value,e):
    identity=text(source,1)
    require(T.tags(row)==[1,2,13,19,20,33,38] and text(row,1)==identity and field(row,2,0)==1000
            and field(row,13,2)==b'' and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0','Repeat mandatory STATUS changed')
    now=wire(field(row,38,2));prior=S.messages(source,38)
    if prior:
        require(S.stable(now)==S.stable(prior[0],(4,7,12)) and not any(f[0] in (4,7,10,12) for f in now),'Repeat STATUS mutated exact source history/heads')
    elif identity!=e.original:E.bootstrap(now,identity,1000,value['session']+'00000000',False)
    else:
        require(T.tags(now)==[1,2,3,5,6,8] and text(now,1)==identity and field(now,2,0)==field(now,3,0)==1000
                and text(now,6)==text(now,8),'Repeat initialized original metadata changed')
        history=T.decode_history(field(now,5,2))[0];changes=history['changes'];require(len(changes)==1,'Repeat original bootstrap contains edits')
        first=changes[0];actor=value['session']+'00000000'
        require(first['actor']==actor and first['seq']==1 and first['deps']==[] and first['keys']==['deleted','text']
                and first['deleted'] is False and first['marks']==[] and first['text']==e.wire_seed+'\ufeff'
                and history['heads']==[first['hash']]==[text(now,8)],'Repeat bootstrap reset original content/head')
        require(first['ops'][0]==dict(action='makeText',obj='_root',key='text',pred=[])
                and first['ops'][-1]==dict(action='set',obj='_root',key='deleted',value=False,pred=[])
                and all(op.get('action')=='set' and op.get('obj')=='1@'+actor and op.get('insert') is True for op in first['ops'][1:-1]),
                'Repeat original bootstrap hidden mutation')


def creation(row,identity,value):
    position=text(row,8)
    require(T.tags(row)==[1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38] and text(row,1)==identity
            and field(row,2,0)==0 and text(row,21)==position and re.fullmatch('[A-Za-z]{1,256}:temp',position)
            and S.body(row,False)=='' and field(row,13,2)==b'' and field(row,16,2)==b'\x20\x00'
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0'
            and all(field(row,tag,0)==0 for tag in (3,9,10,11,29,35)),'Repeat new creation changed')
    E.bootstrap(wire(field(row,38,2)),identity,0,value['session']+'00000000',True)


def write(value,original,e):
    require(value.get('kind')=='repeat' and value.get('parent')==e.original and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and value.get('clearsChild') is False and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Repeat request kind/owner changed')
    first,second=value.get('child'),value.get('secondChild')
    require(first==e.following if e.reused else isinstance(first,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',first) and first not in original,
            'Repeat first selection violated final-empty reuse')
    require(isinstance(second,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',second) and second not in original and second!=first,'Repeat second creation identity merged/reused')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,1,1,14],'Repeat Save is not one complete three-row request');S.rich_title(data)
    parent,one,two=S.messages(data,1);status(parent,original[e.original],value,e)
    if e.reused:status(one,original[first],value,e)
    else:creation(one,first,value)
    creation(two,second,value)
    require([text(row,1) for row in (parent,one,two)]==[e.original,first,second],'Repeat collected row order changed')
    positions={identity:text(row,8) for identity,row in original.items()}
    if not e.reused:positions[first]=text(one,8).removesuffix(':temp')
    positions[second]=text(two,8).removesuffix(':temp')
    order=ordered_ids(e,(first,second));require(all(positions[a]<positions[b] for a,b in zip(order,order[1:])),'Repeat allocated relative ordering changed')
    return (first,second),parent,one,two


def journal(value,e,write_value=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Repeat journal scope changed')
    if write_value is None:require(value['receipt'] is None,'Unexpected retained repeat');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and set(receipt)=={'operation','checkpoint'} and isinstance(receipt.get('operation'),str) and receipt['operation'],'Repeat operation missing')
    c=receipt['checkpoint'];require(isinstance(c,dict) and set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'},'Repeat checkpoint fields changed')
    outgoing=S.messages(decoded(write_value['documentData']),1)
    position='aae' if e.reused else text(outgoing[1],8)
    require(type(c['version']) is int and c['version']==12 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.original and c['session']==write_value['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']==e.wire_seed and c['action']=={'EndRepeat':dict(child=write_value['child'],position=position,reused=e.reused,
                second_child=write_value['secondChild'],second_position=text(outgoing[2],8))}
            and type(c['action']['EndRepeat']['reused']) is bool,'Repeat durable three-row intent changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write_value['documentData'],validate=True),'Repeat durable request differs from dispatched triple')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};sections=c['sections']
    require(isinstance(sections,list) and len(sections)==len(expected)
            and {text(wire(S.byte_vector(row)),1):S.byte_vector(row) for row in sections}==expected,'Repeat whole original inventory lost')


def ack(value,sent,source,contents,sequences,e,partial=False):
    require(value.get('partial') is partial and value.get('accepted') is (not partial),'Repeat ACK admission changed')
    outer=decoded(value.get('raw'));require(T.tags(outer)==[1,2] and field(outer,1,0)==1,'Repeat ACK malformed')
    data=wire(field(outer,2,2));require(T.tags(data)==[1]*len(sent)+[15] and text(data,15)=='document','Repeat ACK incomplete/duplicated/wrong owner')
    rows=S.messages(data,1)
    for row,outgoing in zip(rows,sent):
        identity=text(outgoing,1);sequence=sequences[identity]
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and S.body(row,identity==e.original)==contents[identity]
                and field(row,2,0)==sequence and all(field(row,k,0)==field(outgoing,k,0) for k in (19,20)) and text(row,33)==text(outgoing,33),
                'Repeat ACK body/owner/revision/status changed')
        now,prior=wire(field(row,38,2)),wire(field(outgoing,38,2))
        require(field(now,3,0)==sequence and field(now,4,0)>0 and not any(f[0]==12 for f in now)
                and S.stable(now,(3,4,12))==S.stable(prior,(3,4,12)),'Repeat ACK changed history/base/processed metadata')
        if identity in source:
            require(S.stable(row,(2,9,12,13,19,20,33,38))==S.stable(source[identity],(2,9,12,13,19,20,33,38)),'Repeat ACK replaced opaque existing row')
        else:
            require(text(row,8)==text(row,21)==text(outgoing,8).removesuffix(':temp')
                    and S.stable(row,(2,6,7,8,21,38))==S.stable(outgoing,(2,6,7,8,21,38)),'Repeat new ACK lost creation attributes')
    return rows


continuation=E.continuation
