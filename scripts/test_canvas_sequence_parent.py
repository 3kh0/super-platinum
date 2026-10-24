"""Parent CONTENT oracle checked against original packets and real controller plans."""
import base64
import copy
import json
from pathlib import Path
import subprocess
import unittest

import canvas_sequence_input as I
import canvas_sequence_oracle as N
import canvas_sequence_wire as W
import canvas_sequence_recovery_oracle as R
import test_canvas_sequence_fixtures as F
from test_canvas_sequence_revisit import witness

ROOT=Path(__file__).resolve().parent.parent
REF=ROOT/'tmp/canvas-parent-sequence-20261023/reference'
VECTORS=json.loads((REF/'parent-bindings.json').read_text(encoding='utf-8'))['vectors']


def fixture(v):
    cadence='parent-'+v['revisit'].replace('exact-','')
    e=W.config(3,v['mode'],v['layout'],cadence,v['corpus'])
    index=next(i for i,s in enumerate(F.SUMMARIES) if s['appendCount']==3 and s['mode']==v['mode']
               and s['layout']==v['layout'] and s['corpus']==v['corpus'] and s['cadence']=='straight')
    source,value,old=F.source_request(index)
    sent=list(map(W.S.raw,W.S.messages(W.decoded(v['wireBase64']),1)))
    for i,prior in enumerate(old):
        identity=W.text(W.wire(prior),1)
        ss=F.F.replace(W.field(W.wire(sent[i]),38,2),1,identity)
        sent[i]=F.F.replace(F.F.replace(F.F.replace(sent[i],1,identity),38,ss),33,F.SESSION+'-0')
        if i and not (i==1 and e.reused):
            pos=W.text(W.wire(prior),8);sent[i]=F.F.replace(F.F.replace(sent[i],8,pos),21,pos)
        if i==0:
            # Native fixture source has language=en; the original runtime adapter
            # has no language. Bind each packet's metadata to its actual source.
            sent[i]=F.F.replace(sent[i],12,F.F.body(e.parent_wire,True))
    value.update(parentUpdates=list(e.parent_reports),parentFinalRtml=e.parent_wire,
                 documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in sent)))
    cp=F.checkpoint(source,value);cp['version']=15
    cp['action']={'EndSequenceParent':dict(parent_updates=list(e.parent_reports),members=cp['action']['EndSequence']['members'])}
    return e,index,source,value,sent,cp


def journal(cp):return dict(scope=W.S.SCOPE,receipt=dict(operation='inert-parent-fixture',checkpoint=cp))


