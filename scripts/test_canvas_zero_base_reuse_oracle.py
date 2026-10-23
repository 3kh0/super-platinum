"""Explicit retained-zero fixtures, real history, and cohort-confusion negatives."""
import base64
import copy
from functools import lru_cache
import json
from pathlib import Path
import subprocess
import unittest

import canvas_reuse_base as B
import canvas_end_reuse_oracle as R
import canvas_end_reuse_recovery_oracle as RR
import canvas_end_typing_oracle as T
import canvas_end_typing_recovery_oracle as TR
import canvas_split_oracle as S
import test_canvas_end_reuse_oracle as RF
import test_canvas_end_reuse_recovery_oracle as RRF
import test_canvas_end_typing_oracle as TF
import test_canvas_end_typing_recovery_oracle as TRF
from test_canvas_split_oracle import f, b64, replace, remove, SESSION, CHILD_HEAD, CONT_HEAD


ROOT = Path(__file__).resolve().parent.parent


def original_session():
    value = json.loads((ROOT/'tmp/canvas-end-reuse-20261023/reference/reuse-projection-result.json').read_text())
    return base64.b64decode(value['vectors'][2]['before'][1]['session'])


@lru_cache(maxsize=6)
def history(corpus='ascii', typing='none'):
    source = S.field(S.wire(original_session()),5,2)
    updates = [] if typing=='none' else TF.literals(corpus,typing)
    continued = ('New paragraph' if corpus=='ascii' else 'New 日本😀 paragraph') if typing=='none' else ('Continued paragraph' if corpus=='ascii' else 'Continued 日本😀 paragraph')
    script = r'''
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';
import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));
let doc=A.load(new Uint8Array(Buffer.from(p.source,'base64')),{actor:p.session+'00000000'});
for(const text of p.updates){
 const previous=A.getHeads(doc), planned=A.change(A.clone(doc),d=>A.updateText(d,['text'],text+'\ufeff'));
 const patches=A.diff(planned,previous,A.getHeads(planned));
 for(const patch of patches){
  if(patch.action==='splice')doc=A.change(doc,d=>A.splice(d,['text'],patch.path[1],0,patch.value));
  else if(patch.action==='del')doc=A.change(doc,d=>A.splice(d,['text'],patch.path[1],patch.length??1,''));
  else throw Error('Unsupported independent fixture patch');
 }
}
const final=Buffer.from(A.save(doc)).toString('base64'),head=A.getHeads(doc)[0];
const after=A.change(A.clone(doc,{actor:'cccccccccccc00000000'}),d=>A.updateText(d,['text'],p.continued+'\ufeff'));
console.log(JSON.stringify({final,head,continued:Buffer.from(A.save(after)).toString('base64'),continuedHead:A.getHeads(after)[0]}));
'''
    result=subprocess.run(['node','--input-type=module','-e',script],cwd=ROOT,input=json.dumps(dict(source=b64(source),session=SESSION,updates=updates,continued=continued)),
                          text=True,encoding='utf-8',capture_output=True,timeout=30)
    if result.returncode:raise RuntimeError(result.stderr[:1000])
    value=json.loads(result.stdout);value['source']=b64(source)
    return value


