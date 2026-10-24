"""Original pre-input history, live split lineage and inert v16 custody."""
import base64
from dataclasses import dataclass
import hashlib
import json
import re

import canvas_split_oracle as S
import canvas_end_typing_oracle as T
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Config:
    corpus: str
    typing: str
    parent_mode: str
    seed: str  # Native post-Enter parent; shared typed DOM helpers use this name.
    tail: str
    full: str
    left_wire: str
    tail_wire: str
    full_wire: str
    offset: int
    wire_offset: int
    updates: tuple
    reports: tuple
    continued: str
    parent_edited: str
    pre_enter: str
    source_offset: int
    original_left: str
    pre_updates: tuple
    pre_reports: tuple
    live: str
    child_reports: tuple

    @property
    def parent(self):return "temp:C:Dte0000000000000000000000001" if self.parent_mode=="retained-zero" else "paragraph"


def rtml(value): return value.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;')

def config(corpus='ascii',typing='none',parent_mode='bootstrap',pre_enter='correction'):
    require(corpus in ('ascii','unicode') and typing in ('none','correction','erase','restore')
            and parent_mode in ('bootstrap','retained','retained-zero') and pre_enter in ('correction','revert','retype'),'Unpinned pre-middle cohort')
    left='Keep this' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600'
    tail='Second paragraph' if corpus=='ascii' else 'Second \u65e5\u672c\U0001f600 & paragraph'
    full=left+tail; live_left=left+' revised' if pre_enter=='correction' else left
    pre=((left+' draftx'+tail,left+' draft'+tail,live_left+tail) if pre_enter=='correction' else
         (left+' draftx'+tail,full) if pre_enter=='revert' else ('',full))
    prefix='New ' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 '
    child=(('New',prefix+'draftx',prefix+'draft',prefix+'paragraph') if typing=='correction' else
           (tail+'x','' if typing=='erase' else tail) if typing!='none' else ())
    updates=child or (tail,)
    return Config(corpus,typing,parent_mode,live_left,tail,full,rtml(live_left),rtml(tail),rtml(full),
                  T.utf16(live_left),T.utf16(rtml(live_left)),updates,tuple(map(rtml,updates)),
                  'Continued paragraph' if corpus=='ascii' else 'Continued \u65e5\u672c\U0001f600 paragraph',left+' edited',
                  pre_enter,T.utf16(left),left,pre,tuple(map(rtml,pre)),live_left+tail,tuple(map(rtml,child)))


def labels(value,e,recovery=False):
    require(value.get('corpus')==e.corpus and value.get('typing')==e.typing and value.get('parentMode')==e.parent_mode
            and value.get('preEnter')==e.pre_enter and value.get('parentId')==e.parent
            and value.get('splitMode' if recovery else 'mode')=='middle','Middle typing caller selection changed')


SOURCE_HASHES={'retained-positive:ascii': 'd6fc393176fef341447cf8fa288b585df6dfd210747b9b4464bac6d26df38263', 'retained-positive:unicode': 'e39c7e9475f1eb3fe5f06da7e689dd5d6baa114b77da8d130912e8be20205084', 'retained-zero:ascii': '3453d5a452de0c756b33945151cba72d48019627661002849eb76ef4908cb47c', 'retained-zero:unicode': 'b1d3be7cb6d3e8321328a68d3b4f432d7ed70e48118212bef8061eaa50c543d8'}


def source_parent(row,e):
    values=S.messages(row,38)
    if e.parent_mode=='bootstrap':require(not values,'Bootstrap source acquired retained history');return None
    require(len(values)==1,'Retained source lacks exact Session');source=values[0]
    require(field(source,4,0)==8_000_000_000_000_000 and sum(f[0]==4 for f in source)==1,'Retained source lease changed')
    raw=b''.join(f[3] for f in sorted(source,key=lambda f:f[0]) if f[0] not in (1,4))
    require(hashlib.sha256(raw).hexdigest()==SOURCE_HASHES[('retained-positive' if e.parent_mode=='retained' else e.parent_mode)+':'+e.corpus],'Retained source history/metadata differs from pinned original')
    return source


