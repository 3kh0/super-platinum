"""36 retained original projections plus hostile ordered-history/lifecycle inputs."""
import base64
import copy
import json
from pathlib import Path
import subprocess
import unittest

import canvas_repeat_typing_wire as W
import canvas_repeat_typing_oracle as N
import canvas_repeat_typing_recovery_oracle as R
import test_canvas_repeat_typing_fixtures as F


def altered(value,rows):return dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in rows)))


class RepeatTypingTests(unittest.TestCase):
    def check(self,typing='correction',history='bootstrap',layout='no-following',corpus='ascii'):
        e=W.config(typing,history,layout,corpus);source,value,sent=F.source_request(history,layout,corpus,typing);rows=W.inventory(source,e)
        selected,*parsed=W.write(value,rows,e);cp=F.checkpoint(source,value,layout,corpus,typing)
        W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic-only',checkpoint=cp)),e,value,source)
        bodies={e.original:e.wire_seed,selected[0]:'',selected[1]:e.reports[-1]}
        sequences={e.original:2000,selected[0]:2000 if e.reused else 1000,selected[1]:1000}
        received=[F.authored(row,W.S.raw(rows[W.text(W.wire(row),1)]) if W.text(W.wire(row),1) in rows else None,
                            bodies[W.text(W.wire(row),1)],sequences[W.text(W.wire(row),1)],i==0) for i,row in enumerate(sent)]
        W.ack(F.F.acknowledgement(received),parsed,rows,bodies,sequences,e)
        return e,source,value,sent,rows,selected,received,bodies,sequences,cp

    def test_all_36_original_vectors_exact_scalar_content_and_history_counts(self):
        for h in F.HISTORIES:
            for c in ('ascii','unicode'):
                for layout in ('no-following','empty-final-following'):
                    for t,count in (('correction',8),('erase',3),('restore',5)):
                        with self.subTest(history=h,corpus=c,layout=layout,typing=t):
                            e,_,_,sent,*_=self.check(t,h,layout,c)
                            history=W.T.decode_history(W.field(W.wire(W.field(W.wire(sent[2]),38,2)),5,2))[0]
                            self.assertEqual(len(history['changes']),count)
        e=W.config(corpus='unicode');self.assertEqual(e.seed,'Caf\u00e9 & \u65e5\u672c\U0001f600');self.assertEqual(e.updates[0],'New \u65e5\u672c\U0001f600 &')
        self.assertEqual(W.T.utf16(e.seed),11);self.assertEqual(W.T.utf16(e.wire_seed),15)
        self.assertEqual(W.T.utf16(e.updates[0]),10);self.assertEqual(W.T.utf16(e.reports[0]),14)

    def test_complete_triple_order_distinct_identity_and_singular_title_order(self):
        e,_,value,sent,rows,*_=self.check()
        for bad in (sent[:2],sent[1:],sent+[sent[2]],sent[:1]+sent[1:][::-1],[sent[0],sent[1],sent[1]]):
            with self.assertRaises(RuntimeError):W.write(altered(value,bad),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,secondChild=value['child']),rows,e)
        W.write(dict(value,documentData=F.F.b64(b''.join(F.F.f(1,row) for row in sent)+F.F.f(14,F.F.TITLE))),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,documentData=F.F.b64(base64.b64decode(value['documentData'])+F.F.f(99,0))),rows,e)

    def test_first_two_exact_history_status_unchanged_for_all_source_modes(self):
        for h in F.HISTORIES:
            e,_,value,sent,rows,*_=self.check('correction',h,'empty-final-following','unicode')
            for i in (0,1):
                crdt=W.field(W.wire(sent[i]),38,2)
                for bad in (F.F.remove(sent[i],19),F.F.replace(sent[i],20,2),sent[i]+F.F.f(12,F.F.body('wrong',i==0)),
                            F.F.replace(sent[i],38,crdt+F.F.f(12,W.text(W.wire(crdt),8))),
                            F.F.replace(sent[i],38,F.F.replace(crdt,2,1000 if h=='retained-zero' else 0))):
                    with self.assertRaises(RuntimeError):W.write(altered(value,sent[:i]+[bad]+sent[i+1:]),rows,e)

    def test_return_empty_keeps_its_own_report_history_not_bootstrap_or_other_cadence(self):
        for t in ('erase','restore'):
            e,_,value,sent,rows,*_=self.check(t)
            alternatives=[F.A.source_request()[2][2],F.source_request(typing='restore' if t=='erase' else 'erase')[2][2]]
            for row in alternatives:
                with self.assertRaises(RuntimeError):W.write(altered(value,sent[:2]+[row]),rows,e)

    def test_valid_same_final_histories_reject_squashing_reordered_patches_extra_cycle(self):
        for mode,t in (('squash','correction'),('wrong-order','correction'),('extra-empty-cycle','erase')):
            e,_,value,sent,rows,*_=self.check(t);crdt=W.field(W.wire(sent[2]),38,2)
            run=subprocess.run(['node',str(Path(__file__).with_name('test_canvas_repeat_typing_history.mjs'))],
                input=json.dumps(dict(document=F.F.b64(W.field(W.wire(crdt),5,2)),mode=mode)),text=True,encoding='utf-8',capture_output=True,timeout=30,check=True)
            hostile=json.loads(run.stdout);self.assertEqual(hostile['text'],e.reports[-1]+'\ufeff')
            crdt=F.F.replace(F.F.replace(F.F.replace(crdt,5,base64.b64decode(hostile['document'])),6,hostile['head']),12,hostile['head'])
            with self.assertRaisesRegex(RuntimeError,'intermediate mutations'):
                W.write(altered(value,sent[:2]+[F.F.replace(sent[2],38,crdt)]),rows,e)

    def test_third_final_body_base_processed_lineage_and_language_are_exact(self):
        e,_,value,sent,rows,*_=self.check(corpus='unicode');row=sent[2];session=W.field(W.wire(row),38,2)
        changes=[F.F.remove(row,12),F.F.replace(row,12,F.F.body(e.updates[-1],False)),F.F.replace(row,12,F.F.body(e.reports[-1],True))]
        for tag,new in ((2,1000),(3,1000),(8,'a'*64),(12,'b'*64),(5,b'fake')):
            changes.append(F.F.replace(row,38,F.F.replace(session,tag,new)))
        changes.extend([F.F.replace(row,38,session+F.F.f(6,W.text(W.wire(session),8))),F.F.replace(row,38,session+F.F.f(10,b''))])
        for bad in changes:
            with self.assertRaises(RuntimeError):W.write(altered(value,sent[:2]+[bad]),rows,e)

    def test_ack_requires_complete_triple_and_explicit_final_empty_content(self):
        for t in ('correction','erase','restore'):
            e,_,_,sent,rows,_,received,bodies,sequences,_=self.check(t,'retained-zero','empty-final-following','unicode')
            for i in range(3):
                for row in (F.F.remove(received[i],12),F.F.replace(received[i],12,F.F.body('wrong',i==0)),F.F.remove(received[i],19),F.F.replace(received[i],2,0)):
                    with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:i]+[row]+received[i+1:]),list(map(W.wire,sent)),rows,bodies,sequences,e)
            with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:2]),list(map(W.wire,sent)),rows,bodies,sequences,e)

    def test_two_enter_then_typed_third_strict_range_placeholder_and_immutable_first(self):
        for c in ('ascii','unicode'):
            for t in ('correction','erase','restore'):
                e=W.config(t,corpus=c);value=F.witness(10,c,t);N.witness(value,e)
                for key,new in (('beforeRevision',1),('revision',2),('beforeInputTrusted',False),('start',1),('afterText','wrong')):
                    bad=copy.deepcopy(value);bad['typing']['events'][0][key]=new
                    with self.assertRaises(RuntimeError):N.witness(bad,e)
                for target in ('after','rawAfter'):
                    bad=copy.deepcopy(value);bad['typing']['events'][0][target][0]['elements'][1]['elements']=[dict(type='text',text='x')]
                    with self.assertRaises(RuntimeError):N.witness(bad,e)
                bad=copy.deepcopy(value);bad['typing']['events'][0]['html']=bad['typing']['events'][0]['html'].replace('<span><br></span>','<br>',1)
                with self.assertRaises(RuntimeError):N.witness(bad,e)
                bad=copy.deepcopy(value);bad['typing']['events'].reverse()
                with self.assertRaises(RuntimeError):N.witness(bad,e)
                if t!='correction':
                    bad=copy.deepcopy(value);bad['typing']['events'][-1]['placeholder']['sectionCount']=2
                    with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_all_36_synthetic_successes_three_later_saves_and_failure_paths(self):
        for h in F.HISTORIES:
            for c in ('ascii','unicode'):
                for layout in ('no-following','empty-final-following'):
                    for t in ('correction','erase','restore'):
                        N.validate(F.receipt('success',h,layout,c,t),t,h,layout,c)
                for scenario in ('stale','partial'):
                    N.validate(F.receipt(scenario,h,'empty-final-following',c,'restore'),'restore',h,'empty-final-following',c)
        # Composition guards: these were not additional combined original vectors.
        for h,layout,c,t in (('bootstrap','empty-nonfinal-following','unicode','erase'),('retained-positive','text-following','ascii','correction'),('retained-zero','text-following','unicode','restore')):
            N.validate(F.receipt('success',h,layout,c,t),t,h,layout,c)

    def test_pending_owners_independent_readback_and_three_later_counters(self):
        value=F.receipt();N.validate(value)
        for index in (5,8,11):
            bad=copy.deepcopy(value);bad['stages'][index]['geometry']['native']['generation']='999'
            with self.assertRaises(RuntimeError):N.validate(bad)
        bad=copy.deepcopy(value);bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop()
        with self.assertRaises(RuntimeError):N.validate(bad)
        bad=copy.deepcopy(value);bad['stages'][11]['fixture']['writes'][3]['sequence']=1
        with self.assertRaises(RuntimeError):N.validate(bad)

    def test_v13_exact_ordered_report_custody_not_v12_downgrade(self):
        e,source,_,_,_,selected,_,_,_,cp=self.check('restore','retained-zero','empty-final-following','unicode')
        self.assertEqual(R.retained_pair(cp,source,e.typing,e.history,e.layout,e.corpus),selected)
        for key,new in (('version',12),('draft',''),('request',cp['request'][:-1]),('sections',cp['sections'][:-1])):
            bad=copy.deepcopy(cp);bad[key]=new
            with self.assertRaises(RuntimeError):R.retained_pair(bad,source,e.typing,e.history,e.layout,e.corpus)
        for reports in ([],list(e.reports[:-1]),list(reversed(e.reports)),[e.reports[-1]],list(e.reports)+['']):
            bad=copy.deepcopy(cp);bad['action']['EndRepeatTyping']['second_child_updates']=reports
            with self.assertRaises(RuntimeError):R.retained_pair(bad,source,e.typing,e.history,e.layout,e.corpus)

    def test_all_recovery_phases_no_replay_whole_triple_preview_and_retained_history(self):
        for h in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                for t in ('correction','erase','restore'):
                    for phase in R.STAGES:
                        stages,terminal,cp=F.recovery(phase,h,layout,'unicode',t);R.validate(phase,stages,terminal,cp,t,h,layout,'unicode')
        for mutation in ('third','replay','pair-warning','operation','account','archive','reports','typing-notice'):
            stages,terminal,cp=F.recovery('review');bad=stages[0]
            if mutation=='third':bad['preview']['split'].pop()
            elif mutation=='replay':terminal['dispatches']=1
            elif mutation=='pair-warning':bad['preview']['warning']=bad['preview']['warning'].replace('all three outcomes','both outcomes')
            elif mutation=='operation':stages[1]['journal']['active']['operation']='replaced'
            elif mutation=='account':stages[4]['preview']['panel']=True
            elif mutation=='archive':stages[-1]['journal']['archiveCount']=0
            elif mutation=='typing-notice':bad['preview']['warning']=bad['preview']['warning'].replace('Those edits remain part of this unresolved change, even if the paragraph is empty again.','')
            else:bad['journal']['active']['checkpoint']['action']['EndRepeatTyping']['second_child_updates']=[]
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,cp)


if __name__=='__main__':unittest.main()
