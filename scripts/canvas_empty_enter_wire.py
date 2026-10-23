"""Canonical-empty Enter: two independently checked rows and unchanged histories."""
import base64
from dataclasses import dataclass
import hashlib
import re

import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text

HISTORIES=('bootstrap','retained-positive','retained-zero')
LAYOUTS=('no-following','text-following-ascii','text-following-unicode','empty-final-following','empty-nonfinal-following')
SOURCE_DIGESTS={'retained-positive':'1036cf46e3c85bd76d5e25b78347cd629bf6f8c781a8c2374ca0f909a33392e7',
                'retained-zero':'44ad2f4407d16fd5fda38b06dee5e8ea1df5a0e627ef662fdf6e76d2c61901ea'}


@dataclass(frozen=True)
class Config:
    history: str
    layout: str
    original: str
    following: str
    reused: bool
    continued: str='Continued paragraph'
    edited: str='Original paragraph edited'

    @property
    def base(self):return 0 if self.history=='retained-zero' else 1000

    @property
    def stale_id(self):return self.following if self.reused else self.original

    @property
    def neighbor(self):
        if self.layout=='text-following-ascii':return 'Another paragraph'
        if self.layout=='text-following-unicode':return 'Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph'
        return ''

    @property
    def ids(self):
        return ['title',self.original]+([] if self.layout=='no-following' else [self.following])+(['last-text'] if self.layout=='empty-nonfinal-following' else [])


def config(empty_history='bootstrap',empty_layout='no-following'):
    require(empty_history in HISTORIES and empty_layout in LAYOUTS,'Unpinned canonical-empty cohort')
    zero=empty_history=='retained-zero'
    return Config(empty_history,empty_layout,'temp:C:Dte0000000000000000000000001' if zero else 'paragraph',
                  'temp:C:Dte0000000000000000000000000' if zero else 'following',empty_layout=='empty-final-following')


def labels(value,e,recovery=False):
    require(value.get('splitMode' if recovery else 'mode')=='empty' and value.get('emptyHistory')==e.history
            and value.get('emptyLayout')==e.layout and value.get('typing')=='none','Empty Enter caller selection changed')
    require(value.get('corpus')=='ascii' and value.get('parentMode')=='bootstrap' and value.get('reuseBase')=='positive'
            and value.get('reuseContent')=='empty' and value.get('reuseChild')==(e.following if e.reused else None),
            'Empty Enter acquired unrelated corpus/typing/reuse controls')


def source_session(row,e):
    sessions=S.messages(row,38)
    if e.history=='bootstrap':require(not sessions,'Empty bootstrap source acquired retained history');return
    require(len(sessions)==1,'Empty retained source lost Session');session=sessions[0]
    require(text(session,1)==text(row,1) and field(session,4,0)==8_000_000_000_000_000 and sum(f[0]==4 for f in session)==1,
            'Empty retained source identity/lease changed')
    raw=b''.join(f[3] for f in sorted(session,key=lambda f:f[0]) if f[0] not in (1,4))
    require(hashlib.sha256(raw).hexdigest()==SOURCE_DIGESTS[e.history],'Empty source history/base/heads changed')


def body(row,identity,e):return S.body(row,identity==e.original)


