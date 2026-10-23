"""Independent start-Enter insertion: one new empty row, untouched old source."""
import base64
from dataclasses import dataclass
import hashlib
import re

import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Config:
    corpus: str
    parent_mode: str
    seed: str
    wire_seed: str
    continued: str
    edited: str


def config(corpus='ascii',parent_mode='bootstrap'):
    require(corpus in ('ascii','unicode') and parent_mode in ('bootstrap','retained'),'Unpinned start cohort')
    seed='Keep thisSecond paragraph' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph'
    return Config(corpus,parent_mode,seed,M.rtml(seed),
                  'Continued paragraph' if corpus=='ascii' else 'Continued \u65e5\u672c\U0001f600 paragraph',seed+' edited')


def labels(value,e,recovery=False):
    require(value.get('splitMode' if recovery else 'mode')=='start' and value.get('corpus')==e.corpus
            and value.get('parentMode')==e.parent_mode and value.get('typing')=='none','Start caller selection changed')
    require(value.get('reuseBase')=='positive' and value.get('reuseContent')=='empty' and value.get('reuseChild') is None,
            'Start acquired reuse controls')


SOURCE_HASHES={'ascii':'15aa17dc43aec19ea4f34eadbc09421c91ea5c6ab566b9f996754ca9ea2cf2f5',
               'unicode':'0855d35fb02fe77e411c420034241ccda3efaa17e4c29b8bc4b5cf1843c09749'}


def source_parent(row,e):
    values=S.messages(row,38)
    if e.parent_mode=='bootstrap':require(not values,'Start bootstrap source acquired history');return
    require(len(values)==1,'Start retained source lost Session');session=values[0]
    require(field(session,4,0)==8_000_000_000_000_000 and sum(f[0]==4 for f in session)==1,'Start source lease changed')
    canonical=b''.join(f[3] for f in sorted(session,key=lambda f:f[0]) if f[0]!=4)
    require(hashlib.sha256(canonical).hexdigest()==SOURCE_HASHES[e.corpus],'Start retained source history changed')


def inventory(value,e,original=None,new=None,new_saved=False,original_saved=False,stale=False):
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document'))==('Ffixture','thread','document'),'Start inventory owner changed')
    tr,doc=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(tr,1)=='thread' and text(tr,10)=='document' and text(doc,1)=='document' and text(doc,6)=='thread'
            and text(doc,42)=='xyz' and text(wire(field(doc,69,2)),1)=='Fixture title','Start document/title changed')
    ids=['title']+([new] if new else [])+['paragraph','neighbor'];rows=value.get('sections')
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids,'Start original identity/order replaced')
    result={}
    for item in rows:
        row=decoded(item.get('raw'));result[item['id']]=row
        require(text(row,1)==item['id'] and text(row,6)=='thread' and text(row,7)=='document'
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0),'Start row/raw association changed')
    title=result['title'];parent=result['paragraph'];neighbor=result['neighbor']
    require(field(title,9,0)==64 and field(title,10,0)==48 and text(title,8)=='aaa' and field(title,2,0)==1000
            and text(wire(field(wire(field(title,12,2)),58,2)),1)=='Fixture title','Start title changed')
    require(S.body(neighbor)=='Another paragraph' and text(neighbor,8)=='aae' and field(neighbor,2,0)==1000
            and next((f[3] for f in neighbor if f[0]==1001),None)==b'\xc8\x3e\x81\x00','Start opaque neighbor changed')
    require(S.body(parent)==(M.rtml(e.edited) if original_saved else e.wire_seed) and text(parent,8)=='aac'
            and field(parent,2,0)==(3000 if original_saved or stale else 1000),'Start original content/revision changed')
    for tag in (3,9,10,35):
        if any(f[0]==tag for f in parent):require(field(parent,tag,0)==0,'Start original became styled/locked/nonordinary')
    if any(f[0]==13 for f in parent):require(field(parent,13,2)==b'','Start original acquired parent/layout')
    if not original_saved:source_parent(parent,e)
    if new:
        row=result[new]
        require(text(row,8)==text(row,21)=='aab' and S.body(row,False)==(M.rtml(e.continued) if new_saved else '')
                and field(row,2,0)==(3000 if new_saved else 1000),'Start inserted row body/order/revision changed')
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Start raw document custody changed')
        before={r['id']:r for r in original['sections']}
        for identity in ('title','paragraph','neighbor'):
            if identity=='paragraph' and original_saved:continue
            current=next(r for r in rows if r['id']==identity)
            if identity=='paragraph' and stale:
                require(S.stable(decoded(current['raw']),(2,))==S.stable(decoded(before[identity]['raw']),(2,)),'Start stale mutation changed more than revision')
            else:require(current==before[identity],'Start mutated untouched original inventory')
    return result


