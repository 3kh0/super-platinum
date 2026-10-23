"""Original twelve projections: independently decode actual saved history bytes."""
import base64
import hashlib
import json
from pathlib import Path
import unittest

import canvas_end_clearing_oracle as O
import canvas_split_oracle as S


class OriginalClearingVectors(unittest.TestCase):
    def test_original_positive_and_zero_clear_and_type_vectors(self):
        root=Path(__file__).resolve().parent.parent/'tmp/canvas-nonliteral-reuse-20261023/reference'
        for base,name in [('positive','nonliteral-projection-result.json'),('zero','nonliteral-zero-result.json')]:
            vectors=json.loads((root/name).read_text(encoding='utf-8'))['vectors'];self.assertEqual(len(vectors),6)
            for v in vectors:
                with self.subTest(base=base,shape=v['corpus'],cadence=v['cadence']):
                    shape='spaces' if v['corpus']=='ascii-space' else v['corpus']
                    expected=O.config(reuse_base=base,reuse_content=shape)
                    source=S.wire(base64.b64decode(v['sourceSession']))
                    projection=S.decoded(v['wireBase64']);parent,child=S.messages(projection,1)
                    current=S.wire(S.field(child,38,2));now,prior=O.decode_history(S.field(current,5,2),S.field(source,5,2))
                    self.assertEqual(O.tags(parent),[1,2,13,19,20,33,38])
                    self.assertEqual(O.tags(child),[1,2,9,12,13,19,20,33,38])
                    self.assertEqual([c['raw'] for c in now['changes'][:len(prior['changes'])]],[c['raw'] for c in prior['changes']])
                    self.assertEqual(len(prior['changes']),1 if base=='positive' else 2)
                    self.assertEqual(prior['changes'][-1]['text'],expected.source_text+'\ufeff')
                    actual=now['changes'][len(prior['changes']):]
                    states=[''] if v['cadence']=='clear-only' else ['','x']
                    self.assertEqual([c['text'] for c in actual],[s+'\ufeff' for s in states])
                    self.assertEqual(actual[0]['patches'],O.clearing_patches(expected))
                    self.assertEqual(S.body(child,False),states[-1])
                    self.assertEqual(S.field(current,2,0),1000 if base=='positive' else 0)
                    self.assertEqual(S.field(current,3,0),1000)
                    prior_heads=[f[2] for f in source if f[0]==6]
                    self.assertEqual([f[2] for f in current if f[0]==6],prior_heads+[S.field(current,12,2)]+prior_heads)
                    self.assertEqual(now['heads'],[S.text(current,12)])
                    digest=hashlib.sha256(b''.join(f[3] for f in sorted(source,key=lambda f:f[0]) if f[0] not in (1,4))).hexdigest()
                    self.assertEqual(digest,O.SOURCE_DIGESTS[(base,shape)])


if __name__=='__main__':unittest.main()
