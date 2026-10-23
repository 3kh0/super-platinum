"""Typed empty Enter: unchanged original STATUS and exact selected report history.

The fixed patch groups are independently transcribed from the retained original
runtime's 42 vectors, not inferred from the request's asserted final content.
"""
import base64
from dataclasses import dataclass
import re

import canvas_empty_enter_wire as A
import canvas_split_oracle as S
import canvas_end_typing_oracle as T
import canvas_middle_typing_wire as M
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Config(A.Config):
    corpus: str='ascii'
    typing: str='correction'
    updates: tuple=()
    reports: tuple=()


def config(typing='correction',empty_history='bootstrap',empty_layout='no-following',corpus='ascii'):
    old=A.config(empty_history,empty_layout)
    require(typing in ('correction','erase','restore') and corpus in ('ascii','unicode'),'Unpinned typed-empty cohort')
    prefix='New' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 &'
    updates=(prefix,prefix+' draftx',prefix+' draft',prefix+' paragraph') if typing=='correction' else (prefix,'') if typing=='erase' else (prefix,prefix+' draftx',prefix,'')
    return Config(**vars(old),corpus=corpus,typing=typing,updates=updates,reports=tuple(map(M.rtml,updates)))


def labels(value,e,recovery=False):
    require(value.get('typing')==e.typing and value.get('corpus')==e.corpus,'Typed-empty corpus/cadence changed')
    A.labels(dict(value,typing='none',corpus='ascii'),e,recovery)


def inventory(value,e,original=None,selected=None,selected_saved=False,original_saved=False,stale=False):
    # The accepted inventory gate checks all raw fields, positions, languages and
    # untouched rows. Substitute only the independently checked selected body for
    # that gate; its complete original bytes remain authoritative below/at ACK.
    if not selected or selected_saved:return A.inventory(value,e,original,selected,selected_saved,original_saved,stale)
    import copy
    checked=copy.deepcopy(value)
    item=next((r for r in checked.get('sections',[]) if r.get('id')==selected),None)
    require(item is not None,'Typed-empty selected canonical row missing')
    row=decoded(item.get('raw'));require(A.body(row,selected,e)==e.reports[-1],'Typed-empty canonical selected body/language changed')
    # A Config.neighbor is empty for reused selection; new selection is also empty.
    content=field(row,12,2);inner=wire(content);leaf=wire(field(inner,1,2))
    require(T.tags(inner)==[1] and T.tags(leaf)==[1],'Typed-empty selected Content shape changed')
    replacement=b'\x0a\x02\x0a\x00'
    item['raw']=base64.b64encode(b''.join((b'\x62'+bytes([len(replacement)])+replacement) if f[0]==12 else f[3] for f in row)).decode()
    A.inventory(checked,e,original,selected,False,original_saved,stale)
    return {r['id']:decoded(r['raw']) for r in value['sections']}


def patch_groups(e):
    p=M.patch;n=3 if e.corpus=='ascii' else 14;prefix=e.reports[0]
    result=[[p('splice',0,prefix)]]
    if e.typing=='erase':return result+[[p('del',0,length=n)]]
    result.append([p('splice',n,' draftx')])
    if e.typing=='restore':return result+[[p('del',n,length=7)],[p('del',0,length=n)]]
    return result+[[p('del',n+6)],[p('splice',n+1,'parag'),p('del',n+6),p('del',n+8,length=2),p('splice',n+8,'ph')]]