def inventory(value,e,original=None,selected=None,selected_saved=False,original_saved=False,stale=False):
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document'))==('Ffixture','thread','document'),'Empty inventory owner changed')
    tr,doc=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(tr,1)=='thread' and text(tr,10)=='document' and text(doc,1)=='document' and text(doc,6)=='thread'
            and text(doc,42)=='xyz' and text(wire(field(doc,69,2)),1)=='Fixture title','Empty document/title changed')
    ids=e.ids.copy()
    if selected and not e.reused:ids.insert(2,selected)
    items=value.get('sections');require(isinstance(items,list) and [r.get('id') for r in items]==ids,'Empty inventory identity/count/order changed')
    rows={}
    for item in items:
        row=decoded(item.get('raw'));identity=item['id'];rows[identity]=row
        position='aaa' if identity=='title' else 'aac' if identity==e.original else 'aae' if identity==e.following else 'aag' if identity=='last-text' else 'aad'
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and text(row,8)==position
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0),'Empty row/raw owner/position/revision association changed')
        if identity=='title':
            require(field(row,9,0)==64 and field(row,10,0)==48 and field(row,2,0)==1000
                    and text(wire(field(wire(field(row,12,2)),58,2)),1)=='Fixture title','Empty title changed');continue
        content=e.edited if original_saved and identity==e.original else e.continued if selected_saved and identity==selected else e.neighbor if identity==e.following else 'Last paragraph' if identity=='last-text' else ''
        sequence=3000 if (original_saved and identity==e.original) or (selected_saved and identity==selected) or (stale and identity==e.stale_id) else 2000 if selected and (identity==e.original or e.reused and identity==selected) else 1000
        require(body(row,identity,e)==M.rtml(content) and field(row,2,0)==sequence,'Empty canonical body/language/revision changed')
        for tag in (3,9,10,35):
            if any(f[0]==tag for f in row):require(field(row,tag,0)==0,'Empty member became styled/locked/nonordinary')
        if any(f[0]==13 for f in row):require(field(row,13,2)==b'','Empty member acquired structural parent')
        if selected is None and (identity==e.original or identity==e.following and e.layout.startswith('empty-')):source_session(row,e)
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Empty raw document custody changed')
        before={r['id']:r for r in original['sections']}
        for item in items:
            identity=item['id']
            if selected and identity in (selected,e.original):continue
            if stale and identity==e.stale_id:
                require(S.stable(decoded(item['raw']),(2,))==S.stable(decoded(before[identity]['raw']),(2,)),'Empty stale source changed beyond revision')
            else:require(item==before[identity],'Empty unrelated raw inventory changed')
    return rows


def bootstrap(session,identity,base,actor,current):
    require(T.tags(session)==([1,2,3,5,6,8,12] if current else [1,2,3,5,6,8]) and text(session,1)==identity
            and field(session,2,0)==field(session,3,0)==base and text(session,6)==text(session,8), 'Empty bootstrap session metadata changed')
    if current:require(text(session,12)==text(session,8),'Empty new current head differs from bootstrap')
    history=T.decode_history(field(session,5,2))[0];changes=history['changes'];M.chain(changes,[],actor)
    require(len(changes)==1 and changes[0]['text']=='\ufeff' and changes[0]['hash']==text(session,8)
            and history['heads']==[text(session,8)],'Empty bootstrap fabricated content/change/history')


def status(row,source,value,e):
    identity=text(source,1)
    require(T.tags(row)==[1,2,13,19,20,33,38] and text(row,1)==identity and field(row,2,0)==1000
            and field(row,13,2)==b'' and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0',
            'Empty existing member is not exact mandatory STATUS')
    now=wire(field(row,38,2));before=S.messages(source,38)
    if before:
        require(S.stable(now)==S.stable(before[0],(4,7,12)) and not any(f[0] in (4,7,10,12) for f in now),
                'Empty STATUS mutated source history/heads/base or fabricated lineage')
    else:bootstrap(now,identity,1000,value['session']+'00000000',False)


def write(value,original,e):
    require(value.get('kind')=='empty' and value.get('parent')==e.original and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and value.get('clearsChild') is False and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Empty request kind/owner changed')
    selected=value.get('child')
    require(selected==e.following if e.reused else isinstance(selected,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',selected) and selected not in original,
            'Empty selected identity contradicts exact final-empty reuse policy')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,1,14],'Empty Enter must send one complete original+selected pair');S.rich_title(data)
    parent,child=S.messages(data,1);status(parent,original[e.original],value,e)
    if e.reused:status(child,original[selected],value,e)
    else:
        require(T.tags(child)==[1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38] and text(child,1)==selected
                and field(child,2,0)==0 and text(child,8)==text(child,21)=='aad:temp' and S.body(child,False)==''
                and field(child,13,2)==b'' and field(child,16,2)==b'\x20\x00' and field(child,19,0)==field(child,20,0)==1
                and text(child,33)==value['session']+'-0' and all(field(child,tag,0)==0 for tag in (3,9,10,11,29,35)),
                'Empty new creation body/position/metadata changed')
        bootstrap(wire(field(child,38,2)),selected,0,value['session']+'00000000',True)
    return selected,parent,child