class ParentTests(unittest.TestCase):
    def test_parent_content_language_is_bound_to_original_source(self):
        for v in VECTORS:
            with self.subTest(ordinal=v['ordinal']):
                e,_,source,value,sent,_=fixture(v)
                native_source=W.inventory(source,e)[e.original]
                W.parent_content(W.wire(sent[0]),native_source,value,e)
                no_language_source=W.wire(F.F.replace(W.S.raw(native_source),12,F.F.body(e.wire_seed,False)))
                # Use the original reference CONTENT bytes unchanged.
                original=W.S.messages(W.decoded(v['wireBase64']),1)[0]
                original_body=W.field(original,12,2)
                original_row=W.wire(F.F.replace(sent[0],12,original_body))
                W.parent_content(original_row,no_language_source,value,e)
                for row,prior in [(original_row,native_source),(W.wire(sent[0]),no_language_source)]:
                    with self.assertRaisesRegex(RuntimeError,'source language/body metadata'):
                        W.parent_content(row,prior,value,e)
                # Matching visible text cannot hide language replacement, duplicate
                # fields, unknown body metadata or a changed content variant.
                for inner in (F.F.f(1,e.parent_wire)+F.F.f(2,'fr'),
                              F.F.f(1,e.parent_wire)+F.F.f(2,'en')+F.F.f(2,'en'),
                              F.F.f(1,e.parent_wire)+F.F.f(2,'en')+F.F.f(3,'extra')):
                    bad=W.wire(F.F.replace(sent[0],12,F.F.f(1,inner)))
                    with self.assertRaises(RuntimeError):W.parent_content(bad,native_source,value,e)

    def test_all_original_histories_packets_v15_and_complete_ack(self):
        self.assertEqual(len(VECTORS),48)
        for v in VECTORS:
            with self.subTest(ordinal=v['ordinal']):
                e,_,source,value,sent,cp=fixture(v);before=W.inventory(source,e)
                ids,rows=W.write(value,before,e);W.journal(journal(cp),e,value,source)
                changed=[r for r in v['parentReports'] if not r['unchanged']]
                self.assertEqual(list(e.parent_reports),[r['rtml'] for r in changed])
                self.assertEqual(W.parent_patch_groups(e),[[p for group in r['patches'] for p in group] for r in changed])
                bodies={e.original:e.parent_wire,**{identity:W.M.rtml(content) for identity,content in zip(ids,e.final)}}
                sequences={e.original:2000,**{identity:2000 if e.reused and i==0 else 1000 for i,identity in enumerate(ids)}}
                accepted=[F.authored(raw,W.S.raw(before[W.text(row,1)]) if W.text(row,1) in before else None,
                    bodies[W.text(row,1)],sequences[W.text(row,1)],i==0) for i,(raw,row) in enumerate(zip(sent,rows))]
                W.ack(F.F.acknowledgement(accepted),rows,before,bodies,sequences,e)
                raw_before={r['id']:base64.b64decode(r['raw']) for r in source['sections']}
                raw_before.update({W.text(W.wire(row),1):row for row in accepted})
                readback=F.F.inventory(sorted(raw_before.values(),key=lambda row:W.text(W.wire(row),8)))
                W.inventory(readback,e,source,ids)
                # Full-operation ACK must reject every missing member, parent included.
                for missing in range(len(accepted)):
                    with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(accepted[:missing]+accepted[missing+1:]),rows,before,bodies,sequences,e)
                for parent in (F.F.remove(accepted[0],12),F.F.replace(accepted[0],12,F.F.body('forged',True))):
                    with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement([parent]+accepted[1:]),rows,before,bodies,sequences,e)

    def test_parent_report_history_and_schema_cannot_be_downgraded(self):
        for v in VECTORS[::8]:
            e,_,source,value,sent,cp=fixture(v);before=W.inventory(source,e)
            for key,bad in [('parentUpdates',[]),('parentUpdates',[e.parent_wire]),('parentFinalRtml','forged')]:
                if key=='parentUpdates' and bad==value[key]:continue
                with self.assertRaises(RuntimeError):W.write({**value,key:bad},before,e)
            for parent in (F.F.remove(sent[0],12),F.F.remove(sent[0],38)):
                bad=dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,r) for r in [parent]+sent[1:])))
                with self.assertRaises(RuntimeError):W.write(bad,before,e)
            for mutation in ('version','draft','action','reports','request','inventory'):
                bad=copy.deepcopy(cp)
                if mutation=='version':bad['version']=14
                elif mutation=='draft':bad['draft']='forged'
                elif mutation=='action':bad['action']={'EndSequence':{'members':bad['action']['EndSequenceParent']['members']}}
                elif mutation=='reports':bad['action']['EndSequenceParent']['parent_updates'].reverse();bad['action']['EndSequenceParent']['parent_updates'].append(e.wire_seed)
                elif mutation=='request':bad['request'].pop()
                else:bad['sections'].pop()
                with self.assertRaises(RuntimeError):W.journal(journal(bad),e,value,source)
        # Same final original text is insufficient: swap revert history for untouched source history.
        for v in (v for v in VECTORS if v['revisit']=='exact-revert'):
            e,_,source,value,sent,_=fixture(v);before=W.inventory(source,e)
            prior=W.field(before[e.original],38,2);now=W.field(W.wire(sent[0]),38,2)
            wrong=F.F.replace(now,5,W.field(W.wire(prior),5,2))
            row=F.F.replace(sent[0],38,wrong)
            bad=dict(value,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,r) for r in [row]+sent[1:])))
            with self.assertRaises(RuntimeError):W.write(bad,before,e)

    def test_controller_reference_cadence_ranges_and_preserved_members(self):
        script="import {actionPlan} from './scripts/canvas_sequence_input.mjs';console.log(JSON.stringify(['ascii','unicode'].flatMap(corpus=>['correction','erase','revert'].flatMap(k=>[3,8].map(count=>({corpus,cadence:'parent-'+k,count,actions:actionPlan(corpus,count,'parent-'+k)}))))));"
        plans=json.loads(subprocess.run(['node','--input-type=module','-e',script],cwd=ROOT,text=True,encoding='utf-8',capture_output=True,check=True,timeout=30).stdout)
        for plan in plans:
            with self.subTest(plan={k:v for k,v in plan.items() if k!='actions'}):
                e=W.config(plan['count'],cadence=plan['cadence'],corpus=plan['corpus'])
                value=witness(plan);N.witness(value,e)
                self.assertEqual([a['activeIndex'] for a in plan['actions'] if a['type']=='click'],[0,2])
                edits=[event for event in value['sequence']['events'] if event['kind']=='edit' and event['activeIndex']==0]
                self.assertEqual([event['afterText'] for event in edits],list(e.parent_updates))
                self.assertEqual([x['activeIndex'] for x in value['sequence']['events'][:5]],[1,1,2,2,0])
                for key,bad_value in [('activeIndex',1),('revision',99),('beforeInputTrusted',False),('beforeText','forged')]:
                    bad=copy.deepcopy(value);bad['sequence']['events'][4][key]=bad_value
                    with self.assertRaises(RuntimeError):N.witness(bad,e)
                bad=copy.deepcopy(value);bad['sequence']['events'][4]['sections'][1]['html']='<span>changed</span>'
                with self.assertRaises(RuntimeError):N.witness(bad,e)

    def test_recovery_retains_parent_history_and_refuses_replay_or_preview_loss(self):
        for v in [v for v in VECTORS if v['schedule']=='parent-after-second' and v['corpus']=='unicode']:
            e,index,source,value,_,cp=fixture(v)
            for phase in R.STAGES:
                stages,terminal,_=F.recovery(phase,index)
                for stage in stages:
                    stage['sequenceCadence']=e.cadence
                    if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
                    seq=stage['preview'].get('sequence')
                    if seq:
                        seq.update(parentText=e.parent_final,parentReportCount=len(e.parent_reports),parentEmptyLabel=None if e.parent_final else 'Empty paragraph',historyNotice=True)
                terminal['sequenceCadence']=e.cadence;terminal['journal']=copy.deepcopy(stages[-1]['journal'])
                R.validate(phase,stages,terminal,cp,append_count=3,history=e.history,layout=e.layout,cadence=e.cadence,corpus=e.corpus)
                if phase=='review':
                    for field,new in [('parentText','forged'),('parentReportCount',0),('historyNotice',False)]:
                        bad=copy.deepcopy(stages);next(s for s in bad if s['preview'].get('sequence'))['preview']['sequence'][field]=new
                        with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,append_count=3,history=e.history,layout=e.layout,cadence=e.cadence,corpus=e.corpus)

    def test_count_admission_and_early_cli_refusals(self):
        for cadence in ('parent-correction','parent-erase','parent-revert'):
            for count in (2,3,16):
                request=dict(ok=True,fixture='canvas-structure',nonce='a'*32,ordinal=0,corpus='ascii',mode='sequence',typing='none',sequenceHistory='bootstrap',sequenceLayout='no-following',sequenceCount=count,sequenceCadence=cadence,fieldId='canvas-text-draft',seed='Keep this',offset=9,revision=0,generation='1',visit='0')
                args=('canvas-structure','a'*32,0,'bootstrap','no-following','ascii',count,cadence)
                if count<3:
                    with self.assertRaises(RuntimeError):I.validate_request(request,*args)
                    with self.assertRaises(RuntimeError):W.config(count,cadence=cadence)
                else:I.validate_request(request,*args)
            for script in ('check-canvas-split-native.py','check-canvas-split-recovery-native.py'):
                common=['--fixture','canvas-structure' if script=='check-canvas-split-native.py' else 'canvas-local-review','--captures',str(ROOT/'tmp/must-not-create-parent-cli')]
                if script=='check-canvas-split-native.py':common+=['--scenario','success']
                for args,message in [(['--mode','sequence','--sequence-cadence',cadence],'at least 3 appends'),(['--mode','middle','--sequence-count','3','--sequence-cadence',cadence],'require sequence mode')]:
                    run=subprocess.run(['python',str(ROOT/'scripts'/script),*common,*args],cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=20)
                    self.assertEqual(run.returncode,2);self.assertIn(message,run.stderr)
        self.assertFalse((ROOT/'tmp/must-not-create-parent-cli').exists())

    def test_v14_additive_ledger_and_preview_remain_legacy(self):
        for index in (0,24,39):
            e=W.config(**F.kwargs(index));source,value,_=F.source_request(index)
            value.update(parentUpdates=[],parentFinalRtml=e.wire_seed)
            W.write(value,W.inventory(source,e),e)
            cp=F.checkpoint(source,value);self.assertEqual(cp['version'],14)
            W.journal(journal(cp),e,value,source)
            stages,terminal,cp=F.recovery('review',index)
            for stage in stages:
                seq=stage['preview'].get('sequence')
                if seq:seq.update(parentReportCount=0,parentEmptyLabel=None,historyNotice=bool(any(e.reports)))
            R.validate('review',stages,terminal,cp,**F.kwargs(index))
            bad=copy.deepcopy(stages)
            next(s for s in bad if s['preview'].get('sequence'))['preview']['sequence']['parentReportCount']=False
            with self.assertRaises(RuntimeError):R.validate('review',bad,terminal,cp,**F.kwargs(index))


if __name__=='__main__':unittest.main()
