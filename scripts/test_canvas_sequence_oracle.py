"""Independent sequence protocol regressions; all source inputs explicitly UTF-8."""
import copy
import unittest
import base64
import json
from pathlib import Path
import subprocess
import hashlib
import canvas_sequence_wire as W
import canvas_sequence_oracle as N
import canvas_sequence_recovery_oracle as R
import test_canvas_sequence_fixtures as F


def altered(value,rows):return dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in rows)))


class SequenceTests(unittest.TestCase):
    def check(self,index=0):
        e=W.config(**F.kwargs(index));source,value,sent=F.source_request(index);before=W.inventory(source,e)
        selected,rows=W.write(value,before,e);cp=F.checkpoint(source,value)
        W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='synthetic-only',checkpoint=cp)),e,value,source)
        contents={e.original:e.wire_seed,**{k:W.M.rtml(v) for k,v in zip(selected,e.final)}}
        sequences={e.original:2000,**{k:2000 if e.reused and i==0 else 1000 for i,k in enumerate(selected)}}
        received=[F.authored(raw,W.S.raw(before[W.text(row,1)]) if W.text(row,1) in before else None,contents[W.text(row,1)],sequences[W.text(row,1)],i==0) for i,(raw,row) in enumerate(zip(sent,rows))]
        W.ack(F.F.acknowledgement(received),rows,before,contents,sequences,e)
        return e,source,value,sent,before,selected,cp,received,contents,sequences

    def test_all_48_original_packets_and_histories(self):
        for index in range(48):
            with self.subTest(vector=index):self.check(index)
        e=W.config(3,corpus='unicode');self.assertEqual(e.seed,'Caf\u00e9 & \u65e5\u672c\U0001f600');self.assertEqual(W.T.utf16(e.seed),11)
        self.assertEqual(e.reports[0],('New \u65e5\u672c\U0001f600 &amp; paragraph',))

    def test_member_loss_reorder_duplicate_and_forged_ledger(self):
        e,source,value,sent,before,*_=self.check(24)
        for rows in (sent[:-1],sent[1:],sent+[sent[-1]],sent[:1]+sent[1:][::-1],sent[:2]+sent[1:2]+sent[3:]):
            with self.assertRaises(RuntimeError):W.write(altered(value,rows),before,e)
        for key,new in (('updates',[]),('reused',True),('finalRtml','wrong'),('position','zzz:temp')):
            bad=copy.deepcopy(value);bad['members'][0][key]=new
            with self.assertRaises(RuntimeError):W.write(bad,before,e)

    def test_return_empty_history_cannot_be_downgraded_or_changed(self):
        for index in (38,39,40,41):
            e,_,value,sent,before,*_=self.check(index)
            raw=sent[1];session=W.field(W.wire(raw),38,2)
            for bad in (F.F.remove(raw,12),F.F.replace(raw,38,F.F.remove(session,12)),F.F.replace(raw,38,session+F.F.f(6,W.text(W.wire(session),8)))):
                with self.assertRaises(RuntimeError):W.write(altered(value,sent[:1]+[bad]+sent[2:]),before,e)

    def test_full_ack_explicit_empty_and_original_status(self):
        for index in (0,24,38,41,45):
            e,_,_,sent,before,_,_,received,contents,sequences=self.check(index);rows=list(map(W.wire,sent))
            for i in range(len(received)):
                for bad in (F.F.remove(received[i],12),F.F.remove(received[i],19),F.F.replace(received[i],2,0)):
                    with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:i]+[bad]+received[i+1:]),rows,before,contents,sequences,e)
            with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:-1]),rows,before,contents,sequences,e)

    def test_v14_members_and_whole_source_custody(self):
        e,source,value,_,_,_,cp,*_=self.check(41)
        for mutate in ('version','members','reports','request','source'):
            bad=copy.deepcopy(cp)
            if mutate=='version':bad['version']=13
            elif mutate=='members':bad['action']['EndSequence']['members'].reverse()
            elif mutate=='reports':bad['action']['EndSequence']['members'][0]['updates']=['']
            elif mutate=='request':bad['request'].pop()
            else:bad['sections'].pop()
            with self.assertRaises(RuntimeError):W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='fixture',checkpoint=bad)),e,value,source)

    def test_all_48_synthetic_lifecycles_and_exact_native_event_sequences(self):
        for index in range(48):
            with self.subTest(vector=index):N.validate(F.receipt(index),**F.kwargs(index))
        for index in (12,23,24,35,36,39,41,44,45,46,47):
            for scenario in ('stale','partial'):N.validate(F.receipt(index,scenario),**F.kwargs(index))

    def test_native_prior_sections_ranges_trust_and_save_owner(self):
        for index in (24,35,36,39,41,44):
            value=F.receipt(index);e=W.config(**F.kwargs(index));witness=value['savedWitness']
            for key,new in (('beforeInputTrusted',False),('revision',999),('activeIndex',0),('start',-1),('beforeText','forged')):
                bad=copy.deepcopy(witness);bad['sequence']['events'][-1][key]=new
                with self.assertRaises(RuntimeError):N.witness(bad,e)
            bad=copy.deepcopy(witness);bad['sequence']['events'].pop(1)
            with self.assertRaises(RuntimeError):N.witness(bad,e)
            bad=copy.deepcopy(witness);bad['sequence']['events'][-1]['sections'][0]['html']+='<br>'
            with self.assertRaises(RuntimeError):N.witness(bad,e)
            for stage in value['stages']:
                if stage['stage'].endswith('-saving'):
                    bad=copy.deepcopy(value);bad['stages'][stage['ordinal']]['geometry']['native']['generation']='999'
                    with self.assertRaises(RuntimeError):N.validate(bad,**F.kwargs(index))
            bad=copy.deepcopy(value);bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop()
            with self.assertRaises(RuntimeError):N.validate(bad,**F.kwargs(index))

    def test_valid_identical_final_forged_histories(self):
        for mode,index in (('squash',36),('wrong-order',36),('extra-empty-cycle',38)):
            e,_,value,sent,before,*_=self.check(index);crdt=W.field(W.wire(sent[1]),38,2)
            run=subprocess.run(['node',str(Path(__file__).with_name('test_canvas_sequence_history.mjs'))],input=json.dumps(dict(document=F.F.b64(W.field(W.wire(crdt),5,2)),mode=mode)),
                               text=True,encoding='utf-8',capture_output=True,timeout=30,check=True)
            hostile=json.loads(run.stdout);self.assertEqual(hostile['text'],e.reports[0][-1]+'\ufeff')
            crdt=F.F.replace(F.F.replace(F.F.replace(crdt,5,base64.b64decode(hostile['document'])),6,hostile['head']),12,hostile['head'])
            with self.assertRaisesRegex(RuntimeError,'intermediate mutations'):W.write(altered(value,sent[:1]+[F.F.replace(sent[1],38,crdt)]+sent[2:]),before,e)

    def test_recovery_complete_inert_operation_and_explanations(self):
        for index in (0,11,12,23,24,35,36,39,41,44,45,46,47):
            for phase in R.STAGES:
                stages,terminal,cp=F.recovery(phase,index);R.validate(phase,stages,terminal,cp,**F.kwargs(index))
        for mutation in ('member','replay','warning','operation','account','archive','reports','notice'):
            stages,terminal,cp=F.recovery('review',41);bad=stages[0]
            if mutation=='member':bad['preview']['sequence']['members'].pop()
            elif mutation=='replay':terminal['dispatches']=1
            elif mutation=='warning':bad['preview']['warning']=bad['preview']['warning'].replace('every outcome unknown','success confirmed')
            elif mutation=='operation':stages[1]['journal']['active']['operation']='new operation'
            elif mutation=='account':stages[4]['preview']['panel']=True
            elif mutation=='archive':stages[-1]['journal']['archiveCount']=0
            elif mutation=='notice':bad['preview']['warning']=bad['preview']['warning'].replace(R.NOTICE,'')
            else:bad['journal']['active']['checkpoint']['action']['EndSequence']['members'][0]['updates']=[]
            with self.assertRaises(RuntimeError):R.validate('review',stages,terminal,cp,**F.kwargs(41))


