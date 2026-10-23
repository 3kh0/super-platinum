"""Independent typed-start contract: original reports plus an untouched new blank."""
import base64
from dataclasses import dataclass
import hashlib
import re

import canvas_start_wire as A
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Config:
    corpus: str
    typing: str
    parent_mode: str
    seed: str
    wire_seed: str
    updates: tuple
    reports: tuple
    continued: str
    edited: str


def config(corpus='ascii',typing='correction',parent_mode='bootstrap'):
    old=A.config(corpus,parent_mode)
    require(typing in ('correction','erase','restore'),'Unpinned typed-start cadence')
    prefix='New ' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 '
    updates=('New '+old.seed,prefix+'draftx '+old.seed,prefix+'draft '+old.seed,prefix+'paragraph '+old.seed) if typing=='correction' else ('x'+old.seed,'' if typing=='erase' else old.seed)
    return Config(corpus,typing,parent_mode,old.seed,old.wire_seed,updates,tuple(map(M.rtml,updates)),old.continued,old.edited)


def labels(value,e,recovery=False):
    require(value.get('splitMode' if recovery else 'mode')=='start' and value.get('corpus')==e.corpus
            and value.get('parentMode')==e.parent_mode and value.get('typing')==e.typing,'Typed-start caller selection changed')
    require(value.get('reuseBase')=='positive' and value.get('reuseContent')=='empty' and value.get('reuseChild') is None,'Typed start acquired reuse controls')


SOURCE_HASHES={'ascii':'9762abaffdb9c0ebd5bf5ed156d77e48ddd4aa4a00f1d650d4bd8a4012a59cab',
               'unicode':'e4e8ac0d44fbdff4c6499afc30090f8f11cee61019d8e360739e886fd9eacad5'}


def source_parent(row,e):
    values=S.messages(row,38)
    if e.parent_mode=='bootstrap':require(not values,'Typed-start bootstrap acquired source38');return None
    require(len(values)==1,'Typed-start retained source missing');session=values[0]
    require(field(session,4,0)==8_000_000_000_000_000 and sum(f[0]==4 for f in session)==1,'Typed-start source lease changed')
    raw=b''.join(f[3] for f in sorted(session,key=lambda f:f[0]) if f[0]!=4)
    require(hashlib.sha256(raw).hexdigest()==SOURCE_HASHES[e.corpus],'Typed-start retained original history changed')
    return session


def inventory(value,e,original=None,new=None,new_saved=False,original_saved=False,stale=False):
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document'))==('Ffixture','thread','document'),'Typed-start owner changed')
    tr,doc=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(tr,1)=='thread' and text(tr,10)=='document' and text(doc,1)=='document' and text(doc,6)=='thread'
            and text(doc,42)=='xyz' and text(wire(field(doc,69,2)),1)=='Fixture title','Typed-start document/title changed')
    rows=value.get('sections');ids=['title']+([new] if new else [])+['paragraph','neighbor']
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids,'Typed-start identity/order changed')
    result={}
    for item in rows:
        row=decoded(item.get('raw'));result[item['id']]=row
        require(text(row,1)==item['id'] and text(row,6)=='thread' and text(row,7)=='document'
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0),'Typed-start row/raw association changed')
    title,parent,neighbor=(result[k] for k in ('title','paragraph','neighbor'))
    require(field(title,9,0)==64 and field(title,10,0)==48 and text(title,8)=='aaa' and field(title,2,0)==1000
            and text(wire(field(wire(field(title,12,2)),58,2)),1)=='Fixture title','Typed-start title changed')
    require(S.body(neighbor)=='Another paragraph' and text(neighbor,8)=='aae' and field(neighbor,2,0)==1000
            and next((f[3] for f in neighbor if f[0]==1001),None)==b'\xc8\x3e\x81\x00','Typed-start opaque neighbor changed')
    require(S.body(parent)==(M.rtml(e.edited) if original_saved else e.reports[-1] if new else e.wire_seed) and text(parent,8)=='aac'
            and field(parent,2,0)==(3000 if original_saved or stale else 2000 if new else 1000),'Typed-start original content/revision changed')
    for tag in (3,9,10,35):
        if any(f[0]==tag for f in parent):require(field(parent,tag,0)==0,'Typed-start original became nonordinary')
    if any(f[0]==13 for f in parent):require(field(parent,13,2)==b'','Typed-start original acquired layout')
    if not new:source_parent(parent,e)
    else:
        row=result[new]
        require(text(row,8)==text(row,21)=='aab' and S.body(row,False)==(M.rtml(e.continued) if new_saved else '')
                and field(row,2,0)==(3000 if new_saved else 1000),'Typed-start inserted blank body/order/revision changed')
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Typed-start raw document custody changed')
        before={r['id']:r for r in original['sections']}
        for item in rows:
            identity=item['id']
            if identity==new or identity=='paragraph' and new:continue
            if identity=='paragraph' and stale:
                require(S.stable(decoded(item['raw']),(2,))==S.stable(decoded(before[identity]['raw']),(2,)),
                        'Typed-start stale source changed beyond revision')
            else:require(item==before[identity],'Typed-start untouched raw inventory changed')
    return result


