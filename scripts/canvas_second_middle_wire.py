"""Three creation identities, two live lineages and exact v18 whole-operation custody."""
import base64
from dataclasses import dataclass
import json
import re
import canvas_premiddle_wire as P
from canvas_premiddle_wire import S,T,require,wire,decoded,field,text,rtml,patch,apply_patch,chain

@dataclass(frozen=True)
class Config:
    corpus:str
    parent_mode:str
    second_middle:str
    second_cadence:str
    left:str
    tail:str
    full:str
    offset:int
    second_offset:int
    second_before:str
    second_left:str
    second_right:str
    final:tuple
    between:tuple
    after:tuple
    @property
    def parent(self):return 'temp:C:Dte0000000000000000000000001' if self.parent_mode=='retained-zero' else 'paragraph'
    @property
    def full_wire(self):return rtml(self.full)
    @property
    def visual(self):return (0,2,1) if self.second_middle=='parent' else (0,1,2)
    @property
    def positions(self):return ('aac','aad','aacaaaa' if self.second_middle=='parent' else 'aadaaaa')
    @property
    def edited(self):return (self.left+' edited',self.final[1]+' edited',self.final[2]+' edited')

def config(corpus='ascii',parent_mode='bootstrap',second_middle='parent',second_cadence='straight'):
    require(corpus in ('ascii','unicode') and parent_mode in ('bootstrap','retained','retained-zero')
            and second_middle in ('parent','child') and second_cadence in ('straight','edited'),'Unpinned second middle cohort')
    p=P.config(corpus,'none',parent_mode,'revert');left,tail=p.original_left,p.tail;target=0 if second_middle=='parent' else 1
    values=[left,tail];between=[];after=[]
    def report(out,index,value):values[index]=value;out.append((index,value))
    if second_cadence=='edited':
        initial=values[target];report(between,target,initial+' draftx');report(between,1-target,values[1-target]+' changed')
        report(between,target,initial+' draft');report(between,target,initial+' revised')
    offset=7 if second_middle=='child' or corpus=='unicode' else 5
    before=values[target];boundary=7 if second_middle=='child' or corpus=='unicode' else 5
    # Prefix contains no supplementary scalar: Python slice equals visible UTF16 boundary here.
    first,second=before[:boundary],before[boundary:];values[target]=first;values.append(second)
    if second_cadence=='edited':
        child=values[1];report(after,1,child+' draftx');report(after,2,second+' draftx');report(after,0,'')
        report(after,2,second+' draft');report(after,1,child);report(after,2,second+' revised')
    return Config(corpus,parent_mode,second_middle,second_cadence,left,tail,left+tail,T.utf16(left),offset,before,first,second,tuple(values),tuple(between),tuple(after))

def labels(value,e,recovery=False):
    require(all(value.get(k)==v for k,v in dict(corpus=e.corpus,parentMode=e.parent_mode,parentId=e.parent,typing='none',preEnter='none',
            middleParent='none',middleCadence='parent-first',secondMiddle=e.second_middle,secondMiddleCadence=e.second_cadence).items())
            and value.get('splitMode' if recovery else 'mode')=='middle','Second middle selector/owner changed')

def report_patches(before,after):
    n=T.utf16(before)
    if not after:return [patch('del',0,length=n)]
    if after==before+' draftx':return [patch('splice',n,' draftx')]
    if after==before+' changed':return [patch('splice',n,' changed')]
    if before.endswith(' draftx') and after==before[:-1]:return [patch('del',n-1)]
    if before.endswith(' draftx') and after==before[:-7]:return [patch('del',n-7,length=7)]
    if before.endswith(' draft') and after==before[:-5]+'revised':return [patch('splice',n-5,'revise'),patch('del',n+2,length=4)]
    raise RuntimeError('Unpinned second-middle report patch')

def programs(e):
    values=[e.full];patches=[[],[],[]];boundaries=[]
    def split(target,left,right,child):
        boundaries.append((target,len(patches[target]),rtml(values[target]),T.utf16(rtml(left)),child))
        patches[target].append(patch('del',T.utf16(rtml(left)),length=T.utf16(rtml(right))))
        patches[child].append(patch('splice',0,rtml(right)));values[target]=left;values.append(right)
    split(0,e.left,e.tail,1)
    for i,value in e.between:patches[i].extend(report_patches(rtml(values[i]),rtml(value)));values[i]=value
    split(0 if e.second_middle=='parent' else 1,e.second_left,e.second_right,2)
    for i,value in e.after:patches[i].extend(report_patches(rtml(values[i]),rtml(value)));values[i]=value
    require(tuple(values)==e.final,'Second middle expected program lost identity')
    return patches,boundaries

