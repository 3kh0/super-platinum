"""Adversarial portable gates. Native execution remains a separate acceptance step."""
import base64
import copy
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import unittest

import canvas_start_typing_wire as W
import canvas_start_typing_oracle as O
import canvas_start_typing_recovery_oracle as R
import test_canvas_start_typing_fixtures as F


@lru_cache(maxsize=48)
def cached(scenario='success',corpus='ascii',parent='bootstrap',typing='correction'):
    return F.receipt(scenario,corpus,parent,typing)


class StartTypingTests(unittest.TestCase):
    def test_twelve_successes_and_literal_unicode_reference(self):
        root=F.ROOT/'tmp/canvas-start-typing-20261023/reference-b'
        self.assertEqual(hashlib.sha256((root/'contract.json').read_bytes()).hexdigest(),'0c1a55e5eef46e1de093cd5b62a14f1bb88da8fc6f91a0d133a62d5a7ab236ff')
        seeds=json.loads((root/'source-seeds.json').read_text(encoding='utf-8'))['seeds']
        for parent in ('bootstrap','retained'):
            for corpus in ('ascii','unicode'):
                expected='Keep thisSecond paragraph' if corpus=='ascii' else 'Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph'
                seed=next(v for v in seeds if v['sourceMode']==parent and v['corpus']==corpus)
                self.assertEqual(seed['rtml'],expected.replace('&','&amp;'))
                if corpus=='unicode':
                    self.assertEqual(len(expected.encode('utf-16-le'))//2,27)
                    self.assertEqual(len(seed['rtml'].encode('utf-16-le'))//2,31)
                    self.assertIn([0x65e5,0x672c,0x1f600],[[ord(c) for c in expected[i:i+3]] for i in range(len(expected)-2)])
                for typing in ('correction','erase','restore'):
                    with self.subTest(parent=parent,corpus=corpus,typing=typing):O.validate(cached('success',corpus,parent,typing),corpus,typing,parent)

    def test_stale_and_partial_do_not_accept_or_discard_pair(self):
        for scenario in ('stale','partial'):
            for parent in ('bootstrap','retained'):
                for typing in ('correction','erase','restore'):
                    with self.subTest(scenario=scenario,parent=parent,typing=typing):O.validate(cached(scenario,'unicode',parent,typing),'unicode',typing,parent)
        packet=copy.deepcopy(cached('partial'))
        packet['stages'][-1]['journal']['receipt']=None
        with self.assertRaisesRegex(RuntimeError,'operation missing|whole pair|receipt'):O.validate(packet)

    def test_exact_wire_order_and_untouched_blank(self):
        source,value,_,_,_=F.source_request('unicode','retained','restore');e=W.config('unicode','restore','retained');parent=W.decoded(source['sections'][1]['raw'])
        data=W.decoded(value['documentData']);rows=W.S.messages(data,1)
        def run(new_data):
            changed=dict(value,documentData=F.F.b64(new_data))
            with self.assertRaises(RuntimeError):W.write(changed,parent,e)
        title=b''.join(f[3] for f in data if f[0]==14)
        run(title+F.F.f(1,W.S.raw(rows[1]))+F.F.f(1,W.S.raw(rows[0])))
        run(title+F.F.f(1,W.S.raw(rows[0])))
        altered=F.F.replace(W.S.raw(rows[1]),12,F.F.body('x',False))
        run(title+F.F.f(1,W.S.raw(rows[0]))+F.F.f(1,altered))
        run(base64.b64decode(value['documentData'])+F.F.f(99,0))

    def test_original_history_cannot_reset_deduplicate_or_revert_without_changes(self):
        source,value,parent,new,_=F.source_request('unicode','retained','restore');e=W.config('unicode','restore','retained');original=W.decoded(source['sections'][1]['raw'])
        crdt=W.wire(W.field(W.wire(parent),38,2));source_crdt=W.S.messages(original,38)[0]
        variants=[F.F.replace(W.S.raw(crdt),2,0),
                  b''.join(f[3] for i,f in enumerate(crdt) if not(f[0]==6 and i==max(j for j,x in enumerate(crdt) if x[0]==6))),
                  F.F.replace(W.S.raw(crdt),5,W.field(source_crdt,5,2)),
                  W.S.raw(crdt)+F.F.f(10,b'')]
        for variant in variants:
            altered=F.F.replace(parent,38,variant);write=dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+F.F.f(1,altered)+F.F.f(1,new)))
            with self.subTest(sha=hashlib.sha256(variant).hexdigest()),self.assertRaises(RuntimeError):W.write(write,original,e)

    def test_native_original_only_ranges_and_empty_projection(self):
        e=W.config('unicode','erase','retained');w=F.witness(11,'unicode','erase');O.witness(w,e)
        mutations=[lambda x:x['typing']['events'][0].update(start=1),
                   lambda x:x['typing']['events'][1].update(beforeInputTrusted=False),
                   lambda x:x['typing']['events'][0]['after'][0]['elements'][0].update(elements=[dict(type='text',text='x')]),
                   lambda x:x['typing']['events'][1].update(html=x['typing']['events'][1]['html'].replace('<span><br></span>','<strong><br></strong>',1)),
                   lambda x:x['typing']['events'][1]['rawAfter'][0]['elements'][1].update(elements=[dict(type='text',text='\n\n')]),
                   lambda x:x['typing']['events'][1].update(placeholder=None),
                   lambda x:x['typing'].update(revision=2)]
        for change in mutations:
            bad=copy.deepcopy(w);change(bad)
            with self.assertRaises(RuntimeError):O.witness(bad,e)

    def test_correct_final_text_and_heads_do_not_replace_patch_history(self):
        source,write=F.coalesced_original_write()
        with self.assertRaisesRegex(RuntimeError,'ordered original mutations'):
            W.write(write,W.decoded(source['sections'][1]['raw']),W.config('unicode','correction','retained'))

    def test_durable_original_target_and_ordered_reports(self):
        source,value,_,_,reports=F.source_request();e=W.config();cp=F.checkpoint(source,value,reports)
        mutations=[lambda x:x.update(version=1),lambda x:x.update(target=value['child']),lambda x:x.update(draft=''),
                   lambda x:x['action']['StartTyping'].update(original_updates=[reports[-1]]),
                   lambda x:x['action']['StartTyping'].update(position='aad:temp'),lambda x:x['sections'].pop()]
        for mutate in mutations:
            bad=copy.deepcopy(cp);mutate(bad)
            with self.assertRaises(RuntimeError):W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='held',checkpoint=bad)),e,value,source)

    def test_ack_readback_and_save_ownership(self):
        original=cached()
        mutations=[lambda x:x['stages'][3]['fixture']['acknowledgements'][0].update(accepted=False),
                   lambda x:x['stages'][5]['geometry']['native'].update(generation='999'),
                   lambda x:x['stages'][7]['geometry']['native'].update(generation=x['stages'][4]['geometry']['native']['generation']),
                   lambda x:x['stages'][8]['fixture']['writes'][2].update(sequence=1),
                   lambda x:x['stages'][3]['fixture']['readbacks'][0]['sections'].reverse()]
        for change in mutations:
            bad=copy.deepcopy(original);change(bad)
            with self.assertRaises(RuntimeError):O.validate(bad)

    def test_recovery_all_phases_preserves_v9_without_replay(self):
        for parent in ('bootstrap','retained'):
            for typing in ('correction','erase','restore'):
                for phase in R.STAGES:
                    with self.subTest(parent=parent,typing=typing,phase=phase):
                        stages,terminal,cp=F.recovery(phase,'unicode',parent,typing)
                        R.validate(phase,stages,terminal,cp,'unicode',typing,parent)

    def test_recovery_rejects_swapped_preview_current_claim_and_archive_loss(self):
        stages,terminal,cp=F.recovery('review','unicode','retained','restore')
        mutations=[lambda x:x[0]['preview']['startTyping'].update(newText='changed blank'),
                   lambda x:x[0]['preview']['startTyping'].update(originalBeforeText='wrong source'),
                   lambda x:x[0]['preview']['startTyping'].update(currentText='accepted'),
                   lambda x:x[0]['preview'].update(warning=x[0]['preview']['warning'].replace('This is a current comparison, not confirmation that the saved changes succeeded.','')),
                   lambda x:x[0].update(dispatches=1),lambda x:x[-1]['journal'].update(archiveCount=0),
                   lambda x:x[4]['preview'].update(panel=True)]
        for mutate in mutations:
            bad=copy.deepcopy(stages);mutate(bad)
            with self.assertRaises(RuntimeError):R.validate('review',bad,terminal,cp,'unicode','restore','retained')


if __name__=='__main__':unittest.main()