def journal(value,e,write_value=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Empty journal scope changed')
    if write_value is None:require(value['receipt'] is None,'Unexpected retained Empty Enter');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and set(receipt)=={'operation','checkpoint'}
            and isinstance(receipt.get('operation'),str) and receipt['operation'],'Empty journal operation missing')
    c=receipt['checkpoint'];require(isinstance(c,dict) and set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'},'Empty checkpoint fields changed')
    require(type(c['version']) is int and c['version']==10 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.original and c['session']==write_value['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']=='' and c['action']=={'EmptyEnter':dict(child=write_value['child'],position='aae' if e.reused else 'aad:temp',reused=e.reused)}
            and type(c['action']['EmptyEnter']['reused']) is bool,'Empty durable original+selected intent changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write_value['documentData'],validate=True),'Empty durable request differs from actual pair')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};sections=c['sections']
    require(isinstance(sections,list) and len(sections)==len(expected)
            and {text(wire(S.byte_vector(row)),1):S.byte_vector(row) for row in sections}==expected,'Empty whole original source inventory lost')


def ack(value,sent,source,contents,sequences,e,partial=False):
    require(value.get('partial') is partial and value.get('accepted') is (not partial),'Empty ACK admission flag changed')
    outer=decoded(value.get('raw'));require(T.tags(outer)==[1,2] and field(outer,1,0)==1,'Empty ACK malformed')
    data=wire(field(outer,2,2));require(T.tags(data)==[1]*len(sent)+[15] and text(data,15)=='document','Empty ACK incomplete/duplicated/wrong owner')
    rows=S.messages(data,1)
    for row,outgoing in zip(rows,sent):
        identity=text(outgoing,1);sequence=sequences[identity]
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and body(row,identity,e)==contents[identity]
                and field(row,2,0)==sequence and all(field(row,k,0)==field(outgoing,k,0) for k in (19,20))
                and text(row,33)==text(outgoing,33),'Empty fixture ACK body/owner/revision/status changed')
        now,prior=wire(field(row,38,2)),wire(field(outgoing,38,2))
        require(field(now,3,0)==sequence and field(now,4,0)>0 and not any(f[0]==12 for f in now)
                and S.stable(now,(3,4,12))==S.stable(prior,(3,4,12)),'Empty ACK changed exact history/base/processed metadata')
        if identity in source:
            require(S.stable(row,(2,9,12,13,19,20,33,38))==S.stable(source[identity],(2,9,12,13,19,20,33,38)),'Empty ACK replaced opaque source')
        else:
            require(text(row,8)==text(row,21)=='aad' and S.stable(row,(2,6,7,8,21,38))==S.stable(outgoing,(2,6,7,8,21,38)),
                    'Empty new ACK lost creation attributes')
    return rows


def continuation(value,source,content,sequence,session,e):
    identity=text(source,1);data=decoded(value.get('documentData'));S.rich_title(data)
    require(value.get('kind')=='text' and value.get('parent')==identity and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==sequence
            and value.get('session')==session and re.fullmatch('[0-9a-f]{12}',session) and T.tags(data)==[1,14],'Empty later Save session/counter changed')
    row=wire(field(data,1,2));current=wire(field(row,38,2));before=wire(field(source,38,2))
    require(T.tags(row)==[1,2,9,12,13,19,20,33,38] and text(row,1)==identity and field(row,2,0)==field(source,2,0)
            and body(row,identity,e)==content and field(row,9,0)==0 and field(row,13,2)==b''
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==session+'-'+str(sequence-1),'Empty later Save body/status changed')
    require(S.stable(current,(4,5,6,12))==S.stable(before,(4,5,6,12)) and not any(f[0] in (4,7,10) for f in current),
            'Empty later Save reset source base/history metadata')
    now,prior=T.decode_history(field(current,5,2),field(before,5,2));M.chain(now['changes'],prior['changes'],session+'00000000')
    require([f[2] for f in current if f[0]==6]==[f[2] for f in before if f[0]==6]+[field(current,12,2)]
            and now['heads']==[text(current,12)] and now['changes'][-1]['text']==content+'\ufeff','Empty later Save lost historical prefix/processed order')
    return row