def inventory(value,e,original=None,children=None,saved=(),stale=False):
    base=P.config(e.corpus,'none',e.parent_mode,'revert')
    if children is None:return P.inventory(value,base,original,stale=stale)
    ids=[e.parent,*children];rows=value.get('sections',[])
    # Snapshot storage order is not display order: same-parent splitting creates
    # child1 before child2, but child2's position places it before child1.
    expected_ids=['title',*ids,'neighbor']
    require(isinstance(rows,list) and len(rows)==len(expected_ids)
            and all(isinstance(r,dict) and isinstance(r.get('id'),str) for r in rows)
            and len({r['id'] for r in rows})==len(expected_ids)
            and {r['id'] for r in rows}==set(expected_ids),'Second middle canonical membership differs')
    require(original is not None and all(value.get(k)==original[k] for k in original if k!='sections'),'Second middle owner changed')
    before={r['id']:r for r in original['sections']};out={}
    for item in rows:
        row=decoded(item.get('raw'));identity=item['id'];out[identity]=row
        require(text(row,1)==identity and text(row,6)=='thread' and text(row,7)=='document' and type(item.get('sequence')) is int
                and field(row,2,0)==item['sequence'],'Second middle raw canonical association changed')
        if identity in ('title','neighbor'):require(item==before[identity],'Second middle untouched neighbor/title changed');continue
        i=ids.index(identity);content=e.edited[i] if i in saved else e.final[i]
        require(S.body(row,i==0)==rtml(content) and field(row,2,0)==(3000 if i in saved else 2000 if i==0 else 1000)
                and text(row,8)==e.positions[i],'Second middle content/position/revision changed')
    # Derive placement only after exact per-identity positions have passed.
    # Sorting cannot rehabilitate swapped, duplicate, or otherwise wrong positions.
    positions={identity:text(row,8) for identity,row in out.items()}
    require(len(set(positions.values()))==len(expected_ids)
            and sorted(positions,key=positions.get)==['title',*[ids[i] for i in e.visual],'neighbor'],
            'Second middle canonical position order differs')
    return out