def inventory(value,e,original=None,child=None,child_saved=False,parent_saved=False,stale=False):
    require(isinstance(value,dict) and tuple(value.get(k) for k in ('file','thread','document'))==('Ffixture','thread','document'),'Middle owner changed')
    tr,doc=decoded(value.get('threadRaw')),decoded(value.get('documentRaw'))
    require(text(tr,1)=='thread' and text(tr,10)=='document' and text(doc,1)=='document' and text(doc,6)=='thread'
            and text(doc,42)=='xyz' and text(wire(field(doc,69,2)),1)=='Fixture title','Middle raw owner/title changed')
    ids=['title',e.parent]+([child] if child else [])+['neighbor'];rows=value.get('sections')
    require(isinstance(rows,list) and [r.get('id') for r in rows]==ids,'Middle inventory identity/order changed')
    result={}
    for item in rows:
        row=decoded(item.get('raw'));result[item['id']]=row
        require(text(row,1)==item['id'] and text(row,6)=='thread' and text(row,7)=='document'
                and type(item.get('sequence')) is int and item['sequence']==field(row,2,0),'Middle raw row association changed')
    title=result['title'];neighbor=result['neighbor'];parent=result[e.parent]
    require(field(title,9,0)==64 and field(title,10,0)==48 and field(title,2,0)==1000 and text(title,8)=='aaa'
            and text(wire(field(wire(field(title,12,2)),58,2)),1)=='Fixture title','Middle title changed')
    require(S.body(neighbor)=='Another paragraph' and text(neighbor,8)=='aae' and field(neighbor,2,0)==1000,'Middle neighbor changed')
    require(text(parent,8)=='aac' and S.body(parent)==(rtml(e.parent_edited) if parent_saved else e.left_wire if child else e.full_wire)
            and field(parent,2,0)==(3000 if parent_saved or stale else 2000 if child else 1000),'Middle parent body/revision changed')
    for ordinary in ([parent,result[child]] if child else [parent]):
        for tag in (3,9,10,35):
            if any(f[0]==tag for f in ordinary):require(field(ordinary,tag,0)==0,'Middle source became styled/locked/nonordinary')
        if any(f[0]==13 for f in ordinary):require(field(ordinary,13,2)==b'','Middle source acquired parent/layout')
    if not child:source_parent(parent,e)
    else:
        tail=result[child]
        require(text(tail,8)=='aad' and field(tail,2,0)==(3000 if child_saved else 1000)
                and S.body(tail,False)==(rtml(e.continued) if child_saved else e.reports[-1]),'Middle canonical child changed')
    if original:
        require(all(value.get(k)==original[k] for k in original if k!='sections'),'Middle raw owner custody changed')
        before={r['id']:r for r in original['sections']}
        require(all(next(r for r in rows if r['id']==identity)==before[identity] for identity in ('title','neighbor')),'Middle untouched title/neighbor changed')
        if not child:
            for item in rows:
                require(S.stable(decoded(item['raw']),(2,) if stale and item['id']==e.parent else ())
                        ==S.stable(decoded(before[item['id']]['raw']),(2,) if stale and item['id']==e.parent else ()),'Middle unconfirmed source changed')
    return result


def patch(action,index,value=None,length=1):
    out=dict(action=action,path=['text',index])
    if action=='splice':out['value']=value
    elif length!=1:out['length']=length
    return out


def patch_groups(e):
    first=[patch('splice',0,e.tail_wire)]
    if e.typing=='none':return [first]
    if e.typing!='correction':
        n=T.utf16(e.tail_wire)
        return [first,[patch('splice',n,'x')],[patch('del',0,length=n+1) if e.typing=='erase' else patch('del',n)]]
    replacement=[patch('splice',0,'N'),patch('del',1),patch('del',2,length=7 if e.corpus=='ascii' else 22),
                 patch('splice',2,'w'),patch('del',3,length=7 if e.corpus=='ascii' else 3)]
    # Remaining reports exactly match the already executed ordinary correction.
    rest=T.mutation_patches(e.updates)[1:]
    return [first,replacement,[rest[0]],[rest[1]],rest[2:]]


def apply_patch(value,p):
    raw=value.encode('utf-16-le');start=p['path'][1]*2
    end=start+(p.get('length',1)*2 if p['action']=='del' else 0)
    require(0<=start<=end<=len(raw),'Invalid expected mutation range')
    return (raw[:start]+(p['value'].encode('utf-16-le') if p['action']=='splice' else b'')+raw[end:]).decode('utf-16-le')


