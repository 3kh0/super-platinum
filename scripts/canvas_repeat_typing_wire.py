"""Third-only ordered typing after two End events; exact v13 triple custody."""
import base64
import copy
from dataclasses import dataclass, replace
import re

import canvas_repeat_enter_wire as A
import canvas_empty_typing_wire as H
from canvas_repeat_enter_wire import S, T, M, require, wire, decoded, field, text


@dataclass(frozen=True)
class Config(A.Config):
    typing:str='correction'
    updates:tuple=()
    reports:tuple=()


def config(typing='correction',repeat_history='bootstrap',repeat_layout='no-following',corpus='ascii'):
    old=A.config(repeat_history,repeat_layout,corpus)
    require(typing in ('correction','erase','restore'),'Unpinned repeat typing cadence')
    prefix='New' if corpus=='ascii' else 'New \u65e5\u672c\U0001f600 &'
    updates=(prefix,prefix+' draftx',prefix+' draft',prefix+' paragraph') if typing=='correction' else (prefix,'') if typing=='erase' else (prefix,prefix+' draftx',prefix,'')
    return Config(**vars(old),typing=typing,updates=updates,reports=tuple(map(M.rtml,updates)))


def labels(value,e,recovery=False):
    require(value.get('typing')==e.typing,'Repeat typing caller cadence changed')
    A.labels(dict(value,typing='none'),e,recovery)


def contents(e,selected=None,completed=0):
    values=A.contents(e,selected,completed)
    if selected and completed==0:values[selected[1]]=e.updates[-1]
    return values


def inventory(value,e,original=None,selected=None,completed=0,stale=False):
    if not selected or completed:return A.inventory(value,e,original,selected,completed,stale)
    checked=copy.deepcopy(value)
    item=next((r for r in checked.get('sections',[]) if r.get('id')==selected[1]),None)
    require(item is not None,'Typed third canonical row missing')
    row=decoded(item.get('raw'));require(S.body(row,False)==e.reports[-1],'Typed third canonical body/language changed')
    # Validate the exact actual body before adapting only that field to the
    # unchanged inventory gate. All original bytes remain authoritative at ACK.
    item['raw']=base64.b64encode(b''.join(b'\x62\x04\x0a\x02\x0a\x00' if f[0]==12 else f[3] for f in row)).decode()
    A.inventory(checked,e,original,selected,completed,stale)
    return {r['id']:decoded(r['raw']) for r in value['sections']}


def creation(row,identity,value,e):
    position=text(row,8)
    require(T.tags(row)==[1,2,3,8,9,10,11,12,13,16,19,20,21,29,33,35,38] and text(row,1)==identity
            and field(row,2,0)==0 and text(row,21)==position and re.fullmatch('[A-Za-z]{1,256}:temp',position)
            and S.body(row,False)==e.reports[-1] and field(row,13,2)==b'' and field(row,16,2)==b'\x20\x00'
            and field(row,19,0)==field(row,20,0)==1 and text(row,33)==value['session']+'-0'
            and all(field(row,tag,0)==0 for tag in (3,9,10,11,29,35)),'Typed third creation/body changed')
    # The same independently pinned patch groups were executed by all 36 repeat
    # vectors. Third is ALWAYS new, irrespective of the first selection's reuse.
    H.selected_history(row,None,replace(e,reused=False),value['session'])


def write(value,original,e):
    require(value.get('kind')=='repeat' and value.get('parent')==e.original and value.get('httpFactoryExecuted') is False
            and value.get('formTitle')=='Fixture title' and type(value.get('sequence')) is int and value['sequence']==1
            and value.get('clearsChild') is False and re.fullmatch('[0-9a-f]{12}',value.get('session','')),'Typed repeat request owner/session changed')
    first,second=value.get('child'),value.get('secondChild')
    require(first==e.following if e.reused else isinstance(first,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',first) and first not in original,
            'Typed repeat first selection changed')
    require(isinstance(second,str) and re.fullmatch('temp:C:xyz[0-9a-f]{25}',second) and second not in original and second!=first,
            'Typed third identity reused or merged')
    data=decoded(value.get('documentData'));require(T.tags(data)==[1,1,1,14],'Typed repeat requires one complete ordered triple');S.rich_title(data)
    parent,one,two=S.messages(data,1);A.status(parent,original[e.original],value,e)
    if e.reused:A.status(one,original[first],value,e)
    else:A.creation(one,first,value)
    creation(two,second,value,e)
    require([text(row,1) for row in (parent,one,two)]==[e.original,first,second],'Typed repeat collected order changed')
    positions={identity:text(row,8) for identity,row in original.items()}
    if not e.reused:positions[first]=text(one,8).removesuffix(':temp')
    positions[second]=text(two,8).removesuffix(':temp');order=A.ordered_ids(e,(first,second))
    require(all(positions[a]<positions[b] for a,b in zip(order,order[1:])),'Typed repeat relative placement changed')
    return (first,second),parent,one,two


def journal(value,e,write_value=None,original=None):
    if write_value is None:return A.journal(value,e)
    require(isinstance(value,dict) and isinstance(value.get('receipt'),dict),'Typed repeat journal missing')
    c=value['receipt'].get('checkpoint',{});sent=S.messages(decoded(write_value['documentData']),1)
    action=dict(child=write_value['child'],position='aae' if e.reused else text(sent[1],8),reused=e.reused,
                second_child=write_value['secondChild'],second_position=text(sent[2],8))
    require(type(c.get('version')) is int and c['version']==13
            and c.get('action')=={'EndRepeatTyping':dict(action,second_child_updates=list(e.reports))}
            and type(c['action']['EndRepeatTyping']['reused']) is bool,'Typed repeat v13 ordered reports changed')
    adapted=dict(value,receipt=dict(value['receipt'],checkpoint=dict(c,version=12,action={'EndRepeat':action})))
    A.journal(adapted,e,write_value,original)


ack=A.ack
continuation=A.continuation
ordered_ids=A.ordered_ids