def zero_fixture(value, corpus='ascii', typing='none'):
    """Rebind existing protocol identity and actual retained history everywhere.

    This builds portable fixture evidence. Oracle expectations remain separate
    fixed literals, source hash, real semantic decoding and selected cohort.
    """
    h=history(corpus,typing);source=original_session();base=S.text(S.wire(source),8)
    old=TF.histories('reuse',corpus,typing) if typing!='none' else None
    docs={b'retained-empty-child':base64.b64decode(h['source']),b'continued-retained-child':base64.b64decode(h['continued'])}
    heads={CHILD_HEAD:base,CONT_HEAD:h['continuedHead']}
    if old:
        docs.update({base64.b64decode(old[k]):base64.b64decode(h[k]) for k in ('source','final','continued')})
        heads.update({old['base']:base,old['head']:h['head'],old['continuedHead']:h['continuedHead']})
    def session(data):
        parsed=S.wire(data);output=[]
        for tag,kind,v,span in parsed:
            if tag==1:span=f(1,B.ZERO_CHILD)
            elif tag==2:span=f(2,0)
            elif tag==4:span=f(4,8_000_000_000_000_000)
            elif tag==5:span=f(5,docs.get(v,v))
            elif tag in (6,8,12):span=f(tag,heads.get(v.decode(),v.decode()))
            elif tag==11:span=f(11,S.field(S.wire(source),11,2))
            output.append(span)
        return b''.join(output)
    def section(data):
        parsed=S.wire(data)
        if S.text(parsed,1)!='empty-final':return data
        return b''.join(f(1,B.ZERO_CHILD) if tag==1 else f(38,session(v)) if tag==38 else span for tag,_,v,span in parsed)
    def document(data):return b''.join(f(1,section(v)) if tag==1 else span for tag,_,v,span in S.wire(data))
    def ack(data):return b''.join(f(2,document(v)) if tag==2 else span for tag,_,v,span in S.wire(data))
    def visit(node):
        if isinstance(node,list):return [visit(v) for v in node]
        if not isinstance(node,dict):return B.ZERO_CHILD if node=='empty-final' else node
        result={k:visit(v) for k,v in node.items()}
        if 'corpus' in result and ('mode' in result or 'splitMode' in result):result.update(reuseBase='zero',reuseChild=B.ZERO_CHILD)
        if 'documentData' in result:result['documentData']=b64(document(base64.b64decode(node['documentData'])))
        if 'raw' in result:
            result['raw']=b64((section if 'id' in node else ack)(base64.b64decode(node['raw'])))
        if node.get('version') in (5,6) and 'request' in node:
            result['request']=list(document(bytes(node['request'])))
            result['sections']=[list(section(bytes(v))) for v in node['sections']]
        return result
    return visit(value)


def receipt(scenario='success',corpus='ascii',typing='none'):
    old=RF.receipt(scenario,corpus) if typing=='none' else TF.receipt(scenario,corpus,'reuse',typing)
    return zero_fixture(old,corpus,typing)


def packets(phase,corpus='ascii',typing='none'):
    old=RRF.packets(phase,corpus) if typing=='none' else TRF.packets(phase,corpus,'reuse',typing)
    return tuple(zero_fixture(list(old),corpus,typing))


def validate(value,corpus='ascii',typing='none',reuse_base='zero'):
    if typing=='none':return R.validate(value,corpus,reuse_base=reuse_base)
    return T.validate(value,corpus,'reuse',typing,reuse_base=reuse_base)