def chain(changes,prefix,actor):
    require(changes and len(changes)>len(prefix) and [c['raw'] for c in changes[:len(prefix)]]==[c['raw'] for c in prefix],'Middle history prefix reset/reordered')
    previous=prefix[-1] if prefix else None;seq=max((c['seq'] for c in prefix if c['actor']==actor),default=0)
    for c in changes[len(prefix):]:
        seq+=1
        require(c['actor']==actor and c['seq']==seq and c['deps']==([] if previous is None else [previous['hash']])
                and c['keys']==['deleted','text'] and c['deleted'] is False and c['marks']==[],'Middle actor/dependency/plain-state custody changed')
        if previous is None:
            require(c['ops']==[dict(action='makeText',obj='_root',key='text',pred=[]),
                    dict(action='set',obj='1@'+actor,elemId='_head',insert=True,value='\ufeff',pred=[]),
                    dict(action='set',obj='_root',key='deleted',value=False,pred=[])],'Middle empty bootstrap structure differs')
        else:
            require(c['ops'] and all(op.get('action') in ('set','del') and op.get('obj')!='_root' for op in c['ops']),
                    'Middle descendant history mutated document structure')
        previous=c


def histories(parent,child,lineage,original,e,session):
    pc,cc=wire(field(parent,38,2)),wire(field(child,38,2));live_doc=field(lineage,7,2);actor=session+'00000000'
    now,live=T.decode_history(field(pc,5,2),live_doc);src=source_parent(original,e)
    if src:
        original_history=T.decode_history(field(src,5,2))[0];before=original_history['changes']
        require(S.stable(pc,(4,5,6,7,10,12))==S.stable(src,(4,5,6,7,10,12)),'Pre-middle retained metadata changed')
        processed=[f[2].decode() for f in src if f[0]==6]
    else:
        before=live['changes'][:1];processed=[before[0]['hash']]
        require(before[0]['actor']==actor and before[0]['seq']==1 and before[0]['deps']==[]
                and before[0]['keys']==['deleted','text'] and before[0]['deleted'] is False and before[0]['marks']==[]
                and before[0]['text']==e.full_wire+'\ufeff' and text(pc,8)==processed[0],'Pre-middle bootstrap changed')
    require(before and before[-1]['text']==e.full_wire+'\ufeff','Pre-middle immutable original text changed')
    chain(live['changes'],before,actor)
    n=T.utf16(rtml(e.original_left));p=patch
    patches=([p('splice',n,' draftx'),p('del',n+6),p('splice',n+1,'revise'),p('del',n+8,length=4)] if e.pre_enter=='correction' else
             [p('splice',n,' draftx'),p('del',n,length=7)] if e.pre_enter=='revert' else
             [p('del',0,length=T.utf16(e.full_wire)),p('splice',0,e.full_wire)])
    value=e.full_wire+'\ufeff';states=[]
    for change in patches:value=apply_patch(value,change);states.append(value)
    added=live['changes'][len(before):]
    require([c['text'] for c in added]==states and [c['patches'] for c in added]==[[p] for p in patches],
            'Pre-middle ordered pre-split patches/history changed')
    require(live['heads']==[live['changes'][-1]['hash']] and json.loads(text(lineage,5))==live['heads']
            and live['changes'][-1]['text']==rtml(e.live)+'\ufeff','Pre-middle lineage binds stale source rather than live draft')
    chain(now['changes'],live['changes'],actor);shortened=now['changes'][len(live['changes']):]
    require(len(shortened)==1 and shortened[0]['text']==e.left_wire+'\ufeff'
            and shortened[0]['patches']==[patch('del',e.wire_offset,length=T.utf16(e.tail_wire))],'Pre-middle shortening changed')
    require(now['heads']==[text(pc,12)] and [f[2].decode() for f in pc if f[0]==6]==processed+[text(pc,12)]+processed,
            'Pre-middle queue/processed custody changed')
    child_report=T.decode_history(field(cc,5,2))[0];changes=child_report['changes'];chain(changes,[],actor)
    require(changes[0]['text']=='\ufeff' and text(cc,8)==changes[0]['hash'],'Pre-middle child bootstrap changed')
    expected=[p for group in patch_groups(e) for p in group];states=[];value='\ufeff'
    for p in expected:value=apply_patch(value,p);states.append(value)
    require([c['text'] for c in changes[1:]]==states and [c['patches'] for c in changes[1:]]==[[p] for p in expected]
            and child_report['heads']==[text(cc,12)] and [f[2].decode() for f in cc if f[0]==6]==[text(cc,12)],
            'Pre-middle ordered child history changed')
    return now,child_report


