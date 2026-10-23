"""Start-only exact insertion, ownership, lineage absence and inert recovery checks."""
import copy
import json
import unittest
import canvas_start_wire as W
import canvas_start_oracle as O
import canvas_start_recovery_oracle as R
import canvas_split_oracle as S
import test_canvas_start_fixtures as F


class Start(unittest.TestCase):
    def test_original_bootstrap_and_both_cohorts_both_later_edits(self):
        for corpus in ('ascii','unicode'):
            for parent in ('bootstrap','retained'):
                with self.subTest(corpus=corpus,parent=parent):
                    p=F.receipt(corpus=corpus,parent_mode=parent);O.validate(p,corpus,parent)
                    new,row=W.write(p['fixture']['writes'][0]);crdt=S.wire(S.field(row,38,2))
                    history=W.T.decode_history(S.field(crdt,5,2))[0]['changes']
                    self.assertEqual(len(history),1);self.assertEqual(history[0]['text'],'\ufeff')
                    self.assertEqual([r['id'] for r in p['fixture']['fresh']['sections']],['title',new,'paragraph','neighbor'])
                    original=p['original']['fresh']['sections'][1]
                    for stage in p['stages'][3:9]:self.assertEqual(stage['fixture']['fresh']['sections'][2],original)
                    self.assertEqual([w['sequence'] for w in p['fixture']['writes']],[1,1,2])

    def test_stale_and_incomplete_ack_keep_exact_v1(self):
        for corpus in ('ascii','unicode'):
            for parent in ('bootstrap','retained'):
                for scenario in ('stale','partial'):O.validate(F.receipt(scenario,corpus,parent),corpus,parent)
        p=F.receipt('partial');self.assertEqual(len(p['fixture']['writes']),1);self.assertEqual(p['fixture']['readbacks'],[])
        self.assertEqual(p['stages'][2]['journal'],p['stages'][3]['journal'])
        bad=copy.deepcopy(p);bad['stages'][3]['journal']['receipt']=None
        with self.assertRaises(RuntimeError):O.validate(bad)

    def test_exact_leading_dom_and_original_caret_required(self):
        e=W.config('unicode');sealed=F.witness(10,'unicode');O.witness(sealed,e)
        for kind in ('offset','caret','order','original','soft-break','wrapper','second-break','raw-content','typing','end'):
            bad=copy.deepcopy(sealed)
            if kind=='offset':bad['offset']=11
            elif kind=='caret':bad['leadingPlaceholder']['caretSection']=0
            elif kind=='order':bad['after'][0]['elements'].reverse()
            elif kind=='original':bad['after'][0]['elements'][1]['elements'][0]['text']='changed'
            elif kind=='soft-break':bad['leadingPlaceholder']['firstHtml']='<br>'
            elif kind=='wrapper':bad['leadingPlaceholder']['firstHtml']='<strong><br></strong>'
            elif kind=='second-break':bad['leadingPlaceholder']['firstHtml']='<span><br><br></span>'
            elif kind=='raw-content':bad['rawAfter'][0]['elements'][0]['elements'][0]['text']='\nvisible'
            elif kind=='typing':bad['typing']={'after':bad['after']}
            else:bad['kind']='end'
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):O.witness(bad,e)

    def test_one_new_packet_empty_content_and_no_split_lineage(self):
        source,write,row=F.source_request();new=write['child'];crdt=S.field(S.wire(row),38,2)
        for kind in ('two-rows','old-id','after','content','language','lineage','extra-head','base','counter'):
            altered=row
            if kind=='old-id':altered=F.F.replace(row,1,'paragraph')
            elif kind=='after':altered=F.F.replace(row,8,'aad:temp')
            elif kind=='content':altered=F.F.replace(row,12,F.F.body('new',False))
            elif kind=='language':altered=F.F.replace(row,12,F.F.body(''))
            elif kind=='lineage':altered=F.F.replace(row,38,crdt+F.F.f(10,b''))
            elif kind=='extra-head':altered=F.F.replace(row,38,crdt+F.F.f(6,'1'*64))
            elif kind=='base':altered=F.F.replace(row,38,F.F.replace(crdt,2,1000))
            elif kind=='counter':altered=F.F.replace(row,20,2)
            data=F.F.f(14,F.F.TITLE)+F.F.f(1,altered)+(F.F.f(1,row) if kind=='two-rows' else b'')
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):W.write(dict(write,documentData=F.F.b64(data)))
        W.write(write)

    def test_original_inventory_must_remain_byte_exact_after_insert(self):
        p=F.receipt(corpus='unicode',parent_mode='retained');new=p['fixture']['writes'][0]['child']
        original=p['original']['fresh'];canonical=p['stages'][3]['fixture']['fresh'];e=W.config('unicode','retained')
        for kind in ('old-history','old-revision','old-position','missing-neighbor','new-after'):
            bad=copy.deepcopy(canonical)
            if kind=='missing-neighbor':bad['sections'].pop()
            elif kind=='new-after':bad['sections'][1:3]=reversed(bad['sections'][1:3])
            else:
                row=bad['sections'][2];raw=S.raw(S.decoded(row['raw']))
                if kind=='old-history':raw=F.F.remove(raw,38)
                elif kind=='old-revision':raw=F.F.replace(raw,2,2000);row['sequence']=2000
                else:raw=F.F.replace(raw,8,'aad')
                row['raw']=F.F.b64(raw)
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):W.inventory(bad,e,original,new)
        styled=copy.deepcopy(original);row=styled['sections'][1];row['raw']=F.F.b64(F.F.replace(S.raw(S.decoded(row['raw'])),9,64))
        with self.assertRaises(RuntimeError):W.inventory(styled,e)

    def test_ack_explicit_empty_body_and_independent_readback_required(self):
        p=F.receipt()
        for kind in ('ack-empty-body-absent','ack-old-id','readback','new-owner-drift','old-owner-reused','counter-reset'):
            bad=copy.deepcopy(p)
            if kind.startswith('ack-'):
                ack=bad['stages'][3]['fixture']['acknowledgements'][0];outer=S.decoded(ack['raw']);data=S.wire(S.field(outer,2,2));row=S.raw(S.messages(data,1)[0])
                row=F.F.remove(row,12) if kind=='ack-empty-body-absent' else F.F.replace(row,1,'paragraph')
                ack['raw']=F.F.b64(F.F.f(1,1)+F.F.f(2,F.F.f(15,'document')+F.F.f(1,row)))
            elif kind=='readback':bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop(1)
            elif kind=='new-owner-drift':bad['stages'][5]['geometry']['native']['generation']='200'
            elif kind=='old-owner-reused':bad['stages'][7]['geometry']['native']['generation']='104'
            else:bad['stages'][8]['fixture']['writes'][2]['sequence']=1
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):O.validate(bad)

    def test_all_inert_v1_recovery_phases(self):
        for corpus in ('ascii','unicode'):
            for parent in ('bootstrap','retained'):
                for phase in R.STAGES:
                    with self.subTest(corpus=corpus,parent=parent,phase=phase):R.validate(phase,*F.recovery(phase,corpus,parent),corpus,parent)

    def test_insertion_preview_and_v1_custody_cannot_be_split_or_replayed(self):
        stages,terminal,cp=F.recovery('review','unicode','retained')
        for kind in ('version','target','direction','integer-direction','draft','source','request'):
            bad=copy.deepcopy(cp)
            if kind=='version':bad['version']=3
            elif kind=='target':bad['target']='paragraph'
            elif kind=='direction':bad['action']['Insert']['before']=False
            elif kind=='integer-direction':bad['action']['Insert']['before']=1
            elif kind=='draft':bad['draft']='original cleared'
            elif kind=='source':bad['sections'][0]=bad['sections'][1]
            else:bad['request'].pop()
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):R.retained_insert(bad,stages[0]['canonical'],'unicode','retained')
        for kind in ('draft','anchor','empty-label','two-part','dispatch','owner'):
            bad=copy.deepcopy(stages);p=bad[0]['preview']
            if kind=='draft':p['insertion']['draftText']='original'
            elif kind=='anchor':p['insertion']['currentText']=''
            elif kind=='empty-label':p['insertion']['emptyLabel']=None
            elif kind=='two-part':p['warning']+=R.review.WARNINGS[0]
            elif kind=='dispatch':bad[0]['dispatches']=1
            else:bad[4]['preview']['reviewActionPresent']=True
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):R.validate('review',bad,terminal,cp,'unicode','retained')


if __name__=='__main__':unittest.main()
