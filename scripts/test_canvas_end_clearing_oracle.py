"""Portable clearing fixtures use real original Automerge, never oracle mutations."""
import base64
import copy
from functools import lru_cache
import json
from pathlib import Path
import subprocess
import unittest

import canvas_end_clearing_oracle as O
import canvas_split_oracle as S
import canvas_reuse_base as B
import test_canvas_end_typing_oracle as T
import test_canvas_end_split_oracle as E
import test_canvas_end_reuse_oracle as R
from test_canvas_split_oracle import f,b64,body,blocks,replace,remove,merge,inventory,acknowledgement,SESSION,TITLE
ROOT=Path(__file__).resolve().parent.parent


def source_literal(content):
    mixed=" \u00a0\u1680\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u200b\u202f\u205f\u3000"
    return {'spaces':'   ','nbsp':'\u00a0','br':'<br>','mixed':mixed,'mixed-br':mixed+'<br>'+mixed[::-1],
            'separators':' \u2028\u00a0\u2029\u200b','feff-leading':'\ufeff ',
            'feff-interior':' \ufeff\u00a0','feff-post-br':'<br>\ufeff'}[content]
def literals(corpus,typing):return [''] if typing=='none' else T.literals(corpus,typing)
def original_session(base,content):
    milestone='canvas-nonliteral-reuse-20261023' if content in ('spaces','nbsp','br') else 'canvas-whitespace-reuse-20261023'
    seeds=json.loads((ROOT/'tmp'/milestone/'reference/child-seeds.json').read_text(encoding='utf-8'))['seeds']
    shape={'spaces':'ascii-space','mixed':'mixed-w','mixed-br':'mixed-w-br','separators':'ls-ps'}.get(content,content)
    return base64.b64decode(next(s['sessionBase64'] for s in seeds if s['cohort']==base and s['shape']==shape))


@lru_cache(maxsize=40)
def histories(corpus='ascii',typing='none',reuse_base='positive',reuse_content='spaces',reports_override=None,squash_first=False):
    source=S.field(S.wire(original_session(reuse_base,reuse_content)),5,2)
    updates=['']+([] if typing=='none' else T.literals(corpus,typing))
    if reports_override is not None:updates=list(reports_override)
    continued=E.expected(corpus).edited if typing=="none" else ("Continued paragraph" if corpus=="ascii" else "Continued \u65e5\u672c\U0001f600 paragraph")
    script=r'''
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';
import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));
let doc=A.load(new Uint8Array(Buffer.from(p.source,'base64')),{actor:p.session+'00000000'});
for(const [index,text] of p.updates.entries()){
 if(p.squash_first&&index===0){doc=A.change(doc,d=>A.updateText(d,['text'],text+'\ufeff'));continue;}
 const old=A.getHeads(doc),planned=A.change(A.clone(doc),d=>A.updateText(d,['text'],text+'\ufeff'));
 const patches=A.diff(planned,old,A.getHeads(planned));
 for(const patch of patches){
  if(patch.action==='splice')doc=A.change(doc,d=>A.splice(d,['text'],patch.path[1],0,patch.value));
  else if(patch.action==='del')doc=A.change(doc,d=>A.splice(d,['text'],patch.path[1],patch.length??1,''));
  else throw Error('Unexpected independent source patch');
 }
}
const final=Buffer.from(A.save(doc)).toString('base64'),head=A.getHeads(doc)[0];
const after=A.change(A.clone(doc,{actor:'cccccccccccc00000000'}),d=>A.updateText(d,['text'],p.continued+'\ufeff'));
console.log(JSON.stringify({final,head,continued:Buffer.from(A.save(after)).toString('base64'),continuedHead:A.getHeads(after)[0]}));
'''
    r=subprocess.run(['node','--input-type=module','-e',script],cwd=ROOT,input=json.dumps(dict(source=b64(source),session=SESSION,updates=updates,continued=continued,squash_first=squash_first)),text=True,encoding='utf-8',capture_output=True,timeout=30)
    if r.returncode:raise RuntimeError(r.stderr[:1000])
    return json.loads(r.stdout)