def write(value):
    require(value.get('kind')=='start' and value.get('parent')=='paragraph' and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Start request owner/kind changed')
    new=value.get('child');require(isinstance(new,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',new),'Start did not allocate a new modern ID')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,14],'Start must send exactly one new row');S.rich_title(data)
    row=wire(field(data,1,2))
    require(T.tags(row)==[1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38]
            and text(row,1)==new and field(row,2,0)==0 and text(row,8)==text(row,21)=='aab:temp'
            and S.body(row,False)=='' and field(row,13,2)==b'' and field(row,16,2)==b'\x20\x00'
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0','Start creation body/metadata changed')
    require(all(field(row,tag,0)==0 for tag in (3,9,10,11,29,35)),'Start creation flags changed')
    session=wire(field(row,38,2))
    require(T.tags(session)==[1,2,3,5,6,8,12] and text(session,1)==new and field(session,2,0)==field(session,3,0)==0
            and text(session,6)==text(session,8)==text(session,12),'Start acquired lineage/history or nonzero base')
    history=T.decode_history(field(session,5,2))[0];changes=history['changes'];M.chain(changes,[],value['session']+'00000000')
    require(len(changes)==1 and changes[0]['text']=='\ufeff' and history['heads']==[text(session,12)]
            and changes[0]['hash']==text(session,8),'Start is not one exact empty bootstrap')
    return new,row


def journal(value,write_value=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Start journal scope changed')
    if write_value is None:require(value['receipt'] is None,'Unexpected retained start insertion');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and set(receipt)=={'operation','checkpoint'}
            and isinstance(receipt.get('operation'),str) and receipt['operation'],'Start journal operation missing')
    c=receipt['checkpoint'];require(isinstance(c,dict) and set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'},'Start checkpoint fields changed')
    require(type(c['version']) is int and c['version']==1 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==write_value['child'] and c['session']==write_value['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']=='' and c['action']=={'Insert':dict(anchor='paragraph',before=True,position='aab:temp')},'Start v1 insertion intent changed')
    require(type(c['action']['Insert']['before']) is bool,'Start insertion direction is not a boolean')
    require(S.byte_vector(c['request'])==base64.b64decode(write_value['documentData'],validate=True),'Start durable request differs')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};sections=c['sections']
    require(isinstance(sections,list) and len(sections)==len(expected)
            and {text(wire(S.byte_vector(row)),1):S.byte_vector(row) for row in sections}==expected,'Start exact durable original inventory lost')


def continuation(value,source,content,sequence,session):
    """Existing ordinary Save; bootstrap the old source only when it had no CRDT."""
    identity=text(source,1);data=decoded(value.get('documentData'));S.rich_title(data)
    require(value.get('kind')=='text' and value.get('parent')==identity and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==sequence
            and value.get('session')==session and re.fullmatch('[0-9a-f]{12}',session) and T.tags(data)==[1,14],
            'Start continuation session/counter changed')
    row=wire(field(data,1,2));current=wire(field(row,38,2));actor=session+'00000000'
    require(T.tags(row)==[1,2,9,12,13,19,20,33,38] and text(row,1)==identity and field(row,2,0)==field(source,2,0)
            and S.body(row,identity=='paragraph')==content and field(row,9,0)==0 and field(row,13,2)==b''
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==session+'-'+str(sequence-1),
            'Start continuation body/trace/metadata changed')
    require(not any(f[0] in (4,7,10) for f in current),'Start continuation fabricated expiry/lineage')
    originals=S.messages(source,38)
    if originals:
        require(len(originals)==1,'Ambiguous original history');before=originals[0]
        require(S.stable(current,(4,5,6,12))==S.stable(before,(4,5,6,12)), 'Start continuation replaced captured history metadata')
        now,prior=T.decode_history(field(current,5,2),field(before,5,2));prefix=prior['changes'];heads=[f[2] for f in before if f[0]==6]
    else:
        require(identity=='paragraph' and T.tags(current)==[1,2,3,5,6,6,8,12] and text(current,1)==identity
                and field(current,2,0)==field(current,3,0)==1000,'Start original bootstrap metadata changed')
        now=T.decode_history(field(current,5,2))[0];prefix=now['changes'][:1]
        require(len(prefix)==1,'Start original bootstrap missing');first=prefix[0]
        require(first['actor']==actor and first['seq']==1 and first['deps']==[] and first['keys']==['deleted','text']
                and first['deleted'] is False and first['marks']==[] and first['text']==S.body(source)+'\ufeff'
                and text(current,8)==first['hash'],'Start original bootstrap does not preserve original full content')
        require(first['ops'][0]==dict(action='makeText',obj='_root',key='text',pred=[])
                and first['ops'][-1]==dict(action='set',obj='_root',key='deleted',value=False,pred=[])
                and all(op.get('action')=='set' and op.get('obj')=='1@'+actor and op.get('insert') is True
                        for op in first['ops'][1:-1]),'Start original bootstrap includes hidden mutation')
        heads=[first['hash'].encode()]
    M.chain(now['changes'],prefix,actor)
    require([f[2] for f in current if f[0]==6]==heads+[field(current,12,2)] and now['heads']==[text(current,12)]
            and now['changes'][-1]['text']==content+'\ufeff','Start continuation history/processed head changed')
    return row