def write(value,original,e):
    require(value.get('kind')=='split' and value.get('httpFactoryExecuted') is False and value.get('formTitle')=='Fixture title'
            and value.get('parent')==e.parent and type(value.get('sequence')) is int and value['sequence']==1
            and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Middle pair request identity changed')
    child=value.get('child');require(isinstance(child,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',child),'Middle new child identity changed')
    data=decoded(value.get('documentData'));S.rich_title(data)
    require(T.tags(data)==[1,1,14],'Middle Save not one complete pair')
    parent,tail=S.messages(data,1)
    require(T.tags(parent)==[1,2,9,12,13,19,20,33,38] and T.tags(tail)==[1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38],'Middle request fields changed')
    require(text(parent,1)==e.parent and field(parent,2,0)==1000 and S.body(parent)==e.left_wire
            and text(tail,1)==child and field(tail,2,0)==0 and S.body(tail,False)==e.reports[-1]
            and text(tail,8)==text(tail,21)=='aad:temp','Middle pair content/base/position changed')
    for row in (parent,tail):
        require(field(row,9,0)==0 and field(row,13,2)==b'' and field(row,19,0)==field(row,20,0)==1
                and text(row,33)==value['session']+'-0','Middle trace/ordinary structure changed')
    require(all(field(tail,t,0)==0 for t in (3,10,11,29,35)) and field(tail,16,2)==b'\x20\x00','Middle new child creation metadata changed')
    pc,cc=(wire(field(row,38,2)) for row in (parent,tail))
    for crdt,identity,base in ((pc,e.parent,0 if e.parent_mode=='retained-zero' else 1000),(cc,child,0)):
        require(text(crdt,1)==identity and field(crdt,2,0)==base and field(crdt,3,0)==(1000 if identity==e.parent else 0)
                and all(f[0] in (1,2,3,5,6,8,10,11,12) for f in crdt),'Middle Session identity/base/fields changed')
        for tag in (1,2,3,5,8,12):require(sum(f[0]==tag for f in crdt)==1,'Duplicate middle Session singular field')
    require(sum(f[0]==10 for f in pc)==1 and not any(f[0] in (10,11) for f in cc),'Middle lineage placement changed')
    lineage=wire(field(pc,10,2))
    require(T.tags(lineage)==list(range(1,8)) and field(lineage,1,0)==0 and text(lineage,2)==value['session']+'00000000'
            and text(lineage,3)==child and field(lineage,4,0)==e.wire_offset
            and text(lineage,6)==str(field(pc,2,0))+'-'+text(pc,8),'Middle immutable SplitInfo changed')
    histories(parent,tail,lineage,original,e,value['session'])
    return child,parent,tail,lineage


def journal(value,e,write=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Middle journal scope changed')
    if write is None:require(value['receipt'] is None,'Unexpected middle journal');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and isinstance(receipt.get('operation'),str) and receipt['operation'],'Middle operation missing')
    c=receipt.get('checkpoint',{})
    require(set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'}
            and type(c.get('version')) is int and c['version']==16 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.parent and c['session']==write['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']==e.full_wire,'Pre-middle v16 source/owner changed')
    require(c['action']=={'SplitPreEnter':dict(child=write['child'],position='aad:temp',offset_utf16=e.offset,enter_draft=rtml(e.live),parent_draft=e.left_wire,
            child_draft=e.tail_wire,pre_enter_updates=list(e.pre_reports),child_updates=list(e.child_reports))}
            and type(c['action']['SplitPreEnter']['offset_utf16']) is int,'Pre-middle v16 live draft/reports changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write['documentData'],validate=True),'Middle durable pair differs from dispatch')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']}
    stored=c['sections']
    # Inventory serializes BTreeMap values; display ordering remains pinned by
    # each exact raw position and the independently checked original inventory.
    require(isinstance(stored,list) and len(stored)==len(expected) and
            {text(wire(S.byte_vector(r)),1):S.byte_vector(r) for r in stored}==expected,
            'Middle durable whole original inventory changed')