def source_and_request(corpus='ascii',typing='none',reuse_base='positive',reuse_content='spaces',reports_override=None,squash_first=False):
    source=R.initial(corpus); child=B.ZERO_CHILD if reuse_base=='zero' else 'empty-final'
    seed=replace(replace(original_session(reuse_base,reuse_content),1,child),4,8_000_000_000_000_000)
    item=source['sections'][-1];raw=base64.b64decode(item['raw'])
    item.update(id=child,raw=b64(replace(replace(replace(raw,1,child),12,body(source_literal(reuse_content),False)),38,seed)))
    h=histories(corpus,typing,reuse_base,reuse_content,reports_override,squash_first)
    parsed={r['id']:S.decoded(r['raw']) for r in source['sections']};sent=[]
    for identity in ('paragraph',child):
        crdt=S.field(parsed[identity],38,2)
        for tag in (4,7,12):crdt=remove(crdt,tag)
        row=f(1,identity)+f(2,1000)+f(13,b'')+f(19,1)+f(20,1)+f(33,SESSION+'-0')
        if identity==child:
            prior=[v for tag,_,v,_ in S.wire(crdt) if tag==6]
            crdt=replace(replace(remove(crdt,6),5,base64.b64decode(h['final'])),12,h['head'])
            crdt+=b''.join(f(6,x) for x in prior+[h['head'].encode()]+prior)
            row+=f(9,0)+f(12,body(literals(corpus,typing)[-1],False))
        sent.append(row+f(38,crdt))
    write=dict(clearsChild=True,kind='reuse',parent='paragraph',child=child,session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,documentData=b64(f(14,TITLE)+b''.join(f(1,r) for r in sent)))
    return source,write,sent


def checkpoint(source,write,corpus='ascii',typing='none'):
    c=E.expected(corpus)
    return dict(version=7,scope=S.SCOPE.copy(),thread='thread',document='document',target='paragraph',session=SESSION,sequence=1,
                action={'EndClearing':dict(child=write['child'],position='aag',offset_utf16=c.offset,child_updates=[] if typing=='none' else T.literals(corpus,typing))},draft=c.wire,
                sections=[list(base64.b64decode(r['raw'])) for r in source['sections']],request=list(base64.b64decode(write['documentData'])))


def typed_witness(generation,corpus,typing):
    if typing!='none':return T.typed_witness(generation,corpus,typing)
    result=E.witness(generation,corpus);result['typing']=None;return result


def authored(sent,source,mode,corpus,typing,continued=False):
    rows=R.authored(sent,source,corpus,continued)
    result=[]
    for row in rows:
        parent=S.text(S.wire(row),1)=='paragraph'
        content=E.expected(corpus).wire if parent else (E.expected(corpus).edited if typing=="none" else ("Continued paragraph" if corpus=="ascii" else "Continued \u65e5\u672c\U0001f600 paragraph")) if continued else literals(corpus,typing)[-1]
        result.append(replace(row,12,body(content,parent)))
    return result