class ZeroBaseOracle(unittest.TestCase):
    def test_four_retained_original_zero_vectors_prove_history_and_duplicate_heads(self):
        values=json.loads((ROOT/'tmp/canvas-zero-base-reuse-20261023/reference/typed-zero-result.json').read_text())['vectors']
        self.assertEqual(len(values),4)
        for value in values:
            child=S.messages(S.decoded(value['wireBase64']),1)[1];session=S.wire(S.field(child,38,2))
            current,prior=T.decode_history(S.field(session,5,2),base64.b64decode(value['sourceHistory']))
            self.assertEqual(S.field(session,2,0),0);self.assertEqual(S.field(session,3,0),1000)
            self.assertEqual(current['changes'][0]['raw'],prior['changes'][0]['raw'])
            self.assertEqual([c['text'] for c in current['changes'][1:]],[v+'\ufeff' for v in T.mutation_states(TF.literals(value['corpus'],value['cadence']))])
            self.assertEqual([c['patches'] for c in current['changes'][1:]],[[p] for p in T.mutation_patches(TF.literals(value['corpus'],value['cadence']))])
            self.assertEqual([v.decode() for n,_,v,_ in session if n==6],[B.ZERO_HEAD,current['heads'][0],B.ZERO_HEAD])

    def test_all_main_scenarios_with_existing_zero_identity_and_history(self):
        for corpus in ('ascii','unicode'):
            for typing in ('none','correction','erase'):
                for scenario in ('success','stale','partial'):
                    with self.subTest(corpus=corpus,typing=typing,scenario=scenario):validate(receipt(scenario,corpus,typing),corpus,typing)

    def test_all_recovery_modes_retain_v5_v6_without_replay(self):
        for corpus in ('ascii','unicode'):
            for typing in ('none','correction','erase'):
                for phase in RR.STAGES:
                    args=packets(phase,corpus,typing)
                    with self.subTest(corpus=corpus,typing=typing,phase=phase):
                        if typing=='none':RR.validate(phase,*args,corpus,reuse_base='zero')
                        else:TR.validate(phase,*args,corpus,'reuse',typing,reuse_base='zero')

    def test_positive_default_and_explicit_zero_cannot_be_confused(self):
        for typing in ('none','correction'):
            r=receipt(typing=typing)
            with self.assertRaises(RuntimeError):validate(r,typing=typing,reuse_base='positive')
            r.pop('reuseBase')
            with self.assertRaises(RuntimeError):validate(r,typing=typing)
            old=RF.receipt() if typing=='none' else TF.receipt(mode='reuse')
            with self.assertRaises(RuntimeError):validate(old,typing=typing)
        with self.assertRaises(RuntimeError):T.config(mode='end',reuse_base='zero')

    def test_known_zero_source_requires_retained_session_and_real_heads(self):
        source=receipt()['original']['fresh'];row=source['sections'][-1];raw=base64.b64decode(row['raw']);crdt=S.field(S.wire(raw),38,2)
        for tag,value in ((2,1000),(3,0),(4,0),(5,b'new empty bootstrap'),(6,'f'*64),(8,'f'*64),(11,f(1,B.ZERO_HEAD)+f(2,0))):
            changed=copy.deepcopy(source);changed['sections'][-1]['raw']=b64(replace(raw,38,replace(crdt,tag,value)))
            with self.subTest(tag=tag),self.assertRaises(RuntimeError):R.inventory(changed,reuse_base='zero')
        changed=copy.deepcopy(source);changed['sections'][-1]['id']='empty-final'
        with self.assertRaises(RuntimeError):R.inventory(changed,reuse_base='zero')

    def test_ack_cannot_normalize_base_zero_to_canonical_revision(self):
        r=receipt(typing='correction');source=r['original']['fresh'];rows=T.inventory(source,T.config(mode='reuse',reuse_base='zero'))
        write=r['stages'][2]['fixture']['writes'][0];_,parent,child=T.typed_write(write,rows,T.config(mode='reuse',reuse_base='zero'))
        value=copy.deepcopy(r['stages'][3]['fixture']['acknowledgements'][0]);outer=S.decoded(value['raw']);data=S.wire(S.field(outer,2,2));members=S.messages(data,1)
        crdt=replace(S.field(members[1],38,2),2,2000);tail=replace(S.raw(members[1]),38,crdt)
        value['raw']=b64(f(1,1)+f(2,f(15,'document')+f(1,S.raw(members[0]))+f(1,tail)))
        with self.assertRaises(RuntimeError):T.ack(value,[parent,child],rows,T.config(mode='reuse',reuse_base='zero'))

    def test_zero_typing_still_requires_prefix_order_and_repeated_processed_heads(self):
        for typing in ('correction','erase'):
            r=receipt(typing=typing);expected=T.config(mode='reuse',typing=typing,reuse_base='zero');rows=T.inventory(r['original']['fresh'],expected)
            write=copy.deepcopy(r['stages'][2]['fixture']['writes'][0]);data=S.decoded(write['documentData']);parent,child=S.messages(data,1);crdt=S.field(child,38,2)
            for altered in (replace(crdt,5,S.field(S.wire(original_session()),5,2)),remove(crdt,11),replace(crdt,6,B.ZERO_HEAD)):
                tail=replace(S.raw(child),38,altered);bad=dict(write,documentData=b64(f(14,S.field(data,14,2))+f(1,S.raw(parent))+f(1,tail)))
                with self.assertRaises(RuntimeError):T.typed_write(bad,rows,expected)

    def test_nonreuse_label_is_null_without_changing_positive_default(self):
        r=TF.receipt();r.update(reuseBase='positive',reuseChild=None);T.validate(r)

    def test_recovery_cannot_change_existing_zero_child_to_creation_action(self):
        for typing in ('none','correction','erase'):
            stages,terminal,checkpoint=packets('restart',typing=typing)
            key='EndReuse' if typing=='none' else 'EndTyping'
            checkpoint['action'][key]['child']='empty-final'
            with self.subTest(typing=typing),self.assertRaises(RuntimeError):
                if typing=='none':RR.validate('restart',stages,terminal,checkpoint,reuse_base='zero')
                else:TR.validate('restart',stages,terminal,checkpoint,mode='reuse',typing=typing,reuse_base='zero')


if __name__=='__main__':unittest.main()
