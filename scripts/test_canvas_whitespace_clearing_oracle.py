"""Finite source/reference, lifecycle and hostile whitespace regressions."""
import base64
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import unittest

import canvas_end_clearing_oracle as O
import canvas_end_clearing_recovery_oracle as R
import canvas_split_oracle as S
import test_canvas_end_clearing_oracle as F
import test_canvas_end_clearing_recovery_oracle as G

ROOT=Path(__file__).resolve().parent.parent
REF=ROOT/'tmp/canvas-whitespace-reuse-20261023/reference'
SHAPES=('mixed','mixed-br','separators','feff-leading','feff-interior','feff-post-br')
MAP={'mixed-w':'mixed','mixed-w-br':'mixed-br','ls-ps':'separators'}


class WhitespaceClearing(unittest.TestCase):
    def test_all_36_original_ordered_histories_and_exact_sources(self):
        count=0
        for base in ('positive','zero'):
            corpus='ascii' if base=='positive' else 'unicode'
            for filename in (f'whitespace-{base}-result.json',f'feff-{base}-extension-result.json'):
                for v in json.loads((REF/filename).read_text(encoding='utf-8'))['vectors']:
                    shape=MAP.get(v['corpus'],v['corpus']);typing='none' if v['cadence']=='clear-only' else v['cadence']
                    expected=O.config(corpus,typing,base,shape)
                    with self.subTest(base=base,shape=shape,typing=typing):
                        self.assertEqual(expected.source_text,F.source_literal(shape))
                        source=S.wire(base64.b64decode(v['sourceSession']))
                        parent,child=S.messages(S.decoded(v['wireBase64']),1)
                        current=S.wire(S.field(child,38,2))
                        # The original probe has synthetic host trace end-oracle-1,
                        # but its independent Session actor is this pinned fixture actor.
                        now,prior=O.decode_history(S.field(current,5,2),S.field(source,5,2))
                        prefix=prior['changes'];actual=now['changes'][len(prefix):]
                        self.assertEqual([c['raw'] for c in now['changes'][:len(prefix)]],[c['raw'] for c in prefix])
                        self.assertEqual(prefix[-1]['text'],F.source_literal(shape)+'\ufeff')
                        states=['']+(O.typed.mutation_states(expected.user_updates) if expected.user_updates else [])
                        self.assertEqual([c['text'] for c in actual],[s+'\ufeff' for s in states])
                        self.assertEqual([c['patches'] for c in actual],[[p] for p in O.clearing_patches(expected)])
                        previous=prefix[-1]
                        for change in actual:
                            self.assertEqual(change['actor'],'0123456789abcdef0123')
                            self.assertEqual(change['seq'],previous['seq']+1)
                            self.assertEqual(change['deps'],[previous['hash']])
                            self.assertEqual((change['keys'],change['deleted'],change['marks']),(['deleted','text'],False,[]))
                            previous=change
                        self.assertEqual(now['heads'],[S.text(current,12)])
                        self.assertEqual(O.tags(parent),[1,2,13,19,20,33,38])
                        self.assertEqual(O.tags(child),[1,2,9,12,13,19,20,33,38])
                        self.assertEqual(S.body(child,False),expected.updates[-1])
                        self.assertEqual(S.field(current,2,0),expected.base)
                        self.assertEqual(S.field(current,3,0),1000)
                        prior=[f[2] for f in source if f[0]==6]
                        self.assertEqual([f[2] for f in current if f[0]==6],prior+[S.field(current,12,2)]+prior)
                        digest=hashlib.sha256(b''.join(f[3] for f in sorted(source,key=lambda f:f[0]) if f[0] not in (1,4))).hexdigest()
                        self.assertEqual(digest,O.SOURCE_DIGESTS[(base,shape)])
                        count+=1
        self.assertEqual(count,36)

    def test_new_cohorts_main_and_recovery_with_exact_reader_projection(self):
        for base in ('positive','zero'):
            for index,shape in enumerate(SHAPES):
                typing=('none','correction','erase')[index%3];corpus='unicode' if base=='zero' else 'ascii'
                with self.subTest(base=base,shape=shape,typing=typing):
                    packet=F.receipt('success',corpus,typing,base,shape)
                    O.validate(packet,corpus,typing,base,shape)
                    R.validate('review',*G.packets('review',corpus,typing,base,shape),corpus,typing,base,shape)
                    expected=O.config(corpus,typing,base,shape)
                    self.assertEqual(expected.reader_text,F.source_literal(shape).replace('<br>',''))
                    self.assertEqual(expected.source_plain,F.source_literal(shape).replace('<br>','\n'))
                    if '<br>' in F.source_literal(shape):
                        packet['stages'][1]['geometry']['paragraphs'][-1]['text']=expected.source_plain
                        with self.assertRaises(RuntimeError):O.validate(packet,corpus,typing,base,shape)
        for scenario,shape in [('stale','separators'),('partial','feff-leading')]:
            O.validate(F.receipt(scenario,'unicode','erase','zero',shape),'unicode','erase','zero',shape)
        R.validate('storage-failure',*G.packets('storage-failure','unicode','correction','zero','mixed-br'),'unicode','correction','zero','mixed-br')

    def test_exact_invisible_sources_cannot_normalize_or_relabel(self):
        for shape in SHAPES:
            expected=O.config(reuse_base='zero',reuse_content=shape)
            source,write,_=F.source_and_request(reuse_base='zero',reuse_content=shape)
            original=copy.deepcopy(source)
            source['sections'][-1]['raw']=F.b64(F.replace(base64.b64decode(source['sections'][-1]['raw']),12,F.body(' ',False)))
            with self.subTest(shape=shape),self.assertRaises(RuntimeError):O.inventory(source,expected)
            stages,terminal,checkpoint=G.packets('review',reuse_base='zero',reuse_content=shape)
            stages[0]['preview']['originalChild']['kind']=shape
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,checkpoint,reuse_base='zero',reuse_content=shape)
            checkpoint['action']['EndClearing']['child_updates']=['']
            with self.assertRaises(RuntimeError):O.journal(dict(scope=S.SCOPE,receipt=dict(operation='held',checkpoint=checkpoint)),expected,write,original)
        for shape in ('entity','two-br','controls','empty'):
            with self.assertRaises(RuntimeError):O.config(reuse_content=shape)

    def test_leading_feff_matching_final_text_does_not_hide_wrong_deletion(self):
        source=S.wire(F.original_session('zero','feff-leading'));expected=O.config(reuse_base='zero',reuse_content='feff-leading')
        script=r"""
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';
import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));
let doc=A.load(new Uint8Array(Buffer.from(p.source,'base64')),{actor:p.actor});
doc=A.change(doc,d=>A.splice(d,['text'],0,2,''));
console.log(JSON.stringify({doc:Buffer.from(A.save(doc)).toString('base64'),head:A.getHeads(doc)[0]}));
"""
        result=subprocess.run(['node','--input-type=module','-e',script],cwd=ROOT,input=json.dumps(dict(source=F.b64(S.field(source,5,2)),actor=F.SESSION+'00000000')),text=True,encoding='utf-8',capture_output=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stderr)
        value=json.loads(result.stdout)
        raw=F.replace(F.replace(F.original_session('zero','feff-leading'),5,base64.b64decode(value['doc'])),12,value['head'])
        current=S.wire(raw)
        self.assertEqual(O.decode_history(S.field(current,5,2))[0]['changes'][-1]['text'],'\ufeff')
        with self.assertRaisesRegex(RuntimeError,'per-patch mutation order'):O.semantic_history(current,source,expected,F.SESSION)

    def test_old_config_literals_labels_reader_and_source_digests_remain(self):
        old=json.loads((ROOT/'tmp/canvas-whitespace-reuse-20261023/oracle/before.json').read_text())
        self.assertIn('scripts/canvas_end_clearing_oracle.py',old)
        for shape,literal,plain,reader,label in [('spaces','   ','   ','   ','3 spaces'),('nbsp','\u00a0','\u00a0','\u00a0','Non-breaking space'),('br','<br>','\n','','Line break')]:
            for base in ('positive','zero'):
                e=O.config(reuse_base=base,reuse_content=shape)
                self.assertEqual((e.source_text,e.source_plain,e.reader_text,e.source_kind,e.source_label),(literal,plain,reader,shape,label))
                source,_,_=F.source_and_request(reuse_base=base,reuse_content=shape)
                O.inventory(source,e)
        O.validate(F.receipt())
        R.validate('review',*G.packets('review'))


if __name__=='__main__':unittest.main()