def selected_history(row,source,e,session):
    now=wire(field(row,38,2));identity=text(row,1);actor=session+'00000000'
    require(text(now,1)==identity and field(now,2,0)==(e.base if e.reused else 0)
            and field(now,3,0)==(1000 if e.reused else 0) and not any(f[0] in (4,7,10) for f in now),'Typed-empty selected session base/owner/lineage changed')
    require(all(f[0] in (1,2,3,5,6,8,11,12) for f in now)
            and all(sum(f[0]==tag for f in now)==1 for tag in (1,2,3,5,8,12)),'Typed-empty selected Session fields changed')
    before=S.messages(source,38) if source else []
    if before:
        A.source_session(source,e)
        require(S.stable(now,(4,5,6,7,12))==S.stable(before[0],(4,5,6,7,12)),'Typed-empty selected retained metadata changed')
        current,prior=T.decode_history(field(now,5,2),field(before[0],5,2));prefix=prior['changes'];heads=[f[2] for f in before[0] if f[0]==6]
    else:
        require(T.tags(now)==([1,2,3,5,6,6,6,8,12] if e.reused else [1,2,3,5,6,8,12]),'Typed-empty bootstrap session shape changed')
        current=T.decode_history(field(now,5,2))[0];prefix=current['changes'][:1];first=prefix[0]
        M.chain(prefix,[],actor)
        require(first['text']=='\ufeff' and first['hash']==text(now,8),'Typed-empty initial bootstrap changed')
        heads=[first['hash'].encode()] if e.reused else []
    M.chain(current['changes'],prefix,actor)
    changes=current['changes'][len(prefix):];groups=patch_groups(e);patches=[p for group in groups for p in group]
    state='\ufeff';states=[]
    for p in patches:state=M.apply_patch(state,p);states.append(state)
    require([c['text'] for c in changes]==states and [c['patches'] for c in changes]==[[p] for p in patches],
            'Typed-empty lost, squashed, reordered or fabricated intermediate mutations')
    cursor=0
    for group,report in zip(groups,e.reports):
        cursor+=len(group);require(changes[cursor-1]['text']==report+'\ufeff','Typed-empty report endpoint differs')
    final=field(now,12,2)
    require(current['heads']==[text(now,12)] and [f[2] for f in now if f[0]==6]==(heads+[final]+heads if e.reused else [final]),
            'Typed-empty processed heads lost exact new/reused order')


def write(value,original,e):
    require(value.get('kind')=='empty' and value.get('parent')==e.original and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and value.get('clearsChild') is False and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Typed-empty request owner/session changed')
    selected=value.get('child')
    require(selected==e.following if e.reused else isinstance(selected,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',selected) and selected not in original,
            'Typed-empty selected identity/placement changed')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,1,14],'Typed-empty Save is not one whole pair');S.rich_title(data)
    parent,child=S.messages(data,1);A.status(parent,original[e.original],value,e)
    expected=[1,2,9,12,13,19,20,33,38] if e.reused else [1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38]
    require(T.tags(child)==expected and text(child,1)==selected and field(child,2,0)==(1000 if e.reused else 0)
            and S.body(child,False)==e.reports[-1] and field(child,9,0)==0 and field(child,13,2)==b''
            and field(child,19,0)==field(child,20,0)==1 and text(child,33)==value['session']+'-0','Typed-empty selected CONTENT/trace changed')
    if not e.reused:
        require(text(child,8)==text(child,21)=='aad:temp' and field(child,16,2)==b'\x20\x00'
                and all(field(child,k,0)==0 for k in (3,10,11,29,35)),'Typed-empty new full creation attributes changed')
    selected_history(child,original.get(selected),e,value['session'])
    return selected,parent,child


def journal(value,e,write_value=None,original=None):
    if write_value is None:return A.journal(value,e)
    require(isinstance(value,dict) and isinstance(value.get('receipt'),dict),'Typed-empty retained journal missing')
    c=value['receipt'].get('checkpoint',{})
    require(type(c.get('version')) is int and c['version']==11 and c.get('action')=={'EmptyTyping':dict(child=write_value['child'],
            position='aae' if e.reused else 'aad:temp',reused=e.reused,child_updates=list(e.reports))}
            and type(c['action']['EmptyTyping']['reused']) is bool,'Typed-empty v11 report custody changed')
    # Reuse the strict unchanged envelope/source/request gate with only the
    # already validated action/version represented in its accepted v10 form.
    adapted=dict(value,receipt=dict(value['receipt'],checkpoint=dict(c,version=10,action={'EmptyEnter':dict(child=write_value['child'],
                 position='aae' if e.reused else 'aad:temp',reused=e.reused)})))
    A.journal(adapted,e,write_value,original)


ack=A.ack
continuation=A.continuation
