"""One split, immutable initial lineage, independent parent/child histories and v17 custody."""
import base64
from dataclasses import dataclass,replace
import json
import re
import canvas_premiddle_wire as P
from canvas_premiddle_wire import S,T,require,wire,decoded,field,text,rtml,source_parent,inventory,patch,patch_groups,apply_patch,chain

@dataclass(frozen=True)
class Config(P.Config):
    middle_parent: str
    middle_cadence: str
    initial_parent: str
    initial_wire: str
    parent_updates: tuple
    parent_reports: tuple

def config(corpus='ascii',typing='none',parent_mode='bootstrap',pre_enter='none',middle_parent='correction',middle_cadence='parent-first'):
    require(pre_enter in ('none','correction','revert','retype') and middle_parent in ('correction','erase','revert')
            and middle_cadence in ('parent-first','child-first','interleaved'),'Unpinned middle parent cohort')
    e=P.config(corpus,typing,parent_mode,'revert' if pre_enter=='none' else pre_enter)
    if pre_enter=='none':e=replace(e,pre_enter='none',pre_updates=(),pre_reports=())
    initial=e.seed
    updates=(initial+' draftx',initial+' draft',initial+' revised') if middle_parent=='correction' else ('',) if middle_parent=='erase' else (initial+' draftx',initial)
    e=replace(e,seed=updates[-1],left_wire=rtml(updates[-1]))
    return Config(**e.__dict__,middle_parent=middle_parent,middle_cadence=middle_cadence,initial_parent=initial,initial_wire=rtml(initial),parent_updates=updates,parent_reports=tuple(map(rtml,updates)))

def labels(value,e,recovery=False):
    P.labels(value,e,recovery)
    require(value.get('middleParent')==e.middle_parent and value.get('middleCadence')==e.middle_cadence,'Middle parent selectors changed')

def parent_patches(e):
    n=T.utf16(e.initial_wire)
    return ([patch('splice',n,' draftx'),patch('del',n+6),patch('splice',n+1,'revise'),patch('del',n+8,length=4)] if e.middle_parent=='correction' else
            [patch('del',0,length=n)] if e.middle_parent=='erase' else [patch('splice',n,' draftx'),patch('del',n,length=7)])

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
    if e.pre_reports:chain(live['changes'],before,actor)
    else:require(live['changes']==before,'Direct split changed original history before Enter')
    n=T.utf16(rtml(e.original_left));p=patch
    patches=([p('splice',n,' draftx'),p('del',n+6),p('splice',n+1,'revise'),p('del',n+8,length=4)] if e.pre_enter=='correction' else
             [p('splice',n,' draftx'),p('del',n,length=7)] if e.pre_enter=='revert' else
             [p('del',0,length=T.utf16(e.full_wire)),p('splice',0,e.full_wire)])
    if not e.pre_reports:patches=[]
    value=e.full_wire+'\ufeff';states=[]
    for change in patches:value=apply_patch(value,change);states.append(value)
    added=live['changes'][len(before):]
    require([c['text'] for c in added]==states and [c['patches'] for c in added]==[[p] for p in patches],
            'Pre-middle ordered pre-split patches/history changed')
    require(live['heads']==[live['changes'][-1]['hash']] and json.loads(text(lineage,5))==live['heads']
            and live['changes'][-1]['text']==rtml(e.live)+'\ufeff','Pre-middle lineage binds stale source rather than live draft')
    chain(now['changes'],live['changes'],actor);shortened=now['changes'][len(live['changes']):]
    expected_parent=[patch('del',e.wire_offset,length=T.utf16(e.tail_wire)),*parent_patches(e)]
    value=rtml(e.live)+'\ufeff';states=[]
    for p in expected_parent:value=apply_patch(value,p);states.append(value)
    require([c['text'] for c in shortened]==states and [c['patches'] for c in shortened]==[[p] for p in expected_parent],
            'Middle initial shortening or complete parent report history changed')
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
            and type(c.get('version')) is int and c['version']==17 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.parent and c['session']==write['session'] and type(c['sequence']) is int and c['sequence']==1
            and c['draft']==e.full_wire,'Middle parent v17 source/owner changed')
    require(c['action']=={'SplitParent':dict(child=write['child'],position='aad:temp',offset_utf16=e.offset,enter_draft=rtml(e.live),parent_draft=e.initial_wire,
            child_draft=e.tail_wire,pre_enter_updates=list(e.pre_reports),child_updates=list(e.child_reports),parent_updates=list(e.parent_reports))}
            and type(c['action']['SplitParent']['offset_utf16']) is int,'Middle parent v17 live draft/reports changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write['documentData'],validate=True),'Middle durable pair differs from dispatch')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']}
    stored=c['sections']
    # Inventory serializes BTreeMap values; display ordering remains pinned by
    # each exact raw position and the independently checked original inventory.
    require(isinstance(stored,list) and len(stored)==len(expected) and
            {text(wire(S.byte_vector(r)),1):S.byte_vector(r) for r in stored}==expected,
            'Middle durable whole original inventory changed')