def write(value,original,e):
    require(value.get('kind')=='second-middle' and value.get('httpFactoryExecuted') is False and value.get('formTitle')=='Fixture title'
            and value.get('parent')==e.parent and type(value.get('sequence')) is int and value['sequence']==1
            and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Second middle request identity changed')
    ids=[e.parent,value.get('child'),value.get('secondChild')]
    require(len(set(ids))==3 and all(isinstance(v,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',v) for v in ids[1:]),'Second middle needs three distinct IDs')
    require(value.get('members')==[dict(id=ids[i],finalRtml=rtml(e.final[i])) for i in (1,2)] and value.get('visualOrder')==list(e.visual),'Second middle metadata lost creation/visual identity')
    data=decoded(value.get('documentData'));S.rich_title(data);rows=S.messages(data,1)
    require(T.tags(data)==[1,1,1,14] and len(rows)==3,'Second middle Save lacks whole three-member packet')
    sessions=[]
    for i,row in enumerate(rows):
        require(T.tags(row)==([1,2,9,12,13,19,20,33,38] if i==0 else [1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38])
                and text(row,1)==ids[i] and field(row,2,0)==(1000 if i==0 else 0) and S.body(row,i==0)==rtml(e.final[i]),'Second middle creation order/content changed')
        require(field(row,9,0)==0 and field(row,13,2)==b'' and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0','Second middle ordinary structure/trace changed')
        if i:
            require(text(row,8)==text(row,21)==e.positions[i]+':temp' and all(field(row,t,0)==0 for t in (3,10,11,29,35))
                    and field(row,16,2)==b'\x20\x00','Second middle child creation metadata changed')
        crdt=wire(field(row,38,2));sessions.append(crdt)
        require(text(crdt,1)==ids[i] and field(crdt,2,0)==(0 if i or e.parent_mode=='retained-zero' else 1000)
                and field(crdt,3,0)==(0 if i else 1000) and all(f[0] in (1,2,3,5,6,8,10,11,12) for f in crdt),'Second middle Session identity/base changed')
        for tag in (1,2,3,5,8,12):require(sum(f[0]==tag for f in crdt)==1,'Second middle duplicate singular field')
        if i:require(not any(f[0]==11 for f in crdt),'Second middle unexpected child merged lineage')
    programs_,boundaries=programs(e);actor=value['session']+'00000000';histories=[];prefixes=[]
    source=P.source_parent(original,P.config(e.corpus,'none',e.parent_mode,'revert'))
    for i,crdt in enumerate(sessions):
        history=T.decode_history(field(crdt,5,2))[0];changes=history['changes'];histories.append(history)
        prefix=T.decode_history(field(source,5,2))[0]['changes'] if i==0 and source else changes[:1];prefixes.append(prefix)
        if i==0 and source:require(S.stable(crdt,(4,5,6,7,10,12))==S.stable(source,(4,5,6,7,10,12)),'Second middle retained source metadata changed')
        else:
            first=prefix[0]
            require(first['actor']==actor and first['seq']==1 and first['deps']==[] and first['keys']==['deleted','text']
                    and first['deleted'] is False and first['marks']==[] and text(crdt,8)==first['hash'],'Second middle bootstrap changed')
        initial=e.full_wire+'\ufeff' if i==0 else '\ufeff';require(prefix[-1]['text']==initial,'Second middle source text changed')
        chain(changes,prefix,actor);states=[];content=initial
        for p in programs_[i]:content=apply_patch(content,p);states.append(content)
        require([c['text'] for c in changes[len(prefix):]]==states and [c['patches'] for c in changes[len(prefix):]]==[[p] for p in programs_[i]],'Second middle complete ordered history changed')
        processed=[f[2].decode() for f in source if f[0]==6] if i==0 and source else [prefix[0]['hash']] if i==0 else []
        require(history['heads']==[text(crdt,12)] and [f[2].decode() for f in crdt if f[0]==6]==processed+[text(crdt,12)]+processed,'Second middle final queue/processed custody changed')
    infos=[S.messages(s,10) for s in sessions]
    require(list(map(len,infos))==([2,0,0] if e.second_middle=='parent' else [1,1,0]),'Second middle per-target lineage placement changed')
    indexes=[0,0,0]
    for target,count,live,offset,child in boundaries:
        info=infos[target][indexes[target]];seq=indexes[target];indexes[target]+=1;session=sessions[target]
        require(T.tags(info)==list(range(1,8)) and field(info,1,0)==seq and text(info,2)==actor and text(info,3)==ids[child]
                and field(info,4,0)==offset and text(info,6)==str(field(session,2,0))+'-'+text(session,8),'Second middle target-bound SplitInfo changed')
        report=T.decode_history(field(info,7,2))[0];expected=histories[target]['changes'][:len(prefixes[target])+count]
        require([c['raw'] for c in report['changes']]==[c['raw'] for c in expected] and report['heads']==[expected[-1]['hash']]
                and json.loads(text(info,5))==report['heads'] and report['changes'][-1]['text']==live+'\ufeff','Second middle lineage is not exact live target prefix')
    return ids[1:],rows,infos

def action(e,children):
    return dict(child=children[0],position=e.positions[1]+':temp',offset_utf16=e.offset,parent_draft=rtml(e.left),child_draft=rtml(e.tail),
                parent_updates=[rtml(t) for i,t in e.between if i==0],child_updates=[rtml(t) for i,t in e.between if i==1],
                second_child=children[1],second_position=e.positions[2]+':temp',second_target='Original' if e.second_middle=='parent' else 'FirstChild',
                second_enter_draft=rtml(e.second_before),second_offset_utf16=e.second_offset,second_parent_draft=rtml(e.second_left),second_child_draft=rtml(e.second_right),
                final_updates=[[rtml(t) for j,t in e.after if j==i] for i in range(3)])

def journal(value,e,write=None,original=None):
    require(isinstance(value,dict) and set(value)=={'scope','receipt'} and value['scope']==S.SCOPE,'Second middle journal scope changed')
    if write is None:require(value['receipt'] is None,'Unexpected second middle journal');return
    receipt=value['receipt'];require(isinstance(receipt,dict) and isinstance(receipt.get('operation'),str) and receipt['operation'],'Second middle operation missing')
    c=receipt.get('checkpoint',{})
    require(set(c)=={'version','scope','thread','document','target','session','sequence','action','draft','sections','request'}
            and type(c.get('version')) is int and c['version']==18 and c['scope']==S.SCOPE and c['thread']=='thread' and c['document']=='document'
            and c['target']==e.parent and c['session']==write['session'] and type(c['sequence']) is int and c['sequence']==1 and c['draft']==e.full_wire
            and c['action']=={'SplitSequence':action(e,[write['child'],write['secondChild']])},'Second middle v18 source/two boundaries/full reports changed')
    require(S.byte_vector(c['request'])==base64.b64decode(write['documentData'],validate=True),'Second middle durable packet differs from dispatch')
    expected={r['id']:base64.b64decode(r['raw'],validate=True) for r in original['sections']};stored=c['sections']
    require(isinstance(stored,list) and len(stored)==len(expected) and {text(wire(S.byte_vector(r)),1):S.byte_vector(r) for r in stored}==expected,'Second middle original inventory changed')


def selectors(mode,typing,pre_enter,middle_parent,middle_cadence,second_middle,second_middle_cadence):
    require(second_middle in ('none','parent','child') and second_middle_cadence in ('straight','edited'), 'Unknown second-middle cohort')
    require((second_middle!='none' or second_middle_cadence=='straight') and (second_middle=='none' or
            (mode=='middle' and typing=='none' and pre_enter=='none' and middle_parent=='none' and middle_cadence=='parent-first')),
            'Second middle requires middle mode and default single-split input selectors')