class LongSequenceTests(unittest.TestCase):
    check=SequenceTests.check

    def test_all_18_long_original_packets_and_histories(self):
        self.assertEqual(hashlib.sha256((F.LONG_REF/'contract.json').read_bytes()).hexdigest(),
                         '7d3277c529d0564e619e104d2716a2704041ee366d3d7c24fa0ac10131097479')
        self.assertEqual(F.SHORT_COUNT,48);self.assertEqual(len(F.SUMMARIES),66)
        for index in range(48,66):
            with self.subTest(vector=index-48):
                e,_,_,_,_,selected,*_=self.check(index)
                self.assertEqual(len(selected),F.SUMMARIES[index]['appendCount'])
                reports=tuple(tuple(t['rtml'] for t in F.SUMMARIES[index]['transitions'] if t['kind']=='report' and t['appendIndex']==i) for i in range(e.append_count))
                self.assertEqual(e.reports,reports)
                if e.cadence in W.CADENCES[1:4]:self.assertEqual(e.target_index,F.SUMMARIES[index]['targetIndex'])
        e=W.config(8,'retained-zero','empty-final-following','correction-before-enter','unicode')
        self.assertEqual(e.updates[5][0],'Row 6 \u65e5\u672c\U0001f600 &')
        self.assertEqual(e.reports[5][0],'Row 6 \u65e5\u672c\U0001f600 &amp;')
        self.assertEqual(W.T.utf16(e.updates[5][0]),12);self.assertEqual(W.T.utf16(e.reports[5][0]),16)
        self.assertEqual(e.final[7],'Row 8 \u65e5\u672c\U0001f600 & paragraph')

    def test_long_synthetic_lifecycles_all_members_and_focused_failures(self):
        # Counts4/8/16 plus late mutations, trailing empty and untouched neighbor;
        # each lifecycle includes every appended member's later ordinary Save.
        for index in (48,52,54,57,58,64,65):
            with self.subTest(vector=index):
                value=F.receipt(index);N.validate(value,**F.kwargs(index))
                count=F.SUMMARIES[index]['appendCount']
                self.assertEqual(len(value['stages']),7+3*count)
                self.assertEqual(len(value['fixture']['writes']),count+2)
        for index in (54,57):
            for scenario in ('stale','partial'):N.validate(F.receipt(index,scenario),**F.kwargs(index))

    def test_long_all_member_loss_order_identity_and_ack_refusals(self):
        e,_,value,sent,before,_,_,received,contents,sequences=self.check(55)
        for offset in (1,8,16):
            for mutation in ('drop','duplicate','wrong-id','body','ack'):
                with self.subTest(offset=offset,mutation=mutation):
                    rows=sent.copy()
                    if mutation=='ack':
                        with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(received[:offset]+received[offset+1:]),list(map(W.wire,sent)),before,contents,sequences,e)
                        continue
                    if mutation=='drop':rows.pop(offset)
                    elif mutation=='duplicate':rows[offset]=rows[1 if offset!=1 else 2]
                    elif mutation=='wrong-id':rows[offset]=F.F.replace(rows[offset],1,'temp:C:xyz'+'f'*25)
                    else:rows[offset]=F.F.remove(rows[offset],12)
                    with self.assertRaises(RuntimeError):W.write(altered(value,rows),before,e)
        rows=sent.copy();rows[8],rows[9]=rows[9],rows[8]
        with self.assertRaises(RuntimeError):W.write(altered(value,rows),before,e)

    def test_long_same_final_history_and_late_event_refusals(self):
        for mode,index in (('squash',58),('wrong-order',58),('extra-empty-cycle',60)):
            e,_,value,sent,before,*_=self.check(index);offset=e.target_index+1
            crdt=W.field(W.wire(sent[offset]),38,2)
            run=subprocess.run(['node',str(Path(__file__).with_name('test_canvas_sequence_history.mjs'))],
                input=json.dumps(dict(document=F.F.b64(W.field(W.wire(crdt),5,2)),mode=mode,prefix='Row 15')),
                text=True,encoding='utf-8',capture_output=True,timeout=30,check=True)
            hostile=json.loads(run.stdout);self.assertEqual(hostile['text'],e.reports[e.target_index][-1]+'\ufeff')
            crdt=F.F.replace(F.F.replace(F.F.replace(crdt,5,base64.b64decode(hostile['document'])),6,hostile['head']),12,hostile['head'])
            rows=sent.copy();rows[offset]=F.F.replace(rows[offset],38,crdt)
            with self.assertRaisesRegex(RuntimeError,'intermediate mutations'):W.write(altered(value,rows),before,e)
        e=W.config(**F.kwargs(58));value=F.witness(58,11);N.witness(value,e)
        for key,new in (('activeIndex',1),('beforeInputTrusted',False),('beforeRevision',0)):
            bad=copy.deepcopy(value);bad['sequence']['events'][-2][key]=new
            with self.assertRaises(RuntimeError):N.witness(bad,e)
        bad=copy.deepcopy(value);bad['sequence']['events'][-1]['sections'][8]['html']='forged'
        with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_long_recovery_whole_ordered_inert_operation(self):
        for index in (55,59):
            for phase in R.STAGES:
                stages,terminal,cp=F.recovery(phase,index)
                ids=R.validate(phase,stages,terminal,cp,**F.kwargs(index))
                self.assertEqual(len(ids),F.SUMMARIES[index]['appendCount'])
        for mutation in ('middle-preview','last-preview','checkpoint','replay','unknown','storage'):
            stages,terminal,cp=F.recovery('storage-failure',55)
            if mutation in ('middle-preview','last-preview'):stages[0]['preview']['sequence']['members'].pop(8 if mutation=='middle-preview' else -1)
            elif mutation=='checkpoint':cp['action']['EndSequence']['members'].pop(8)
            elif mutation=='replay':terminal['dispatches']=1
            elif mutation=='unknown':stages[0]['preview']['warning']=stages[0]['preview']['warning'].replace(R.WARNING,'accepted')
            else:next(s for s in stages if s['stage']=='storage-failed')['journal']['active']['operation']='replacement'
            with self.assertRaises(RuntimeError):R.validate('storage-failure',stages,terminal,cp,**F.kwargs(55))

    def test_finite_fixture_selector_and_later_owner_readback_refusals(self):
        for count in (0,-1,17,4099,True,4.0):
            with self.assertRaises(RuntimeError):W.config(count)
        with self.assertRaises(RuntimeError):W.config(4,cadence='correction-before-enter')
        self.assertEqual(W.config(2,cadence='correction-before-enter').target_index,0)
        value=F.receipt(54)
        for name in ('member-15-saving','member-8-saving','member-0-saving'):
            bad=copy.deepcopy(value);stage=next(s for s in bad['stages'] if s['stage']==name);stage['geometry']['native']['generation']='9999'
            with self.assertRaises(RuntimeError):N.validate(bad,**F.kwargs(54))
        bad=copy.deepcopy(value);bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop(9)
        with self.assertRaises(RuntimeError):N.validate(bad,**F.kwargs(54))


if __name__=='__main__':unittest.main()
