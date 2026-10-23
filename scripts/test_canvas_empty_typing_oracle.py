"""Portable typed-empty adversaries; synthetic hosts are not native acceptance."""
import base64
import copy
from functools import lru_cache
import hashlib
import json
import subprocess
import unittest

import canvas_empty_typing_wire as W
import canvas_empty_typing_oracle as N
import canvas_empty_typing_recovery_oracle as R
import test_canvas_empty_typing_fixtures as F


def altered(value,rows):return dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in rows)))


@lru_cache(maxsize=48)
def receipt(history='bootstrap',layout='no-following',typing='correction',corpus='ascii',scenario='success'):
    return F.receipt(scenario,history,layout,typing,corpus)


class EmptyTypingTests(unittest.TestCase):
    def setup_wire(self,history='bootstrap',layout='no-following',typing='correction',corpus='ascii'):
        e=W.config(typing,history,layout,corpus);source,value,sent=F.source_request(history,layout,typing,corpus);rows=W.inventory(source,e)
        selected,*parsed=W.write(value,rows,e)
        checkpoint=F.checkpoint(source,value,layout,typing,corpus)
        W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic-only',checkpoint=checkpoint)),e,value,source)
        received=[F.authored(row,W.S.raw(rows[W.text(W.wire(row),1)]) if W.text(W.wire(row),1) in rows else None,
                             '' if i==0 else e.reports[-1],2000 if i==0 or e.reused else 1000,i==0) for i,row in enumerate(sent)]
        W.ack(F.F.acknowledgement(received),parsed,rows,{e.original:'',selected:e.reports[-1]},
              {e.original:2000,selected:2000 if e.reused else 1000},e)
        return e,source,value,sent,rows,selected,received

    def test_all_42_original_vectors_and_exact_unicode(self):
        self.assertEqual(hashlib.sha256((F.REF/'contract.json').read_bytes()).hexdigest(),'39acef56aa4d0c9cb7cd9da5b0a64ce9b7ae8e0fe11454a52b33da54e7d06036')
        vectors=json.loads((F.REF/'empty-typing-result.json').read_text(encoding='utf-8'))['vectors'];self.assertEqual(len(vectors),42)
        for vector in vectors:
            typing='restore' if vector['cadence']=='exact-revert' else vector['cadence']
            with self.subTest(history=vector['mode'],layout=vector['layout'],corpus=vector['corpus'],typing=typing):
                e,*_=self.setup_wire(vector['mode'],vector['layout'],typing,vector['corpus'])
                self.assertEqual(vector['states'],list(e.reports))
                self.assertEqual(vector['patchReports'][vector['selectedId']],W.patch_groups(e))
                if vector['corpus']=='unicode':
                    self.assertEqual(e.updates[0],'New \u65e5\u672c\U0001f600 &')
                    self.assertEqual([ord(c) for c in e.updates[0][4:7]],[0x65e5,0x672c,0x1f600])
                    self.assertEqual(W.T.utf16(e.updates[0]),10);self.assertEqual(W.T.utf16(e.reports[0]),14)

    def test_36_synthetic_success_lifecycles_and_six_placement_guards(self):
        for history in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                for corpus in ('ascii','unicode'):
                    for typing in ('correction','erase','restore'):
                        with self.subTest(history=history,layout=layout,corpus=corpus,typing=typing):
                            N.validate(receipt(history,layout,typing,corpus),typing,history,layout,corpus)
            for layout in ('text-following-unicode','empty-nonfinal-following'):
                N.validate(receipt(history,layout,'correction','unicode'),'correction',history,layout,'unicode')
                _,new,_=F.source_request(history,'no-following','correction','unicode')
                _,placed,_=F.source_request(history,layout,'correction','unicode')
                self.assertEqual(new['documentData'],placed['documentData'])

    def test_stale_partial_keep_draft_and_whole_pair(self):
        for history in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                for scenario in ('stale','partial'):
                    N.validate(receipt(history,layout,'restore','unicode',scenario),'restore',history,layout,'unicode')
        bad=copy.deepcopy(receipt(scenario='partial'));bad['stages'][-1]['journal']['receipt']=None
        with self.assertRaises(RuntimeError):N.validate(bad)
        bad=copy.deepcopy(receipt(scenario='stale'));bad['stages'][2]['fixture']['writes']=[receipt()['stages'][2]['fixture']['writes'][0]]
        with self.assertRaises(RuntimeError):N.validate(bad)

    def test_exact_pair_order_and_original_status(self):
        e,_,value,sent,rows,*_=self.setup_wire('retained-zero','empty-final-following','restore','unicode')
        W.write(dict(value,documentData=F.F.b64(b''.join(F.F.f(1,r) for r in sent)+F.F.f(14,F.F.TITLE))),rows,e)
        for bad in (sent[::-1],sent[:1],sent+[sent[1]],
                    [sent[0]+F.F.f(12,F.F.body('')),sent[1]],
                    [F.F.replace(sent[0],20,2),sent[1]],
                    [sent[0],F.F.remove(sent[1],12)]):
            with self.assertRaises(RuntimeError):W.write(altered(value,bad),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,documentData=F.F.b64(base64.b64decode(value['documentData'])+F.F.f(91,1))),rows,e)

    def test_retained_prefix_zero_base_and_processed_order(self):
        for history in F.HISTORIES:
            e,source,value,sent,rows,*_=self.setup_wire(history,'empty-final-following','restore','unicode')
            crdt=W.wire(W.field(W.wire(sent[1]),38,2));raw=W.S.raw(crdt)
            source_history=W.field(W.S.messages(rows[e.following],38)[0],5,2) if history!='bootstrap' else W.field(W.wire(W.field(W.wire(sent[0]),38,2)),5,2)
            variants=[F.F.replace(raw,2,42),F.F.replace(raw,5,source_history),F.F.replace(raw,8,'a'*64),raw+F.F.f(10,b''),
                      b''.join(f[3] for i,f in enumerate(crdt) if not(f[0]==6 and i==max(j for j,x in enumerate(crdt) if x[0]==6)))]
            for variant in variants:
                with self.assertRaises(RuntimeError):W.write(altered(value,[sent[0],F.F.replace(sent[1],38,variant)]),rows,e)
        e,_,value,sent,rows,*_=self.setup_wire('bootstrap','no-following','erase')
        crdt=W.field(W.wire(sent[1]),38,2)
        with self.assertRaises(RuntimeError):W.write(altered(value,[sent[0],F.F.replace(sent[1],38,crdt+F.F.f(6,W.text(W.wire(crdt),8)))]),rows,e)

    def test_actual_histories_with_correct_final_text_still_reject_wrong_mutations(self):
        e,_,value,sent,rows,*_=self.setup_wire('retained-positive','empty-final-following','correction','unicode')
        source=W.S.messages(rows[e.following],38)[0];crdt=W.field(W.wire(sent[1]),38,2)
        script=r"""
import {A} from './tmp/canvas-structure-prep/rich-runtime.mjs';import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8'));const out=[];
for(const mode of ['squashed','coalesced','different-patch-order']){
 let d=A.load(new Uint8Array(Buffer.from(p.doc,'base64')),{actor:p.actor});
 const report=t=>{d=A.change(d,x=>A.updateText(x,['text'],t+'\ufeff'))};
 if(mode==='squashed')report(p.reports.at(-1));
 else if(mode==='coalesced')for(const t of p.reports)report(t);
 else{for(const t of p.reports.slice(0,3))report(t);
  for(const t of [p.prefix+' ',p.prefix+' para',p.prefix+' paragra',p.prefix+' paragraph'])report(t);}
 out.push({mode,doc:Buffer.from(A.save(d)).toString('base64'),head:A.getHeads(d)[0]});
}console.log(JSON.stringify(out));
"""
        result=subprocess.run(['node','--input-type=module','-e',script],input=json.dumps(dict(doc=F.F.b64(W.field(source,5,2)),
                              actor=F.SESSION+'00000000',reports=e.reports,prefix=e.reports[0])),cwd=F.REF.parents[2],
                              text=True,encoding='utf-8',capture_output=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stderr[:1000])
        for variant in json.loads(result.stdout):
            current=F.F.replace(F.F.replace(F.F.remove(crdt,6),5,base64.b64decode(variant['doc'])),12,variant['head'])
            heads=[f[2] for f in source if f[0]==6]
            current+=b''.join(F.F.f(6,h) for h in heads+[variant['head'].encode()]+heads)
            with self.subTest(mode=variant['mode']),self.assertRaisesRegex(RuntimeError,'intermediate mutations'):
                W.write(altered(value,[sent[0],F.F.replace(sent[1],38,current)]),rows,e)

    def test_v11_custody_cannot_downgrade_final_empty_or_drop_reports(self):
        e,source,value,*_=self.setup_wire('retained-zero','empty-final-following','restore','unicode')
        cp=F.checkpoint(source,value,e.layout,e.typing,e.corpus)
        for mutate in (lambda c:c.update(version=10),lambda c:c.update(draft=' '),lambda c:c.update(target=value['child']),
                       lambda c:c['action']['EmptyTyping'].update(child_updates=['']),
                       lambda c:c['action']['EmptyTyping']['child_updates'].reverse(),
                       lambda c:c['action']['EmptyTyping'].update(reused=False),lambda c:c['sections'].pop(),lambda c:c['request'].pop()):
            bad=copy.deepcopy(cp);mutate(bad)
            with self.assertRaises(RuntimeError):W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='held',checkpoint=bad)),e,value,source)

    def test_complete_ack_explicit_final_empty_and_independent_readback(self):
        e,_,_,sent,rows,selected,received=self.setup_wire('retained-zero','empty-final-following','restore','unicode')
        contents={e.original:'',selected:''};sequences={e.original:2000,selected:2000};parsed=list(map(W.wire,sent))
        for row in (F.F.remove(received[1],12),F.F.replace(received[1],12,F.F.body('wrong',False)),
                    F.F.remove(received[1],19),F.F.replace(received[1],2,1000),F.F.replace(received[1],38,W.field(W.wire(sent[1]),38,2))):
            with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement([received[0],row]),parsed,rows,contents,sequences,e)
        bad=copy.deepcopy(receipt());item=bad['stages'][3]['fixture']['readbacks'][0]['sections'][2]
        item['raw']=F.F.b64(F.F.replace(base64.b64decode(item['raw']),12,F.F.body('',False)))
        with self.assertRaises(RuntimeError):N.validate(bad)

    def test_owned_native_reports_utf16_and_exact_empty_projection(self):
        for typing in ('correction','erase','restore'):
            e=W.config(typing,corpus='unicode');valid=F.witness(11,typing,'unicode');N.witness(valid,e)
            mutations=[lambda x:x['typing']['events'][0].update(inputTrusted=False),
                       lambda x:x['typing']['events'][1].update(start=14,selectionStart=14),
                       lambda x:x['typing']['events'][0]['after'][0]['elements'][0].update(elements=[dict(type='text',text='x')]),
                       lambda x:x['typing']['events'][0].update(html=x['typing']['events'][0]['html'].replace('<br>','<span><br></span>',1)),
                       lambda x:x['typing']['events'].pop(),lambda x:x['typing'].update(revision=1)]
            if typing!='correction':mutations += [lambda x:x['typing']['events'][-1].update(placeholder=None),
                         lambda x:x['typing']['events'][-1]['rawAfter'][0]['elements'][1].update(elements=[dict(type='text',text='\n')])]
            for mutate in mutations:
                bad=copy.deepcopy(valid);mutate(bad)
                with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_both_later_editors_keep_owner_during_pending_and_session_counter(self):
        valid=receipt('retained-zero','empty-final-following','restore','unicode')
        for mutate in (lambda x:x['stages'][5]['geometry']['native'].update(generation='999'),
                       lambda x:x['stages'][7]['geometry']['native'].update(generation=x['stages'][4]['geometry']['native']['generation']),
                       lambda x:x['stages'][8]['fixture']['writes'][2].update(sequence=1)):
            bad=copy.deepcopy(valid);mutate(bad)
            with self.assertRaises(RuntimeError):N.validate(bad,'restore','retained-zero','empty-final-following','unicode')

    def test_recovery_all_phases_new_reused_histories_and_cadences(self):
        for history in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                for typing in ('correction','erase','restore'):
                    for phase in R.STAGES:
                        with self.subTest(history=history,layout=layout,typing=typing,phase=phase):
                            stages,terminal,cp=F.recovery(phase,history,layout,typing,'unicode')
                            self.assertEqual(R.validate(phase,stages,terminal,cp,typing,history,layout,'unicode'),cp['action']['EmptyTyping']['child'])

    def test_recovery_no_replay_exact_current_comparison_and_edited_empty_notice(self):
        for mutate in (lambda s,t:s[0].update(dispatches=1),lambda s,t:s[0]['preview'].update(missing=['child']),
                       lambda s,t:s[0]['preview']['split'][2].update(text='wrong'),lambda s,t:s[0]['preview']['current'].pop(),
                       lambda s,t:s[0]['preview'].update(warning=s[0]['preview']['warning'].replace('Those edits remain part of this unresolved change, even if the paragraph is empty again.','')),
                       lambda s,t:s[3]['preview'].update(submitDisabled=False),lambda s,t:s[4]['preview'].update(panel=True),
                       lambda s,t:s[-1]['journal'].update(archiveCount=0),
                       lambda s,t:t['accountGuard'].update(restoredRead=t['accountGuard']['beforeRead'])):
            stages,terminal,cp=F.recovery('review','retained-zero','empty-final-following','restore','unicode');mutate(stages,terminal)
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,cp,'restore','retained-zero','empty-final-following','unicode')


if __name__=='__main__':unittest.main()