def patch_groups(e):
    p=M.patch
    if e.typing!='correction':return [[p('splice',0,'x')],[p('del',0,length=T.utf16(e.wire_seed)+1) if e.typing=='erase' else p('del',0)]]
    at=4 if e.corpus=='ascii' else 9
    return [[p('splice',0,'New ')],[p('splice',4,('' if e.corpus=='ascii' else '\u65e5\u672c\U0001f600 ')+'draftx ')],
            [p('del',at+5)],[p('splice',at,'parag'),p('del',at+5),p('del',at+7,length=2),p('splice',at+7,'ph')]]


def original_history(row,original,e,session):
    current=wire(field(row,38,2));actor=session+'00000000';source=source_parent(original,e)
    require(text(current,1)=='paragraph' and field(current,2,0)==field(current,3,0)==1000
            and all(f[0] in (1,2,3,5,6,8,11,12) for f in current),'Typed-start original base/lineage changed')
    for tag in (1,2,3,5,8,12):require(sum(f[0]==tag for f in current)==1,'Duplicate typed-start Session field')
    if source:
        require(S.stable(current,(4,5,6,7,12))==S.stable(source,(4,5,6,7,12)),'Typed-start retained metadata replaced')
        now,prior=T.decode_history(field(current,5,2),field(source,5,2));prefix=prior['changes'];heads=[f[2] for f in source if f[0]==6]
    else:
        require(T.tags(current)==[1,2,3,5,6,6,6,8,12],'Typed-start bootstrap Session fields changed')
        now=T.decode_history(field(current,5,2))[0];prefix=now['changes'][:1];first=prefix[0]
        require(first['actor']==actor and first['seq']==1 and first['deps']==[] and first['keys']==['deleted','text']
                and first['deleted'] is False and first['marks']==[] and first['text']==e.wire_seed+'\ufeff'
                and text(current,8)==first['hash'],'Typed-start bootstrap lost full original')
        require(first['ops'][0]==dict(action='makeText',obj='_root',key='text',pred=[])
                and first['ops'][-1]==dict(action='set',obj='_root',key='deleted',value=False,pred=[])
                and all(op.get('action')=='set' and op.get('obj')=='1@'+actor and op.get('insert') is True for op in first['ops'][1:-1]),
                'Typed-start bootstrap hidden mutation')
        heads=[first['hash'].encode()]
    M.chain(now['changes'],prefix,actor);patches=[p for group in patch_groups(e) for p in group];value=e.wire_seed+'\ufeff';states=[]
    for p in patches:value=M.apply_patch(value,p);states.append(value)
    actual=now['changes'][len(prefix):]
    require([c['text'] for c in actual]==states and [c['patches'] for c in actual]==[[p] for p in patches],
            'Typed-start ordered original mutations differ from executed source')
    require(now['heads']==[text(current,12)] and [f[2] for f in current if f[0]==6]==heads+[field(current,12,2)]+heads,
            'Typed-start processed order/source duplication changed')


def write(value,original,e):
    require(value.get('kind')=='start' and value.get('parent')=='paragraph' and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Typed-start request identity changed')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,1,14],'Typed-start Save is not one complete pair');S.rich_title(data)
    parent,new=S.messages(data,1)
    # Validate the actual blank row with the accepted exact one-bootstrap creation gate.
    creation=dict(value,documentData=base64.b64encode(b''.join(f[3] for f in data if f[0]==14)+next(f[3] for f in reversed(data) if f[0]==1)).decode())
    identity,blank=A.write(creation)
    require(T.tags(parent)==[1,2,9,12,13,19,20,33,38] and text(parent,1)=='paragraph' and field(parent,2,0)==1000
            and S.body(parent)==e.reports[-1] and field(parent,9,0)==0 and field(parent,13,2)==b''
            and field(parent,19,0)==field(parent,20,0)==1 and text(parent,33)==value['session']+'-0','Typed-start original CONTENT/body/trace changed')
    require(S.raw(blank)==S.raw(new),'Typed-start creation association changed')
    original_history(parent,original,e,value['session'])
    return identity,parent,new


def journal(value,e,write_value=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Typed-start journal scope changed')
    if write_value is None:require(value['receipt'] is None,'Unexpected typed-start receipt');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and set(receipt)=={'operation','checkpoint'}
            and isinstance(receipt.get('operation'),str) and receipt['operation'],'Typed-start operation missing')
    c=receipt['checkpoint']
    require(isinstance(c,dict) and set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'}
            and type(c['version']) is int and c['version']==9 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']=='paragraph' and c['session']==write_value['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']==e.wire_seed,'Typed-start v9 original custody changed')
    require(c['action']=={'StartTyping':dict(child=write_value['child'],position='aab:temp',original_updates=list(e.reports))},'Typed-start v9 report history changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write_value['documentData'],validate=True),'Typed-start durable pair differs')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};sections=c['sections']
    require(isinstance(sections,list) and len(sections)==len(expected)
            and {text(wire(S.byte_vector(row)),1):S.byte_vector(row) for row in sections}==expected,'Typed-start durable whole inventory changed')


continuation=A.continuation