def receipt(scenario="success", corpus="ascii", typing="none", reuse_base="positive", reuse_content="spaces"):
    mode='reuse'
    packet = R.receipt(scenario, corpus) if mode == 'reuse' else E.receipt(scenario, corpus)
    source, write, sent = source_and_request(corpus,typing,reuse_base,reuse_content); child=write['child']; c=E.expected(corpus)
    pair = authored(sent,source,mode,corpus,typing)
    originals={r['id']:base64.b64decode(r['raw']) for r in source['sections']}
    def inv(pair): return inventory([originals['title'],originals['neighbor'],*pair] if mode=='reuse' else [originals['title'],*pair,originals['neighbor']])
    readback=inv(pair); h=histories(corpus,typing,reuse_base,reuse_content)
    crdt=remove(S.field(S.wire(pair[1]),38,2),4)
    crdt=replace(replace(crdt,5,base64.b64decode(h['continued'])),12,h['continuedHead'])+f(6,h['continuedHead'])
    continuation=E.expected(corpus).edited if typing=="none" else ("Continued paragraph" if corpus=="ascii" else "Continued \u65e5\u672c\U0001f600 paragraph")
    row=f(1,child)+f(2,2000 if mode=='reuse' else 1000)+f(9,0)+f(12,body(continuation,False))+f(13,b'')+f(19,1)+f(20,1)+f(33,'c'*12+'-0')+f(38,crdt)
    second=dict(kind='text',parent=child,session='c'*12,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,documentData=b64(f(1,row)+f(14,TITLE)))
    edited=authored([row],readback,mode,corpus,typing,True)[0]; final=inv([pair[0],edited])
    held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='whole-typed-pair',checkpoint=checkpoint(source,write,corpus,typing)))
    original=dict(packet['original'],typing=typing,reuseBase=reuse_base,reuseChild=child,reuseContent=reuse_content,fresh=source,visible=source)
    packet.update(typing=typing,reuseBase=reuse_base,reuseChild=child,reuseContent=reuse_content,original=copy.deepcopy(original),cancelledWitness=typed_witness(1,corpus,typing),savedWitness=typed_witness(2,corpus,typing))
    for i,stage in enumerate(packet['stages']):
        if i==0: stage['stage']='split' if typing=='none' else 'typed-child'
        name=stage['stage']; proof=copy.deepcopy(original)
        if i>=2:
            fresh=copy.deepcopy(source)
            if scenario=='stale':
                target=next(r for r in fresh['sections'] if r['id']==(child if mode=='reuse' else 'paragraph'))
                target.update(sequence=3000,raw=b64(replace(base64.b64decode(target['raw']),2,3000)))
            proof.update(fresh=fresh,preflights=[dict(source=source,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':
                proof.update(writes=[write],reservations=1)
                if i>=3:
                    proof['acknowledgements']=[acknowledgement(pair,scenario=='partial')]
                    if scenario=='success': proof.update(fresh=readback,visible=readback,readbacks=[readback],read=2)
                if i>=5:
                    proof['preflights'].append(dict(source=readback,fresh=readback,outcome='prepared',kind='text'));proof['writes'].append(second)
                    if i==6: proof.update(fresh=final,visible=final,readbacks=[readback,final],read=3,acknowledgements=[acknowledgement(pair),acknowledgement([edited],continued=True)])
        pending=name in ('saving','continued-saving'); uncertain=name=='uncertain'
        proof.update(pending=pending,stale=pending or uncertain,operation=dict(serial=1,stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        stage['fixture']=copy.deepcopy(proof);stage['journal']=copy.deepcopy(held if name in ('saving','uncertain') else packet['initialJournal'])
        geometry=stage['geometry']; editor=geometry['native']
        if editor is not None:
            if name in ('split','typed-child','saving','refused','uncertain'):
                w=packet['cancelledWitness'] if i==0 else packet['savedWitness'];typed=w if typing=='none' else w['typing']
                editor.update(generation=w['generation'],visit=w['visit'],blocks=typed['after'],rawBlocks=typed['rawAfter'],placeholder=typed['placeholder'],typing=None if typing=='none' else typed,text=c.seed+literals(corpus,typing)[-1])
            else:
                content=literals(corpus,typing)[-1] if name=='child-editing' else continuation
                snapshot=blocks(content)
                if not content:snapshot[0]['elements'][0]['elements']=[]
                editor.update(blocks=snapshot,rawBlocks=copy.deepcopy(snapshot),placeholder=None,typing=None,text=content)
        else:
            items=[dict(id='paragraph',text=c.seed)]
            if mode=='reuse' or scenario=='success' and i>=3: items.append(dict(id=child,text=continuation if i==6 else literals(corpus,typing)[-1] if scenario=='success' and i>=3 else source_literal(reuse_content).replace('<br>','')))
            neighbor=dict(id='neighbor',text='Another paragraph');geometry['paragraphs']=[neighbor,*items] if mode=='reuse' else [*items,neighbor]
    packet.update(fixture=packet['stages'][-1]['fixture'],layout=packet['stages'][-1]['geometry'],finalJournal=packet['stages'][-1]['journal'])
    return packet


class ClearingOracle(unittest.TestCase):
    def test_all_native_lifecycles_exact_shapes_bases_and_corpora(self):
        for base in ('positive','zero'):
            for shape in ('spaces','nbsp','br'):
                for corpus in ('ascii','unicode'):
                    for typing in ('none','correction','erase'):
                        for scenario in ('success','stale','partial'):
                            with self.subTest(base=base,shape=shape,corpus=corpus,typing=typing,scenario=scenario):
                                O.validate(receipt(scenario,corpus,typing,base,shape),corpus,typing,base,shape)

    def test_clear_boundary_cannot_be_skipped_even_with_same_final_body(self):
        for base in ('positive','zero'):
            for shape in ('spaces','nbsp','br'):
                e=O.config('unicode','correction',base,shape)
                for reports in (tuple(T.literals('unicode','correction')), (' ', '')+tuple(T.literals('unicode','correction'))):
                    source,write,_=source_and_request('unicode','correction',base,shape,reports,squash_first=reports[0]=='New')
                    with self.subTest(base=base,shape=shape),self.assertRaises(RuntimeError):O.typed_write(write,O.inventory(source,e),e)

    def test_net_empty_cannot_drop_or_reorder_user_history(self):
        e=O.config(typing='erase')
        for reports in (('',), ('','x','','x','')):
            source,write,_=source_and_request(typing='erase',reports_override=reports)
            with self.assertRaises(RuntimeError):O.typed_write(write,O.inventory(source,e),e)

    def test_original_source_content_history_base_and_lease_are_pinned(self):
        source,_,_=source_and_request(reuse_base='zero',reuse_content='nbsp');e=O.config(reuse_base='zero',reuse_content='nbsp')
        for target in ('body','history','base','lease','position','identity','duplicate'):
            value=copy.deepcopy(source);r=value['sections'][-1];raw=base64.b64decode(r['raw']);crdt=S.field(S.wire(raw),38,2)
            if target=='body':raw=replace(raw,12,body('&nbsp;',False))
            elif target=='history':raw=replace(raw,38,replace(crdt,5,b'fake'))
            elif target=='base':raw=replace(raw,38,replace(crdt,2,1000))
            elif target=='lease':raw=replace(raw,38,replace(crdt,4,1))
            elif target=='position':raw=replace(raw,8,'aae')
            elif target=='identity':raw=replace(raw,1,'allocated-child')
            else:raw=replace(raw,38,crdt+f(1,B.ZERO_CHILD))
            r['raw']=b64(raw)
            with self.subTest(target=target),self.assertRaises(RuntimeError):O.inventory(value,e)

    def test_entire_pair_and_ordered_processed_heads_are_required(self):
        source,write,sent=source_and_request();e=O.config();rows=O.inventory(source,e)
        mutations=[f(14,TITLE)+f(1,sent[1])+f(1,sent[0]),f(14,TITLE)+f(1,sent[0]),
                   f(14,TITLE)+f(1,sent[0])+f(1,sent[1])+f(99,b'unknown')]
        crdt=S.field(S.wire(sent[1]),38,2)
        processed=[v for tag,_,v,_ in S.wire(crdt) if tag==6]
        wrong=remove(crdt,6)+b''.join(f(6,v) for v in [processed[1],processed[0],processed[2]])
        mutations.append(f(14,TITLE)+f(1,sent[0])+f(1,replace(sent[1],38,wrong)))
        mutations.append(f(14,TITLE)+f(1,replace(sent[0],12,body('Keep this')))+f(1,sent[1]))
        for data in mutations:
            with self.assertRaises(RuntimeError):O.typed_write(dict(write,documentData=b64(data)),rows,e)
        with self.assertRaises(RuntimeError):O.typed_write(dict(write,clearsChild=False),rows,e)

    def test_clear_only_requires_explicit_empty_ack_and_full_readback(self):
        source,write,sent=source_and_request();e=O.config();rows=O.inventory(source,e);pair=authored(sent,source,'reuse','ascii','none')
        for content in (None,'   ','<br>','x'):
            child=remove(pair[1],12) if content is None else replace(pair[1],12,body(content,False))
            with self.subTest(content=content),self.assertRaises(RuntimeError):O.ack(acknowledgement([pair[0],child]),[S.wire(x) for x in sent],rows,e)
        packet=receipt();packet['stages'][3]['fixture']['readbacks'][0]['sections'][-1]['raw']=source['sections'][-1]['raw']
        with self.assertRaises(RuntimeError):O.validate(packet)

    def test_checkpoint_v7_keeps_only_post_enter_reports_and_exact_whole_source(self):
        source,write,_=source_and_request(typing='erase');e=O.config(typing='erase')
        original=checkpoint(source,write,typing='erase')
        for target in ('v5','v6','clear-prefix','drop-report','source','request'):
            c=copy.deepcopy(original)
            if target in ('v5','v6'):c['version']=int(target[1])
            elif target=='clear-prefix':c['action']['EndClearing']['child_updates'].insert(0,'')
            elif target=='drop-report':c['action']['EndClearing']['child_updates']=[]
            elif target=='source':c['sections'].pop()
            else:c['request']=c['request'][:-1]
            with self.subTest(target=target),self.assertRaises(RuntimeError):O.journal(dict(scope=S.SCOPE,receipt=dict(operation='held',checkpoint=c)),e,write,source)

    def test_stale_partial_and_cancel_cannot_release_or_dispatch(self):
        for scenario in ('stale','partial'):
            packet=receipt(scenario)
            if scenario=='stale':packet['stages'][2]['fixture']['writes']=[{}]
            else:packet['stages'][-1]['journal']['receipt']=None
            with self.subTest(scenario=scenario),self.assertRaises(RuntimeError):O.validate(packet)
        packet=receipt();packet['stages'][1]['fixture']['fresh']['sections'].pop()
        with self.assertRaises(RuntimeError):O.validate(packet)

    def test_br_reader_text_is_empty_while_review_and_canonical_stay_distinct(self):
        for corpus in ('ascii','unicode'):
            for base in ('positive','zero'):
                with self.subTest(corpus=corpus,base=base):
                    packet=receipt('success',corpus,'none',base,'br')
                    O.validate(packet,corpus,'none',base,'br')
                    expected=O.config(corpus,'none',base,'br')
                    self.assertEqual(expected.reader_text,'')
                    self.assertEqual(expected.source_plain,'\n')
                    self.assertEqual(S.body(S.decoded(packet['original']['fresh']['sections'][-1]['raw']),False),'<br>')
                    packet['stages'][1]['geometry']['paragraphs'][-1]['text']='\n'
                    with self.assertRaises(RuntimeError):O.validate(packet,corpus,'none',base,'br')

    def test_caller_selection_and_trusted_empty_projection_are_required(self):
        for fieldname,value in [('reuseContent','empty'),('reuseBase','zero'),('typing','erase'),('mode','end')]:
            packet=receipt();packet[fieldname]=value
            with self.subTest(field=fieldname),self.assertRaises(RuntimeError):O.validate(packet)
        for key,value in [('inputTrusted',False),('offset',8),('kind','split')]:
            packet=receipt();packet['savedWitness'][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):O.validate(packet)


if __name__=='__main__':unittest.main()
