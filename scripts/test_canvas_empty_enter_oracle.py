"""Portable adversaries for one Enter in a previously canonical empty paragraph."""
import base64
import copy
import unittest

import canvas_empty_enter_wire as W
import canvas_empty_enter_oracle as N
import canvas_empty_enter_recovery_oracle as R
import test_canvas_empty_enter_fixtures as F


def altered(value,rows):
    return dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in rows)))


class EmptyEnterTests(unittest.TestCase):
    def check(self,h='bootstrap',layout='no-following'):
        e=W.config(h,layout);source,value,sent=F.source_request(h,layout);rows=W.inventory(source,e)
        selected,*parsed=W.write(value,rows,e)
        checkpoint=F.checkpoint(source,value,layout)
        W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic-only',checkpoint=checkpoint)),e,value,source)
        received=[F.authored(row,W.S.raw(rows[W.text(W.wire(row),1)]) if W.text(W.wire(row),1) in rows else None,
                             '',2000 if i==0 or e.reused else 1000,i==0) for i,row in enumerate(sent)]
        a=F.F.acknowledgement(received)
        W.ack(a,parsed,rows,{e.original:'',selected:''},{e.original:2000,selected:2000 if e.reused else 1000},e)
        return e,source,value,sent,rows,selected,received,a

    def test_all_fifteen_original_vectors(self):
        for history in F.HISTORIES:
            for layout in F.LAYOUTS:
                with self.subTest(history=history,layout=layout):self.check(history,layout)

    def test_document_singular_field_order_not_row_order(self):
        e,_,value,sent,rows,*_=self.check()
        W.write(dict(value,documentData=F.F.b64(F.F.f(1,sent[0])+F.F.f(1,sent[1])+F.F.f(14,F.F.TITLE))),rows,e)
        for bad in (sent[::-1],sent[:1],sent+[sent[1]]):
            with self.assertRaises(RuntimeError):W.write(altered(value,bad),rows,e)
        with self.assertRaises(RuntimeError):W.write(dict(value,documentData=F.F.b64(base64.b64decode(value['documentData'])+F.F.f(90,0))),rows,e)

    def test_status_is_required_and_never_content_or_split(self):
        for history in F.HISTORIES:
            e,_,value,sent,rows,*_=self.check(history,'empty-final-following')
            for index in (0,1):
                row=sent[index];session=W.field(W.wire(row),38,2)
                mutations=[F.F.remove(row,19),F.F.replace(row,20,2),row+F.F.f(12,F.F.body('',index==0)),
                           F.F.replace(row,38,session+F.F.f(12,W.text(W.wire(session),8))),
                           F.F.replace(row,38,session+F.F.f(10,b'')),F.F.replace(row,38,session+F.F.f(6,W.text(W.wire(session),8)))]
                for bad in mutations:
                    with self.subTest(history=history,index=index,bad=len(bad)):
                        with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)

    def test_zero_base_prefix_and_reference_source_are_not_reset(self):
        e,source,value,sent,rows,*_=self.check('retained-zero','empty-final-following')
        for index in (0,1):
            session=W.field(W.wire(sent[index]),38,2)
            for tag,content in ((2,1000),(3,0),(5,b'fake'),(8,'a'*64)):
                bad=F.F.replace(sent[index],38,F.F.replace(session,tag,content))
                with self.assertRaises(RuntimeError):W.write(altered(value,sent[:index]+[bad]+sent[index+1:]),rows,e)
        bad=copy.deepcopy(source);raw=base64.b64decode(bad['sections'][1]['raw'])
        bad['sections'][1]['raw']=F.F.b64(F.F.replace(raw,38,F.F.replace(W.field(W.wire(raw),38,2),2,1000)))
        with self.assertRaises(RuntimeError):W.inventory(bad,e)

    def test_only_final_empty_is_reused(self):
        e,_,value,sent,rows,*_=self.check('retained-positive','empty-nonfinal-following')
        with self.assertRaises(RuntimeError):W.write(dict(value,child=e.following),rows,e)
        e,_,value,sent,rows,*_=self.check('retained-positive','empty-final-following')
        with self.assertRaises(RuntimeError):W.write(dict(value,child='temp:C:xyz'+'d'*25),rows,e)

    def test_direct_ack_requires_complete_fixture_bodies_and_history(self):
        e,_,value,sent,rows,selected,received,a=self.check('retained-zero','empty-final-following')
        content={e.original:'',selected:''};seq={e.original:2000,selected:2000};parsed=list(map(W.wire,sent))
        for index in (0,1):
            for bad in (F.F.remove(received[index],12),F.F.replace(received[index],12,F.F.body('wrong',index==0)),
                        F.F.remove(received[index],19),F.F.replace(received[index],2,1000)):
                packet=F.F.acknowledgement(received[:index]+[bad]+received[index+1:])
                with self.assertRaises(RuntimeError):W.ack(packet,parsed,rows,content,seq,e)
        with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:1]),parsed,rows,content,seq,e)

    def test_v10_whole_pair_and_original_inventory_custody(self):
        e,source,value,_,_,selected,*_=self.check('retained-zero','empty-final-following')
        c=F.checkpoint(source,value,e.layout)
        for field,content in (('version',5),('draft',' '),('target',selected),('request',c['request'][:-1]),('sections',c['sections'][:-1])):
            bad=copy.deepcopy(c);bad[field]=content
            with self.assertRaises(RuntimeError):W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic',checkpoint=bad)),e,value,source)
        for key,content in (('reused',False),('position','aae:temp'),('child','elsewhere')):
            bad=copy.deepcopy(c);bad['action']['EmptyEnter'][key]=content
            with self.assertRaises(RuntimeError):W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic',checkpoint=bad)),e,value,source)

    def test_both_later_edits_preserve_each_base_and_history(self):
        for history in F.HISTORIES:
            for layout in ('no-following','empty-final-following'):
                e,_,_,_,_,_,received,_=self.check(history,layout)
                for index,(row,content) in enumerate(((received[1],'Continued paragraph'),(received[0],'Original paragraph edited')),1):
                    value,wire=F.continued(row,content,index,index==2)
                    W.continuation(value,W.wire(row),content,index,'c'*12,e)
                    bad=dict(value,sequence=index+1)
                    with self.assertRaises(RuntimeError):W.continuation(bad,W.wire(row),content,index,'c'*12,e)

    def test_native_success_all_fifteen_and_stale_partial_pairs(self):
        for history in F.HISTORIES:
            for layout in F.LAYOUTS:
                with self.subTest(history=history,layout=layout):N.validate(F.receipt('success',history,layout),history,layout)
        for history,layout in [('bootstrap','no-following'),('retained-positive','empty-nonfinal-following'),('retained-zero','empty-final-following')]:
            for scenario in ('stale','partial'):N.validate(F.receipt(scenario,history,layout),history,layout)

    def test_exact_dom_caret_and_one_event_only(self):
        N.witness(F.witness(10))
        for mutate in [lambda w:w.update(offset=1),lambda w:w.update(inputTrusted=False),lambda w:w.update(revision=2),
                       lambda w:w.update(typing={}),lambda w:w['emptyProof'].update(caretSection=0),
                       lambda w:w['emptyProof'].update(caretOffset=1),
                       lambda w:w['emptyProof'].update(afterHtml=w['emptyProof']['afterHtml'].replace('<br>','<br><br>',1)),
                       lambda w:w['emptyProof'].update(afterHtml=w['emptyProof']['afterHtml'].replace('<br>','<b><br></b>',1)),
                       lambda w:w['after'][0]['elements'][0].update(elements=[dict(type='text',text='x')]),
                       lambda w:w['rawAfter'][0]['elements'][0].update(elements=[dict(type='text',text='\n')])]:
            bad=F.witness(10);mutate(bad)
            with self.assertRaises(RuntimeError):N.witness(bad)

    def test_independent_readback_owner_and_durable_unknown_outcome(self):
        history='retained-zero';layout='empty-final-following';valid=F.receipt('success',history,layout)
        bad=copy.deepcopy(valid);bad['stages'][5]['geometry']['native']['generation']='999'
        with self.assertRaises(RuntimeError):N.validate(bad,history,layout)
        bad=copy.deepcopy(valid);bad['stages'][7]['geometry']['native']['generation']=bad['stages'][4]['geometry']['native']['generation']
        with self.assertRaises(RuntimeError):N.validate(bad,history,layout)
        bad=copy.deepcopy(valid);row=bad['stages'][3]['fixture']['readbacks'][0]['sections'][1]
        row['raw']=F.F.b64(F.F.replace(base64.b64decode(row['raw']),12,F.F.body(' ',True)))
        with self.assertRaises(RuntimeError):N.validate(bad,history,layout)
        partial=F.receipt('partial',history,layout)
        partial['stages'][-1]['journal']['receipt']=None
        with self.assertRaises(RuntimeError):N.validate(partial,history,layout)
        stale=F.receipt('stale',history,layout);stale['stages'][2]['fixture']['writes']=[valid['stages'][2]['fixture']['writes'][0]]
        with self.assertRaises(RuntimeError):N.validate(stale,history,layout)

    def test_recovery_all_histories_new_and_reused_no_replay(self):
        for history in F.HISTORIES:
            for layout in ('no-following','empty-final-following','empty-nonfinal-following','text-following-unicode'):
                for phase in R.STAGES:
                    with self.subTest(history=history,layout=layout,phase=phase):
                        stages,terminal,checkpoint=F.recovery(phase,history,layout)
                        self.assertEqual(R.validate(phase,stages,terminal,checkpoint,history,layout),checkpoint['action']['EmptyEnter']['child'])

    def test_recovery_readonly_preview_and_whole_checkpoint(self):
        history='retained-zero';layout='empty-final-following'
        for mutate in [lambda stages,terminal:stages[0].update(dispatches=1),
                       lambda stages,terminal:stages[0]['preview'].update(missing=['child']),
                       lambda stages,terminal:stages[0]['preview'].update(emptyLabels=[]),
                       lambda stages,terminal:stages[0]['preview']['split'][1].update(text=' '),
                       lambda stages,terminal:stages[0]['preview']['current'].pop(),
                       lambda stages,terminal:stages[3]['preview'].update(submitDisabled=False),
                       lambda stages,terminal:stages[4]['preview'].update(panel=True),
                       lambda stages,terminal:stages[0]['journal']['active']['checkpoint'].update(version=5),
                       lambda stages,terminal:terminal['accountGuard'].update(restoredRead=terminal['accountGuard']['beforeRead'])]:
            stages,terminal,checkpoint=F.recovery('review',history,layout);mutate(stages,terminal)
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,checkpoint,history,layout)


if __name__=='__main__':unittest.main()
