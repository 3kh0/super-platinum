"""Original 19 projections and adversaries for retained whole-triple custody."""
import base64
import copy
import unittest

import canvas_repeat_enter_wire as W
import canvas_repeat_enter_oracle as N
import canvas_repeat_enter_recovery_oracle as R
import test_canvas_repeat_enter_fixtures as F


def altered(value,rows):return dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in rows)))


class RepeatEnterTests(unittest.TestCase):
    def check(self,h='bootstrap',layout='no-following',corpus='ascii'):
        e=W.config(h,layout,corpus);source,value,sent=F.source_request(h,layout,corpus);rows=W.inventory(source,e)
        selected,*parsed=W.write(value,rows,e);checkpoint=F.checkpoint(source,value,layout,corpus)
        W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic-only',checkpoint=checkpoint)),e,value,source)
        contents={e.original:e.wire_seed,selected[0]:'',selected[1]:''}
        sequences={e.original:2000,selected[0]:2000 if e.reused else 1000,selected[1]:1000}
        received=[F.authored(row,W.S.raw(rows[W.text(W.wire(row),1)]) if W.text(W.wire(row),1) in rows else None,
                            contents[W.text(W.wire(row),1)],sequences[W.text(W.wire(row),1)],i==0) for i,row in enumerate(sent)]
        W.ack(F.F.acknowledgement(received),parsed,rows,contents,sequences,e)
        return e,source,value,sent,rows,selected,received,contents,sequences

    def test_all_nineteen_original_vectors_and_scalar_offsets(self):
        for h in F.HISTORIES:
            for c in ('ascii','unicode'):
                for layout in F.LAYOUTS[:3]:
                    with self.subTest(history=h,corpus=c,layout=layout):self.check(h,layout,c)
        self.check('bootstrap','empty-nonfinal-following','unicode')
        e=W.config(corpus='unicode');self.assertEqual(e.seed,'Caf\u00e9 & \u65e5\u672c\U0001f600');self.assertEqual(W.T.utf16(e.seed),11);self.assertEqual(W.T.utf16(e.wire_seed),15)

    def test_three_rows_and_distinct_ids_even_equal_bootstrap_heads(self):
        e,_,value,sent,rows,selected,*_=self.check()
        sessions=[W.wire(W.field(W.wire(row),38,2)) for row in sent[1:]]
        self.assertEqual(W.text(sessions[0],8),W.text(sessions[1],8));self.assertNotEqual(*selected)
        for bad in (sent[:2],sent[1:],sent+[sent[2]],sent[:1]+sent[1:][::-1],[sent[0],sent[1],sent[1]]):
            with self.assertRaises(RuntimeError):W.write(altered(value,bad),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,secondChild=value['child']),rows,e)
        # Protobuf singular title position is immaterial; repeated section order is not.
        W.write(dict(value,documentData=F.F.b64(b''.join(F.F.f(1,row) for row in sent)+F.F.f(14,F.F.TITLE))),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,documentData=F.F.b64(base64.b64decode(value['documentData'])+F.F.f(99,0))),rows,e)

    def test_existing_status_never_content_mutation_or_head_append(self):
        for history in F.HISTORIES:
            e,_,value,sent,rows,*_=self.check(history,'empty-final-following')
            for index in (0,1):
                row=sent[index];session=W.field(W.wire(row),38,2)
                for bad in (F.F.remove(row,19),F.F.replace(row,20,2),row+F.F.f(12,F.F.body('',index==0)),
                            F.F.replace(row,38,session+F.F.f(12,W.text(W.wire(session),8))),
                            F.F.replace(row,38,session+F.F.f(6,W.text(W.wire(session),8))),F.F.replace(row,38,session+F.F.f(10,b''))):
                    with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)

    def test_each_new_member_has_only_its_own_empty_bootstrap(self):
        e,_,value,sent,rows,*_=self.check()
        for index in (1,2):
            session=W.field(W.wire(sent[index]),38,2)
            for tag,content in ((1,'other'),(2,1000),(3,1000),(5,W.field(W.wire(W.field(W.wire(sent[0]),38,2)),5,2)),(12,'a'*64)):
                bad=F.F.replace(sent[index],38,F.F.replace(session,tag,content))
                with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)
            bad=F.F.replace(sent[index],38,session+F.F.f(6,W.text(W.wire(session),8)))
            with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)

    def test_exact_zero_and_positive_source_history_not_fabricated(self):
        for history in ('retained-positive','retained-zero'):
            e,source,value,sent,rows,*_=self.check(history,'empty-final-following','unicode')
            for index in (0,1):
                session=W.field(W.wire(sent[index]),38,2)
                for tag,content in ((2,1000 if history=='retained-zero' else 0),(3,0),(5,b'fake'),(8,'a'*64)):
                    bad=F.F.replace(sent[index],38,F.F.replace(session,tag,content))
                    with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)
            bad=copy.deepcopy(source);raw=base64.b64decode(bad['sections'][1]['raw'])
            bad['sections'][1]['raw']=F.F.b64(F.F.replace(raw,38,F.F.replace(W.field(W.wire(raw),38,2),5,b'fake')))
            with self.assertRaises(RuntimeError):W.inventory(bad,e)

    def test_only_final_literal_empty_reused_and_relative_positions(self):
        e,_,value,sent,rows,*_=self.check('bootstrap','empty-nonfinal-following','unicode')
        with self.assertRaises(RuntimeError):W.write(dict(value,child=e.following),rows,e)
        for index,position in ((1,'aae:temp'),(2,'aad:temp'),(2,'aag:temp')):
            bad=F.F.replace(F.F.replace(sent[index],8,position),21,position)
            with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)

    def test_direct_ack_complete_owned_triple_and_explicit_empty(self):
        e,_,_,sent,rows,_,received,contents,sequences=self.check('retained-zero','empty-final-following')
        parsed=list(map(W.wire,sent))
        for index in range(3):
            for bad in (F.F.remove(received[index],12),F.F.replace(received[index],12,F.F.body('wrong',index==0)),
                        F.F.remove(received[index],19),F.F.replace(received[index],2,0),F.F.replace(received[index],7,'other')):
                with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:index]+[bad]+received[index+1:]),parsed,rows,contents,sequences,e)
        for bad in (received[:2],received[1:],received[::-1],received+[received[2]]):
            with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(bad),parsed,rows,contents,sequences,e)

    def test_v12_not_pair_downgrade_and_exact_third_custody(self):
        e,source,value,_,_,selected,*_=self.check('retained-zero','empty-final-following')
        cp=F.checkpoint(source,value,e.layout,e.corpus)
        for key,new in (('version',5),('draft',''),('request',cp['request'][:-1]),('sections',cp['sections'][:-1])):
            bad=copy.deepcopy(cp);bad[key]=new
            with self.assertRaises(RuntimeError):R.retained_pair(bad,source,e.history,e.layout,e.corpus)
        for key,new in (('child',selected[1]),('second_child',selected[0]),('second_position','aae'),('reused',False)):
            bad=copy.deepcopy(cp);bad['action']['EndRepeat'][key]=new
            with self.assertRaises(RuntimeError):R.retained_pair(bad,source,e.history,e.layout,e.corpus)
        self.assertEqual(R.retained_pair(cp,source,e.history,e.layout,e.corpus),selected)

    def test_two_trusted_events_and_exact_raw_projection(self):
        for c in ('ascii','unicode'):
            e=W.config(corpus=c);value=F.witness(10,c);N.witness(value,e)
            mutations=[('revision',3),('beforeInputTrusted',False),('inputTrusted',False),('inputType','insertText'),('caretSection',1),('caretOffset',1)]
            for key,new in mutations:
                bad=copy.deepcopy(value);bad['repeat'][key]=new
                with self.assertRaises(RuntimeError):N.witness(bad,e)
            for old,new in (('<span><br></span>','<span><br><br></span>'),('<span><br></span>','<span>x<br></span>')):
                bad=copy.deepcopy(value);bad['repeat']['html']=bad['repeat']['html'].replace(old,new,1)
                with self.assertRaises(RuntimeError):N.witness(bad,e)
            bad=copy.deepcopy(value);bad['repeat']['rawAfter'][0]['elements'][1]['elements'][0]['text']='x'
            with self.assertRaises(RuntimeError):N.witness(bad,e)
            bad=copy.deepcopy(value);bad['repeat']['placeholders'][0]=dict(html='<br>',spanDepth=0)
            bad['repeat']['html']=bad['repeat']['html'].replace('<span><br></span>','<br>',1)
            bad['repeat']['rawAfter'][0]['elements'][1]['elements']=[]
            with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_nineteen_full_lifecycles_three_later_edits_and_failures(self):
        for h in F.HISTORIES:
            for c in ('ascii','unicode'):
                for layout in F.LAYOUTS[:3]:N.validate(F.receipt('success',h,layout,c),h,layout,c)
                for scenario in ('stale','partial'):N.validate(F.receipt(scenario,h,'empty-final-following',c),h,'empty-final-following',c)
        N.validate(F.receipt('success','bootstrap','empty-nonfinal-following','unicode'),'bootstrap','empty-nonfinal-following','unicode')

    def test_pending_owners_readback_and_third_save_counter(self):
        value=F.receipt();N.validate(value)
        for index in (5,8,11):
            bad=copy.deepcopy(value);bad['stages'][index]['geometry']['native']['generation']='999'
            with self.assertRaises(RuntimeError):N.validate(bad)
        bad=copy.deepcopy(value);bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop()
        with self.assertRaises(RuntimeError):N.validate(bad)
        bad=copy.deepcopy(value);bad['stages'][11]['fixture']['writes'][3]['sequence']=1
        with self.assertRaises(RuntimeError):N.validate(bad)
        bad=F.receipt('partial');bad['stages'][-1]['journal']['receipt']['checkpoint']['action']['EndRepeat'].pop('second_child')
        with self.assertRaises(RuntimeError):N.validate(bad)

    def test_inert_recovery_all_phases_and_three_member_preview(self):
        for h in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                for phase in R.STAGES:
                    stages,terminal,cp=F.recovery(phase,h,layout,'unicode');R.validate(phase,stages,terminal,cp,h,layout,'unicode')
        for mutation in ('third','replay','pair-warning','operation','account','archive'):
            stages,terminal,cp=F.recovery('review');bad=stages[0]
            if mutation=='third':bad['preview']['split'].pop()
            elif mutation=='replay':terminal['dispatches']=1
            elif mutation=='pair-warning':bad['preview']['warning']=bad['preview']['warning'].replace('all three outcomes','both outcomes')
            elif mutation=='operation':stages[1]['journal']['active']['operation']='replaced'
            elif mutation=='account':stages[4]['preview']['panel']=True
            else:stages[-1]['journal']['archiveCount']=0
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,cp)


if __name__=='__main__':unittest.main()
